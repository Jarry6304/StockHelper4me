<script lang="ts">
  import type { FibZone } from '$contracts/neely/FibZone';
  import type { Monowave } from '$contracts/neely/Monowave';
  import type { Scenario } from '$contracts/neely/Scenario';
  import { createEventDispatcher } from 'svelte';
  import { pickDefaultScenario, scenarioRecencyDays } from '$lib/wave/power';
  import PowerBadge from './PowerBadge.svelte';
  import CountsBadge from './CountsBadge.svelte';
  import PlotlyWaveChart from './PlotlyWaveChart.svelte';

  export let monowaves: Monowave[];
  export let flatFibZones: FibZone[];
  export let scenarios: Scenario[];
  export let asOf: string | null = null;

  const dispatch = createEventDispatcher<{ expand: void }>();

  // 預設選 recency tier 內最強 scenario(對齊 L1「forest 無 primary」— 此只是 UI
  // 預設焦點,非答案)。對 production 一次回傳跨年 forest 防護:近 1 年內結尾的
  // scenario 優先,避免畫面只看到 2022 的舊結構。
  $: topScenario = pickDefaultScenario(scenarios, asOf);
  $: structureLabel = topScenario?.structure_label ?? null;
  $: powerRating = topScenario?.power_rating ?? null;
  $: rulesPassed = topScenario?.rules_passed_count ?? null;
  $: rulesDeferred = topScenario?.deferred_rules_count ?? null;
  $: scenarioStaleDays = topScenario ? Math.round(scenarioRecencyDays(topScenario, asOf)) : null;
  $: isStale = scenarioStaleDays !== null && scenarioStaleDays > 365;
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
    <span class="struct" class:stale={isStale}>{structureLabel}</span>
    {#if isStale && scenarioStaleDays !== null}
      <span class="stale-tag" title="本 scenario 結尾距 as_of 已超過 1 年,可能是 historical anchor">
        ⚠ 結尾 {scenarioStaleDays}d 前
      </span>
    {/if}
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

  .struct.stale {
    color: var(--fib);
  }

  .stale-tag {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--fib);
    background: #1c160a;
    border: 1px dashed #5a4a2a;
    border-radius: 4px;
    padding: 1px 6px;
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
