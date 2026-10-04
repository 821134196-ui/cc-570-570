<script>
  import { onMount } from 'svelte'
  import { api, fmt, fmtTime, STATUS_TEXT } from '../api.js'
  import BreakdownTable from './BreakdownTable.svelte'

  let customer = '张三'
  let orders = []
  let details = {} // orderId -> detail
  let skus = []
  let error = ''

  // 申请面板状态
  let applying = null // { order, itemView }
  let applySkuId = null
  let preview = null
  let applyResult = null
  let busy = false

  async function load() {
    error = ''
    try {
      const [o, s] = await Promise.all([
        api.get(`/orders?customer=${encodeURIComponent(customer)}`),
        api.get('/skus'),
      ])
      orders = o.orders
      skus = s.skus
      details = {}
      for (const { order } of orders) {
        details[order.id] = await api.get(`/orders/${order.id}`)
      }
    } catch (e) {
      error = e.message
    }
  }

  onMount(load)

  function openApply(order, itemView) {
    applying = { order, itemView }
    applySkuId = itemView.item.sku_id
    preview = null
    applyResult = null
  }

  async function doPreview() {
    busy = true
    applyResult = null
    try {
      preview = await api.post('/pp/preview', {
        order_item_id: applying.itemView.item.id,
        sku_id: applySkuId,
      })
    } catch (e) {
      preview = { ok: false, reason: e.message }
    }
    busy = false
  }

  async function doApply() {
    busy = true
    try {
      applyResult = await api.post('/pp/apply', {
        order_item_id: applying.itemView.item.id,
        sku_id: applySkuId,
      })
      if (applyResult.ok) await load()
    } catch (e) {
      applyResult = { ok: false, reason: e.message }
    }
    busy = false
  }

  function statusTag(s) {
    return { pending: 'orange', approved: 'blue', rejected: 'red', refunded: 'green' }[s] || ''
  }
</script>

<div class="card">
  <label>顾客:</label>
  <select bind:value={customer} on:change={load}>
    <option>张三</option>
    <option>李四</option>
  </select>
  <button class="ghost" on:click={load}>刷新</button>
  <span class="muted"> 在「演示控制台」降价或发布新活动后,回到这里申请价保</span>
</div>

