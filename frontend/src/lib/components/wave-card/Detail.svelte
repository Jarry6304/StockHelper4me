<script lang="ts">
  import type { FibZone } from '$contracts/neely/FibZone';
  import type { Monowave } from '$contracts/neely/Monowave';
  import type { ResonanceFusion } from '$contracts/fusion';
  import type { Scenario } from '$contracts/neely/Scenario';
  import type { ActiveJudgmentSummary } from '$lib/api/waves';
  import { createEventDispatcher } from 'svelte';
  import { sortScenarios } from '$lib/wave/power';
  import type { OhlcPoint } from '$lib/wave/plotly-build';
  import PlotlyWaveChart from './PlotlyWaveChart.svelte';
  import ScenarioList from './ScenarioList.svelte';
  import InvalidationBar from './InvalidationBar.svelte';

  export let monowaves: Monowave[];
  export let scenarios: Scenario[];
  export let asOf: string | null = null;
  export let ohlcSeries: OhlcPoint[] | null = null;
  export let selectedScenarioId: string | null = null;
  export let resonance: ResonanceFusion | null = null;
  export let layers: {
    fib: boolean;
    waveMarkers: boolean;
    track2: boolean;
    invalidation: boolean;
  } = { fib: true, waveMarkers: true, track2: true, invalidation: true };
  /** v4.39:active judgment(WaveCard 解析 anchor→scenario id)。 */
  export let judgedScenarioId: string | null = null;
  export let acceptedScenarioIds: string[] = [];
  export let judgment: ActiveJudgmentSummary | null = null;
  /** dossier 有 snapshot_ref 才能錨定(POST /judgments 的 as_of 基準)。 */
  export let anchorEnabled: boolean = false;
  export let anchorPending: boolean = false;

  const dispatch = createEventDispatcher<{
    'scenario-select': { scenarioId: string };
    anchor: { scenarioId: string };
    collapse: void;
  }>();

  $: sorted = sortScenarios(scenarios);
  // v4.39(wave_judgment_loop §8):**不預選** — 有 active judgment ⇒ 預設
  // 焦點 = accepted[preferred];無 ⇒ null(顯示候選 + 證據,判讀者自行點選)。
  // pickDefaultScenario 不再於此呼叫(保留於 power.ts 供 V2 鏡射對)。
  $: defaultSelected = judgedScenarioId ?? null;
  $: effectiveSelected = selectedScenarioId ?? defaultSelected;
  $: selectedScenario = sorted.find((s) => s.id === effectiveSelected) ?? null;
  $: fibZones =
    (selectedScenario?.expected_fib_zones as FibZone[] | undefined) ?? [];
  $: invalidationTriggers = selectedScenario?.invalidation_triggers ?? [];
  $: canAnchorSelected =
    anchorEnabled &&
    !!selectedScenario &&
    selectedScenario.id !== judgedScenarioId;

  // displayId 對映 ScenarioList 用「S1, S2...」順序
  $: displayMap = new Map(sorted.map((s, i) => [s.id, `S${i + 1}`]));
  $: selectedDisplay = effectiveSelected ? displayMap.get(effectiveSelected) ?? null : null;

  // Track2 帶 — 從 resonance 取(若有)
  $: track2Bands = extractTrack2Bands(resonance);

  function extractTrack2Bands(
    r: ResonanceFusion | null
  ): Array<{ low: number; high: number; horizon: string }> | null {
    if (!r) return null;
    const bands: Array<{ low: number; high: number; horizon: string }> = [];
    const t2 = r.track2;
    if (!t2 || typeof t2 !== 'object') return null;
    // track2 contracts.py 的 horizons 是 dict<int, Track2Band | null>
    const horizons = (t2 as { horizons?: Record<string, unknown> }).horizons;
    if (!horizons) return null;
    for (const [h, raw] of Object.entries(horizons)) {
      if (raw && typeof raw === 'object') {
        const b = raw as { low?: number; high?: number; horizon_days?: number };
        if (typeof b.low === 'number' && typeof b.high === 'number') {
          bands.push({ low: b.low, high: b.high, horizon: `${h}d` });
        }
      }
    }
    return bands.length > 0 ? bands : null;
  }

  function handleSelect(e: CustomEvent<{ scenarioId: string }>) {
    dispatch('scenario-select', e.detail);
  }
</script>

<div class="detail-body">
  <div class="detail-chart">
    <PlotlyWaveChart
      {monowaves}
      {fibZones}
      {ohlcSeries}
      {selectedScenario}
      {asOf}
      invalidationTriggers={layers.invalidation ? invalidationTriggers : null}
      {track2Bands}
      {layers}
      height={520}
      xRangeDaysBack={540}
      xRangeDaysForward={120}
    />
  </div>

  <div class="scen-side">
    {#if judgment && judgedScenarioId}
      <div class="judged-banner" role="status">
        ⚓ active judgment #{judgment.id} · {judgment.judged_by} ·
        {judgment.confidence_class}({judgment.status ?? 'active'})
      </div>
    {:else}
      <div class="judged-banner faint" role="status">
        未判讀 — 候選平權顯示,不預選;點選候選後可「錨定」寫入判讀
      </div>
    {/if}
    <ScenarioList
      {scenarios}
      selectedId={effectiveSelected}
      acceptedIds={acceptedScenarioIds}
      {asOf}
      on:select={handleSelect}
    />
    {#if canAnchorSelected && selectedScenario}
      <div class="anchor-row">
        <button
          class="anchor-btn"
          type="button"
          disabled={anchorPending}
          on:click={() => dispatch('anchor', { scenarioId: selectedScenario.id })}
        >
          {anchorPending ? '錨定中…' : `⚓ 錨定此候選(${selectedDisplay ?? selectedScenario.id})`}
        </button>
      </div>
    {/if}
  </div>
</div>

{#if selectedScenario && layers.invalidation}
  <InvalidationBar
    triggers={selectedScenario.invalidation_triggers}
    selectedScenarioId={selectedDisplay}
  />
{/if}

<style>
  .detail-body {
    display: grid;
    grid-template-columns: 1fr 296px;
    gap: 0;
  }

  @media (max-width: 780px) {
    .detail-body {
      grid-template-columns: 1fr;
    }
  }

  .detail-chart {
    padding: 14px;
    border-right: 1px solid var(--line);
  }

  .scen-side {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .judged-banner {
    font-family: var(--mono);
    font-size: 10.5px;
    color: var(--wave);
    background: #0c2030;
    border-bottom: 1px solid var(--line);
    padding: 8px 12px;
  }

  .judged-banner.faint {
    color: var(--ink-faint);
    background: transparent;
  }

  .anchor-row {
    padding: 8px 12px 12px;
    background: var(--header-bg);
  }

  .anchor-btn {
    width: 100%;
    font-family: var(--mono);
    font-size: 11px;
    color: var(--wave);
    border: 1px solid #21466a;
    border-radius: 6px;
    padding: 6px 10px;
    background: #0c2030;
  }

  .anchor-btn:hover:enabled {
    background: #0e2840;
  }

  .anchor-btn:disabled {
    opacity: 0.5;
  }

  @media (max-width: 780px) {
    .detail-chart {
      border-right: none;
      border-bottom: 1px solid var(--line);
    }
  }
</style>
