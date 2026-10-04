mod calc;
mod db;
mod seed;
mod service;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone)]
struct AppState {
    pool: SqlitePool,
}

struct ApiErr(StatusCode, String);
impl IntoResponse for ApiErr {
    fn into_response(self) -> axum::response::Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}
fn bad(msg: impl Into<String>) -> ApiErr {
    ApiErr(StatusCode::BAD_REQUEST, msg.into())
}
fn conflict(msg: impl Into<String>) -> ApiErr {
    ApiErr(StatusCode::CONFLICT, msg.into())
}
fn internal(e: impl std::fmt::Display) -> ApiErr {
    ApiErr(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://priceprotect.db".into());
    let pool = db::connect(&db_url).await?;
    seed::seed_if_empty(&pool).await?;

    let state = AppState { pool };
    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "../frontend/dist".into());

    let api = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/orders", get(list_orders))
        .route("/orders/{id}", get(order_detail))
        .route("/skus", get(list_skus))
        .route("/pp/preview", post(pp_preview))
        .route("/pp/apply", post(pp_apply))
        .route("/applications", get(list_applications))
        .route("/applications/{id}/approve", post(approve_application))
        .route("/applications/{id}/reject", post(reject_application))
        .route("/refunds", get(list_refunds))
        .route("/receipts", get(list_receipts))
        // mock 接口:价格与支付
        .route("/mock/prices/{sku_id}", get(mock_price))
        .route("/mock/receipts/{receipt_no}/resend", post(resend_receipt))
        .route("/webhooks/payment-receipt", post(payment_receipt_webhook))
        // 演示控制台
        .route("/admin/prices", post(admin_add_price))
        .route("/admin/rules", get(admin_list_rules).post(admin_add_rule))
        .route("/admin/reset", post(admin_reset))
        .with_state(state);

    let app = Router::new()
        .nest("/api", api)
        .fallback_service(ServeDir::new(&static_dir).not_found_service(ServeFile::new(
            format!("{static_dir}/index.html"),
        )))
        .layer(CorsLayer::permissive());

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("价保系统已启动: http://localhost:{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

// ---------------- 查询 ----------------

#[derive(Deserialize)]
struct OrderQuery {
    customer: Option<String>,
}

async fn list_orders(
    State(s): State<AppState>,
    Query(q): Query<OrderQuery>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    let orders = if let Some(c) = q.customer.filter(|c| !c.is_empty()) {
        sqlx::query_as::<_, db::Order>("SELECT * FROM orders WHERE customer_name = ? ORDER BY id")
            .bind(c)
            .fetch_all(&s.pool)
            .await
    } else {
        sqlx::query_as::<_, db::Order>("SELECT * FROM orders ORDER BY id")
            .fetch_all(&s.pool)
            .await
    }
    .map_err(internal)?;
    let mut out = Vec::new();
    for o in &orders {
        let items = sqlx::query_as::<_, db::OrderItem>("SELECT * FROM order_items WHERE order_id = ?")
            .bind(o.id)
            .fetch_all(&s.pool)
            .await
            .map_err(internal)?;
        out.push(json!({ "order": o, "items": items }));
    }
    Ok(Json(json!({ "orders": out })))
}

async fn order_detail(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    let order = sqlx::query_as::<_, db::Order>("SELECT * FROM orders WHERE id = ?")
        .bind(id)
        .fetch_optional(&s.pool)
        .await
        .map_err(internal)?
        .ok_or_else(|| bad("订单不存在"))?;
    let items = sqlx::query_as::<_, db::OrderItem>("SELECT * FROM order_items WHERE order_id = ?")
        .bind(id)
        .fetch_all(&s.pool)
        .await
        .map_err(internal)?;
    let mut item_views = Vec::new();
    for it in &items {
        let cur = service::current_price(&s.pool, it.sku_id).await.unwrap_or(0);
        let refunded = service::refunded_so_far(&s.pool, it.id).await.unwrap_or(0);
        item_views.push(json!({
            "item": it, "current_price_cents": cur, "refunded_cents": refunded,
        }));
    }
    let apps = sqlx::query_as::<_, db::Application>(
        "SELECT * FROM pp_applications WHERE order_id = ? ORDER BY id DESC",
    )
    .bind(id)
    .fetch_all(&s.pool)
    .await
    .map_err(internal)?;
    let refunds = sqlx::query_as::<_, db::Refund>(
        "SELECT * FROM refunds WHERE order_id = ? ORDER BY id DESC",
    )
    .bind(id)
    .fetch_all(&s.pool)
    .await
    .map_err(internal)?;
    Ok(Json(json!({
        "order": order, "items": item_views, "applications": apps, "refunds": refunds,
    })))
}

async fn list_skus(State(s): State<AppState>) -> Result<Json<serde_json::Value>, ApiErr> {
    let rows: Vec<(i64, i64, String, String)> = sqlx::query_as(
        "SELECT skus.id, skus.product_id, products.name, skus.spec FROM skus JOIN products ON products.id = skus.product_id ORDER BY skus.id",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(internal)?;
    let mut out = Vec::new();
    for (id, pid, name, spec) in rows {
        let price = service::current_price(&s.pool, id).await.unwrap_or(0);
        out.push(json!({"id": id, "product_id": pid, "product_name": name, "spec": spec, "price_cents": price}));
    }
    Ok(Json(json!({ "skus": out })))
}

// ---------------- 价保申请 ----------------

#[derive(Deserialize)]
struct ApplyReq {
    order_item_id: i64,
    sku_id: i64,
}

fn breakdown_json(b: &calc::Breakdown, rule_version: i64) -> serde_json::Value {
    json!({
        "qty": b.qty,
        "orig_unit_price_cents": b.orig_unit_price,
        "orig_activity_disc_cents": b.orig_activity_disc,
        "orig_coupon_disc_cents": b.orig_coupon_disc,
        "orig_paid_cents": b.orig_paid,
        "cur_unit_price_cents": b.cur_unit_price,
        "cur_activity_disc_cents": b.cur_activity_disc,
        "cur_effective_cents": b.cur_effective,
        "cumulative_refundable_cents": b.cumulative_refundable,
        "already_refunded_cents": b.already_refunded,
        "this_refund_cents": b.this_refund,
        "rule_version": rule_version,
        "coupon_note": "活动券已在原订单抵扣,当前价不重复抵扣",
    })
}

async fn pp_preview(
    State(s): State<AppState>,
    Json(req): Json<ApplyReq>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    match service::compute_breakdown(&s.pool, req.order_item_id, req.sku_id).await {
        Ok((ctx, b, rule)) => Ok(Json(json!({
            "ok": true,
            "breakdown": breakdown_json(&b, rule.version),
            "rule_name": rule.name,
            "order_no": ctx.order.order_no,
            "product_name": ctx.item.product_name,
            "sku_spec": ctx.item.sku_spec,
            "protect_until": ctx.order.protect_until,
        }))),
        Err(reason) => Ok(Json(json!({ "ok": false, "reason": reason }))),
    }
}

async fn pp_apply(
    State(s): State<AppState>,
    Json(req): Json<ApplyReq>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    match service::submit_application(&s.pool, req.order_item_id, req.sku_id).await {
        Ok(app) => Ok(Json(json!({ "ok": true, "application": app }))),
        Err(reason) => Ok(Json(json!({ "ok": false, "reason": reason }))),
    }
}

// ---------------- 店员审核 ----------------

#[derive(Deserialize)]
struct AppQuery {
    status: Option<String>,
}

async fn list_applications(
    State(s): State<AppState>,
    Query(q): Query<AppQuery>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    let apps = match q.status.as_deref().filter(|s| !s.is_empty()) {
        Some(st) => sqlx::query_as::<_, db::Application>(
            "SELECT * FROM pp_applications WHERE status = ? ORDER BY id DESC",
        )
        .bind(st)
        .fetch_all(&s.pool)
        .await
        .map_err(internal)?,
        None => sqlx::query_as::<_, db::Application>("SELECT * FROM pp_applications ORDER BY id DESC")
            .fetch_all(&s.pool)
            .await
            .map_err(internal)?,
    };
    let mut out = Vec::new();
    for a in &apps {
        let order = sqlx::query_as::<_, db::Order>("SELECT * FROM orders WHERE id = ?")
            .bind(a.order_id)
            .fetch_one(&s.pool)
            .await
            .map_err(internal)?;
        let item = sqlx::query_as::<_, db::OrderItem>("SELECT * FROM order_items WHERE id = ?")
            .bind(a.order_item_id)
            .fetch_one(&s.pool)
            .await
            .map_err(internal)?;
        let refund = sqlx::query_as::<_, db::Refund>(
            "SELECT * FROM refunds WHERE application_id = ?",
        )
        .bind(a.id)
        .fetch_optional(&s.pool)
        .await
        .map_err(internal)?;
        out.push(json!({
            "application": a,
            "order_no": order.order_no,
            "customer_name": order.customer_name,
            "product_name": item.product_name,
            "sku_spec": item.sku_spec,
            "refund": refund,
        }));
    }
    Ok(Json(json!({ "applications": out })))
}

#[derive(Deserialize)]
struct ReviewReq {
    reviewer: Option<String>,
    reason: Option<String>,
}

async fn approve_application(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<ReviewReq>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    let reviewer = req.reviewer.unwrap_or_else(|| "店员小王".into());
    service::approve(&s.pool, id, &reviewer)
        .await
        .map(Json)
        .map_err(conflict)
}

async fn reject_application(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<ReviewReq>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    let reviewer = req.reviewer.unwrap_or_else(|| "店员小王".into());
    let reason = req.reason.unwrap_or_else(|| "不符合价保规则".into());
    service::reject(&s.pool, id, &reviewer, &reason)
        .await
        .map_err(conflict)?;
    Ok(Json(json!({ "ok": true })))
}

// ---------------- 退款与回执 ----------------

async fn list_refunds(State(s): State<AppState>) -> Result<Json<serde_json::Value>, ApiErr> {
    let refunds = sqlx::query_as::<_, db::Refund>("SELECT * FROM refunds ORDER BY id DESC")
        .fetch_all(&s.pool)
        .await
        .map_err(internal)?;
    let mut out = Vec::new();
    for r in &refunds {
        let order_no: (String,) = sqlx::query_as("SELECT order_no FROM orders WHERE id = ?")
            .bind(r.order_id)
            .fetch_one(&s.pool)
            .await
            .map_err(internal)?;
        let deliveries: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM refund_receipts WHERE receipt_no = (SELECT receipt_no FROM refunds WHERE id = ?)",
        )
        .bind(r.id)
        .fetch_one(&s.pool)
        .await
        .map_err(internal)?;
        out.push(json!({ "refund": r, "order_no": order_no.0, "receipt_deliveries": deliveries.0 }));
    }
    Ok(Json(json!({ "refunds": out })))
}

async fn list_receipts(State(s): State<AppState>) -> Result<Json<serde_json::Value>, ApiErr> {
    let receipts = sqlx::query_as::<_, db::Receipt>(
        "SELECT * FROM refund_receipts ORDER BY id DESC",
    )
    .fetch_all(&s.pool)
    .await
    .map_err(internal)?;
    Ok(Json(json!({ "receipts": receipts })))
}

// ---------------- mock 接口 ----------------

async fn mock_price(
    State(s): State<AppState>,
    Path(sku_id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    let price = service::current_price(&s.pool, sku_id)
        .await
        .map_err(|e| bad(e.to_string()))?;
    Ok(Json(json!({ "sku_id": sku_id, "price_cents": price, "source": "mock-price-service" })))
}

#[derive(Deserialize)]
struct WebhookReceipt {
    receipt_no: String,
    refund_no: String,
    amount_cents: i64,
    idempotency_key: String,
}

async fn payment_receipt_webhook(
    State(s): State<AppState>,
    Json(r): Json<WebhookReceipt>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    let receipt = service::GatewayReceipt {
        receipt_no: r.receipt_no,
        refund_no: r.refund_no,
        amount_cents: r.amount_cents,
        idempotency_key: r.idempotency_key,
    };
    let res = service::deliver_receipt(&s.pool, &receipt)
        .await
        .map_err(internal)?;
    Ok(Json(json!({ "duplicated": res.duplicated, "message": res.message })))
}

/// 模拟支付网关重复推送同一张回执
async fn resend_receipt(
    State(s): State<AppState>,
    Path(receipt_no): Path<String>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    let row: Option<(String, i64, String)> = sqlx::query_as(
        "SELECT r.refund_no, r.amount_cents, r.idempotency_key FROM refunds r
         JOIN refund_receipts rc ON rc.receipt_no = ? AND rc.refund_no = r.refund_no LIMIT 1",
    )
    .bind(&receipt_no)
    .fetch_optional(&s.pool)
    .await
    .map_err(internal)?;
    let (refund_no, amount, idem) = row.ok_or_else(|| bad("回执不存在"))?;
    let receipt = service::mock_gateway_refund(&refund_no, amount, &idem);
    let res = service::deliver_receipt(&s.pool, &receipt)
        .await
        .map_err(internal)?;
    Ok(Json(json!({ "duplicated": res.duplicated, "message": res.message })))
}

// ---------------- 演示控制台 ----------------

#[derive(Deserialize)]
struct PriceReq {
    sku_id: i64,
    price_cents: i64,
}

async fn admin_add_price(
    State(s): State<AppState>,
    Json(req): Json<PriceReq>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    if req.price_cents < 0 {
        return Err(bad("价格不能为负"));
    }
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO price_history (sku_id, price_cents, effective_at, created_at) VALUES (?,?,?,?)")
        .bind(req.sku_id)
        .bind(req.price_cents)
        .bind(&now)
        .bind(&now)
        .execute(&s.pool)
        .await
        .map_err(internal)?;
    Ok(Json(json!({ "ok": true, "sku_id": req.sku_id, "price_cents": req.price_cents })))
}

async fn admin_list_rules(State(s): State<AppState>) -> Result<Json<serde_json::Value>, ApiErr> {
    let rules = sqlx::query_as::<_, db::ActivityRule>("SELECT * FROM activity_rules ORDER BY version")
        .fetch_all(&s.pool)
        .await
        .map_err(internal)?;
    Ok(Json(json!({ "rules": rules })))
}

#[derive(Deserialize)]
struct RuleReq {
    name: String,
    rule_type: String,
    percent_off: Option<i64>,
    full_threshold_cents: Option<i64>,
    reduction_cents: Option<i64>,
}

async fn admin_add_rule(
    State(s): State<AppState>,
    Json(req): Json<RuleReq>,
) -> Result<Json<serde_json::Value>, ApiErr> {
    let (max_v,): (Option<i64>,) = sqlx::query_as("SELECT MAX(version) FROM activity_rules")
        .fetch_one(&s.pool)
        .await
        .map_err(internal)?;
    let version = max_v.unwrap_or(0) + 1;
    let now = chrono::Utc::now().to_rfc3339();
    match req.rule_type.as_str() {
        "percent_off" if req.percent_off.is_some() => {}
        "full_reduction" if req.full_threshold_cents.is_some() && req.reduction_cents.is_some() => {}
        _ => return Err(bad("规则参数不完整:percent_off 需要 percent_off;full_reduction 需要 threshold 与 reduction")),
    }
    sqlx::query(
        "INSERT INTO activity_rules (version, name, rule_type, percent_off, full_threshold_cents, reduction_cents, effective_at, created_at)
         VALUES (?,?,?,?,?,?,?,?)",
    )
    .bind(version)
    .bind(&req.name)
    .bind(&req.rule_type)
    .bind(req.percent_off)
    .bind(req.full_threshold_cents)
    .bind(req.reduction_cents)
    .bind(&now)
    .bind(&now)
    .execute(&s.pool)
    .await
    .map_err(internal)?;
    Ok(Json(json!({ "ok": true, "version": version })))
}

async fn admin_reset(State(s): State<AppState>) -> Result<Json<serde_json::Value>, ApiErr> {
    seed::reset(&s.pool).await.map_err(internal)?;
    Ok(Json(json!({ "ok": true })))
}
