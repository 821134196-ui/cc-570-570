use crate::models::*;
use sqlx::SqlitePool;

/// 应用活动规则，返回 (应付金额, 规则优惠额)
pub fn apply_rule(cfg: &RuleConfig, base: i64) -> (i64, i64) {
    match cfg {
        RuleConfig::None => (base, 0),
        RuleConfig::PercentOff { rate } => {
            let payable = base * rate / 100;
            (payable, base - payable)
        }
        RuleConfig::Threshold { threshold, off } => {
            if base >= *threshold {
                (base - off, *off)
            } else {
                (base, 0)
            }
        }
    }
}

pub fn parse_rule(config_json: &str) -> RuleConfig {
    serde_json::from_str(config_json).unwrap_or(RuleConfig::None)
}

/// 当前生效价格事件（最新一条）
pub async fn current_price_event(
    pool: &SqlitePool,
    sku_id: i64,
) -> Result<PriceEvent, sqlx::Error> {
    sqlx::query_as::<_, PriceEvent>(
        "SELECT * FROM price_events WHERE sku_id = ? ORDER BY effective_at DESC, id DESC LIMIT 1",
    )
    .bind(sku_id)
    .fetch_one(pool)
    .await
}

/// 该订单明细已成功退还的差价总额
pub async fn refunded_so_far(pool: &SqlitePool, order_item_id: i64) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(amount),0) FROM refunds WHERE order_item_id = ? AND status = 'succeeded'",
    )
    .bind(order_item_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0)
}

/// 计算价保依据。claimed_sku_id 用于演示“相同商品不同规格”场景：
/// 顾客可能拿着另一规格（如同款 Pro 版）的降价来申请，必须拒绝。
pub async fn compute_breakdown(
    pool: &SqlitePool,
    order_item_id: i64,
    claimed_sku_id: Option<i64>,
) -> Result<Breakdown, String> {
    let item = sqlx::query_as::<_, OrderItem>("SELECT * FROM order_items WHERE id = ?")
        .bind(order_item_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "订单明细不存在".to_string())?;

    let order = sqlx::query_as::<_, Order>("SELECT * FROM orders WHERE id = ?")
        .bind(item.order_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    let sku = sqlx::query_as::<_, Sku>("SELECT * FROM skus WHERE id = ?")
        .bind(item.sku_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;
    let product_name: String =
        sqlx::query_scalar("SELECT name FROM products WHERE id = ?")
            .bind(sku.product_id)
            .fetch_one(pool)
            .await
            .map_err(|e| e.to_string())?;

    let orig_rule = sqlx::query_as::<_, RuleVersion>("SELECT * FROM rule_versions WHERE id = ?")
        .bind(item.rule_version_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    let pe = current_price_event(pool, item.sku_id)
        .await
        .map_err(|_| "该商品暂无有效价格".to_string())?;
    let cur_rule = sqlx::query_as::<_, RuleVersion>("SELECT * FROM rule_versions WHERE id = ?")
        .bind(pe.rule_version_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    // ---- 核心计算 ----
    // 当前应付 = 当前活动价 × 数量，再按当前规则版本优惠；
    // 注意：原订单已使用的活动券【不】在此重复抵扣（顾客已享受过一次）。
    let base = pe.activity_price * item.qty;
    let (current_payable, rule_discount) = apply_rule(&parse_rule(&cur_rule.config), base);
    let total_diff = item.paid_amount - current_payable;
    let already = refunded_so_far(pool, order_item_id).await;
    let refundable_cap = item.paid_amount - already; // 累计退款不得超过原实付
    let this_refund = (total_diff - already).clamp(0, refundable_cap.max(0));

    // ---- 逐项资格检查 ----
    let now = chrono::Utc::now().to_rfc3339();
    let mut checks = Vec::new();

    let spec_ok = match claimed_sku_id {
        Some(c) if c != item.sku_id => false,
        _ => true,
    };
    checks.push(CheckItem {
        name: "商品规格一致".into(),
        passed: spec_ok,
        detail: if spec_ok {
            format!("订单规格：{} / {}", product_name, sku.spec)
        } else {
            "申请依据的降价商品与订单商品规格不同（相同商品不同规格不参与价保）".into()
        },
    });

    let not_returned = order.status != "returned" && item.returned == 0;
    checks.push(CheckItem {
        name: "订单未退货".into(),
        passed: not_returned,
        detail: if not_returned {
            "订单状态正常".into()
        } else {
            "该商品已退货，不参与价保".into()
        },
    });

    let in_window = now <= order.protected_until;
    checks.push(CheckItem {
        name: "在价保期限内".into(),
        passed: in_window,
        detail: if in_window {
            format!("价保截止 {}", order.protected_until)
        } else {
            format!("已超过价保期限（{}），不再受理", order.protected_until)
        },
    });

    let has_diff = total_diff > 0;
    checks.push(CheckItem {
        name: "存在差价".into(),
        passed: has_diff,
        detail: if has_diff {
            format!(
                "原实付 {} 分 - 当前应付 {} 分 = 差价 {} 分",
                item.paid_amount, current_payable, total_diff
            )
        } else {
            format!(
                "当前应付 {} 分不低于原实付 {} 分，无差价可退",
                current_payable, item.paid_amount
            )
        },
    });

    let has_new_diff = has_diff && this_refund > 0;
    checks.push(CheckItem {
        name: "存在新增差价".into(),
        passed: has_new_diff,
        detail: if has_new_diff {
            format!(
                "总差价 {} 分 - 已退 {} 分 = 本次可退 {} 分",
                total_diff, already, this_refund
            )
        } else if has_diff {
            format!("差价 {} 分已在此前全部退还，无新增差价", already)
        } else {
            "无差价".into()
        },
    });

    let reject_reason = checks
        .iter()
        .find(|c| !c.passed)
        .map(|c| c.detail.clone());

    Ok(Breakdown {
        order_no: order.order_no,
        order_item_id: item.id,
        product_name,
        spec: sku.spec,
        qty: item.qty,
        original_activity_price: item.activity_price,
        coupon_amount: item.coupon_amount,
        original_paid: item.paid_amount,
        original_rule_version: orig_rule.version,
        original_rule_name: orig_rule.name,
        current_activity_price: pe.activity_price,
        current_rule_version: cur_rule.version,
        current_rule_name: cur_rule.name,
        rule_discount,
        current_payable,
        total_diff,
        already_refunded: already,
        refundable_cap,
        this_refund,
        eligible: reject_reason.is_none(),
        reject_reason,
        checks,
    })
}
