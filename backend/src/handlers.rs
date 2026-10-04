use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::calc;
use crate::db::Pool;
use crate::models::*;
use crate::mockpay;

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
fn short_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_uppercase()
}

// ---------------- 顾客端 ----------------

#[derive(Deserialize)]
pub struct CustomerQuery {
    customer: Option<String>,
}

pub async fn list_orders(
    State(pool): State<Pool>,
    Query(q): Query<CustomerQuery>,
) -> Json<Value> {
    let customer = q.customer.unwrap_or_else(|| "张三".into());
    let orders = sqlx::query_as::<_, Order>(
        "SELECT * FROM orders WHERE customer = ? ORDER BY id",
    )
    .bind(&customer)
    .fetch_all(&pool)
    .await
    .unwrap_or_default();

    let mut out = Vec::new();
    for o in orders {
        let items = sqlx::query_as::<_, OrderItem>(
            "SELECT * FROM order_items WHERE order_id = ? ORDER BY id",
        )
        .bind(o.id)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();
        let mut item_json = Vec::new();
        for it in items {
            let (product_name, spec): (String, String) = sqlx::query_as(
                "SELECT p.name, s.spec FROM skus s JOIN products p ON p.id = s.product_id WHERE s.id = ?",
            )
            .bind(it.sku_id)
            .fetch_one(&pool)
            .await
            .unwrap_or(("?".into(), "?".into()));
            let rule_version: String =
                sqlx::query_scalar("SELECT version FROM rule_versions WHERE id = ?")
                    .bind(it.rule_version_id)
                    .fetch_one(&pool)
                    .await
                    .unwrap_or_default();
            let already = calc::refunded_so_far(&pool, it.id).await;
            item_json.push(json!({
                "id": it.id, "sku_id": it.sku_id, "product_name": product_name, "spec": spec,
                "qty": it.qty, "activity_price": it.activity_price, "coupon_amount": it.coupon_amount,
                "paid_amount": it.paid_amount, "returned": it.returned,
                "rule_version": rule_version, "already_refunded": already,
            }));
        }
        out.push(json!({
            "id": o.id, "order_no": o.order_no, "customer": o.customer, "status": o.status,
            "created_at": o.created_at, "protected_until": o.protected_until,
            "items": item_json,
        }));
    }
    Json(json!({ "orders": out }))
}

#[derive(Deserialize)]
pub struct PreviewQuery {
    order_item_id: i64,
    claimed_sku_id: Option<i64>,
}

/// 申请前预览：逐项展示计算依据与资格检查
pub async fn preview(
    State(pool): State<Pool>,
    Query(q): Query<PreviewQuery>,
) -> Json<Value> {
    match calc::compute_breakdown(&pool, q.order_item_id, q.claimed_sku_id).await {
        Ok(b) => Json(json!({ "ok": true, "breakdown": b })),
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
}

#[derive(Deserialize)]
pub struct ApplyBody {
    order_item_id: i64,
    claimed_sku_id: Option<i64>,
}

/// 提交价保申请。幂等：Idempotency-Key 唯一；同一明细有待审核申请时直接复用。
pub async fn apply(
    State(pool): State<Pool>,
    headers: HeaderMap,
    Json(body): Json<ApplyBody>,
) -> (axum::http::StatusCode, Json<Value>) {
    let idem = headers
        .get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let Some(idem) = idem else {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "ok": false, "error": "缺少 Idempotency-Key 请求头" })),
        );
    };

    // 1) 幂等键去重：重复提交返回原申请
    if let Some(existing) = sqlx::query_as::<_, Application>(
        "SELECT * FROM applications WHERE idempotency_key = ?",
    )
    .bind(&idem)
    .fetch_optional(&pool)
    .await
    .unwrap_or(None)
    {
        return (
            axum::http::StatusCode::OK,
            Json(json!({ "ok": true, "deduplicated": true, "application": existing,
                         "message": "重复提交被幂等键拦截，返回原申请" })),
        );
    }

    // 2) 同一明细已有待审核申请 → 不重复建单
    if let Some(existing) = sqlx::query_as::<_, Application>(
        "SELECT * FROM applications WHERE order_item_id = ? AND status = 'pending' ORDER BY id DESC LIMIT 1",
    )
    .bind(body.order_item_id)
    .fetch_optional(&pool)
    .await
    .unwrap_or(None)
    {
        return (
            axum::http::StatusCode::OK,
            Json(json!({ "ok": true, "deduplicated": true, "application": existing,
                         "message": "该商品已有待审核的价保申请，复用原申请" })),
        );
    }

    // 3) 计算依据（含资格检查）
    let breakdown = match calc::compute_breakdown(&pool, body.order_item_id, body.claimed_sku_id).await {
        Ok(b) => b,
        Err(e) => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(json!({ "ok": false, "error": e })),
            )
        }
    };

    let status = if breakdown.eligible { "pending" } else { "rejected" };
    let reviewed_at = if breakdown.eligible { None } else { Some(now()) };
    let app_no = format!("PP{}", short_id());
    let snapshot = serde_json::to_string(&breakdown).unwrap();

    let res = sqlx::query(
        "INSERT INTO applications (app_no, order_item_id, idempotency_key, status, calc_snapshot, refund_amount, reject_reason, created_at, reviewed_at)
         VALUES (?,?,?,?,?,0,?,?,?)",
    )
    .bind(&app_no)
    .bind(body.order_item_id)
    .bind(&idem)
    .bind(status)
    .bind(&snapshot)
    .bind(&breakdown.reject_reason)
    .bind(now())
    .bind(&reviewed_at)
    .execute(&pool)
    .await;

    match res {
        Ok(_) => {
            let app = sqlx::query_as::<_, Application>(
                "SELECT * FROM applications WHERE idempotency_key = ?",
            )
            .bind(&idem)
            .fetch_one(&pool)
            .await
            .unwrap();
            (
                axum::http::StatusCode::OK,
                Json(json!({ "ok": true, "deduplicated": false, "application": app, "breakdown": breakdown })),
            )
        }
        // 并发下同键冲突 → 返回既有申请，不多建
        Err(_) => {
            let app = sqlx::query_as::<_, Application>(
                "SELECT * FROM applications WHERE idempotency_key = ?",
            )
            .bind(&idem)
            .fetch_one(&pool)
            .await
            .unwrap();
            (
                axum::http::StatusCode::OK,
                Json(json!({ "ok": true, "deduplicated": true, "application": app,
                             "message": "并发重复提交被唯一约束拦截" })),
            )
        }
    }
}

