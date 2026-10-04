//! 价保业务逻辑:试算、申请、审核、退款、回执幂等处理。
use anyhow::{anyhow, Result};
use chrono::Utc;
use sqlx::SqlitePool;

use crate::calc;
use crate::db::{ActivityRule, Application, Order, OrderItem};

/// 当前生效的活动规则(版本号最大者)
pub async fn current_rule(pool: &SqlitePool) -> Result<ActivityRule> {
    sqlx::query_as::<_, ActivityRule>("SELECT * FROM activity_rules ORDER BY version DESC LIMIT 1")
        .fetch_one(pool)
        .await
        .map_err(|e| anyhow!("无可用活动规则: {e}"))
}

/// mock 价格接口:当前吊牌价 = 价格流水最新一条
pub async fn current_price(pool: &SqlitePool, sku_id: i64) -> Result<i64> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT price_cents FROM price_history WHERE sku_id = ? ORDER BY effective_at DESC, id DESC LIMIT 1",
    )
    .bind(sku_id)
    .fetch_optional(pool)
    .await?;
    row.map(|r| r.0).ok_or_else(|| anyhow!("SKU {sku_id} 无价格数据"))
}

/// 该明细此前已成功退款的差价合计
pub async fn refunded_so_far(pool: &SqlitePool, order_item_id: i64) -> Result<i64> {
    let (sum,): (Option<i64>,) = sqlx::query_as(
        "SELECT SUM(amount_cents) FROM refunds WHERE order_item_id = ? AND status = 'success'",
    )
    .bind(order_item_id)
    .fetch_one(pool)
    .await?;
    Ok(sum.unwrap_or(0))
}

pub struct ItemCtx {
    pub order: Order,
    pub item: OrderItem,
}

pub async fn load_item(pool: &SqlitePool, order_item_id: i64) -> Result<ItemCtx> {
    let item = sqlx::query_as::<_, OrderItem>("SELECT * FROM order_items WHERE id = ?")
        .bind(order_item_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| anyhow!("订单明细不存在"))?;
    let order = sqlx::query_as::<_, Order>("SELECT * FROM orders WHERE id = ?")
        .bind(item.order_id)
        .fetch_one(pool)
        .await?;
    Ok(ItemCtx { order, item })
}

/// 申请前校验,返回拒绝原因(None = 通过)
pub async fn eligibility_rejection(
    pool: &SqlitePool,
    ctx: &ItemCtx,
    sku_id: i64,
) -> Result<Option<String>> {
    if ctx.order.status == "returned" {
        return Ok(Some("订单已退货,不支持价保".into()));
    }
    let protect_until = chrono::DateTime::parse_from_rfc3339(&ctx.order.protect_until)
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or(Utc::now());
    if Utc::now() > protect_until {
        return Ok(Some(format!(
            "已超过价保期限(截止 {})",
            protect_until.format("%Y-%m-%d %H:%M")
        )));
    }
    if sku_id != ctx.item.sku_id {
        return Ok(Some(format!(
            "相同商品不同规格不参与价保(订单规格:{},申请规格:{})",
            ctx.item.sku_spec,
            sku_spec_name(pool, sku_id).await.unwrap_or_else(|| format!("SKU#{sku_id}"))
        )));
    }
    let (open,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM pp_applications WHERE order_item_id = ? AND status IN ('pending','approved')",
    )
    .bind(ctx.item.id)
    .fetch_one(pool)
    .await?;
    if open > 0 {
        return Ok(Some("已有进行中的价保申请,请勿重复提交".into()));
    }
    Ok(None)
}

async fn sku_spec_name(pool: &SqlitePool, sku_id: i64) -> Option<String> {
    let row: Option<(String,)> = sqlx::query_as("SELECT spec FROM skus WHERE id = ?")
        .bind(sku_id)
        .fetch_optional(pool)
        .await
        .ok()?;
    row.map(|r| r.0)
}

/// 计算差价明细;前置校验失败时返回 Err(原因)
pub async fn compute_breakdown(
    pool: &SqlitePool,
    order_item_id: i64,
    sku_id: i64,
) -> Result<(ItemCtx, calc::Breakdown, ActivityRule), String> {
    let ctx = load_item(pool, order_item_id)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(reason) = eligibility_rejection(pool, &ctx, sku_id)
        .await
        .map_err(|e| e.to_string())?
    {
        return Err(reason);
    }
    let rule = current_rule(pool).await.map_err(|e| e.to_string())?;
    let cur_price = current_price(pool, sku_id).await.map_err(|e| e.to_string())?;
    let already = refunded_so_far(pool, ctx.item.id)
        .await
        .map_err(|e| e.to_string())?;
    let b = calc::compute(
        ctx.item.qty,
        ctx.item.unit_price_cents,
        ctx.item.activity_discount_cents,
        ctx.item.coupon_discount_cents,
        ctx.item.paid_cents,
        cur_price,
        rule.to_calc_rule(),
        already,
    );
    if b.this_refund <= 0 {
        if b.cumulative_refundable > 0 {
            return Err("此前差价已全部退还,本次无新增差价".into());
        }
        return Err("当前价格不低于原实付金额,无差价可退".into());
    }
    Ok((ctx, b, rule))
}

