<script>
  import { onMount } from 'svelte'
  import { api, fmt, fmtTime } from '../api.js'

  let skus = []
  let rules = []
  let refunds = []
  let error = ''
  let msg = ''

  // 降价表单
  let priceSkuId = 1
  let newPrice = 2799.0
  // 新规则表单
  let ruleName = '周年庆满减'
  let ruleType = 'full_reduction'
  let percentOff = 85
  let threshold = 1000.0
  let reduction = 150.0

  async function load() {
    error = ''
    try {
      const [s, r, f] = await Promise.all([
        api.get('/skus'),
        api.get('/admin/rules'),
        api.get('/refunds'),
      ])
      skus = s.skus
      rules = r.rules
      refunds = f.refunds.filter((x) => x.refund.receipt_no)
    } catch (e) {
      error = e.message
    }
  }
  onMount(load)

  async function dropPrice() {
    msg = error = ''
    try {
      await api.post('/admin/prices', {
        sku_id: Number(priceSkuId),
        price_cents: Math.round(Number(newPrice) * 100),
      })
      msg = `已降价:SKU ${priceSkuId} → ¥${Number(newPrice).toFixed(2)}`
      await load()
    } catch (e) {
      error = e.message
    }
  }

  async function publishRule() {
    msg = error = ''
    try {
      const body =
        ruleType === 'percent_off'
          ? { name: ruleName, rule_type: 'percent_off', percent_off: Number(percentOff) }
          : {
              name: ruleName,
              rule_type: 'full_reduction',
              full_threshold_cents: Math.round(Number(threshold) * 100),
              reduction_cents: Math.round(Number(reduction) * 100),
            }
      const res = await api.post('/admin/rules', body)
      msg = `已发布新规则版本 v${res.version},此后申请按新版本计算`
      await load()
    } catch (e) {
      error = e.message
    }
  }

  async function resend(receiptNo) {
    msg = error = ''
    try {
      const res = await api.post(`/mock/receipts/${receiptNo}/resend`)
      msg = res.message
      await load()
    } catch (e) {
      error = e.message
    }
  }

  async function reset() {
    if (!confirm('重置所有演示数据?')) return
    msg = error = ''
    try {
      await api.post('/admin/reset')
      msg = '演示数据已重置'
      await load()
    } catch (e) {
      error = e.message
    }
  }
</script>

{#if msg}<div class="card ok">{msg}</div>{/if}
{#if error}<div class="card err">{error}</div>{/if}

<div class="card">
  <h3>① 模拟降价(mock 价格接口)</h3>
  <div class="row">
    <select bind:value={priceSkuId}>
      {#each skus as s}
        <option value={s.id}>{s.product_name} · {s.spec}(当前 {fmt(s.price_cents)})</option>
      {/each}
    </select>
    <label>新价格 ¥</label>
    <input type="number" step="0.01" bind:value={newPrice} />
    <button class="primary" on:click={dropPrice}>降价</button>
  </div>
  <p class="muted">降价后请回到「顾客价保」页申请,可看到差价计算明细;再次降价只会退新增部分。</p>
</div>

<div class="card">
  <h3>② 发布新活动规则版本</h3>
  <div class="row">
    <input type="text" bind:value={ruleName} placeholder="规则名称" style="width:140px" />
    <select bind:value={ruleType}>
      <option value="percent_off">折扣</option>
      <option value="full_reduction">满减</option>
    </select>
    {#if ruleType === 'percent_off'}
      <label>折扣(如 85 = 八五折)</label>
      <input type="number" bind:value={percentOff} style="width:80px" />
    {:else}
      <label>满 ¥</label><input type="number" bind:value={threshold} style="width:90px" />
      <label>减 ¥</label><input type="number" bind:value={reduction} style="width:90px" />
    {/if}
    <button class="primary" on:click={publishRule}>发布新版本</button>
  </div>
  <table style="margin-top:10px">
    <thead><tr><th>版本</th><th>名称</th><th>类型</th><th>参数</th><th>发布时间</th></tr></thead>
    <tbody>
      {#each rules as r}
        <tr>
          <td>v{r.version}</td>
          <td>{r.name}</td>
          <td>{r.rule_type === 'percent_off' ? '折扣' : '满减'}</td>
          <td>
            {#if r.rule_type === 'percent_off'}{r.percent_off / 10} 折
            {:else}满 {fmt(r.full_threshold_cents)} 减 {fmt(r.reduction_cents)}{/if}
          </td>
          <td class="muted">{fmtTime(r.created_at)}</td>
        </tr>
      {/each}
    </tbody>
  </table>
  <p class="muted">历史申请与退款保留各自计算时的规则版本,不受新版本影响。</p>
</div>

<div class="card">
  <h3>③ 模拟支付网关重复推送回执</h3>
  {#if !refunds.length}
    <p class="muted">暂无已完成的退款。先在「店员复核」通过一笔申请。</p>
  {:else}
    <table>
      <thead><tr><th>退款单</th><th>金额</th><th>回执号</th><th></th></tr></thead>
      <tbody>
        {#each refunds as r}
          <tr>
            <td>{r.refund.refund_no}</td>
            <td class="money">{fmt(r.refund.amount_cents)}</td>
            <td class="muted">{r.refund.receipt_no}</td>
            <td><button class="ghost" on:click={() => resend(r.refund.receipt_no)}>重发回执</button></td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="muted">重发后可在「店员复核 → 支付回执投递记录」看到重复送达被忽略,退款金额不变。</p>
  {/if}
</div>

<div class="card">
  <h3>④ 重置演示数据</h3>
  <button class="danger" on:click={reset}>重置全部数据</button>
</div>

<style>
  h3 { margin: 0 0 12px; font-size: 16px; }
  .row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  select, input {
    padding: 6px 10px; border: 1px solid #d0d5db; border-radius: 6px; font-size: 14px;
  }
  .ok { color: #1a7f37; }
  .err { color: #d83931; }
</style>
