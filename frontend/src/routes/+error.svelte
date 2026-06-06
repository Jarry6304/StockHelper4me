<script lang="ts">
  import { page } from '$app/stores';

  $: status = $page.status;
  $: message = $page.error?.message ?? '未知錯誤';
</script>

<svelte:head>
  <title>StockHelper4me · 錯誤 {status}</title>
</svelte:head>

<div class="error-page">
  <h1>{status}</h1>
  <p class="msg">{message}</p>
  <div class="actions">
    <a href="/">回首頁</a>
    <a href="/stocks/2330">範例個股(2330)</a>
    <a href="/screens/magic_formula">範例篩選(magic_formula)</a>
  </div>
</div>

<style>
  .error-page {
    max-width: 640px;
    margin: 60px auto;
    text-align: center;
    padding: 0 20px;
  }

  h1 {
    font-size: 80px;
    margin: 0;
    color: var(--inval);
    font-family: var(--mono);
    font-weight: 700;
  }

  .msg {
    color: var(--ink-dim);
    font-family: var(--mono);
    font-size: 14px;
    margin: 12px 0 32px;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 12px;
  }

  .actions a {
    font-family: var(--mono);
    font-size: 13px;
    color: var(--wave);
    background: var(--tag-bg);
    border: 1px solid #21466a;
    border-radius: 8px;
    padding: 8px 16px;
  }

  .actions a:hover {
    background: #0c2030;
    text-decoration: none;
  }
</style>