/// 提交申请:在事务内重算并写入,依赖唯一索引兜底并发重复
pub async fn submit_application(
    pool: &SqlitePool,
    order_item_id: i64,
    sku_id: i64,
) -> Result<Application, String> {
    let (ctx, b, rule) = compute_breakdown(pool, order_item_id, sku_id).await?;
    let now = Utc::now();
    let apply_no = format!(
        "PP{}{:06}",
        now.format("%Y%m%d%H%M%S"),
        now.timestamp_subsec_nanos() % 1_000_000
    );
    let res = sqlx::query(
        "INSERT INTO pp_applications
         (apply_no, order_id, order_item_id, sku_id, status, qty,
          orig_unit_price_cents, orig_activity_disc_cents, orig_coupon_disc_cents, orig_paid_cents,
          cur_unit_price_cents, cur_activity_disc_cents, cur_effective_cents, rule_version,
          cumulative_refundable_cents, already_refunded_cents, this_refund_cents, created_at)
         VALUES (?,?,?,?, 'pending', ?, ?,?,?,?, ?,?,?,?, ?,?,?, ?)",
    )
    .bind(&apply_no)
    .bind(ctx.order.id)
    .bind(ctx.item.id)
    .bind(sku_id)
    .bind(b.qty)
    .bind(b.orig_unit_price)
    .bind(b.orig_activity_disc)
    .bind(b.orig_coupon_disc)
    .bind(b.orig_paid)
    .bind(b.cur_unit_price)
    .bind(b.cur_activity_disc)
    .bind(b.cur_effective)
    .bind(rule.version)
    .bind(b.cumulative_refundable)
    .bind(b.already_refunded)
    .bind(b.this_refund)
    .bind(now.to_rfc3339())
    .execute(pool)
    .await;
    match res {
        Ok(r) => {
            let id = r.last_insert_rowid();
            let app = sqlx::query_as::<_, Application>("SELECT * FROM pp_applications WHERE id = ?")
                .bind(id)
                .fetch_one(pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(app)
        }
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            Err("已有进行中的价保申请,请勿重复提交".into())
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 审核通过:事务内校验状态与累计退款上限,创建退款单(幂等键唯一),
/// 然后调用 mock 支付网关并投递回执。
pub async fn approve(pool: &SqlitePool, app_id: i64, reviewer: &str) -> Result<serde_json::Value, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let app = sqlx::query_as::<_, Application>(
        "SELECT * FROM pp_applications WHERE id = ?",
    )
    .bind(app_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "申请不存在".to_string())?;

    if app.status != "pending" {
        return Err(format!("申请当前状态为 {},不可重复审核", app.status));
    }

    // 累计退款上限复核:明细级 + 订单级(此前已成功退款 + 本次 <= 实付)
    let (item_refunded,): (Option<i64>,) = sqlx::query_as(
        "SELECT SUM(amount_cents) FROM refunds WHERE order_item_id = ? AND status = 'success'",
    )
    .bind(app.order_item_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let item_refunded = item_refunded.unwrap_or(0);
    if item_refunded + app.this_refund_cents > app.orig_paid_cents {
        return Err("累计退款将超出该明细可退金额,已拦截".into());
    }
    let (order_refunded, order_paid): (Option<i64>, i64) = sqlx::query_as(
        "SELECT (SELECT SUM(amount_cents) FROM refunds WHERE order_id = ? AND status = 'success'), paid_cents FROM orders WHERE id = ?",
    )
    .bind(app.order_id)
    .bind(app.order_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    if order_refunded.unwrap_or(0) + app.this_refund_cents > order_paid {
        return Err("累计退款将超出原订单可退金额,已拦截".into());
    }

    let now = Utc::now().to_rfc3339();
    let changed = sqlx::query(
        "UPDATE pp_applications SET status='approved', reviewed_by=?, reviewed_at=? WHERE id=? AND status='pending'",
    )
    .bind(reviewer)
    .bind(&now)
    .bind(app_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?
    .rows_affected();
    if changed == 0 {
        return Err("申请已被其他店员处理".into());
    }

    // 幂等键:同一申请重复触发退款只会产生一张退款单
    let idem = format!("refund:{}", app.apply_no);
    let refund_no = format!("RF{}", &app.apply_no[2..]);
    sqlx::query(
        "INSERT INTO refunds (refund_no, application_id, order_id, order_item_id, amount_cents, rule_version, status, idempotency_key, created_at, updated_at)
         VALUES (?,?,?,?,?,?, 'processing', ?, ?, ?)
         ON CONFLICT(idempotency_key) DO NOTHING",
    )
    .bind(&refund_no)
    .bind(app.id)
    .bind(app.order_id)
    .bind(app.order_item_id)
    .bind(app.this_refund_cents)
    .bind(app.rule_version)
    .bind(&idem)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    let refund = sqlx::query_as::<_, crate::db::Refund>(
        "SELECT * FROM refunds WHERE idempotency_key = ?",
    )
    .bind(&idem)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;

    // 调用 mock 支付网关发起退款,网关按幂等键返回回执
    let receipt = mock_gateway_refund(&refund.refund_no, refund.amount_cents, &refund.idempotency_key);
    // 回执投递(与 webhook / 重发共用同一幂等入口)
    let delivery = deliver_receipt(pool, &receipt).await.map_err(|e| e.to_string())?;

    // 返回回执处理后的最终退款状态
    let refund = sqlx::query_as::<_, crate::db::Refund>(
        "SELECT * FROM refunds WHERE idempotency_key = ?",
    )
    .bind(&idem)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
        "application_id": app.id,
        "refund": refund,
        "receipt": receipt,
        "delivery": delivery,
    }))
}

/// 审核拒绝
pub async fn reject(pool: &SqlitePool, app_id: i64, reviewer: &str, reason: &str) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    let changed = sqlx::query(
        "UPDATE pp_applications SET status='rejected', reject_reason=?, reviewed_by=?, reviewed_at=? WHERE id=? AND status='pending'",
    )
    .bind(reason)
    .bind(reviewer)
    .bind(&now)
    .bind(app_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?
    .rows_affected();
    if changed == 0 {
        return Err("申请不存在或已被处理".into());
    }
    Ok(())
}

// ---------------- mock 支付网关 ----------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GatewayReceipt {
    pub receipt_no: String,
    pub refund_no: String,
    pub amount_cents: i64,
    pub idempotency_key: String,
}

/// mock 支付网关:按幂等键确定性地生成回执。
/// 同一幂等键重复调用返回同一张回执(网关侧幂等)。
pub fn mock_gateway_refund(refund_no: &str, amount_cents: i64, idempotency_key: &str) -> GatewayReceipt {
    // 用稳定哈希从幂等键派生回执号,重复调用结果一致
    let mut h: u64 = 0xcbf29ce484222325;
    for b in idempotency_key.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    GatewayReceipt {
        receipt_no: format!("RCPT{h:016X}"),
        refund_no: refund_no.to_string(),
        amount_cents,
        idempotency_key: idempotency_key.to_string(),
    }
}

#[derive(Debug, serde::Serialize)]
pub struct DeliveryResult {
    pub duplicated: bool,
    pub message: String,
}

/// 回执统一入口(网关同步返回 / webhook / 重发都走这里)。
/// 重复送达只记录、不改变任何状态,绝不多退。
pub async fn deliver_receipt(pool: &SqlitePool, receipt: &GatewayReceipt) -> Result<DeliveryResult> {
    let mut tx = pool.begin().await?;
    let now = Utc::now().to_rfc3339();

    let existing: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM refund_receipts WHERE receipt_no = ? AND duplicated = 0")
            .bind(&receipt.receipt_no)
            .fetch_optional(&mut *tx)
            .await?;

    if existing.is_some() {
        // 重复回执:记录投递痕迹,不做任何状态变更
        sqlx::query(
            "INSERT INTO refund_receipts (receipt_no, refund_no, amount_cents, duplicated, received_at) VALUES (?,?,?,1,?)",
        )
        .bind(&receipt.receipt_no)
        .bind(&receipt.refund_no)
        .bind(receipt.amount_cents)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(DeliveryResult {
            duplicated: true,
            message: format!("回执 {} 重复送达,已忽略,不产生重复退款", receipt.receipt_no),
        });
    }

    // 首次送达:记录回执并推进退款状态
    sqlx::query(
        "INSERT INTO refund_receipts (receipt_no, refund_no, amount_cents, duplicated, received_at) VALUES (?,?,?,0,?)",
    )
    .bind(&receipt.receipt_no)
    .bind(&receipt.refund_no)
    .bind(receipt.amount_cents)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    let refund = sqlx::query_as::<_, crate::db::Refund>(
        "SELECT * FROM refunds WHERE refund_no = ?",
    )
    .bind(&receipt.refund_no)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| anyhow!("回执对应的退款单 {} 不存在", receipt.refund_no))?;

    if refund.amount_cents != receipt.amount_cents {
        anyhow::bail!(
            "回执金额({})与退款单金额({})不一致,需人工核查",
            receipt.amount_cents,
            refund.amount_cents
        );
    }

    sqlx::query(
        "UPDATE refunds SET status='success', receipt_no=?, updated_at=? WHERE refund_no=? AND status='processing'",
    )
    .bind(&receipt.receipt_no)
    .bind(&now)
    .bind(&receipt.refund_no)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE pp_applications SET status='refunded' WHERE id=? AND status='approved'",
    )
    .bind(refund.application_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(DeliveryResult {
        duplicated: false,
        message: format!("回执 {} 已确认,退款 {} 完成", receipt.receipt_no, receipt.refund_no),
    })
}
