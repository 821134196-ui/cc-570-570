<script>
  import { fen } from '../api.js'
  export let b // Breakdown 对象
</script>

{#if b}
<div class="bd">
  <div class="bd-grid">
    <section>
      <h4>原订单（下单时快照）</h4>
      <table>
        <tbody>
        <tr><td>商品</td><td>{b.product_name} / {b.spec} × {b.qty}</td></tr>
        <tr><td>下单活动价</td><td>{fen(b.original_activity_price)} /件</td></tr>
        <tr><td>下单规则版本</td><td><span class="tag">{b.original_rule_version}</span> {b.original_rule_name}</td></tr>
        <tr><td>已用活动券</td><td class="neg">- {fen(b.coupon_amount)}</td></tr>
        <tr class="sum"><td>原实付</td><td>{fen(b.original_paid)}</td></tr>
        </tbody>
      </table>
    </section>
    <section>
      <h4>当前价格（申请时刻）</h4>
      <table>
        <tbody>
        <tr><td>当前活动价</td><td>{fen(b.current_activity_price)} /件 × {b.qty}</td></tr>
        <tr><td>当前规则版本</td><td><span class="tag">{b.current_rule_version}</span> {b.current_rule_name}</td></tr>
        <tr><td>规则优惠</td><td class="neg">- {fen(b.rule_discount)}</td></tr>
        <tr><td>活动券</td><td class="muted">不重复抵扣（原单已使用）</td></tr>
        <tr class="sum"><td>当前应付</td><td>{fen(b.current_payable)}</td></tr>
        </tbody>
      </table>
    </section>
    <section>
      <h4>差价计算</h4>
      <table>
        <tbody>
        <tr><td>总差价</td><td>{fen(b.original_paid)} − {fen(b.current_payable)} = <b>{fen(b.total_diff)}</b></td></tr>
        <tr><td>此前已退</td><td class="neg">- {fen(b.already_refunded)}</td></tr>
        <tr><td>可退上限</td><td class="muted">{fen(b.refundable_cap)}（原实付 − 已退）</td></tr>
        <tr class="sum"><td>本次应退</td><td class="hl">{fen(b.this_refund)}</td></tr>
        </tbody>
      </table>
    </section>
  </div>

  <h4>资格检查</h4>
  <ul class="checks">
    {#each b.checks as c}
      <li class:ok={c.passed} class:bad={!c.passed}>
        <span class="mark">{c.passed ? '✓' : '✗'}</span>
        <b>{c.name}</b> — {c.detail}
      </li>
    {/each}
  </ul>
  {#if !b.eligible}
    <div class="reject-box">拒绝原因：{b.reject_reason}</div>
  {/if}
</div>
{/if}

<style>
  .bd { border: 1px solid #e2e6ee; border-radius: 10px; padding: 12px 14px; background: #fbfcfe; }
  .bd-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); gap: 12px; }
  h4 { margin: 4px 0 8px; font-size: 13px; color: #445; }
  table { width: 100%; border-collapse: collapse; font-size: 13px; }
  td { padding: 3px 4px; border-bottom: 1px dashed #e8ebf2; }
  td:first-child { color: #778; white-space: nowrap; }
  .sum td { font-weight: 700; border-bottom: none; }
  .neg { color: #c0392b; }
  .hl { color: #0a7d3b; font-size: 15px; }
  .muted { color: #99a; font-size: 12px; }
  .tag { background: #eef2ff; color: #3b5bdb; border-radius: 4px; padding: 1px 6px; font-size: 12px; }
  .checks { list-style: none; padding: 0; margin: 6px 0; font-size: 13px; }
  .checks li { padding: 3px 0; }
  .checks .mark { display: inline-block; width: 18px; font-weight: 700; }
  .checks .ok .mark { color: #0a7d3b; }
  .checks .bad .mark { color: #c0392b; }
  .reject-box { margin-top: 8px; background: #fdecea; color: #a5281b; border: 1px solid #f5c6c0; border-radius: 8px; padding: 8px 10px; font-size: 13px; }
</style>
