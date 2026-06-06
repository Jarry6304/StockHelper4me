<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import Screener from '$lib/components/screener/Screener.svelte';
  import type { ActiveToolkit } from '$lib/api';
  import type { PageData } from './$types';

  export let data: PageData;

  // dev / ?debug=1 → 顯示 placeholder badge
  $: debug =
    typeof window !== 'undefined' &&
    (import.meta.env.DEV || new URLSearchParams(window.location.search).has('debug'));

  let toast: { msg: string; ts: number } | null = null;

  function urlSet(updates: Record<string, string | null>): string {
    const u = new URL($page.url);
    for (const [k, v] of Object.entries(updates)) {
      if (v === null) u.searchParams.delete(k);
      else u.searchParams.set(k, v);
    }
    return u.toString();
  }

  function onToolkitChange(e: CustomEvent<{ toolkit: ActiveToolkit }>) {
    void goto(`/screens/${e.detail.toolkit}${$page.url.search}`, { keepFocus: true });
  }

  function onDateChange(e: CustomEvent<{ date: string }>) {
    void goto(urlSet({ date: e.detail.date }), { keepFocus: true });
  }

  function onTopNChange(e: CustomEvent<{ topN: number }>) {
    void goto(urlSet({ top_n: String(e.detail.topN) }), { keepFocus: true });
  }

  function onDisabledClick(e: CustomEvent<{ toolkit: string; reason: string }>) {
    toast = { msg: `${e.detail.toolkit}: ${e.detail.reason}`, ts: Date.now() };
    setTimeout(() => {
      if (toast && Date.now() - toast.ts >= 4500) toast = null;
    }, 5000);
  }
</script>

<svelte:head>
  <title>StockHelper4me · {data.toolkit} 跨股篩選</title>
</svelte:head>

<div class="page">
  <header class="page-header">
    <h1>跨股篩選表 <span class="muted">/ {data.toolkit}</span></h1>
    <div class="sub">
      backbone ◂ <code>/screens/{data.toolkit}</code> · wave/共振欄 ◂⚠ 需新端點(原型用 placeholder)
    </div>
  </header>

  {#if data.error?.kind === 'not_found'}
    <div class="err">
      <strong>無資料</strong> · toolkit <code>{data.toolkit}</code> 在 date={data.date} 沒有 ranking。
      <div class="hint">
        提示:跑 <code>python src/main.py cross_cores phase 8</code> 重算 ranking;或檢查 toolkit 是否啟用。
      </div>
    </div>
  {:else if data.error?.kind === 'network'}
    <div class="err">
      <strong>服務不可用</strong> · {data.error.message}
      <div class="hint">提示:確認後端 <code>uvicorn web_api.app:app</code> 是否啟動。</div>
    </div>
  {:else}
    <Screener
      toolkit={data.toolkit}
      date={data.date}
      topN={data.topN}
      rows={data.rows}
      rankingDate={data.rankingDate}
      showPlaceholderBadge={debug}
      on:toolkit-change={onToolkitChange}
      on:date-change={onDateChange}
      on:top-n-change={onTopNChange}
      on:disabled-toolkit-click={onDisabledClick}
    />
  {/if}

  {#if toast}
    <div class="toast" role="status">
      <span class="warn">⚠</span> {toast.msg}
    </div>
  {/if}
</div>

<style>
  .page {
    max-width: 1120px;
    margin: 0 auto;
  }

  .page-header {
    margin-bottom: 22px;
  }

  h1 {
    margin: 0 0 6px;
    font-size: 20px;
  }

  h1 .muted {
    color: var(--ink-faint);
    font-weight: 400;
  }

  .sub {
    color: var(--ink-dim);
    font-family: var(--mono);
    font-size: 12px;
  }

  .sub code {
    color: var(--wave);
  }

  .err {
    background: #2c1419;
    border: 1px solid #5a3340;
    border-radius: 10px;
    padding: 16px 18px;
    color: #ffb3bd;
    font-size: 14px;
  }

  .err strong {
    color: var(--ink);
  }

  .err .hint {
    margin-top: 6px;
    font-family: var(--mono);
    font-size: 12px;
    color: var(--ink-dim);
  }

  .err code {
    color: var(--wave);
  }

  .toast {
    position: fixed;
    bottom: 20px;
    left: 50%;
    transform: translateX(-50%);
    background: #2c2114;
    border: 1px solid #5a4a2a;
    color: #f3c463;
    padding: 10px 16px;
    border-radius: 8px;
    font-size: 12px;
    font-family: var(--mono);
    box-shadow: 0 12px 32px -16px #000;
    max-width: 800px;
  }

  .toast .warn {
    margin-right: 6px;
  }
</style>
