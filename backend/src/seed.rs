use crate::db::Pool;
use chrono::{Duration, Utc};

/// 幂等种子：仅当 products 为空时写入演示数据
pub async fn seed_if_empty(pool: &Pool) {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    if count > 0 {
        return;
    }
    reset(pool).await;
}

/// 清空并重置全部演示数据（演示控制台“重置”按钮用）
pub async fn reset(pool: &Pool) {
    let tables = [
        "payment_receipts",
        "refunds",
        "applications",
        "order_items",
        "orders",
        "price_events",
        "rule_versions",
        "skus",
        "products",
        "mock_pay_refunds",
    ];
    for t in tables {
        sqlx::query(&format!("DELETE FROM {}", t))
            .execute(pool)
            .await
            .ok();
    }

    let now = Utc::now();
    let ts = |d: Duration| (now + d).to_rfc3339();

    // ---- 活动规则版本（历史版本永久保留） ----
    // v1 直降无叠加 / v2 满300减30 / v3 9折
    let rules = [
        ("v1", "直降活动（无叠加优惠）", r#"{"type":"none"}"#),
        ("v2", "满300减30", r#"{"type":"threshold","threshold":30000,"off":3000}"#),
        ("v3", "全场9折", r#"{"type":"percent_off","rate":90}"#),
    ];
    for (i, (v, name, cfg)) in rules.iter().enumerate() {
        sqlx::query("INSERT INTO rule_versions (id, version, name, config, created_at) VALUES (?,?,?,?,?)")
            .bind(i as i64 + 1)
            .bind(v)
            .bind(name)
            .bind(cfg)
            .bind(ts(Duration::days(-30)))
            .execute(pool)
            .await
            .unwrap();
    }

    // ---- 商品与规格 ----
    sqlx::query("INSERT INTO products (id, name) VALUES (1,'降噪耳机'),(2,'机械键盘')")
        .execute(pool)
        .await
        .unwrap();
    // 同一商品的不同规格：用于演示“规格不符”拒绝
    sqlx::query("INSERT INTO skus (id, product_id, spec) VALUES (1,1,'标准版/黑色'),(2,1,'Pro版/黑色'),(3,2,'87键/白色')")
        .execute(pool)
        .await
        .unwrap();

    // ---- 初始价格事件（均按 v1 规则） ----
    let prices = [(1i64, 50000i64), (2, 80000), (3, 30000)];
    for (sku, price) in prices {
        sqlx::query("INSERT INTO price_events (sku_id, activity_price, rule_version_id, effective_at) VALUES (?,?,1,?)")
            .bind(sku)
            .bind(price)
            .bind(ts(Duration::days(-2)))
            .execute(pool)
            .await
            .unwrap();
    }

    // ---- 订单（实付/券/规则版本为下单时快照） ----
    // O1：价保期内，用了 50 元活动券（演示“券不重复抵扣”）
    sqlx::query("INSERT INTO orders (id, order_no, customer, status, created_at, protected_until) VALUES (1,'O1001','张三','paid',?,?)")
        .bind(ts(Duration::days(-2))).bind(ts(Duration::days(5))).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO order_items (id, order_id, sku_id, qty, activity_price, coupon_amount, paid_amount, rule_version_id, returned) VALUES (1,1,1,1,50000,5000,45000,1,0)")
        .execute(pool).await.unwrap();

    // O2：已退货订单（演示拒绝）
    sqlx::query("INSERT INTO orders (id, order_no, customer, status, created_at, protected_until) VALUES (2,'O1002','张三','returned',?,?)")
        .bind(ts(Duration::days(-3))).bind(ts(Duration::days(4))).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO order_items (id, order_id, sku_id, qty, activity_price, coupon_amount, paid_amount, rule_version_id, returned) VALUES (2,2,3,1,30000,0,30000,1,1)")
        .execute(pool).await.unwrap();

    // O3：已超过价保期限（演示拒绝）
    sqlx::query("INSERT INTO orders (id, order_no, customer, status, created_at, protected_until) VALUES (3,'O1003','张三','paid',?,?)")
        .bind(ts(Duration::days(-30))).bind(ts(Duration::days(-23))).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO order_items (id, order_id, sku_id, qty, activity_price, coupon_amount, paid_amount, rule_version_id, returned) VALUES (3,3,2,1,80000,0,80000,1,0)")
        .execute(pool).await.unwrap();

    // O4：价保期内、无券（演示多次降价、增量退款）
    sqlx::query("INSERT INTO orders (id, order_no, customer, status, created_at, protected_until) VALUES (4,'O1004','张三','paid',?,?)")
        .bind(ts(Duration::days(-1))).bind(ts(Duration::days(6))).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO order_items (id, order_id, sku_id, qty, activity_price, coupon_amount, paid_amount, rule_version_id, returned) VALUES (4,4,1,1,50000,0,50000,1,0)")
        .execute(pool).await.unwrap();

    // O5：多件 + 券（演示数量与券共同作用）
    sqlx::query("INSERT INTO orders (id, order_no, customer, status, created_at, protected_until) VALUES (5,'O1005','张三','paid',?,?)")
        .bind(ts(Duration::days(-1))).bind(ts(Duration::days(6))).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO order_items (id, order_id, sku_id, qty, activity_price, coupon_amount, paid_amount, rule_version_id, returned) VALUES (5,5,3,2,30000,2000,58000,1,0)")
        .execute(pool).await.unwrap();
}
