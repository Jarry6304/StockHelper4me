<script lang="ts">
  import type { FibZone } from '$contracts/neely/FibZone';
  import type { Monowave } from '$contracts/neely/Monowave';
  import type { ResonanceFusion } from '$contracts/fusion';
  import type { Scenario } from '$contracts/neely/Scenario';
  import { createEventDispatcher } from 'svelte';
  import {
    extractCurrentPriceFromMonowaves,
    pickDefaultScenario,
    sortScenarios
  } from '$lib/wave/power';
  import PlotlyWaveChart from './PlotlyWaveChart.svelte';
  import ScenarioList from './ScenarioList.svelte';
  import InvalidationBar from './InvalidationBar.svelte';

  export let monowaves: Monowave[];
  export let scenarios: Scenario[];
  export let asOf: string | null = null;
  export let selectedScenarioId: string | null = null;
  export let resonance: ResonanceFusion | null = null;
  export let layers: {
    fib: boolean;
    waveMarkers: boolean;
    track2: boolean;
    invalidation: boolean;
  } = { fib: true, waveMarkers: true, track2: true, invalidation: true };

  const dispatch = createEventDispatcher<{
    'scenario-select': { scenarioId: string };
    collapse: void;
  }>();

  $: sorted = sortScenarios(scenarios);
  // 預設選 invalidation-filter + tier-by-recency + within-tier-by-power
  // (對齊 Overview 的 pickDefaultScenario;詳見 power.ts rationale)。
  $: currentPrice = extractCurrentPriceFromMonowaves(monowaves);
  $: defaultScenario = pickDefaultScenario(scenarios, asOf, { currentPrice });
  $: defaultSelected = defaultScenario?.id ?? sorted[0]?.id ?? null;
  $: effectiveSelected = selectedScenarioId ?? defaultSelected;
  $: selectedScenario =
    sorted.find((s) => s.id === effectiveSelected) ?? defaultScenario ?? sorted[0] ?? null;
  $: fibZones =
    (selectedScenario?.expected_fib_zones as FibZone[] | undefined) ?? [];
  $: invalidationTriggers = selectedScenario?.invalidation_triggers ?? [];

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
      {selectedScenario}
      {asOf}
      invalidationTriggers={layers.invalidation ? invalidationTriggers : null}
      {track2Bands}
      {layers}
      height={270}
      xRangeDaysBack={540}
      xRangeDaysForward={120}
    />
  </div>

  <ScenarioList {scenarios} selectedId={effectiveSelected} on:select={handleSelect} />
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

  @media (max-width: 780px) {
    .detail-chart {
      border-right: none;
      border-bottom: 1px solid var(--line);
    }
  }
</style>
