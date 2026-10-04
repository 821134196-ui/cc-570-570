use serde::{Deserialize, Serialize};

/// 活动规则配置（按版本固化在 rule_versions.config 中）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleConfig {
    /// 无额外优惠，活动价即应付
    None,
    /// 折扣：rate=90 表示 9 折
    PercentOff { rate: i64 },
    /// 满减：满 threshold 减 off（单位：分）
    Threshold { threshold: i64, off: i64 },
}

/// 价保计算依据（逐项展示用），同时作为申请快照固化
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakdown {
    // ---- 订单原始信息（下单时快照，不可变） ----
    pub order_no: String,
    pub order_item_id: i64,
    pub product_name: String,
    pub spec: String,
    pub qty: i64,
    pub original_activity_price: i64, // 下单时活动单价
    pub coupon_amount: i64,           // 已使用活动券（价保时不得重复抵扣）
    pub original_paid: i64,           // 原实付
    pub original_rule_version: String,
    pub original_rule_name: String,
    // ---- 当前价格信息 ----
    pub current_activity_price: i64, // 当前活动单价
    pub current_rule_version: String,
    pub current_rule_name: String,
    pub rule_discount: i64,   // 当前规则优惠额（满减/折扣省的钱）
    pub current_payable: i64, // 当前应付 = 当前活动价×数量 - 当前规则优惠（券不重复抵扣）
    // ---- 差价计算 ----
    pub total_diff: i64,       // 总差价 = 原实付 - 当前应付
    pub already_refunded: i64, // 此前已退差价
    pub refundable_cap: i64,   // 可退上限 = 原实付 - 已退（累计退款不得超出）
    pub this_refund: i64,      // 本次应退 = clamp(总差价 - 已退, 0, 可退上限)
    // ---- 资格检查（逐项） ----
    pub eligible: bool,
    pub reject_reason: Option<String>,
    pub checks: Vec<CheckItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckItem {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RuleVersion {
    pub id: i64,
    pub version: String,
    pub name: String,
    pub config: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Sku {
    pub id: i64,
    pub product_id: i64,
    pub spec: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PriceEvent {
    pub id: i64,
    pub sku_id: i64,
    pub activity_price: i64,
    pub rule_version_id: i64,
    pub effective_at: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Order {
    pub id: i64,
    pub order_no: String,
    pub customer: String,
    pub status: String,
    pub created_at: String,
    pub protected_until: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct OrderItem {
    pub id: i64,
    pub order_id: i64,
    pub sku_id: i64,
    pub qty: i64,
    pub activity_price: i64,
    pub coupon_amount: i64,
    pub paid_amount: i64,
    pub rule_version_id: i64,
    pub returned: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Application {
    pub id: i64,
    pub app_no: String,
    pub order_item_id: i64,
    pub idempotency_key: String,
    pub status: String,
    pub calc_snapshot: String,
    pub refund_amount: i64,
    pub reject_reason: Option<String>,
    pub created_at: String,
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Refund {
    pub id: i64,
    pub refund_no: String,
    pub application_id: i64,
    pub order_item_id: i64,
    pub amount: i64,
    pub idempotency_key: String,
    pub status: String,
    pub receipt_no: Option<String>,
    pub created_at: String,
    pub succeeded_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PaymentReceipt {
    pub id: i64,
    pub receipt_no: String,
    pub refund_id: i64,
    pub amount: i64,
    pub payload: String,
    pub received_at: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MockPayRefund {
    pub id: i64,
    pub idempotency_key: String,
    pub receipt_no: String,
    pub amount: i64,
    pub created_at: String,
}
