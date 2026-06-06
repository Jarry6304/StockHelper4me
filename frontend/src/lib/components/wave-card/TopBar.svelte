<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { Timeframe } from '$lib/api';

  export let stockId: string;
  export let stockName: string | null = null;
  export let timeframe: Timeframe = 'daily';
  export let waveSource: 'neely' | 'traditional' = 'neely';
  export let showLayerPills: boolean = false;
  export let layers: {
    fib: boolean;
    waveMarkers: boolean;
    track2: boolean;
    invalidation: boolean;
  } = { fib: true, waveMarkers: true, track2: true, invalidation: true };

  const dispatch = createEventDispatcher<{
    'timeframe-change': { timeframe: Timeframe };
    'source-change': { source: 'neely' | 'traditional' };
    'layer-change': { layers: typeof layers };
  }>();

  const timeframes: { value: Timeframe; label: string }[] = [
    { value: 'daily', label: '日' },
    { value: 'weekly', label: '週' },
    { value: 'monthly', label: '月' },
    { value: 'quarterly', label: '季' }
  ];

  function setTimeframe(tf: Timeframe) {
    if (tf === timeframe) return;
    timeframe = tf;
    dispatch('timeframe-change', { timeframe: tf });
  }

  function setSource(src: 'neely' | 'traditional') {
    if (src === waveSource) return;
    waveSource = src;
    dispatch('source-change', { source: src });
  }

  function toggleLayer(key: keyof typeof layers) {
    layers = { ...layers, [key]: !layers[key] };
    dispatch('layer-change', { layers });
  }
</script>

<div class="topbar">
  <span class="tick">
    {stockId}
    {#if stockName}<small>{stockName}</small>{/if}
  </span>

  <span class="spacer"></span>

  <div class="pills" role="group" aria-label="時間框架">
    {#each timeframes as tf}
      <button
        type="button"
        class:on={timeframe === tf.value}
        on:click={() => setTimeframe(tf.value)}
      >
        {tf.label}
      </button>
    {/each}
  </div>

  <div class="toggle" role="group" aria-label="波浪派別">
    <button type="button" class:on={waveSource === 'neely'} on:click={() => setSource('neely')}>
      Neely
    </button>
    <span class="bar">∥</span>
    <button
      type="button"
      class:on={waveSource === 'traditional'}
      on:click={() => setSource('traditional')}
    >
      傳統
    </button>
  </div>

  {#if showLayerPills}
    <div class="layers" role="group" aria-label="圖層">
      <button class="lyr fib" class:on={layers.fib} on:click={() => toggleLayer('fib')}>fib</button>
      <button
        class="lyr wv"
        class:on={layers.waveMarkers}
        on:click={() => toggleLayer('waveMarkers')}
      >
        波標
      </button>
      <button class="lyr t2" class:on={layers.track2} on:click={() => toggleLayer('track2')}>
        Track2
      </button>
      <button
        class="lyr iv"
        class:on={layers.invalidation}
        on:click={() => toggleLayer('invalidation')}
      >
        失效
      </button>
    </div>
  {/if}
</div>

<style>
  .topbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--line);
    background: var(--header-bg);
    flex-wrap: wrap;
  }

  .tick {
    font-weight: 700;
    font-size: 15px;
  }

  .tick small {
    color: var(--ink-dim);
    font-weight: 400;
    margin-left: 4px;
  }

  .spacer {
    flex: 1;
  }

  .pills {
    display: flex;
    gap: 3px;
    background: var(--tag-bg);
    border: 1px solid var(--line);
    border-radius: 7px;
    padding: 2px;
  }

  .pills button {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--ink-dim);
    padding: 3px 8px;
    border-radius: 5px;
    background: transparent;
    border: none;
  }

  .pills button.on {
    background: var(--wave);
    color: #04212b;
    font-weight: 600;
  }

  .pills button:hover:not(.on) {
    color: var(--ink);
  }

  .toggle {
    display: flex;
    align-items: center;
    border: 1px solid var(--line);
    border-radius: 7px;
    overflow: hidden;
  }

  .toggle button {
    font-family: var(--mono);
    font-size: 11px;
    padding: 4px 10px;
    color: var(--ink-dim);
    background: transparent;
    border: none;
  }

  .toggle button.on {
    background: #1c2c46;
    color: var(--wave);
  }

  .toggle button:hover:not(.on) {
    color: var(--ink);
  }

  .toggle .bar {
    color: var(--ink-faint);
    padding: 0;
    font-family: var(--mono);
    font-size: 11px;
  }

  .layers {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .lyr {
    font-family: var(--mono);
    font-size: 10.5px;
    padding: 3px 8px;
    border-radius: 5px;
    border: 1px solid var(--line);
    color: var(--ink-faint);
    background: transparent;
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .lyr::before {
    content: '';
    width: 9px;
    height: 9px;
    border-radius: 2px;
    background: currentColor;
  }

  .lyr.fib {
    color: var(--fib);
  }
  .lyr.wv {
    color: var(--wave);
  }
  .lyr.t2 {
    color: var(--track2);
  }
  .lyr.iv {
    color: var(--inval);
  }

  .lyr.on {
    color: var(--ink);
  }

  .lyr.fib.on {
    color: var(--fib);
  }
  .lyr.wv.on {
    color: var(--wave);
  }
  .lyr.t2.on {
    color: var(--track2);
  }
  .lyr.iv.on {
    color: var(--inval);
  }
</style>
