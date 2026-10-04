use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};

pub type Pool = SqlitePool;

pub async fn init(db_url: &str) -> Pool {
    // SQLite 连接串形如 sqlite://data/app.db；ensure 目录存在
    if let Some(path) = db_url.strip_prefix("sqlite://") {
        let path = path.split('?').next().unwrap_or(path);
        if let Some(dir) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(dir).ok();
        }
        // sqlx 需要文件存在或使用 ?mode=rwc
        if !std::path::Path::new(path).exists() {
            std::fs::File::create(path).expect("create sqlite db file");
        }
    }
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(db_url)
        .await
        .expect("connect sqlite");
    sqlx::query("PRAGMA journal_mode=WAL;").execute(&pool).await.ok();
    sqlx::query("PRAGMA foreign_keys=ON;").execute(&pool).await.ok();
    create_schema(&pool).await;
    pool
}

async fn create_schema(pool: &Pool) {
    // 金额一律用整数“分”存储，避免浮点误差
    let schema = r#"
    -- 商品与规格（SKU）
    CREATE TABLE IF NOT EXISTS products (
        id   INTEGER PRIMARY KEY,
        name TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS skus (
        id         INTEGER PRIMARY KEY,
        product_id INTEGER NOT NULL REFERENCES products(id),
        spec       TEXT NOT NULL
    );

    -- 活动规则版本：每次活动规则变更产生新版本，历史版本永久保留
    CREATE TABLE IF NOT EXISTS rule_versions (
        id         INTEGER PRIMARY KEY,
        version    TEXT NOT NULL UNIQUE,   -- 如 v1 / v2 / v3
        name       TEXT NOT NULL,
        config     TEXT NOT NULL,          -- JSON: {"type":"none"} | {"type":"percent_off","rate":90} | {"type":"threshold","threshold":30000,"off":3000}
        created_at TEXT NOT NULL
    );

    -- 价格事件：每次降价产生一条记录，并记录当时生效的规则版本
    CREATE TABLE IF NOT EXISTS price_events (
        id              INTEGER PRIMARY KEY,
        sku_id          INTEGER NOT NULL REFERENCES skus(id),
        activity_price  INTEGER NOT NULL,  -- 活动单价（分）
        rule_version_id INTEGER NOT NULL REFERENCES rule_versions(id),
        effective_at    TEXT NOT NULL
    );

    -- 订单（实付金额、券、规则版本快照在下单时固化，永不被价保流程修改）
    CREATE TABLE IF NOT EXISTS orders (
        id              INTEGER PRIMARY KEY,
        order_no        TEXT NOT NULL UNIQUE,
        customer        TEXT NOT NULL,
        status          TEXT NOT NULL,      -- paid / returned
        created_at      TEXT NOT NULL,
        protected_until TEXT NOT NULL       -- 价保截止（下单后 7 天）
    );
    CREATE TABLE IF NOT EXISTS order_items (
        id               INTEGER PRIMARY KEY,
        order_id         INTEGER NOT NULL REFERENCES orders(id),
        sku_id           INTEGER NOT NULL REFERENCES skus(id),
        qty              INTEGER NOT NULL,
        activity_price   INTEGER NOT NULL,  -- 下单时活动单价（分）
        coupon_amount    INTEGER NOT NULL,  -- 本明细已使用的活动券（分），价保时不得重复抵扣
        paid_amount      INTEGER NOT NULL,  -- 实付（分）= 下单时规则计算结果 - 券
        rule_version_id  INTEGER NOT NULL REFERENCES rule_versions(id),
        returned         INTEGER NOT NULL DEFAULT 0
    );

    -- 价保申请：calc_snapshot 固化申请时刻的完整计算依据
    CREATE TABLE IF NOT EXISTS applications (
        id              INTEGER PRIMARY KEY,
        app_no          TEXT NOT NULL UNIQUE,
        order_item_id   INTEGER NOT NULL REFERENCES order_items(id),
        idempotency_key TEXT NOT NULL UNIQUE,   -- 客户端幂等键：重复提交不产生新申请
        status          TEXT NOT NULL,          -- pending / refunded / rejected
        calc_snapshot   TEXT NOT NULL,          -- JSON 计算依据快照
        refund_amount   INTEGER NOT NULL DEFAULT 0,
        reject_reason   TEXT,
        created_at      TEXT NOT NULL,
        reviewed_at     TEXT
    );

    -- 退款记录：一个申请最多一条退款（application_id 唯一），支付幂等键唯一
    CREATE TABLE IF NOT EXISTS refunds (
        id              INTEGER PRIMARY KEY,
        refund_no       TEXT NOT NULL UNIQUE,
        application_id  INTEGER NOT NULL UNIQUE REFERENCES applications(id),
        order_item_id   INTEGER NOT NULL,
        amount          INTEGER NOT NULL,
        idempotency_key TEXT NOT NULL UNIQUE,
        status          TEXT NOT NULL,          -- initiated / succeeded
        receipt_no      TEXT UNIQUE,
        created_at      TEXT NOT NULL,
        succeeded_at    TEXT
    );

    -- 支付回执：receipt_no 唯一约束保证重复回执只入账一次
    CREATE TABLE IF NOT EXISTS payment_receipts (
        id          INTEGER PRIMARY KEY,
        receipt_no  TEXT NOT NULL UNIQUE,
        refund_id   INTEGER NOT NULL,
        amount      INTEGER NOT NULL,
        payload     TEXT NOT NULL,
        received_at TEXT NOT NULL
    );

    -- 模拟支付网关台账（相当于“第三方支付”自己的账本）
    CREATE TABLE IF NOT EXISTS mock_pay_refunds (
        id              INTEGER PRIMARY KEY,
        idempotency_key TEXT NOT NULL UNIQUE,  -- 网关级幂等：同键重复调用返回同一回执
        receipt_no      TEXT NOT NULL UNIQUE,
        amount          INTEGER NOT NULL,
        created_at      TEXT NOT NULL
    );
    "#;
    for stmt in schema.split(';') {
        let s = stmt.trim();
        if !s.is_empty() {
            sqlx::query(s).execute(pool).await.expect("create schema");
        }
    }
}
