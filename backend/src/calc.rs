//! 价保差价计算核心逻辑(纯函数,便于单元测试)。
//!
//! 规则:
//! - 差价 = 原订单实付 - 当前有效价格(当前吊牌价 - 当前活动优惠)
//! - 活动券已在原订单中抵扣,计算当前有效价格时不得重复抵扣
//! - 多次降价:累计可退 = max(0, 实付 - 当前有效价);本次应退 = 累计可退 - 此前已退
//! - 累计退款不得超过原订单明细实付金额

/// 活动规则(版本化,发布后不可变)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// 折扣:percent=90 表示九折
    PercentOff { percent: i64 },
    /// 满减:满 threshold 减 reduction(单位:分)
    FullReduction { threshold: i64, reduction: i64 },
}

impl Rule {
    /// 计算该规则下,金额为 total_cents 时的活动优惠
    pub fn discount(&self, total_cents: i64) -> i64 {
        match *self {
            Rule::PercentOff { percent } => {
                let pct = percent.clamp(0, 100);
                total_cents * (100 - pct) / 100
            }
            Rule::FullReduction {
                threshold,
                reduction,
            } => {
                if total_cents >= threshold {
                    reduction.min(total_cents)
                } else {
                    0
                }
            }
        }
    }
}

/// 一次价保计算的逐项依据(金额单位:分)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breakdown {
    pub qty: i64,
    // 原订单
    pub orig_unit_price: i64,
    pub orig_activity_disc: i64,
    pub orig_coupon_disc: i64,
    pub orig_paid: i64,
    // 当前
    pub cur_unit_price: i64,
    pub cur_activity_disc: i64,
    pub cur_effective: i64, // 当前有效价(整行)= 当前吊牌*qty - 当前活动优惠,不含券
    // 结论
    pub cumulative_refundable: i64, // 累计可退 = max(0, 实付 - 当前有效价)
    pub already_refunded: i64,      // 此前已退差价
    pub this_refund: i64,           // 本次应退 = 累计可退 - 已退
}

/// 计算价保差价。
///
/// * `orig_*` — 原订单明细快照(吊牌价、活动优惠、券分摊、实付)
/// * `cur_unit_price` — mock 价格接口返回的当前吊牌价
/// * `rule` — 当前生效的活动规则版本
/// * `already_refunded` — 该明细此前已成功退款的差价合计
pub fn compute(
    qty: i64,
    orig_unit_price: i64,
    orig_activity_disc: i64,
    orig_coupon_disc: i64,
    orig_paid: i64,
    cur_unit_price: i64,
    rule: Rule,
    already_refunded: i64,
) -> Breakdown {
    let cur_total = cur_unit_price * qty;
    let cur_activity_disc = rule.discount(cur_total);
    // 当前有效价不含券:券已在原订单抵扣,不得重复抵扣
    let cur_effective = (cur_total - cur_activity_disc).max(0);

    // 累计可退差价,封顶为原实付(累计退款不得超过可退金额)
    let cumulative_refundable = (orig_paid - cur_effective).clamp(0, orig_paid);
    // 只退新增部分;已退超过累计可退时本次为 0
    let this_refund = (cumulative_refundable - already_refunded).max(0);

    Breakdown {
        qty,
        orig_unit_price,
        orig_activity_disc,
        orig_coupon_disc,
        orig_paid,
        cur_unit_price,
        cur_activity_disc,
        cur_effective,
        cumulative_refundable,
        already_refunded,
        this_refund,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PCT90: Rule = Rule::PercentOff { percent: 90 };
    const PCT80: Rule = Rule::PercentOff { percent: 80 };

    /// 场景:吊牌 2999,九折,券 50 → 实付 2649.10
    fn orig() -> (i64, i64, i64, i64, i64) {
        // qty, unit, activity_disc, coupon_disc, paid
        (1, 299_900, 29_990, 5_000, 264_910)
    }

    #[test]
    fn first_price_drop_refunds_full_diff() {
        let (qty, unit, ad, cd, paid) = orig();
        // 降到 2799,仍九折 → 当前有效 2519.10
        let b = compute(qty, unit, ad, cd, paid, 279_900, PCT90, 0);
        assert_eq!(b.cur_activity_disc, 27_990);
        assert_eq!(b.cur_effective, 251_910);
        assert_eq!(b.cumulative_refundable, 13_000);
        assert_eq!(b.this_refund, 13_000);
    }

    #[test]
    fn second_drop_only_refunds_increment() {
        let (qty, unit, ad, cd, paid) = orig();
        // 再降到 2599,九折 → 有效 2339.10;累计可退 310,已退 130 → 本次 180
        let b = compute(qty, unit, ad, cd, paid, 259_900, PCT90, 13_000);
        assert_eq!(b.cumulative_refundable, 31_000);
        assert_eq!(b.this_refund, 18_000);
    }

    #[test]
    fn no_new_drop_no_refund() {
        let (qty, unit, ad, cd, paid) = orig();
        // 价格未变,且已退过 → 本次 0
        let b = compute(qty, unit, ad, cd, paid, 279_900, PCT90, 13_000);
        assert_eq!(b.this_refund, 0);
    }

    #[test]
    fn coupon_not_double_counted() {
        // 当前价计算不含券:若把 50 元券再抵一次,有效价会少 5000 分
        let (qty, unit, ad, cd, paid) = orig();
        let b = compute(qty, unit, ad, cd, paid, 299_900, PCT90, 0);
        // 价格未降:有效价 2699.10 > 实付 2649.10(因为券不重复抵)→ 无差价
        assert_eq!(b.cur_effective, 269_910);
        assert_eq!(b.this_refund, 0);
    }

    #[test]
    fn rule_version_change_affects_result() {
        let (qty, unit, ad, cd, paid) = orig();
        // 同价 2999,规则从九折变为八折 → 有效 2399.20 → 可退 249.90
        let b = compute(qty, unit, ad, cd, paid, 299_900, PCT80, 0);
        assert_eq!(b.cur_activity_disc, 59_980);
        assert_eq!(b.this_refund, 24_990);
    }

    #[test]
    fn full_reduction_rule() {
        let rule = Rule::FullReduction {
            threshold: 100_000,
            reduction: 15_000,
        };
        assert_eq!(rule.discount(99_999), 0);
        assert_eq!(rule.discount(100_000), 15_000);
        assert_eq!(rule.discount(299_900), 15_000);
    }

    #[test]
    fn cumulative_refund_capped_at_paid() {
        // 价格跌到 0:累计可退不得超过实付
        let (qty, unit, ad, cd, paid) = orig();
        let b = compute(qty, unit, ad, cd, paid, 0, PCT90, 0);
        assert_eq!(b.cumulative_refundable, paid);
        assert_eq!(b.this_refund, paid);
        // 已退过一部分时,本次只退剩余额度
        let b = compute(qty, unit, ad, cd, paid, 0, PCT90, 200_000);
        assert_eq!(b.this_refund, paid - 200_000);
    }

    #[test]
    fn qty_multiplies() {
        // 2 件:吊牌 500*2=1000,九折 → 有效 900;实付 850(券 50)→ 无差价
        let b = compute(2, 50_000, 10_000, 5_000, 85_000, 50_000, PCT90, 0);
        assert_eq!(b.cur_effective, 90_000);
        assert_eq!(b.this_refund, 0);
        // 降到 450:有效 810 → 可退 40
        let b = compute(2, 50_000, 10_000, 5_000, 85_000, 45_000, PCT90, 0);
        assert_eq!(b.this_refund, 4_000);
    }
}
