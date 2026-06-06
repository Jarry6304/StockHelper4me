<script lang="ts">
  import type { FibZone } from '$contracts/neely/FibZone';
  import type { Monowave } from '$contracts/neely/Monowave';
  import type { Scenario } from '$contracts/neely/Scenario';
  import { createEventDispatcher } from 'svelte';
  import { sortScenarios } from '$lib/wave/power';
  import PowerBadge from './PowerBadge.svelte';
  import CountsBadge from './CountsBadge.svelte';
  import PlotlyWaveChart from './PlotlyWaveChart.svelte';

  export let monowaves: Monowave[];
  export let flatFibZones: FibZone[];
  export let scenarios: Scenario[];
  export let asOf: string | null = null;

  const dispatch = createEventDispatcher<{ expand: void }>();

  // 取 power 排序首條 scenario 作主結構標題(不是 primary 選定 — 對齊 L1
  // 「forest 無 primary」設計;這只是顯示用標題,標題下標明「僅作標題,非答案」)
  $: sorted = sortScenarios(scenarios);
  $: topScenario = sorted[0] ?? null;
  $: structureLabel = topScenario?.structure_label ?? null;
  $: powerRating = topScenario?.power_rating ?? null;
  $: rulesPassed = topScenario?.rules_passed_count ?? null;
  $: rulesDeferred = topScenario?.deferred_rules_count ?? null;
</script>

<div class="chartbox">
  <PlotlyWaveChart
    {monowaves}
    fibZones={flatFibZones}
    selectedScenario={topScenario}
    {asOf}
    height={196}
  />
</div>

<div class="meta">
  {#if structureLabel}
    <span class="struct">{structureLabel}</span>
  {:else}
    <span class="struct faint">(無 scenario)</span>
  {/if}
</div>

<div class="meta meta2">
  <CountsBadge count={scenarios.length} label="情境" />
  {#if powerRating}<PowerBadge power={powerRating} />{/if}
  {#if rulesPassed !== null && rulesDeferred !== null}
    <CountsBadge label="rules" count={rulesPassed} secondary={rulesDeferred} />
  {/if}
  <span class="spacer"></span>
  <button class="expand" type="button" on:click={() => dispatch('expand')}>
    展開詳情 →
  </button>
</div>

<style>
  .chartbox {
    padding: 14px;
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 14px 6px;
    flex-wrap: wrap;
  }

  .meta2 {
    padding-bottom: 14px;
  }

  .struct {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--wave);
    flex: 1;
    min-width: 200px;
  }

  .struct.faint {
    color: var(--ink-faint);
  }

  .spacer {
    flex: 1;
  }

  .expand {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--wave);
    border: 1px solid #21466a;
    border-radius: 6px;
    padding: 4px 10px;
    background: #0c2030;
  }

  .expand:hover {
    background: #0e2840;
  }
</style>
