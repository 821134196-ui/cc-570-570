<script>
  import { fmt } from '../api.js'
  export let b
  export let ruleName = ''
</script>

<div class="bd">
  <h4>计算依据(逐项)</h4>
  <table>
    <tbody>
      <tr><td>购买数量</td><td class="num">{b.qty} 件</td></tr>
      <tr><td>原订单吊牌价</td><td class="num">{fmt(b.orig_unit_price_cents)} / 件</td></tr>
      <tr><td>原订单活动优惠</td><td class="num">− {fmt(b.orig_activity_disc_cents)}</td></tr>
      <tr><td>原订单活动券抵扣</td><td class="num">− {fmt(b.orig_coupon_disc_cents)}</td></tr>
      <tr class="sub"><td>原订单实付金额</td><td class="num">{fmt(b.orig_paid_cents)}</td></tr>
      <tr><td>当前吊牌价</td><td class="num">{fmt(b.cur_unit_price_cents)} / 件</td></tr>
      <tr>
        <td>当前活动优惠(规则 v{b.rule_version}{ruleName ? ` ${ruleName}` : ''})</td>
        <td class="num">− {fmt(b.cur_activity_disc_cents)}</td>
      </tr>
      <tr><td class="muted">活动券说明</td><td class="num muted">券已在原订单抵扣,不重复抵扣</td></tr>
      <tr class="sub"><td>当前有效价格</td><td class="num">{fmt(b.cur_effective_cents)}</td></tr>
      <tr class="sub"><td>累计可退差价 = 实付 − 当前有效价</td><td class="num">{fmt(b.cumulative_refundable_cents)}</td></tr>
      <tr><td>此前已退差价</td><td class="num">− {fmt(b.already_refunded_cents)}</td></tr>
      <tr class="total"><td>本次应退(新增部分)</td><td class="num">{fmt(b.this_refund_cents)}</td></tr>
    </tbody>
  </table>
</div>

<style>
  .bd { border: 1px solid #e5e9ef; border-radius: 8px; padding: 12px 16px; margin: 10px 0; }
  h4 { margin: 0 0 8px; font-size: 14px; }
  td { padding: 5px 8px; border-bottom: 1px dashed #eef1f4; font-size: 13px; }
  .num { text-align: right; font-variant-numeric: tabular-nums; }
  .sub td { font-weight: 600; background: #f7f9fc; }
  .total td { font-weight: 700; color: #d83931; background: #fff5f4; font-size: 14px; }
</style>
