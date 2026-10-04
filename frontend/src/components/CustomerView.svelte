<script>
  import { onMount } from 'svelte'
  import { api, fen, fmtTime } from '../api.js'
  import BreakdownTable from './BreakdownTable.svelte'

  export let refreshKey = 0 // 演示面板操作后触发刷新

  let orders = []
  let applications = []
  let previewMap = {}   // itemId -> breakdown
  let message = ''
  let lastKeys = {}     // itemId -> 上次使用的幂等键（演示重复提交）

  async function load() {
    const [o, a] = await Promise.all([api.orders(), api.myApplications()])
    orders = o.data.orders || []
    applications = a.data.applications || []
  }
  onMount(load)
  $: if (refreshKey > 0) load()

  async function showPreview(item) {
    const { data } = await api.preview(item.id)
    previewMap = { ...previewMap, [item.id]: data.breakdown }
  }

  async function apply(item, reuseKey = false, claimedSkuId = null) {
    const key = reuseKey && lastKeys[item.id]
      ? lastKeys[item.id]
      : `app-${item.id}-${Math.random().toString(36).slice(2, 10)}`
    lastKeys = { ...lastKeys, [item.id]: key }
    const { data } = await api.apply(item.id, key, claimedSkuId)
    if (data.ok) {
      message = data.deduplicated
        ? `✔ ${data.message}（申请号 ${data.application.app_no}）`
        : data.application.status === 'rejected'
          ? `✗ 申请被系统拒绝：${data.application.reject_reason}`
          : `✔ 申请已提交（${data.application.app_no}），等待店员复核`
      if (data.breakdown) previewMap = { ...previewMap, [item.id]: data.breakdown }
    } else {
      message = `✗ ${data.error}`
    }
    await load()
  }

  const statusText = { pending: '待审核', approved: '退款中', refunded: '已退款', rejected: '已拒绝' }
</script>

<h2>我的订单 <button class="ghost" on:click={load}>刷新</button></h2>
{#if message}<div class="toast">{message}</div>{/if}

{#each orders as o}
  <div class="card">
    <div class="card-head">
      <b>{o.order_no}</b>
      <span class="tag" class:tag-red={o.status === 'returned'}>{o.status === 'returned' ? '已退货' : '已支付'}</span>
      <span class="muted">下单 {fmtTime(o.created_at)} ｜ 价保截止 {fmtTime(o.protected_until)}</span>
    </div>
    {#each o.items as it}
      <div class="item">
        <div class="item-info">
          <b>{it.product_name}</b> <span class="muted">{it.spec}</span> × {it.qty}
          {#if it.returned}<span class="tag tag-red">已退货</span>{/if}
          <div class="muted small">
            下单活动价 {fen(it.activity_price)}/件 ｜ 活动券 -{fen(it.coupon_amount)} ｜
            实付 <b>{fen(it.paid_amount)}</b> ｜ 规则 {it.rule_version}
            {#if it.already_refunded > 0}｜ 已退差价 <b class="green">{fen(it.already_refunded)}</b>{/if}
          </div>
        </div>
        <div class="item-actions">
          <button on:click={() => showPreview(it)}>计算依据</button>
          <button class="primary" on:click={() => apply(it)}>申请价保</button>
          <button class="ghost" title="用同一幂等键重复提交，验证不会重复建单" on:click={() => apply(it, true)}>重复提交</button>
          {#if it.sku_id === 1}
            <button class="ghost warn" title="拿 Pro 版的降价来申请标准版，应被拒绝"
              on:click={() => apply(it, false, 2)}>按Pro版降价申请</button>
          {/if}
        </div>
      </div>
      {#if previewMap[it.id]}
        <BreakdownTable b={previewMap[it.id]} />
      {/if}
    {/each}
  </div>
{/each}

<h2>我的价保申请</h2>
{#if applications.length === 0}<p class="muted">暂无申请</p>{/if}
{#each applications as row}
  <div class="card">
    <div class="card-head">
      <b>{row.application.app_no}</b>
      <span class="tag st-{row.application.status}">{statusText[row.application.status] || row.application.status}</span>
      <span class="muted">{fmtTime(row.application.created_at)}</span>
      {#if row.application.refund_amount > 0}
        <span>应退 <b class="green">{fen(row.application.refund_amount)}</b></span>
      {/if}
      {#if row.refund?.receipt_no}
        <span class="muted">回执 {row.refund.receipt_no}</span>
      {/if}
    </div>
    {#if row.application.reject_reason}
      <div class="reject-box">拒绝原因：{row.application.reject_reason}</div>
    {/if}
    <BreakdownTable b={row.breakdown} />
  </div>
{/each}

<style>
  .item { display: flex; justify-content: space-between; align-items: center; gap: 8px; padding: 8px 0; flex-wrap: wrap; }
  .item-actions { display: flex; gap: 6px; flex-wrap: wrap; }
  .small { font-size: 12px; margin-top: 2px; }
  .green { color: #0a7d3b; }
  .st-refunded { background: #e6f7ec; color: #0a7d3b; }
  .st-rejected { background: #fdecea; color: #a5281b; }
  .st-pending { background: #fff7e0; color: #92610a; }
  .st-approved { background: #e8f0fe; color: #1a56c4; }
</style>
