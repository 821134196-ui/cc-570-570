<script>
  import { onMount, createEventDispatcher } from 'svelte'
  import { api, fen, fmtTime } from '../api.js'

  const dispatch = createEventDispatcher()

  let meta = { rule_versions: [], skus: [], price_events: [] }
  let message = ''

  // 降价表单
  let dropSku = 1
  let dropPrice = 45000
  let dropRule = 1

  async function load() {
    const { data } = await api.meta()
    meta = data
  }
  onMount(load)

  async function doDrop() {
    const { data } = await api.priceDrop(Number(dropSku), Number(dropPrice), Number(dropRule))
    message = data.ok ? '✔ 已发布新价格（新价格事件已生效）' : `✗ ${data.error}`
    await load()
    dispatch('changed')
  }

  async function reset() {
    await api.reset()
    message = '✔ 演示数据已重置'
    await load()
    dispatch('changed')
  }

  // 预置演示脚本：一键执行常见降价
  const presets = [
    { label: '① 耳机标准版 ¥500→¥450（v1 直降）', sku: 1, price: 45000, rule: 1 },
    { label: '② 耳机标准版 ¥450→¥400（v2 满300减30）', sku: 1, price: 40000, rule: 2 },
    { label: '③ 耳机标准版 ¥400→¥380（v3 全场9折）', sku: 1, price: 38000, rule: 3 },
    { label: '键盘 ¥300→¥260（v1 直降）', sku: 3, price: 26000, rule: 1 },
  ]
  async function runPreset(p) {
    dropSku = p.sku; dropPrice = p.price; dropRule = p.rule
    await doDrop()
  }
</script>

<h2>演示控制台</h2>
{#if message}<div class="toast">{message}</div>{/if}

<div class="card">
  <h3>模拟降价（价格接口）</h3>
  <div class="presets">
    {#each presets as p}
      <button class="ghost" on:click={() => runPreset(p)}>{p.label}</button>
    {/each}
  </div>
  <div class="form-row">
    <label>商品规格
      <select bind:value={dropSku}>
        {#each meta.skus as s}
          <option value={s.id}>{s.product_name} / {s.spec}（现价 {fen(s.current_price)}，规则 {s.current_rule_version}）</option>
        {/each}
      </select>
    </label>
    <label>新活动价（分）<input type="number" bind:value={dropPrice} min="1" /></label>
    <label>生效规则版本
      <select bind:value={dropRule}>
        {#each meta.rule_versions as r}
          <option value={r.id}>{r.version} — {r.name}</option>
        {/each}
      </select>
    </label>
    <button class="primary" on:click={doDrop}>发布降价</button>
    <button class="danger" on:click={reset}>重置演示数据</button>
  </div>
</div>

<div class="card">
  <h3>活动规则版本（历史版本永久保留）</h3>
  <table class="grid">
    <thead><tr><th>版本</th><th>名称</th><th>规则内容</th><th>创建时间</th></tr></thead>
    <tbody>
      {#each meta.rule_versions as r}
        <tr><td><span class="tag">{r.version}</span></td><td>{r.name}</td><td><code>{r.config}</code></td><td class="muted">{fmtTime(r.created_at)}</td></tr>
      {/each}
    </tbody>
  </table>
</div>

<div class="card">
  <h3>价格事件流水</h3>
  <table class="grid">
    <thead><tr><th>#</th><th>SKU</th><th>活动价</th><th>规则版本ID</th><th>生效时间</th></tr></thead>
    <tbody>
      {#each meta.price_events as e}
        <tr><td>{e.id}</td><td>SKU-{e.sku_id}</td><td>{fen(e.activity_price)}</td><td>{e.rule_version_id}</td><td class="muted">{fmtTime(e.effective_at)}</td></tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .presets { display: flex; gap: 6px; flex-wrap: wrap; margin-bottom: 10px; }
  .form-row { display: flex; gap: 10px; align-items: flex-end; flex-wrap: wrap; }
  label { display: flex; flex-direction: column; font-size: 12px; color: #667; gap: 4px; }
  select, input { padding: 6px 8px; border: 1px solid #ccd3e0; border-radius: 6px; font-size: 13px; }
  code { background: #f1f3f9; padding: 2px 6px; border-radius: 4px; font-size: 12px; }
</style>
