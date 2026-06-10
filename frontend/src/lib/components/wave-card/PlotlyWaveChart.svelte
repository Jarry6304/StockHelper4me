<script lang="ts">
  import type { FibZone } from '$contracts/neely/FibZone';
  import type { Monowave } from '$contracts/neely/Monowave';
  import type { Scenario } from '$contracts/neely/Scenario';
  import type { Trigger } from '$contracts/neely/Trigger';
  import { onMount, onDestroy } from 'svelte';
  import { buildLayout, buildTraces, type ClosePoint } from '$lib/wave/plotly-build';

  export let monowaves: Monowave[];
  export let fibZones: FibZone[];
  export let closeSeries: ClosePoint[] | null = null;
  export let selectedScenario: Scenario | null = null;
  export let asOf: string | null = null;
  export let invalidationTriggers: Trigger[] | null = null;
  export let track2Bands: Array<{ low: number; high: number; horizon: string }> | null = null;
  export let layers: {
    fib: boolean;
    waveMarkers: boolean;
    track2: boolean;
    invalidation: boolean;
  } = { fib: true, waveMarkers: true, track2: true, invalidation: true };
  export let height: number = 240;
  /** 顯式覆寫 x 軸範圍(若 null,自動由 asOf + xRangeDaysBack/Forward 算)。 */
  export let xRange: [string, string] | null = null;
  /** 預設 12 個月歷史。State 1 總覽建議用此,State 2 詳情可調大。 */
  export let xRangeDaysBack: number = 365;
  export let xRangeDaysForward: number = 90;

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
      monowaves,
      fibZones,
      closeSeries: closeSeries ?? undefined,
      selectedScenario,
      asOf: asOf ?? undefined,
      invalidationTriggers: invalidationTriggers ?? undefined,
      track2Bands: track2Bands ?? undefined,
      layers,
      xRange,
      xRangeDaysBack,
      xRangeDaysForward
    };
    const traces = buildTraces(opts);
    const layout = buildLayout(opts);
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
        if (Plotly && container && mounted) {
          Plotly.Plots.resize(container);
        }
      });
      resizeObserver.observe(container);
    }
  });

  onDestroy(() => {
    if (resizeObserver) resizeObserver.disconnect();
    if (Plotly && container && mounted) {
      Plotly.purge(container);
    }
  });

  // 響應式 re-render
  $: if (mounted && container) {
    void render();
  }
  // 上行的 reactive triggers:依賴 monowaves / fibZones / selectedScenario / layers
  // 因為 svelte reactive scope 跟著用到的 prop,顯式列以下避免 unused-var lint 與重要訊號被優化掉:
  $: void [
    monowaves,
    fibZones,
    closeSeries,
    selectedScenario,
    asOf,
    invalidationTriggers,
    track2Bands,
    layers,
    xRange,
    xRangeDaysBack,
    xRangeDaysForward
  ];
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
