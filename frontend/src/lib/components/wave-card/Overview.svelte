<script lang="ts">
  import type { Monowave } from '$contracts/neely/Monowave';
  import type { Scenario } from '$contracts/neely/Scenario';
  import type { ActiveJudgmentSummary } from '$lib/api/waves';
  import { createEventDispatcher } from 'svelte';
  import {
    collectLiveFibZones,
    extractCurrentPriceFromMonowaves,
    isScenarioInvalidated,
    scenarioRecencyDays
  } from '$lib/wave/power';
  import type { OhlcPoint } from '$lib/wave/plotly-build';
  import PowerBadge from './PowerBadge.svelte';
  import CountsBadge from './CountsBadge.svelte';
  import PlotlyWaveChart from './PlotlyWaveChart.svelte';

  export let monowaves: Monowave[];
  export let scenarios: Scenario[];
  export let asOf: string | null = null;
  export let ohlcSeries: OhlcPoint[] | null = null;
  /** v4.39:active judgment 的 preferred 候選(WaveCard 解析 anchor→id)。 */
  export let judgedScenarioId: string | null = null;
  export let judgment: ActiveJudgmentSummary | null = null;

  const dispatch = createEventDispatcher<{ expand: void }>();

  // v4.39(wave_judgment_loop §8):本頁不再呼叫 pickDefaultScenario —
  // 有 active judgment ⇒ 焦點 = accepted[preferred] 候選 + 判讀 badge;
  // 無 ⇒ **不預選**(顯示候選 + 證據;判讀走「展開詳情」選取→錨定)。
  // pickDefaultScenario 保留在 power.ts 供 V2 cell 無判讀 fallback 鏡射對。
  $: currentPrice = extractCurrentPriceFromMonowaves(monowaves);
  $: topScenario = judgedScenarioId
    ? (scenarios.find((s) => s.id === judgedScenarioId) ?? null)
    : null;
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
    {ohlcSeries}
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
    {#if judgment && judgedScenarioId}
      <span
        class="judged-tag"
        title="active judgment #{judgment.id} · {judgment.judged_by} · {judgment.confidence_class}"
      >
        ⚓ 已判讀({judgment.status ?? 'active'})
      </span>
    {/if}
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
  {:else if scenarios.length > 0}
    <span class="struct faint">
      (未判讀 — {scenarios.length} 個候選;展開詳情選取後錨定)
    </span>
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

  .judged-tag {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--wave);
    background: #0c2030;
    border: 1px solid #21466a;
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
