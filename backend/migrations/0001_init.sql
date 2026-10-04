-- 价保系统数据库 schema
-- 金额一律使用整数分(cents),避免浮点误差。

CREATE TABLE IF NOT EXISTS products (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS skus (
    id         INTEGER PRIMARY KEY,
    product_id INTEGER NOT NULL REFERENCES products(id),
    spec       TEXT NOT NULL,
    UNIQUE (product_id, spec)
);

-- 价格流水:mock 价格接口的数据源,只追加不修改
CREATE TABLE IF NOT EXISTS price_history (
    id          INTEGER PRIMARY KEY,
    sku_id      INTEGER NOT NULL REFERENCES skus(id),
    price_cents INTEGER NOT NULL CHECK (price_cents >= 0),
    effective_at TEXT NOT NULL,
    created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_price_history_sku ON price_history(sku_id, effective_at);

-- 活动规则版本:发布后不可变,新规则以新版本号追加
CREATE TABLE IF NOT EXISTS activity_rules (
    id                   INTEGER PRIMARY KEY,
    version              INTEGER NOT NULL UNIQUE,
    name                 TEXT NOT NULL,
    rule_type            TEXT NOT NULL CHECK (rule_type IN ('percent_off', 'full_reduction')),
    percent_off          INTEGER,               -- 例如 90 表示九折
    full_threshold_cents INTEGER,               -- 满 X 分
    reduction_cents      INTEGER,               -- 减 Y 分
    effective_at         TEXT NOT NULL,
    created_at           TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS coupons (
    id              INTEGER PRIMARY KEY,
    code            TEXT NOT NULL UNIQUE,
    title           TEXT NOT NULL,
    discount_cents  INTEGER NOT NULL CHECK (discount_cents > 0)
);

-- 订单:创建后金额字段不再变更(原订单不可被覆盖)
CREATE TABLE IF NOT EXISTS orders (
    id                       INTEGER PRIMARY KEY,
    order_no                 TEXT NOT NULL UNIQUE,
    customer_name            TEXT NOT NULL,
    status                   TEXT NOT NULL CHECK (status IN ('paid', 'returned')),
    rule_version             INTEGER NOT NULL,          -- 下单时使用的活动规则版本
    coupon_id                INTEGER REFERENCES coupons(id),
    coupon_discount_cents    INTEGER NOT NULL DEFAULT 0,
    total_amount_cents       INTEGER NOT NULL,          -- 商品吊牌价合计
    activity_discount_cents  INTEGER NOT NULL,          -- 活动优惠合计
    paid_cents               INTEGER NOT NULL,          -- 实付金额(可退上限)
    protect_until            TEXT NOT NULL,             -- 价保截止时间
    created_at               TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS order_items (
    id                        INTEGER PRIMARY KEY,
    order_id                  INTEGER NOT NULL REFERENCES orders(id),
    sku_id                    INTEGER NOT NULL REFERENCES skus(id),
    product_name              TEXT NOT NULL,            -- 快照
    sku_spec                  TEXT NOT NULL,            -- 快照
    qty                       INTEGER NOT NULL CHECK (qty > 0),
    unit_price_cents          INTEGER NOT NULL,         -- 下单时吊牌价
    activity_discount_cents   INTEGER NOT NULL,         -- 该明细的活动优惠(整行)
    coupon_discount_cents     INTEGER NOT NULL,         -- 分摊到该明细的券优惠(整行)
    paid_cents                INTEGER NOT NULL          -- 该明细实付 = 吊牌*qty - 活动 - 券分摊
);
CREATE INDEX IF NOT EXISTS ix_order_items_order ON order_items(order_id);

-- 价保申请:计算依据逐项快照,审核后也不可改
CREATE TABLE IF NOT EXISTS pp_applications (
    id                            INTEGER PRIMARY KEY,
    apply_no                      TEXT NOT NULL UNIQUE,
    order_id                      INTEGER NOT NULL REFERENCES orders(id),
    order_item_id                 INTEGER NOT NULL REFERENCES order_items(id),
    sku_id                        INTEGER NOT NULL REFERENCES skus(id),
    status                        TEXT NOT NULL CHECK (status IN ('pending','approved','rejected','refunded')),
    -- 计算依据快照(申请时刻)
    qty                           INTEGER NOT NULL,
    orig_unit_price_cents         INTEGER NOT NULL,
    orig_activity_disc_cents      INTEGER NOT NULL,
    orig_coupon_disc_cents        INTEGER NOT NULL,
    orig_paid_cents               INTEGER NOT NULL,
    cur_unit_price_cents          INTEGER NOT NULL,
    cur_activity_disc_cents       INTEGER NOT NULL,
    cur_effective_cents           INTEGER NOT NULL,
    rule_version                  INTEGER NOT NULL,     -- 本次计算使用的活动规则版本
    cumulative_refundable_cents   INTEGER NOT NULL,     -- 累计可退差价
    already_refunded_cents        INTEGER NOT NULL,     -- 此前已退差价
    this_refund_cents             INTEGER NOT NULL,     -- 本次应退(新增部分)
    reject_reason                 TEXT,
    review_note                   TEXT,
    reviewed_by                   TEXT,
    created_at                    TEXT NOT NULL,
    reviewed_at                   TEXT
);
-- 同一订单明细同一时间只允许一个进行中的申请(防重复申请)
CREATE UNIQUE INDEX IF NOT EXISTS ux_pp_open
    ON pp_applications(order_item_id) WHERE status IN ('pending','approved');
CREATE INDEX IF NOT EXISTS ix_pp_status ON pp_applications(status);

-- 退款单:一个申请最多一张退款单(幂等键唯一)
CREATE TABLE IF NOT EXISTS refunds (
    id              INTEGER PRIMARY KEY,
    refund_no       TEXT NOT NULL UNIQUE,
    application_id  INTEGER NOT NULL UNIQUE REFERENCES pp_applications(id),
    order_id        INTEGER NOT NULL REFERENCES orders(id),
    order_item_id   INTEGER NOT NULL REFERENCES order_items(id),
    amount_cents    INTEGER NOT NULL CHECK (amount_cents > 0),
    rule_version    INTEGER NOT NULL,          -- 计算所用规则版本快照
    status          TEXT NOT NULL CHECK (status IN ('processing','success','failed')),
    idempotency_key TEXT NOT NULL UNIQUE,      -- 幂等键:重复提交返回同一退款单
    receipt_no      TEXT UNIQUE,               -- 支付网关回执号
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_refunds_item ON refunds(order_item_id, status);

-- 回执投递记录:每次送达都记录;重复送达标记 duplicated=1 且不产生任何状态变更
CREATE TABLE IF NOT EXISTS refund_receipts (
    id          INTEGER PRIMARY KEY,
    receipt_no  TEXT NOT NULL,
    refund_no   TEXT NOT NULL,
    amount_cents INTEGER NOT NULL,
    duplicated  INTEGER NOT NULL DEFAULT 0,
    received_at TEXT NOT NULL
);
-- 同一回执号只允许一条"有效"记录
CREATE UNIQUE INDEX IF NOT EXISTS ux_receipt_first
    ON refund_receipts(receipt_no) WHERE duplicated = 0;
