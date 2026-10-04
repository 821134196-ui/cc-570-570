<script>
  import { onMount } from 'svelte'
  import { api, fen, fmtTime } from '../api.js'
  import BreakdownTable from './BreakdownTable.svelte'

  export let refreshKey = 0

  let tab = 'pending'
  let applications = []
  let refunds = []
  let ledger = []
  let message = ''

  async function load() {
    const [q, r, g] = await Promise.all([api.staffQueue(tab), api.refunds(), api.gatewayLedger()])
    applications = q.data.applications || []
    refunds = r.data.refunds || []
    ledger = g.data.gateway_refunds || []
  }
  onMount(load)
  $: if (refreshKey > 0) load()
  $: if (tab) load()

  async function approve(id) {
    const { data } = await api.approve(id)
    message = data.ok
      ? data.auto_rejected
        ? `⚠ ${data.message}`
        : `✔ 已退款 ${fen(data.refund.amount)}，回执 ${data.receipt.receipt_no}`
      : `✗ ${data.error}`
    await load()
  }

  async function reject(id) {
    const reason = prompt('拒绝原因', '经复核不符合价保规则')
    if (!reason) return
    const { data } = await api.reject(id, reason)
    message = data.ok ? '✔ 已拒绝' : `✗ ${data.error}`
    await load()
  }

  async function resend(receiptNo) {
    const { data } = await api.resendReceipt(receiptNo)
    message = data.ok ? `↻ 重放回执：${data.result.message}` : `✗ ${data.error}`
    await load()
  }

  const statusText = { pending: '待审核', approved: '退款中', refunded: '已退款', rejected: '已拒绝' }
</script>

<h2>店员复核台 <button class="ghost" on:click={load}>刷新</button></h2>
{#if message}<div class="toast">{message}</div>{/if}

<div class="tabs">
  <button class:active={tab === 'pending'} on:click={() => (tab = 'pending')}>待审核</button>
  <button class:active={tab === 'all'} on:click={() => (tab = 'all')}>全部申请</button>
</div>

{#if applications.length === 0}<p class="muted">暂无申请</p>{/if}
{#each applications as row}
  <div class="card">
    <div class="card-head">
      <b>{row.application.app_no}</b>
      <span class="tag">{statusText[row.application.status] || row.application.status}</span>
      <span class="muted">提交于 {fmtTime(row.application.created_at)} ｜ 幂等键 {row.application.idempotency_key}</span>
      {#if row.application.status === 'pending'}
        <span class="actions">
          <button class="primary" on:click={() => approve(row.application.id)}>复核通过并退款</button>
          <button class="danger" on:click={() => reject(row.application.id)}>拒绝</button>
        </span>
      {/if}
      {#if row.application.refund_amount > 0}<span>退款 <b class="green">{fen(row.application.refund_amount)}</b></span>{/if}
    </div>
    {#if row.application.reject_reason}
      <div class="reject-box">拒绝原因：{row.application.reject_reason}</div>
    {/if}
    <BreakdownTable b={row.breakdown} />
  </div>
{/each}

<h2>退款记录（不可变台账）</h2>
<table class="grid">
  <thead>
    <tr><th>退款单号</th><th>金额</th><th>状态</th><th>支付回执</th><th>回执送达次数</th><th>时间</th><th></th></tr>
  </thead>
  <tbody>
    {#each refunds as r}
      <tr>
        <td>{r.refund.refund_no}</td>
        <td>{fen(r.refund.amount)}</td>
        <td>{r.refund.status === 'succeeded' ? '✅ 已到账' : '⏳ 处理中'}</td>
        <td>{r.refund.receipt_no || '-'}</td>
        <td>{r.receipts.length} 次（重复送达自动去重）</td>
        <td class="muted">{fmtTime(r.refund.created_at)}</td>
        <td>
          {#if r.refund.receipt_no}
            <button class="ghost" title="模拟支付网关重复投递同一回执" on:click={() => resend(r.refund.receipt_no)}>重放回执</button>
          {/if}
        </td>
      </tr>
    {/each}
  </tbody>
</table>

<h2>模拟支付网关台账（第三方视角）</h2>
<table class="grid">
  <thead><tr><th>幂等键</th><th>回执号</th><th>金额</th><th>时间</th></tr></thead>
  <tbody>
    {#each ledger as g}
      <tr><td>{g.idempotency_key}</td><td>{g.receipt_no}</td><td>{fen(g.amount)}</td><td class="muted">{fmtTime(g.created_at)}</td></tr>
    {/each}
  </tbody>
</table>
<p class="muted small">网关对同一幂等键只出款一次；重复调用返回同一回执。</p>

<style>
  .tabs { display: flex; gap: 6px; margin: 8px 0; }
  .tabs button.active { background: #1a56c4; color: #fff; }
  .actions { display: inline-flex; gap: 6px; }
  .green { color: #0a7d3b; }
  .small { font-size: 12px; }
</style>