/// 我的申请列表（含退款信息）
pub async fn my_applications(
    State(pool): State<Pool>,
    Query(q): Query<CustomerQuery>,
) -> Json<Value> {
    let customer = q.customer.unwrap_or_else(|| "张三".into());
    let rows = sqlx::query_as::<_, Application>(
        "SELECT a.* FROM applications a
         JOIN order_items i ON i.id = a.order_item_id
         JOIN orders o ON o.id = i.order_id
         WHERE o.customer = ? ORDER BY a.id DESC",
    )
    .bind(&customer)
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let mut out = Vec::new();
    for a in rows {
        let refund = sqlx::query_as::<_, Refund>(
            "SELECT * FROM refunds WHERE application_id = ?",
        )
        .bind(a.id)
        .fetch_optional(&pool)
        .await
        .unwrap_or(None);
        let snapshot: Value = serde_json::from_str(&a.calc_snapshot).unwrap_or(json!({}));
        out.push(json!({
            "application": a, "breakdown": snapshot, "refund": refund,
        }));
    }
    Json(json!({ "applications": out }))
}

// ---------------- 店员端 ----------------

#[derive(Deserialize)]
pub struct StatusQuery {
    status: Option<String>,
}

pub async fn staff_queue(State(pool): State<Pool>, Query(q): Query<StatusQuery>) -> Json<Value> {
    let status = q.status.unwrap_or_else(|| "pending".into());
    let rows = if status == "all" {
        sqlx::query_as::<_, Application>("SELECT * FROM applications ORDER BY id DESC")
            .fetch_all(&pool)
            .await
    } else {
        sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE status = ? ORDER BY id")
            .bind(&status)
            .fetch_all(&pool)
            .await
    }
    .unwrap_or_default();
    let mut out = Vec::new();
    for a in rows {
        let snapshot: Value = serde_json::from_str(&a.calc_snapshot).unwrap_or(json!({}));
        let refund = sqlx::query_as::<_, Refund>("SELECT * FROM refunds WHERE application_id = ?")
            .bind(a.id)
            .fetch_optional(&pool)
            .await
            .unwrap_or(None);
        out.push(json!({ "application": a, "breakdown": snapshot, "refund": refund }));
    }
    Json(json!({ "applications": out }))
}

