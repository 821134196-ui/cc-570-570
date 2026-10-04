<script>
  import { onMount } from 'svelte'
  import { api, fmt, fmtTime, STATUS_TEXT } from '../api.js'
  import BreakdownTable from './BreakdownTable.svelte'

  let applications = []
  let refunds = []
  let receipts = []
  let error = ''
  let msg = ''
  let expanded = null

  async function load() {
    error = ''
    try {
      const [a, r, rc] = await Promise.all([
        api.get('/applications'),
        api.get('/refunds'),
        api.get('/receipts'),
      ])
      applications = a.applications
      refunds = r.refunds
      receipts = rc.receipts
    } catch (e) {
      error = e.message
    }
  }
  onMount(load)

  async function approve(id) {
    msg = error = ''
    try {
      const res = await api.post(`/applications/${id}/approve`, { reviewer: '店员小王' })
      msg = `已复核通过,退款单 ${res.refund.refund_no},回执 ${res.receipt.receipt_no}:${res.delivery.message}`
      await load()
    } catch (e) {
      error = e.message
    }
  }

  async function reject(id) {
    const reason = prompt('请输入拒绝原因', '不符合价保规则')
    if (!reason) return
    msg = error = ''
    try {
      await api.post(`/applications/${id}/reject`, { reviewer: '店员小王', reason })
      msg = '已拒绝'
      await load()
    } catch (e) {
      error = e.message
    }
  }

  function appToBreakdown(a) {
    return {
      qty: a.qty,
      orig_unit_price_cents: a.orig_unit_price_cents,
      orig_activity_disc_cents: a.orig_activity_disc_cents,
      orig_coupon_disc_cents: a.orig_coupon_disc_cents,
      orig_paid_cents: a.orig_paid_cents,
      cur_unit_price_cents: a.cur_unit_price_cents,
      cur_activity_disc_cents: a.cur_activity_disc_cents,
      cur_effective_cents: a.cur_effective_cents,
      cumulative_refundable_cents: a.cumulative_refundable_cents,
      already_refunded_cents: a.already_refunded_cents,
      this_refund_cents: a.this_refund_cents,
      rule_version: a.rule_version,
    }
  }

  function statusTag(s) {
    return { pending: 'orange', approved: 'blue', rejected: 'red', refunded: 'green' }[s] || ''
  }
</script>

{#if msg}<div class="card" style="color:#1a7f37">{msg}</div>{/if}
{#if error}<div class="card" style="color:#d83931">{error}</div>{/if}

<div class="card">
  <h3>价保申请复核 <button class="ghost" on:click={load}>刷新</button></h3>
  {#if !applications.length}<p class="muted">暂无申请</p>{/if}
  {#each applications as row (row.application.id)}
    {@const a = row.application}
    <div class="app-row">
      <div class="app-head" on:click={() => (expanded = expanded === a.id ? null : a.id)}>
        <span><b>{a.apply_no}</b></span>
        <span>{row.customer_name} · {row.product_name}({row.sku_spec})</span>
        <span>订单 {row.order_no}</span>
        <span class="money">应退 {fmt(a.this_refund_cents)}</span>
        <span class="tag {statusTag(a.status)}">{STATUS_TEXT[a.status]}</span>
      </div>
      {#if expanded === a.id}
        <BreakdownTable b={appToBreakdown(a)} />
        {#if a.reject_reason}<p class="muted">拒绝原因:{a.reject_reason}</p>{/if}
        {#if row.refund}
          <p class="muted">
            退款单 {row.refund.refund_no} · {fmt(row.refund.amount_cents)} · {STATUS_TEXT[row.refund.status]}
            · 幂等键 {row.refund.idempotency_key}
            {#if row.refund.receipt_no}· 回执 {row.refund.receipt_no}{/if}
          </p>
        {/if}
        {#if a.status === 'pending'}
          <div style="margin-top:8px">
            <button class="primary" on:click={() => approve(a.id)}>复核通过并退款</button>
            <button class="danger" on:click={() => reject(a.id)}>拒绝</button>
          </div>
        {/if}
      {/if}
    </div>
  {/each}
</div>

<div class="card">
  <h3>退款记录</h3>
  {#if !refunds.length}<p class="muted">暂无退款</p>{:else}
  <table>
    <thead><tr><th>退款单号</th><th>订单</th><th>金额</th><th>规则版本</th><th>状态</th><th>幂等键</th><th>回执送达次数</th></tr></thead>
    <tbody>
      {#each refunds as r (r.refund.id)}
        <tr>
          <td>{r.refund.refund_no}</td>
          <td>{r.order_no}</td>
          <td class="money">{fmt(r.refund.amount_cents)}</td>
          <td>v{r.refund.rule_version}</td>
          <td><span class="tag {r.refund.status === 'success' ? 'green' : 'orange'}">{STATUS_TEXT[r.refund.status]}</span></td>
          <td class="muted">{r.refund.idempotency_key}</td>
          <td>{r.receipt_deliveries}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  {/if}
</div>

<div class="card">
  <h3>支付回执投递记录(重复送达自动忽略)</h3>
  {#if !receipts.length}<p class="muted">暂无回执</p>{:else}
  <table>
    <thead><tr><th>回执号</th><th>退款单</th><th>金额</th><th>处理结果</th><th>送达时间</th></tr></thead>
    <tbody>
      {#each receipts as r (r.id)}
        <tr>
          <td>{r.receipt_no}</td>
          <td>{r.refund_no}</td>
          <td class="money">{fmt(r.amount_cents)}</td>
          <td>
            {#if r.duplicated}
              <span class="tag red">重复送达,已忽略</span>
            {:else}
              <span class="tag green">首次确认</span>
            {/if}
          </td>
          <td class="muted">{fmtTime(r.received_at)}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  {/if}
</div>

<style>
  h3 { margin: 0 0 12px; font-size: 16px; }
  .app-row { border: 1px solid #e5e9ef; border-radius: 8px; margin-bottom: 8px; }
  .app-head {
    display: flex; gap: 16px; align-items: center; flex-wrap: wrap;
    padding: 10px 14px; cursor: pointer; font-size: 13px;
  }
  .app-head:hover { background: #f7f9fc; }
  .app-row > div:last-child { padding: 0 14px 12px; }
</style>
