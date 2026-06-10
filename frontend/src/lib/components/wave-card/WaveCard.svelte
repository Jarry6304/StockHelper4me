<script lang="ts">
  import type { NeelyCoreOutput } from '$contracts/neely/NeelyCoreOutput';
  import type { ResonanceFusion } from '$contracts/fusion';
  import type { TraditionalForestOutput } from '$lib/api/traditional';
  import type { Timeframe } from '$lib/api/neely';
  import type { ClosePoint } from '$lib/wave/plotly-build';
  import { createEventDispatcher } from 'svelte';
  import TopBar from './TopBar.svelte';
  import DegreeBar from './DegreeBar.svelte';
  import Overview from './Overview.svelte';
  import Detail from './Detail.svelte';
  import InsufficientDataView from './InsufficientDataView.svelte';
  import TraditionalView from './TraditionalView.svelte';

  export let stockId: string;
  export let stockName: string | null = null;
  export let asOf: string;
  export let timeframe: Timeframe = 'daily';
  export let waveSource: 'neely' | 'traditional' = 'neely';

  /** /waves 端點回 { neely, traditional } 並排 — 對映 spec L3 不合併。 */
  export let neely: NeelyCoreOutput | null = null;
  /** Traditional (Frost & Prechter EWP) — 沒有 contracts/ 嚴格型別,用寬鬆 shape。 */
  export let traditional: TraditionalForestOutput | null = null;
  /** Track2 統計帶來源(可選)。 */
  export let resonance: ResonanceFusion | null = null;
  /** 後復權收盤序列(/stocks/{id}/ohlc)— 兩張波浪圖的淡色時間背景線(可選)。 */
  export let closeSeries: ClosePoint[] | null = null;

  /** 初始狀態 — 由 URL ?state=detail / overview 控制。 */
  export let initialState: 'overview' | 'detail' = 'overview';

  let state: 'overview' | 'detail' = initialState;
  let selectedScenarioId: string | null = null;
  let layers = { fib: true, waveMarkers: true, track2: true, invalidation: true };

  const dispatch = createEventDispatcher<{
    'timeframe-change': { timeframe: Timeframe };
    'source-change': { source: 'neely' | 'traditional' };
    'state-change': { state: 'overview' | 'detail' };
  }>();

  // 對應當前 source 解出 active output(Neely 或 Traditional 並排不合併)。
  // 兩派的 scenario_forest 結構不同 — Traditional 自有 vertical,scenarios 屬性
  // 可能是 traditional 自己的 shape;在原型階段 Traditional 走「scenarios 直接顯示
  // 但不疊 Neely fib 帶」的簡化策略。
  $: activeNeely = waveSource === 'neely' ? neely : null;
  $: activeTraditional = waveSource === 'traditional' ? traditional : null;

  // Neely 路徑
  $: neelyScenarios = activeNeely?.scenario_forest ?? [];
  $: neelyMonowaves = activeNeely?.monowave_series ?? [];
  // flat_fib_zones(全 forest 聯集)不再餵 UI — Overview 內部自組 live-only 雲層;
  // payload 欄位保留給 fusion key_levels。
  $: neelyInsufficient = activeNeely?.insufficient_data ?? false;
  $: neelyCompactionTimeout = activeNeely?.compaction_timeout ?? false;
  $: degreeCeiling = activeNeely?.degree_ceiling ?? null;

  // Traditional 路徑(寬鬆)— v0.1 只顯示 scenarios.length / structure_label
  $: traditionalScenarios = activeTraditional?.scenario_forest ?? [];

  // 是否顯式無法判斷(L6)
  $: shouldShowInsufficient =
    waveSource === 'neely' &&
    (neelyInsufficient || neelyCompactionTimeout || neelyScenarios.length === 0) &&
    !!activeNeely;

  $: insufficientReason = (
    neelyCompactionTimeout
      ? 'compaction_timeout'
      : neelyInsufficient
        ? 'insufficient_data'
        : 'empty_forest'
  ) as 'insufficient_data' | 'compaction_timeout' | 'empty_forest';

  function toOverview() {
    if (state !== 'overview') {
      state = 'overview';
      dispatch('state-change', { state });
    }
  }

  function toDetail() {
    if (state !== 'detail') {
      state = 'detail';
      dispatch('state-change', { state });
    }
  }

  function onScenarioSelect(e: CustomEvent<{ scenarioId: string }>) {
    selectedScenarioId = e.detail.scenarioId;
  }

  function onTimeframeChange(e: CustomEvent<{ timeframe: Timeframe }>) {
    timeframe = e.detail.timeframe;
    selectedScenarioId = null;
    dispatch('timeframe-change', { timeframe });
  }

  function onSourceChange(e: CustomEvent<{ source: 'neely' | 'traditional' }>) {
    waveSource = e.detail.source;
    selectedScenarioId = null;
    dispatch('source-change', { source: waveSource });
  }

  function onLayerChange(e: CustomEvent<{ layers: typeof layers }>) {
    layers = e.detail.layers;
  }
