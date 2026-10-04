use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{FromRow, SqlitePool};
use std::str::FromStr;

pub type Db = SqlitePool;

pub async fn connect(url: &str) -> Result<Db> {
    let opts = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(std::time::Duration::from_secs(5))
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct ActivityRule {
    pub id: i64,
    pub version: i64,
    pub name: String,
    pub rule_type: String,
    pub percent_off: Option<i64>,
    pub full_threshold_cents: Option<i64>,
    pub reduction_cents: Option<i64>,
    pub effective_at: String,
    pub created_at: String,
}

impl ActivityRule {
    pub fn to_calc_rule(&self) -> crate::calc::Rule {
        match self.rule_type.as_str() {
            "percent_off" => crate::calc::Rule::PercentOff {
                percent: self.percent_off.unwrap_or(100),
            },
            _ => crate::calc::Rule::FullReduction {
                threshold: self.full_threshold_cents.unwrap_or(0),
                reduction: self.reduction_cents.unwrap_or(0),
            },
        }
    }
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct Order {
    pub id: i64,
    pub order_no: String,
    pub customer_name: String,
    pub status: String,
    pub rule_version: i64,
    pub coupon_id: Option<i64>,
    pub coupon_discount_cents: i64,
    pub total_amount_cents: i64,
    pub activity_discount_cents: i64,
    pub paid_cents: i64,
    pub protect_until: String,
    pub created_at: String,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct OrderItem {
    pub id: i64,
    pub order_id: i64,
    pub sku_id: i64,
    pub product_name: String,
    pub sku_spec: String,
    pub qty: i64,
    pub unit_price_cents: i64,
    pub activity_discount_cents: i64,
    pub coupon_discount_cents: i64,
    pub paid_cents: i64,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct Application {
    pub id: i64,
    pub apply_no: String,
    pub order_id: i64,
    pub order_item_id: i64,
    pub sku_id: i64,
    pub status: String,
    pub qty: i64,
    pub orig_unit_price_cents: i64,
    pub orig_activity_disc_cents: i64,
    pub orig_coupon_disc_cents: i64,
    pub orig_paid_cents: i64,
    pub cur_unit_price_cents: i64,
    pub cur_activity_disc_cents: i64,
    pub cur_effective_cents: i64,
    pub rule_version: i64,
    pub cumulative_refundable_cents: i64,
    pub already_refunded_cents: i64,
    pub this_refund_cents: i64,
    pub reject_reason: Option<String>,
    pub review_note: Option<String>,
    pub reviewed_by: Option<String>,
    pub created_at: String,
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct Refund {
    pub id: i64,
    pub refund_no: String,
    pub application_id: i64,
    pub order_id: i64,
    pub order_item_id: i64,
    pub amount_cents: i64,
    pub rule_version: i64,
    pub status: String,
    pub idempotency_key: String,
    pub receipt_no: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct Receipt {
    pub id: i64,
    pub receipt_no: String,
    pub refund_no: String,
    pub amount_cents: i64,
    pub duplicated: i64,
    pub received_at: String,
}
