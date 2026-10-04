<script>
  import CustomerView from './lib/CustomerView.svelte'
  import StaffView from './lib/StaffView.svelte'
  import ConsoleView from './lib/ConsoleView.svelte'

  let tab = 'customer'
  const tabs = [
    { key: 'customer', label: '顾客价保' },
    { key: 'staff', label: '店员复核' },
    { key: 'console', label: '演示控制台' },
  ]
</script>

<header>
  <h1>门店价保中心</h1>
  <nav>
    {#each tabs as t}
      <button class:active={tab === t.key} on:click={() => (tab = t.key)}>
        {t.label}
      </button>
    {/each}
  </nav>
</header>

<main>
  {#if tab === 'customer'}
    <CustomerView />
  {:else if tab === 'staff'}
    <StaffView />
  {:else}
    <ConsoleView />
  {/if}
</main>

<style>
  :global(body) {
    margin: 0;
    font-family: -apple-system, 'PingFang SC', 'Microsoft YaHei', sans-serif;
    background: #f3f5f7;
    color: #1f2329;
  }
  header {
    background: #1f2329;
    color: #fff;
    padding: 12px 24px;
    display: flex;
    align-items: center;
    gap: 32px;
    flex-wrap: wrap;
  }
  h1 {
    font-size: 18px;
    margin: 0;
  }
  nav {
    display: flex;
    gap: 8px;
  }
  nav button {
    background: transparent;
    color: #aab2bd;
    border: 1px solid transparent;
    padding: 6px 16px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 14px;
  }
  nav button.active {
    color: #fff;
    border-color: #4c9aff;
    background: rgba(76, 154, 255, 0.15);
  }
  main {
    max-width: 1080px;
    margin: 20px auto;
    padding: 0 16px;
  }
  :global(.card) {
    background: #fff;
    border-radius: 10px;
    padding: 16px 20px;
    margin-bottom: 16px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.08);
  }
  :global(button.primary) {
    background: #2f6bff;
    color: #fff;
    border: none;
    padding: 7px 18px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 14px;
  }
  :global(button.primary:disabled) {
    background: #b6c4e8;
    cursor: not-allowed;
  }
  :global(button.ghost) {
    background: #fff;
    color: #2f6bff;
    border: 1px solid #2f6bff;
    padding: 6px 14px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
  }
  :global(button.danger) {
    background: #fff;
    color: #d83931;
    border: 1px solid #d83931;
    padding: 6px 14px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
  }
  :global(table) {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  :global(th, td) {
    text-align: left;
    padding: 8px 10px;
    border-bottom: 1px solid #eef1f4;
  }
  :global(th) {
    color: #646a73;
    font-weight: 500;
  }
  :global(.tag) {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 10px;
    font-size: 12px;
    background: #eef1f4;
    color: #646a73;
  }
  :global(.tag.green) {
    background: #e5f6e5;
    color: #1a7f37;
  }
  :global(.tag.orange) {
    background: #fff3e0;
    color: #b26a00;
  }
  :global(.tag.red) {
    background: #fde8e7;
    color: #d83931;
  }
  :global(.tag.blue) {
    background: #e8f0fe;
    color: #2f6bff;
  }
  :global(.muted) {
    color: #8a919c;
    font-size: 12px;
  }
  :global(.money) {
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }
</style>
