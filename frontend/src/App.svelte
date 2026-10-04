<script>
  import CustomerView from './components/CustomerView.svelte'
  import StaffView from './components/StaffView.svelte'
  import DemoPanel from './components/DemoPanel.svelte'

  let view = 'customer'
  let refreshKey = 0
  const bump = () => (refreshKey += 1)
</script>

<header>
  <h1>🛡️ 门店价保中心</h1>
  <nav>
    <button class:active={view === 'customer'} on:click={() => (view = 'customer')}>顾客</button>
    <button class:active={view === 'staff'} on:click={() => (view = 'staff')}>店员</button>
    <button class:active={view === 'demo'} on:click={() => (view = 'demo')}>演示控制台</button>
  </nav>
</header>

<main>
  {#if view === 'customer'}
    <CustomerView {refreshKey} />
  {:else if view === 'staff'}
    <StaffView {refreshKey} />
  {:else}
    <DemoPanel on:changed={bump} />
    <p class="muted">提示：降价后切到「顾客」页申请价保，再到「店员」页复核退款；退款记录页的「重放回执」可模拟支付回执重复送达。</p>
  {/if}
</main>

<style>
  header { display: flex; align-items: center; gap: 20px; padding: 14px 24px; background: #16213e; color: #fff; flex-wrap: wrap; }
  h1 { font-size: 18px; margin: 0; }
  nav { display: flex; gap: 8px; }
  nav button { background: transparent; color: #cdd6f4; border: 1px solid #3a4a7a; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  nav button.active { background: #4c6ef5; color: #fff; border-color: #4c6ef5; }
  main { max-width: 1080px; margin: 0 auto; padding: 18px 16px 60px; }
  .muted { color: #99a; }
</style>
