<script lang="ts">
  import type { Monowave } from '$contracts/neely/Monowave';
  import type { Scenario } from '$contracts/neely/Scenario';
  import { createEventDispatcher } from 'svelte';
  import {
    collectLiveFibZones,
    extractCurrentPriceFromMonowaves,
    isScenarioInvalidated,
    pickDefaultScenario,
    scenarioRecencyDays
  } from '$lib/wave/power';
  import type { ClosePoint } from '$lib/wave/plotly-build';
  import PowerBadge from './PowerBadge.svelte';
  import CountsBadge from './CountsBadge.svelte';
  import PlotlyWaveChart from './PlotlyWaveChart.svelte';

  export let monowaves: Monowave[];
  export let scenarios: Scenario[];
  export let asOf: string | null = null;
  export let closeSeries: ClosePoint[] | null = null;

  const dispatch = createEventDispatcher<{ expand: void }>();

  // 預設選 invalidation-filter + tier-by-recency + within-tier-by-power(對齊 spec L1
  // 「forest 無 primary」— 此只是 UI 預設焦點,非答案)。
  //
  // 2 層防護(production 1 次回傳跨年 forest):
  //   (1) currentPrice 過濾已 invalidated scenario(triggers vs current_price)
  //   (2) tier 化 recency(≤60d / ≤180d / ≤365d / >365d)— 即時優先於強訊號
  // 詳見 power.ts §pickDefaultScenario rationale。
  $: currentPrice = extractCurrentPriceFromMonowaves(monowaves);
  $: topScenario = pickDefaultScenario(scenarios, asOf, { currentPrice });
  // 雲層只畫 live(結尾 ≤180d)scenario 的 fib zones — 不再用 flat_fib_zones
  // 全 forest 聯集(historical anchor 的價位會被畫進今天的投影窗 = 失準)。
  $: liveFibZones = collectLiveFibZones(scenarios, asOf);
  $: cloudHidden = scenarios.length > 0 && liveFibZones.length === 0;
  $: structureLabel = topScenario?.structure_label ?? null;
  $: powerRating = topScenario?.power_rating ?? null;
  $: rulesPassed = topScenario?.rules_passed_count ?? null;
  $: rulesDeferred = topScenario?.deferred_rules_count ?? null;
  $: scenarioStaleDays = topScenario ? Math.round(scenarioRecencyDays(topScenario, asOf)) : null;
  $: isStale = scenarioStaleDays !== null && scenarioStaleDays > 365;
  $: topInvalidated = topScenario ? isScenarioInvalidated(topScenario, currentPrice) : false;
</script>

<div class="chartbox">
  <PlotlyWaveChart
    {monowaves}
    fibZones={liveFibZones}
    {closeSeries}
    selectedScenario={topScenario}
    {asOf}
    height={380}
  />
</div>

{#if cloudHidden}
  <div class="cloud-hint" role="status">
    ☁ 近 180d 無進行中形態 — fib 雲層已隱藏(歷史形態的投影見「展開詳情」逐條檢視)
  </div>
{/if}

<div class="meta">
  {#if structureLabel}
    <span class="struct" class:stale={isStale}>{structureLabel}</span>
    {#if isStale && scenarioStaleDays !== null}
      <span class="stale-tag" title="本 scenario 結尾距 as_of 已超過 1 年,可能是 historical anchor">
        ⚠ 結尾 {scenarioStaleDays}d 前
      </span>
    {/if}
    {#if topInvalidated}
      <span class="stale-tag inval-tag" title="當前價格已觸發本 scenario 的 InvalidateScenario trigger,理論上已失效">
        ⚠ 已 invalidated
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

  .cloud-hint {
    padding: 0 14px 8px;
    font-family: var(--mono);
    font-size: 10.5px;
    color: var(--ink-faint);
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

  .stale-tag.inval-tag {
    color: var(--inval);
    background: #1c0f14;
    border-color: #5a3340;
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