</script>

<section class="card" class:detail={state === 'detail'} aria-label="WAVE 卡 · {stockId}">
  <TopBar
    {stockId}
    {stockName}
    {timeframe}
    {waveSource}
    showLayerPills={state === 'detail'}
    {layers}
    on:timeframe-change={onTimeframeChange}
    on:source-change={onSourceChange}
    on:layer-change={onLayerChange}
  />

  {#if shouldShowInsufficient}
    <DegreeBar degree={null} ceiling={degreeCeiling} selectedScenarioId={null} />
    <InsufficientDataView reason={insufficientReason} />
  {:else if waveSource === 'neely' && activeNeely}
    <DegreeBar
      degree={degreeCeiling?.max_reachable_degree ?? null}
      ceiling={degreeCeiling}
      selectedScenarioId={state === 'detail' ? selectedScenarioId : null}
    />

    {#if state === 'overview'}
      <Overview
        monowaves={neelyMonowaves}
        scenarios={neelyScenarios}
        {asOf}
        {closeSeries}
        on:expand={toDetail}
      />
    {:else}
      <Detail
        monowaves={neelyMonowaves}
        scenarios={neelyScenarios}
        {asOf}
        {closeSeries}
        {selectedScenarioId}
        {resonance}
        {layers}
        on:scenario-select={onScenarioSelect}
      />
      <div class="collapse">
        <button type="button" on:click={toOverview} aria-label="收合到總覽">
          ← 收合
        </button>
      </div>
    {/if}
  {:else if waveSource === 'traditional'}
    {#if !activeTraditional || traditionalScenarios.length === 0}
      <InsufficientDataView
        reason="empty_forest"
        detail="傳統 (Frost & Prechter EWP) 無 forest;此 vertical 與 Neely 並排不合併。"
      />
    {:else}
      <TraditionalView traditional={activeTraditional} {asOf} {closeSeries} />
    {/if}
  {:else}
    <InsufficientDataView reason="empty_forest" detail="API 未回 wave 資料" />
  {/if}
</section>

<style>
  .card {
    background: var(--panel-solid);
    border: 1px solid var(--line);
    border-radius: 12px;
    box-shadow:
      0 18px 40px -24px #000,
      inset 0 1px 0 #ffffff08;
    overflow: hidden;
    max-width: 460px;
    transition: max-width 0.2s ease;
  }

  .card.detail {
    max-width: none;
  }

  .collapse {
    padding: 8px 14px 14px;
    display: flex;
    justify-content: flex-end;
  }

  .collapse button {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--ink-dim);
    background: transparent;
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 4px 10px;
  }

  .collapse button:hover {
    color: var(--ink);
    border-color: var(--ink-dim);
  }

  /* TraditionalView 取代了舊 placeholder,其樣式內含 */
</style>
