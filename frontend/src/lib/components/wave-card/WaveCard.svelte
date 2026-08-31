<script lang="ts">
  import type { NeelyCoreOutput } from '$contracts/neely/NeelyCoreOutput';
  import type { ResonanceFusion } from '$contracts/fusion';
  import type { TraditionalForestOutput } from '$lib/api/traditional';
  import type { Timeframe } from '$lib/api/neely';
  import type { ActiveJudgmentSummary, WaveDossier } from '$lib/api/waves';
  import { buildAnchorJudgment, JudgmentRejectedError, postJudgment } from '$lib/api/judgments';
  import type { OhlcPoint } from '$lib/wave/plotly-build';
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
  /** 判讀證據卷宗(/waves 第三段;v4.39 additive — 缺 → 判讀功能降級隱藏)。 */
  export let dossier: WaveDossier | null = null;
  /** Track2 統計帶來源(可選)。 */
  export let resonance: ResonanceFusion | null = null;
  /** 後復權 OHLCV(/stocks/{id}/ohlc)— 兩張波浪圖的 K 棒時間背景(可選)。 */
  export let ohlcSeries: OhlcPoint[] | null = null;

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

  // ── v4.39 判讀迴路(wave_judgment_loop §8 前端行)────────────────────────
  // active judgment 由 dossier 附載;「選取→錨定」POST 成功後以 localJudgment
  // 蓋過(免重抓整頁)。accepted anchor_key → scenario id 的對映走 dossier
  // 候選(候選已含 anchor_key + id,前端不重算錨定鍵)。
  let localJudgment: ActiveJudgmentSummary | null = null;
  let anchorPending = false;
  let anchorError: string | null = null;
  let anchorNotice: string | null = null;

  $: tfSection = dossier?.timeframes?.[timeframe] ?? null;
  $: activeJudgment = localJudgment ?? dossier?.active_judgment?.[timeframe] ?? null;
  $: candidateByAnchor = new Map((tfSection?.candidates ?? []).map((c) => [c.anchor_key, c]));
  $: acceptedEntries = activeJudgment?.accepted ?? [];
  $: judgedScenarioId = (() => {
    const preferred = acceptedEntries.find((a) => a.role === 'preferred');
    if (!preferred) return null;
    return candidateByAnchor.get(preferred.anchor_key)?.id ?? null;
  })();
  $: acceptedScenarioIds = acceptedEntries
    .map((a) => candidateByAnchor.get(a.anchor_key)?.id)
    .filter((id): id is string => typeof id === 'string');

  async function onAnchor(e: CustomEvent<{ scenarioId: string }>) {
    anchorError = null;
    anchorNotice = null;
    const snapshotDate = tfSection?.snapshot_ref?.snapshot_date;
    const candidate = (tfSection?.candidates ?? []).find((c) => c.id === e.detail.scenarioId);
    if (!candidate || !snapshotDate) {
      anchorError = '此候選不在 dossier 候選集(live-edge)內,無法錨定';
      return;
    }
    anchorPending = true;
    try {
      const res = await postJudgment(
        buildAnchorJudgment({ stockId, timeframe, snapshotDate, candidate })
      );
      localJudgment = {
        id: res.id,
        as_of: snapshotDate,
        judged_by: 'human',
        accepted: [{ role: 'preferred', anchor_key: candidate.anchor_key }],
        degree_read: null,
        confidence_class: res.confidence_class,
        invalidation: null,
        status: res.status,
        assumption_hash: dossier?.engine?.assumption_hash ?? null,
        engine_version: dossier?.engine?.neely ?? null
      };
      anchorNotice = `已錨定判讀 #${res.id}(${res.confidence_class})`;
    } catch (err) {
      if (err instanceof JudgmentRejectedError) {
        anchorError = `判讀被拒:${err.message}`;
      } else if (err instanceof Error) {
        anchorError = `錨定失敗:${err.message}`;
      } else {
        anchorError = '錨定失敗(未知錯誤)';
      }
    } finally {
      anchorPending = false;
    }
  }

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
    localJudgment = null; // 判讀是 per-timeframe 的
    anchorError = null;
    anchorNotice = null;
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

    {#if anchorNotice}
      <div class="anchor-msg ok" role="status">⚓ {anchorNotice}</div>
    {/if}
    {#if anchorError}
      <div class="anchor-msg err" role="alert">{anchorError}</div>
    {/if}

    {#if state === 'overview'}
      <Overview
        monowaves={neelyMonowaves}
        scenarios={neelyScenarios}
        {asOf}
        {ohlcSeries}
        {judgedScenarioId}
        judgment={activeJudgment}
        on:expand={toDetail}
      />
    {:else}
      <Detail
        monowaves={neelyMonowaves}
        scenarios={neelyScenarios}
        {asOf}
        {ohlcSeries}
        {selectedScenarioId}
        {resonance}
        {layers}
        {judgedScenarioId}
        {acceptedScenarioIds}
        judgment={activeJudgment}
        anchorEnabled={!!tfSection?.snapshot_ref}
        {anchorPending}
        on:scenario-select={onScenarioSelect}
        on:anchor={onAnchor}
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
      <TraditionalView traditional={activeTraditional} {asOf} {ohlcSeries} />
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
    max-width: 920px;
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

  .anchor-msg {
    margin: 8px 14px 0;
    padding: 6px 10px;
    font-family: var(--mono);
    font-size: 11px;
    border-radius: 6px;
  }

  .anchor-msg.ok {
    color: var(--wave);
    background: #0c2030;
    border: 1px solid #21466a;
  }

  .anchor-msg.err {
    color: var(--inval);
    background: #1c0f14;
    border: 1px dashed #5a3340;
    white-space: pre-wrap;
  }

  /* TraditionalView 取代了舊 placeholder,其樣式內含 */
</style>
