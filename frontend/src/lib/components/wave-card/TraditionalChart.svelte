<script lang="ts">
  import type { TraditionalScenario } from '$lib/api/traditional';
  import { onMount, onDestroy } from 'svelte';
  import {
    buildTradLayout,
    buildTradTraces,
    type TradPivot
  } from '$lib/wave/traditional-build';

  export let pivots: TradPivot[];
  export let selectedScenario: TraditionalScenario | null = null;
  export let asOf: string | null = null;
  export let layers: { fib: boolean; waveMarkers: boolean; invalidation: boolean } = {
    fib: true,
    waveMarkers: true,
    invalidation: true
  };
  export let height: number = 270;
  export let xRangeDaysBack: number = 540;
  export let xRangeDaysForward: number = 120;

  let container: HTMLDivElement | undefined;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let Plotly: any = null;
  let mounted = false;
  let resizeObserver: ResizeObserver | undefined;

  async function loadPlotly() {
    if (Plotly) return Plotly;
    Plotly = (await import('plotly.js-dist-min')).default;
    return Plotly;
  }

  async function render() {
    if (!container) return;
    const plotly = await loadPlotly();
    const opts = {
      pivots,
      selectedScenario,
      asOf: asOf ?? undefined,
      layers,
      xRangeDaysBack,
      xRangeDaysForward
    };
    const traces = buildTradTraces(opts);
    const layout = buildTradLayout(opts);
    if (mounted) {
      await plotly.react(container, traces, layout, { responsive: true, displayModeBar: false });
    } else {
      await plotly.newPlot(container, traces, layout, {
        responsive: true,
        displayModeBar: false
      });
      mounted = true;
    }
  }

  onMount(() => {
    void render();
    if (container && typeof ResizeObserver !== 'undefined') {
      resizeObserver = new ResizeObserver(() => {
        if (Plotly && container && mounted) Plotly.Plots.resize(container);
      });
      resizeObserver.observe(container);
    }
  });

  onDestroy(() => {
    if (resizeObserver) resizeObserver.disconnect();
    if (Plotly && container && mounted) Plotly.purge(container);
  });

  $: if (mounted && container) void render();
  $: void [pivots, selectedScenario, asOf, layers, xRangeDaysBack, xRangeDaysForward];
</script>

<div bind:this={container} class="chart" style="height: {height}px"></div>

<style>
  .chart {
    width: 100%;
    min-height: 200px;
  }

  :global(.chart .modebar) {
    display: none !important;
  }
</style>