{#if error}<div class="card" style="color:#d83931">{error}</div>{/if}

{#each orders as { order, items } (order.id)}
  {@const detail = details[order.id]}
  <div class="card">
    <div style="display:flex;justify-content:space-between;flex-wrap:wrap;gap:8px">
      <div>
        <b>{order.order_no}</b>
        <span class="tag {order.status === 'returned' ? 'red' : 'green'}">{STATUS_TEXT[order.status]}</span>
        <span class="tag blue">下单规则 v{order.rule_version}</span>
      </div>
      <div class="muted">价保截止:{fmtTime(order.protect_until)}</div>
    </div>
    <table>
      <thead>
        <tr><th>商品</th><th>规格</th><th>数量</th><th>实付</th><th>当前价</th><th>已退差价</th><th></th></tr>
      </thead>
      <tbody>
        {#each detail ? detail.items : items.map((i) => ({ item: i })) as iv (iv.item.id)}
          <tr>
            <td>{iv.item.product_name}</td>
            <td>{iv.item.sku_spec}</td>
            <td>{iv.item.qty}</td>
            <td class="money">{fmt(iv.item.paid_cents)}</td>
            <td class="money">{iv.current_price_cents != null ? fmt(iv.current_price_cents) : '-'}</td>
            <td class="money">{fmt(iv.refunded_cents ?? 0)}</td>
            <td>
              {#if order.status === 'paid'}
                <button class="ghost" on:click={() => openApply(order, iv)}>申请价保</button>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    <div class="muted">
      订单实付 <span class="money">{fmt(order.paid_cents)}</span>
      (吊牌合计 {fmt(order.total_amount_cents)} − 活动优惠 {fmt(order.activity_discount_cents)} − 活动券 {fmt(order.coupon_discount_cents)})
    </div>

    {#if detail && detail.applications.length}
      <h4>价保申请记录</h4>
      <table>
        <thead>
          <tr><th>申请单号</th><th>本次应退</th><th>规则版本</th><th>状态</th><th>说明</th><th>时间</th></tr>
        </thead>
        <tbody>
          {#each detail.applications as a (a.id)}
            <tr>
              <td>{a.apply_no}</td>
              <td class="money">{fmt(a.this_refund_cents)}</td>
              <td>v{a.rule_version}</td>
              <td><span class="tag {statusTag(a.status)}">{STATUS_TEXT[a.status]}</span></td>
              <td class="muted">{a.reject_reason || ''}</td>
              <td class="muted">{fmtTime(a.created_at)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}

    {#if detail && detail.refunds.length}
      <h4>退款记录</h4>
      <table>
        <thead>
          <tr><th>退款单号</th><th>金额</th><th>状态</th><th>回执号</th></tr>
        </thead>
        <tbody>
          {#each detail.refunds as r (r.id)}
            <tr>
              <td>{r.refund_no}</td>
              <td class="money">{fmt(r.amount_cents)}</td>
              <td><span class="tag {r.status === 'success' ? 'green' : 'orange'}">{STATUS_TEXT[r.status]}</span></td>
              <td class="muted">{r.receipt_no || '待回执'}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </div>
{/each}

{#if applying}
  <div class="mask" on:click={() => (applying = null)}>
    <div class="modal" on:click|stopPropagation>
      <h3>申请价保 — {applying.itemView.item.product_name}</h3>
      <p class="muted">
        订单 {applying.order.order_no} · 订单规格:{applying.itemView.item.sku_spec}
      </p>
      <div style="margin:10px 0">
        <label>申请规格:</label>
        <select bind:value={applySkuId} on:change={() => { preview = null; applyResult = null }}>
          {#each skus.filter((s) => s.product_name === applying.itemView.item.product_name) as s}
            <option value={s.id}>{s.spec}(当前价 {fmt(s.price_cents)})</option>
          {/each}
        </select>
        <span class="muted"> 选择不同规格可演示“规格不符”拒绝</span>
      </div>
      <div style="margin-bottom:12px">
        <button class="ghost" on:click={doPreview} disabled={busy}>试算差价</button>
        {#if preview && preview.ok}
          <button class="primary" on:click={doApply} disabled={busy}>确认申请(应退 {fmt(preview.breakdown.this_refund_cents)})</button>
        {/if}
      </div>

      {#if preview}
        {#if preview.ok}
          <BreakdownTable b={preview.breakdown} ruleName={preview.rule_name} />
        {:else}
          <p class="reject">✕ 暂不符合价保条件:{preview.reason}</p>
        {/if}
      {/if}

      {#if applyResult}
        {#if applyResult.ok}
          <p class="ok">✓ 申请已提交,单号 {applyResult.application.apply_no},等待店员复核</p>
        {:else}
          <p class="reject">✕ 申请未通过:{applyResult.reason}</p>
        {/if}
      {/if}

      <div style="text-align:right;margin-top:10px">
        <button class="ghost" on:click={() => (applying = null)}>关闭</button>
      </div>
    </div>
  </div>
{/if}

<style>
  select {
    padding: 6px 10px;
    border: 1px solid #d0d5db;
    border-radius: 6px;
    font-size: 14px;
    margin: 0 8px;
  }
  h4 { margin: 14px 0 6px; font-size: 14px; }
  .mask {
    position: fixed; inset: 0; background: rgba(0,0,0,.4);
    display: flex; align-items: center; justify-content: center; z-index: 10;
  }
  .modal {
    background: #fff; border-radius: 12px; padding: 20px 24px;
    width: 640px; max-width: 92vw; max-height: 86vh; overflow: auto;
  }
  .reject { color: #d83931; background: #fde8e7; padding: 10px 14px; border-radius: 8px; }
  .ok { color: #1a7f37; background: #e5f6e5; padding: 10px 14px; border-radius: 8px; }
</style>