/// 店员复核通过：事务内重算差价 → 建退款单 → 调支付网关 → 回执入账。
/// 防重：申请状态条件更新 + refunds.application_id 唯一 + 支付幂等键唯一。
pub async fn approve(State(pool): State<Pool>, Path(id): Path<i64>) -> (axum::http::StatusCode, Json<Value>) {
    let mut tx = match pool.begin().await {
        Ok(t) => t,
        Err(e) => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok":false,"error":e.to_string()}))),
    };

    let app = match sqlx::query_as::<_, Application>(
        "SELECT * FROM applications WHERE id = ? AND status = 'pending'",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    {
        Ok(Some(a)) => a,
        _ => {
            return (
                axum::http::StatusCode::CONFLICT,
                Json(json!({ "ok": false, "error": "申请不存在或已处理（重复审核已被拦截）" })),
            )
        }
    };

    // 复核时按最新价格重算，保证金额准确
    let breakdown = match calc::compute_breakdown(&pool, app.order_item_id, None).await {
        Ok(b) => b,
        Err(e) => return (axum::http::StatusCode::BAD_REQUEST, Json(json!({"ok":false,"error":e}))),
    };

    if !breakdown.eligible || breakdown.this_refund <= 0 {
        let reason = breakdown
            .reject_reason
            .clone()
            .unwrap_or_else(|| "复核时无新增差价可退".into());
        sqlx::query("UPDATE applications SET status='rejected', reject_reason=?, reviewed_at=? WHERE id=?")
            .bind(&reason)
            .bind(now())
            .bind(id)
            .execute(&mut *tx)
            .await
            .ok();
        tx.commit().await.ok();
        let app = sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        return (
            axum::http::StatusCode::OK,
            Json(json!({ "ok": true, "auto_rejected": true, "application": app, "breakdown": breakdown,
                         "message": format!("复核未通过：{}", reason) })),
        );
    }

    // 累计退款不得超过原订单可退金额
    let amount = breakdown.this_refund.min(breakdown.refundable_cap);
    let refund_no = format!("RF{}", short_id());
    let idem_key = format!("refund-app-{}", app.id);

    let ins = sqlx::query(
        "INSERT INTO refunds (refund_no, application_id, order_item_id, amount, idempotency_key, status, created_at)
         VALUES (?,?,?,?,?,'initiated',?)",
    )
    .bind(&refund_no)
    .bind(app.id)
    .bind(app.order_item_id)
    .bind(amount)
    .bind(&idem_key)
    .bind(now())
    .execute(&mut *tx)
    .await;

    if let Err(e) = ins {
        tx.rollback().await.ok();
        return (
            axum::http::StatusCode::CONFLICT,
            Json(json!({ "ok": false, "error": format!("退款单已存在（防重约束拦截）：{}", e) })),
        );
    }

    sqlx::query("UPDATE applications SET status='approved', refund_amount=?, reviewed_at=? WHERE id=?")
        .bind(amount)
        .bind(now())
        .bind(id)
        .execute(&mut *tx)
        .await
        .ok();
    tx.commit().await.ok();

    // 调用模拟支付网关（网关级幂等）并投递回执
    let receipt = mockpay::gateway_refund(&pool, &idem_key, amount).await;
    match receipt {
        Ok(r) => {
            let delivered = mockpay::deliver_receipt(&pool, &r.receipt_no, &refund_no, amount).await;
            let refund = sqlx::query_as::<_, Refund>("SELECT * FROM refunds WHERE refund_no = ?")
                .bind(&refund_no)
                .fetch_one(&pool)
                .await
                .unwrap();
            let app = sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
            (
                axum::http::StatusCode::OK,
                Json(json!({ "ok": true, "application": app, "refund": refund,
                             "receipt": r, "delivery": delivered.unwrap_or(json!(null)) })),
            )
        }
        Err(e) => (
            axum::http::StatusCode::BAD_GATEWAY,
            Json(json!({ "ok": false, "error": format!("支付网关调用失败：{}", e) })),
        ),
    }
}

#[derive(Deserialize)]
pub struct RejectBody {
    reason: String,
}

pub async fn reject(
    State(pool): State<Pool>,
    Path(id): Path<i64>,
    Json(body): Json<RejectBody>,
) -> (axum::http::StatusCode, Json<Value>) {
    let updated = sqlx::query(
        "UPDATE applications SET status='rejected', reject_reason=?, reviewed_at=? WHERE id=? AND status='pending'",
    )
    .bind(&body.reason)
    .bind(now())
    .bind(id)
    .execute(&pool)
    .await
    .map(|r| r.rows_affected())
    .unwrap_or(0);
    if updated == 0 {
        return (
            axum::http::StatusCode::CONFLICT,
            Json(json!({ "ok": false, "error": "申请不存在或已处理" })),
        );
    }
    let app = sqlx::query_as::<_, Application>("SELECT * FROM applications WHERE id = ?")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    (axum::http::StatusCode::OK, Json(json!({ "ok": true, "application": app })))
}

pub async fn list_refunds(State(pool): State<Pool>) -> Json<Value> {
    let refunds = sqlx::query_as::<_, Refund>("SELECT * FROM refunds ORDER BY id DESC")
        .fetch_all(&pool)
        .await
        .unwrap_or_default();
    let mut out = Vec::new();
    for r in refunds {
        let receipts = sqlx::query_as::<_, PaymentReceipt>(
            "SELECT * FROM payment_receipts WHERE refund_id = ? ORDER BY id",
        )
        .bind(r.id)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();
        out.push(json!({ "refund": r, "receipts": receipts }));
    }
    Json(json!({ "refunds": out }))
}

// ---------------- 支付回执 webhook ----------------

#[derive(Deserialize)]
pub struct WebhookBody {
    receipt_no: String,
    refund_no: String,
    amount: i64,
}

pub async fn payment_webhook(
    State(pool): State<Pool>,
    Json(body): Json<WebhookBody>,
) -> Json<Value> {
    match mockpay::deliver_receipt(&pool, &body.receipt_no, &body.refund_no, body.amount).await {
        Ok(v) => Json(json!({ "ok": true, "result": v })),
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
}

// ---------------- 元数据 / 演示控制 ----------------

pub async fn meta(State(pool): State<Pool>) -> Json<Value> {
    let rules = sqlx::query_as::<_, RuleVersion>("SELECT * FROM rule_versions ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap_or_default();
    let skus = sqlx::query_as::<_, Sku>("SELECT * FROM skus ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap_or_default();
    let mut sku_json = Vec::new();
    for s in skus {
        let product_name: String = sqlx::query_scalar("SELECT name FROM products WHERE id = ?")
            .bind(s.product_id)
            .fetch_one(&pool)
            .await
            .unwrap_or_default();
        let pe = calc::current_price_event(&pool, s.id).await.ok();
        let (price, rule_version, rule_id) = match &pe {
            Some(p) => {
                let v: String = sqlx::query_scalar("SELECT version FROM rule_versions WHERE id = ?")
                    .bind(p.rule_version_id)
                    .fetch_one(&pool)
                    .await
                    .unwrap_or_default();
                (p.activity_price, v, p.rule_version_id)
            }
            None => (0, "-".into(), 0),
        };
        sku_json.push(json!({
            "id": s.id, "product_id": s.product_id, "product_name": product_name, "spec": s.spec,
            "current_price": price, "current_rule_version": rule_version, "current_rule_id": rule_id,
        }));
    }
    let events = sqlx::query_as::<_, PriceEvent>(
        "SELECT * FROM price_events ORDER BY id DESC LIMIT 20",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    Json(json!({ "rule_versions": rules, "skus": sku_json, "price_events": events }))
}

#[derive(Deserialize)]
pub struct PriceDropBody {
    sku_id: i64,
    activity_price: i64,
    rule_version_id: i64,
}

/// 演示：模拟一次降价（新价格事件 + 当时生效的规则版本）
pub async fn price_drop(
    State(pool): State<Pool>,
    Json(body): Json<PriceDropBody>,
) -> (axum::http::StatusCode, Json<Value>) {
    let sku_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM skus WHERE id = ?")
        .bind(body.sku_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    let rule_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rule_versions WHERE id = ?")
        .bind(body.rule_version_id)
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    if sku_exists == 0 || rule_exists == 0 {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "ok": false, "error": "SKU 或规则版本不存在" })),
        );
    }
    sqlx::query("INSERT INTO price_events (sku_id, activity_price, rule_version_id, effective_at) VALUES (?,?,?,?)")
        .bind(body.sku_id)
        .bind(body.activity_price)
        .bind(body.rule_version_id)
        .bind(now())
        .execute(&pool)
        .await
        .unwrap();
    (axum::http::StatusCode::OK, Json(json!({ "ok": true })))
}

/// 演示：支付网关重放回执（模拟重复送达）
pub async fn resend_receipt(
    State(pool): State<Pool>,
    Path(receipt_no): Path<String>,
) -> (axum::http::StatusCode, Json<Value>) {
    let row = sqlx::query_as::<_, MockPayRefund>(
        "SELECT * FROM mock_pay_refunds WHERE receipt_no = ?",
    )
    .bind(&receipt_no)
    .fetch_optional(&pool)
    .await
    .unwrap_or(None);
    let Some(m) = row else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({ "ok": false, "error": "回执不存在" })),
        );
    };
    let refund = sqlx::query_as::<_, Refund>(
        "SELECT * FROM refunds WHERE idempotency_key = ?",
    )
    .bind(&m.idempotency_key)
    .fetch_one(&pool)
    .await
    .unwrap();
    let result = mockpay::deliver_receipt(&pool, &m.receipt_no, &refund.refund_no, m.amount).await;
    (
        axum::http::StatusCode::OK,
        Json(json!({ "ok": true, "result": result.unwrap_or(json!(null)) })),
    )
}

pub async fn reset_demo(State(pool): State<Pool>) -> Json<Value> {
    crate::seed::reset(&pool).await;
    Json(json!({ "ok": true, "message": "演示数据已重置" }))
}

pub async fn mock_pay_ledger(State(pool): State<Pool>) -> Json<Value> {
    let rows = sqlx::query_as::<_, MockPayRefund>(
        "SELECT * FROM mock_pay_refunds ORDER BY id DESC",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    Json(json!({ "gateway_refunds": rows }))
}
