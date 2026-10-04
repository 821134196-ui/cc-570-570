//! 演示数据:仅在空库时写入;提供 reset 供演示控制台重置。
use anyhow::Result;
use chrono::{Duration, Utc};
use sqlx::SqlitePool;

pub const PROTECT_DAYS: i64 = 7;

pub async fn seed_if_empty(pool: &SqlitePool) -> Result<()> {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM orders")
        .fetch_one(pool)
        .await?;
    if n == 0 {
        seed(pool).await?;
        tracing::info!("已写入演示数据");
    }
    Ok(())
}

pub async fn reset(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin().await?;
    for t in [
        "refund_receipts",
        "refunds",
        "pp_applications",
        "order_items",
        "orders",
        "coupons",
        "activity_rules",
        "price_history",
        "skus",
        "products",
    ] {
        sqlx::query(&format!("DELETE FROM {t}"))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    seed(pool).await
}

async fn seed(pool: &SqlitePool) -> Result<()> {
    let now = Utc::now();
    let fmt = |t: chrono::DateTime<Utc>| t.to_rfc3339();
    let mut tx = pool.begin().await?;

    // 商品与 SKU(同商品不同规格,用于"规格不符"拒绝演示)
    sqlx::query("INSERT INTO products(id, name) VALUES (1,'Acme 智能手机 X1'),(2,'Acme 降噪耳机 H2')")
        .execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO skus(id, product_id, spec) VALUES
         (1,1,'黑色 / 128G'),(2,1,'白色 / 256G'),(3,2,'曜石黑')",
    )
    .execute(&mut *tx).await?;

    // 价格流水(当前价 = 最新一条)
    let t0 = now - Duration::days(2);
    sqlx::query(
        "INSERT INTO price_history(sku_id, price_cents, effective_at, created_at) VALUES
         (1, 299900, ?1, ?1),
         (2, 329900, ?1, ?1),
         (3,  79900, ?1, ?1)",
    )
    .bind(fmt(t0))
    .execute(&mut *tx).await?;

    // 活动规则版本:初始仅 v1 九折;后续版本通过演示控制台发布
    sqlx::query(
        "INSERT INTO activity_rules(version, name, rule_type, percent_off, full_threshold_cents, reduction_cents, effective_at, created_at) VALUES
         (1, '首发九折', 'percent_off', 90, NULL, NULL, ?1, ?1)",
    )
    .bind(fmt(t0))
    .execute(&mut *tx).await?;

    // 活动券
    sqlx::query("INSERT INTO coupons(id, code, title, discount_cents) VALUES (1,'NEW50','新人券 ¥50',5000)")
        .execute(&mut *tx).await?;

    // 订单 1:张三,手机 黑色/128G ×1,九折 + 新人券 → 可正常申请价保
    // 吊牌 2999.00,活动优惠 299.90,券 50.00,实付 2649.10
    let o1_created = now - Duration::days(1);
    sqlx::query(
        "INSERT INTO orders(id, order_no, customer_name, status, rule_version, coupon_id, coupon_discount_cents,
            total_amount_cents, activity_discount_cents, paid_cents, protect_until, created_at)
         VALUES (1,'SO20261003001','张三','paid',1,1,5000, 299900,29990,264910, ?1, ?2)",
    )
    .bind(fmt(o1_created + Duration::days(PROTECT_DAYS)))
    .bind(fmt(o1_created))
    .execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO order_items(id, order_id, sku_id, product_name, sku_spec, qty, unit_price_cents,
            activity_discount_cents, coupon_discount_cents, paid_cents)
         VALUES (1,1,1,'Acme 智能手机 X1','黑色 / 128G',1,299900,29990,5000,264910)",
    )
    .execute(&mut *tx).await?;

    // 订单 2:张三,耳机 ×1,九折无券 → 已退货(拒绝演示)
    let o2_created = now - Duration::days(1);
    sqlx::query(
        "INSERT INTO orders(id, order_no, customer_name, status, rule_version, coupon_id, coupon_discount_cents,
            total_amount_cents, activity_discount_cents, paid_cents, protect_until, created_at)
         VALUES (2,'SO20261003002','张三','returned',1,NULL,0, 79900,7990,71910, ?1, ?2)",
    )
    .bind(fmt(o2_created + Duration::days(PROTECT_DAYS)))
    .bind(fmt(o2_created))
    .execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO order_items(id, order_id, sku_id, product_name, sku_spec, qty, unit_price_cents,
            activity_discount_cents, coupon_discount_cents, paid_cents)
         VALUES (2,2,3,'Acme 降噪耳机 H2','曜石黑',1,79900,7990,0,71910)",
    )
    .execute(&mut *tx).await?;

    // 订单 3:李四,手机 白色/256G ×1,九折 → 10 天前下单,价保期已过(拒绝演示)
    let o3_created = now - Duration::days(10);
    sqlx::query(
        "INSERT INTO orders(id, order_no, customer_name, status, rule_version, coupon_id, coupon_discount_cents,
            total_amount_cents, activity_discount_cents, paid_cents, protect_until, created_at)
         VALUES (3,'SO20260924001','李四','paid',1,NULL,0, 329900,32990,296910, ?1, ?2)",
    )
    .bind(fmt(o3_created + Duration::days(PROTECT_DAYS)))
    .bind(fmt(o3_created))
    .execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO order_items(id, order_id, sku_id, product_name, sku_spec, qty, unit_price_cents,
            activity_discount_cents, coupon_discount_cents, paid_cents)
         VALUES (3,3,2,'Acme 智能手机 X1','白色 / 256G',1,329900,32990,0,296910)",
    )
    .execute(&mut *tx).await?;

    // 订单 4:张三,耳机 ×2,九折 → 多件数量场景
    let o4_created = now - Duration::days(1);
    sqlx::query(
        "INSERT INTO orders(id, order_no, customer_name, status, rule_version, coupon_id, coupon_discount_cents,
            total_amount_cents, activity_discount_cents, paid_cents, protect_until, created_at)
         VALUES (4,'SO20261003004','张三','paid',1,NULL,0, 159800,15980,143820, ?1, ?2)",
    )
    .bind(fmt(o4_created + Duration::days(PROTECT_DAYS)))
    .bind(fmt(o4_created))
    .execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO order_items(id, order_id, sku_id, product_name, sku_spec, qty, unit_price_cents,
            activity_discount_cents, coupon_discount_cents, paid_cents)
         VALUES (4,4,3,'Acme 降噪耳机 H2','曜石黑',2,79900,15980,0,143820)",
    )
    .execute(&mut *tx).await?;

    tx.commit().await?;
    Ok(())
}
