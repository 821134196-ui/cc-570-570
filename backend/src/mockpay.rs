use crate::db::Pool;
use crate::models::MockPayRefund;

/// 模拟第三方支付网关：发起退款。
/// 幂等：同一 idempotency_key 重复调用，返回首次生成的同一张回执，不会重复出款。
pub async fn gateway_refund(
    pool: &Pool,
    idempotency_key: &str,
    amount: i64,
) -> Result<MockPayRefund, String> {
    // 先查：网关级幂等
    if let Some(existing) = sqlx::query_as::<_, MockPayRefund>(
        "SELECT * FROM mock_pay_refunds WHERE idempotency_key = ?",
    )
    .bind(idempotency_key)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    {
        return Ok(existing);
    }
    let receipt_no = format!("RC{}", &uuid::Uuid::new_v4().simple().to_string()[..12].to_uppercase());
    let now = chrono::Utc::now().to_rfc3339();
    let insert = sqlx::query(
        "INSERT INTO mock_pay_refunds (idempotency_key, receipt_no, amount, created_at) VALUES (?,?,?,?)",
    )
    .bind(idempotency_key)
    .bind(&receipt_no)
    .bind(amount)
    .bind(&now)
    .execute(pool)
    .await;
    match insert {
        Ok(_) => sqlx::query_as::<_, MockPayRefund>(
            "SELECT * FROM mock_pay_refunds WHERE idempotency_key = ?",
        )
        .bind(idempotency_key)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string()),
        // 并发冲突：另一个请求已创建，返回既有回执
        Err(_) => sqlx::query_as::<_, MockPayRefund>(
            "SELECT * FROM mock_pay_refunds WHERE idempotency_key = ?",
        )
        .bind(idempotency_key)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string()),
    }
}

/// 支付回执 webhook 的处理逻辑（HTTP 入口与演示重放共用）。
/// 幂等：receipt_no 唯一约束 + 条件更新，重复送达只入账一次、绝不多退。
pub async fn deliver_receipt(
    pool: &Pool,
    receipt_no: &str,
    refund_no: &str,
    amount: i64,
) -> Result<serde_json::Value, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let refund = sqlx::query_as::<_, crate::models::Refund>(
        "SELECT * FROM refunds WHERE refund_no = ?",
    )
    .bind(refund_no)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("退款单 {} 不存在", refund_no))?;

    if refund.amount != amount {
        return Err(format!(
            "回执金额 {} 与退款单金额 {} 不一致，拒绝入账",
            amount, refund.amount
        ));
    }

    // 回执落账：唯一约束去重
    let inserted = sqlx::query(
        "INSERT OR IGNORE INTO payment_receipts (receipt_no, refund_id, amount, payload, received_at) VALUES (?,?,?,?,?)",
    )
    .bind(receipt_no)
    .bind(refund.id)
    .bind(amount)
    .bind(serde_json::json!({"receipt_no": receipt_no, "refund_no": refund_no, "amount": amount}).to_string())
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?
    .rows_affected();

    if inserted == 0 {
        tx.rollback().await.ok();
        return Ok(serde_json::json!({
            "deduplicated": true,
            "message": format!("回执 {} 重复送达，已忽略，退款状态不变", receipt_no),
        }));
    }

    // 条件更新：只有 initiated 状态的退款才会被置为成功，天然防重
    let updated = sqlx::query(
        "UPDATE refunds SET status='succeeded', receipt_no=?, succeeded_at=? WHERE id=? AND status='initiated'",
    )
    .bind(receipt_no)
    .bind(&now)
    .bind(refund.id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?
    .rows_affected();

    if updated > 0 {
        sqlx::query("UPDATE applications SET status='refunded' WHERE id=? AND status='approved'")
            .bind(refund.application_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "deduplicated": false,
        "message": format!("回执 {} 已入账，退款 {} 成功", receipt_no, refund_no),
    }))
}
