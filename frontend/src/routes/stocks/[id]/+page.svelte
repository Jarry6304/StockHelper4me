<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import WaveCard from '$lib/components/wave-card/WaveCard.svelte';
  import type { Timeframe } from '$lib/api/neely';
  import type { PageData } from './$types';

  export let data: PageData;

  $: hasError = data.error !== null;

  function handleTimeframeChange(e: CustomEvent<{ timeframe: Timeframe }>) {
    const u = new URL($page.url);
    u.searchParams.set('timeframe', e.detail.timeframe);
    void goto(u.toString(), { keepFocus: true });
  }

  function handleStateChange(e: CustomEvent<{ state: 'overview' | 'detail' }>) {
    const u = new URL($page.url);
    if (e.detail.state === 'overview') u.searchParams.delete('state');
    else u.searchParams.set('state', 'detail');
    void goto(u.toString(), { keepFocus: true, replaceState: true });
  }
</script>

<svelte:head>
  <title>StockHelper4me · {data.stockId} WAVE</title>
</svelte:head>

<div class="page">
  <header class="page-header">
    <h1>個股 WAVE 卡 <span class="muted">/ {data.stockId}</span></h1>
    <div class="sub">
      data ◂ <code>NeelyCoreOutput (via /stocks/{data.stockId}/neely/forest)</code> · 並排傳統 via <code>/waves</code>
    </div>
  </header>

  {#if data.error?.kind === 'not_found'}
    <div class="err">
      <strong>無資料</strong> · 該股票在 as_of={data.asOf} / timeframe={data.timeframe} 沒有 wave 物化 snapshot。
      <div class="hint">提示:跑 <code>tw_cores run-all --write</code> 重算上游 cores;或檢查
      stock_id 是否正確。</div>
    </div>
  {:else if data.error?.kind === 'overflow'}
    <div class="err warn">
      <strong>情境過多</strong> · {data.error.message}
    </div>
  {:else if data.error?.kind === 'network'}
    <div class="err">
      <strong>服務不可用</strong> · {data.error.message}
      <div class="hint">提示:確認後端 <code>uvicorn web_api.app:app</code> 是否啟動。</div>
    </div>
  {:else}
    <WaveCard
      stockId={data.stockId}
      asOf={data.asOf}
      timeframe={data.timeframe}
      initialState={data.initialState}
      neely={data.waves?.neely ?? null}
      traditional={data.waves?.traditional ?? null}
      resonance={data.resonance}
      closeSeries={data.ohlc}
      on:timeframe-change={handleTimeframeChange}
      on:state-change={handleStateChange}
    />
  {/if}
</div>

<style>
  .page {
    max-width: 1440px;
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

  .err.warn {
    background: #2c2114;
    border-color: #5a4a2a;
    color: #f3c463;
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
</style>
