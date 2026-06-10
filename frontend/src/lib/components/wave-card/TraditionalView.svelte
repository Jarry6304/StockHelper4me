<script lang="ts">
  import type { TraditionalForestOutput, TraditionalScenario } from '$lib/api/traditional';
  import {
    extractTradInvalidationLines,
    sortTradScenarios,
    tradRecencyDays,
    tradRecencyTier,
    type TradPivot,
    type TradWaveNode
  } from '$lib/wave/traditional-build';
  import TraditionalChart from './TraditionalChart.svelte';

  export let traditional: TraditionalForestOutput;
  export let asOf: string | null = null;

  let selectedId: string | null = null;
  let layers = { fib: true, waveMarkers: true, invalidation: true };
  /** 預設過濾 365d 外的歷史形態;user 拍版可切「顯示全部」看舊形態。 */
  let showAll = false;

  /**
   * 時間範圍 preset:null = 智慧預設(asOf−540d;選中 stale 形態時窗自動錨定形態本身)。
   * 點 preset → 顯式 asOf 錨定窗(不做形態擴窗);「全部」→ autorange。
   */
  let rangePreset: number | 'auto' | null = null;
  const RANGE_PRESETS: Array<{ label: string; value: number | 'auto' }> = [
    { label: '6m', value: 180 },
    { label: '1.5y', value: 540 },
    { label: '3y', value: 1095 },
    { label: '全部', value: 'auto' }
  ];

  $: scenarios = (traditional.scenario_forest ?? []) as TraditionalScenario[];
  $: pivots = ((traditional as unknown as { pivot_series?: TradPivot[] }).pivot_series ?? []);
  $: allSorted = sortTradScenarios(scenarios, asOf);

  // 分流 recent(tier ≥ 1,即 ≤ 365d)vs old(tier 0)
  $: recentSorted = allSorted.filter(
    (s) => tradRecencyTier(tradRecencyDays(s, asOf)) >= 1
  );
  $: hasRecent = recentSorted.length > 0;
  $: visibleSorted = showAll || !hasRecent ? allSorted : recentSorted;

  // 若全部都是 tier 0(老化形態),記錄最新一條的日期 + 距今天數給警示用
  $: newestStale = !hasRecent && allSorted.length > 0 ? (() => {
    const top = allSorted[0];
    const tree = top.wave_tree as TradWaveNode | undefined;
    const end = tree?.end ?? null;
    if (!end) return null;
    const days = Math.round(tradRecencyDays(top, asOf));
    return { end, days };
  })() : null;

  $: defaultSelected = visibleSorted[0]?.id ?? null;
  $: effectiveSelected = selectedId ?? defaultSelected;
  $: selectedScenario =
    visibleSorted.find((s) => s.id === effectiveSelected) ??
    visibleSorted[0] ??
    allSorted[0] ??
    null;
  $: invalidationLines = extractTradInvalidationLines(selectedScenario);
  $: displayMap = new Map(visibleSorted.map((s, i) => [s.id, `T${i + 1}`]));
  $: selectedDisplay = effectiveSelected ? displayMap.get(effectiveSelected) ?? null : null;

  interface AugmentedRow {
    id: string;
    structureLabel: string;
    direction: string | undefined;
    degree: string | undefined;
    score: number | null;
    start: string | null;
    end: string | null;
    recencyDays: number | null;
    stale: boolean;
    dirSymbol: string;
  }

  function recencyOf(end: string | null): number | null {
    if (!end || !asOf) return null;
    const t = Date.parse(end);
    const a = Date.parse(asOf);
    if (Number.isNaN(t) || Number.isNaN(a)) return null;
    return Math.round((a - t) / 86400000);
  }

  function dirSymbolOf(dir: string | undefined): string {
    if (dir === 'Up') return '↑';
    if (dir === 'Down') return '↓';
    return '·';
  }

  function augment(s: TraditionalScenario): AugmentedRow {
    const tree = s.wave_tree as TradWaveNode | undefined;
    const direction = (s as { direction?: string }).direction;
    const degree = (s as { degree?: string }).degree;
    const score = (s as { preference_score?: number }).preference_score;
    const start = tree?.start ?? null;
    const end = tree?.end ?? null;
    const rec = recencyOf(end);
    const id = typeof (s as { id?: unknown }).id === 'string' ? ((s as { id: string }).id) : '';
    return {
      id,
      structureLabel:
        typeof s.structure_label === 'string' ? s.structure_label : '(無 structure_label)',
      direction,
      degree,
      score: typeof score === 'number' ? score : null,
      start,
      end,
      recencyDays: rec,
      stale: rec !== null && rec > 365,
      dirSymbol: dirSymbolOf(direction)
    };
  }

  $: sortedAugmented = visibleSorted.map(augment);

  function selectScenario(id: string) {
    selectedId = id;
  }

  function handleKey(e: KeyboardEvent, id: string) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      selectScenario(id);
    }
  }
</script>

<section class="trad" aria-label="傳統波浪(Frost & Prechter EWP)">
  <div class="hint">
    傳統(Frost & Prechter EWP)forest:<b>{scenarios.length}</b> 個 scenarios — 與 Neely
    並排,<b>不合成</b>。<code>as_of</code> 僅作用於 Neely 側,傳統永遠取 latest computed。
  </div>

  {#if newestStale}
    <div class="stale-banner" role="alert">
      ⚠ <b>傳統波浪近 1 年無有效形態</b> · 最新一條結尾 <code>{newestStale.end}</code>
      ({newestStale.days}d 前)。
      <div class="stale-detail">
        上游 <code>traditional_core</code> 引擎需要 corrective patterns
        (Flat / Zigzag / Triangle / Combination 的 3-pivot A-B-C 序列)才產 scenario;
        近一年走勢未形成可完整標記的 corrective 形態 → forest 僅含歷史片段。
        全部 <b>{allSorted.length}</b> 條形態均 > 365d,以下按 preference + 結尾日排序;
        圖表視窗已錨定所選形態的時間段(可用上方範圍鈕切回現在)。
      </div>
    </div>
  {:else if !showAll && allSorted.length > visibleSorted.length}
    <div class="filter-banner">
      顯示近期(≤ 365d)<b>{visibleSorted.length}</b> 條,
      隱藏歷史 <b>{allSorted.length - visibleSorted.length}</b> 條。
      <button type="button" class="toggle-btn" on:click={() => (showAll = true)}>
        顯示全部 →
      </button>
    </div>
  {:else if showAll && hasRecent}
    <div class="filter-banner">
      顯示全部 <b>{allSorted.length}</b> 條(含歷史)。
      <button type="button" class="toggle-btn" on:click={() => (showAll = false)}>
        ← 只看近期
      </button>
    </div>
  {/if}

  <div class="body">
    <div class="chart-pane">
      <div class="range-presets" role="group" aria-label="圖表時間範圍">
        <span class="rp-label">範圍</span>
        {#each RANGE_PRESETS as p (p.label)}
          <button
            type="button"
            class="preset"
            class:active={rangePreset === p.value}
            on:click={() => (rangePreset = rangePreset === p.value ? null : p.value)}
          >
            {p.label}
          </button>
        {/each}
        {#if rangePreset === null}
          <span class="rp-auto">自動(錨定所選形態)</span>
        {/if}
      </div>
      <TraditionalChart
        {pivots}
        {selectedScenario}
        {asOf}
        {layers}
        height={300}
        xRangeDaysBack={typeof rangePreset === 'number' ? rangePreset : 540}
        forceAutorange={rangePreset === 'auto'}
        explicitRange={typeof rangePreset === 'number'}
      />
    </div>

    <div class="list-pane">
      <div class="lh">
        <span>傳統情境清單 · scenario_forest</span>
        <span>排序 ▾ preference_score</span>
      </div>

      {#each sortedAugmented as row, i (row.id)}
        <div
          class="scen"
          class:sel={effectiveSelected === row.id}
          role="button"
          tabindex="0"
          aria-pressed={effectiveSelected === row.id}
          on:click={() => selectScenario(row.id)}
          on:keydown={(e) => handleKey(e, row.id)}
        >
          <div class="r1">
            <span class="sid">T{i + 1}</span>
            <span class="dir" data-dir={row.direction}>{row.dirSymbol}</span>
            {#if row.degree}<span class="degree">{row.degree}</span>{/if}
            <span class="pref" title="preference_score(指引 + qualifiers 計分,非機率)">
              pref {row.score === null ? '—' : row.score}
            </span>
          </div>
          <div class="lbl">{row.structureLabel}</div>
          {#if row.start && row.end}
            <div class="time" class:stale={row.stale}>
              <span class="t-date">{row.start}</span>
              <span class="t-arrow">→</span>
              <span class="t-date">{row.end}</span>
              {#if row.recencyDays !== null}<span class="t-rec">· {row.recencyDays}d 前</span>{/if}
            </div>
          {/if}
        </div>
      {/each}

      <div class="footer">
        … {visibleSorted.length}{#if visibleSorted.length !== allSorted.length} / {allSorted.length}{/if}
        條(無 primary 旗標 · 並排不整合 Neely)
      </div>
    </div>
  </div>

  {#if selectedScenario && invalidationLines.length > 0}
    <div class="inval-bar" role="status">
      選中 {selectedDisplay} → 失效條件:
      {#each invalidationLines as t, idx}
        {#if idx > 0} ｜ {/if}
        <b>
          {t.kind === 'PriceBreakBelow' ? `跌破 ${t.price.toFixed(2)}` : `漲破 ${t.price.toFixed(2)}`}
        </b>
      {/each}
      <span class="src">◂ invalidation_triggers[]</span>
    </div>
  {/if}
</section>

<style>
  .trad {
    display: flex;
    flex-direction: column;
  }

  .hint {
    padding: 10px 14px;
    border-bottom: 1px solid var(--line);
    font-size: 11.5px;
    color: var(--ink-faint);
    line-height: 1.5;
  }

  .hint code {
    color: var(--wave);
    background: var(--tag-bg);
    padding: 0 4px;
    border-radius: 3px;
  }

  .stale-banner {
    padding: 10px 14px;
    background: #1c160a;
    border-bottom: 1px solid var(--line);
    border-top: 1px dashed #5a4a2a;
    font-size: 12px;
    color: var(--fib);
    line-height: 1.5;
  }

  .stale-banner b {
    color: #ffd28a;
  }

  .stale-banner code {
    color: #ffd28a;
    background: #2a1f07;
    padding: 0 4px;
    border-radius: 3px;
  }

  .stale-detail {
    margin-top: 6px;
    color: var(--ink-faint);
    font-size: 11.5px;
  }

  .filter-banner {
    padding: 8px 14px;
    background: var(--header-bg);
    border-bottom: 1px solid var(--line);
    font-family: var(--mono);
    font-size: 11.5px;
    color: var(--ink-dim);
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .filter-banner b {
    color: var(--ink);
  }

  .toggle-btn {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--wave);
    background: var(--tag-bg);
    border: 1px solid #21466a;
    border-radius: 6px;
    padding: 3px 10px;
    cursor: pointer;
    margin-left: auto;
  }

  .toggle-btn:hover {
    background: #0c2030;
  }

  .body {
    display: grid;
    grid-template-columns: 1fr 296px;
    gap: 0;
  }

  @media (max-width: 780px) {
    .body {
      grid-template-columns: 1fr;
    }
  }

  .chart-pane {
    padding: 14px;
    border-right: 1px solid var(--line);
  }

  .range-presets {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
    font-family: var(--mono);
    font-size: 10.5px;
    color: var(--ink-faint);
    flex-wrap: wrap;
  }

  .rp-label {
    letter-spacing: 1px;
  }

  .preset {
    font-family: var(--mono);
    font-size: 10.5px;
    color: var(--ink-dim);
    background: var(--tag-bg);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 2px 8px;
    cursor: pointer;
  }

  .preset:hover {
    border-color: #3b6080;
  }

  .preset.active {
    color: var(--wave);
    border-color: #2e6f8c;
    background: #0c2433;
  }

  .rp-auto {
    color: var(--ink-faint);
    font-size: 10px;
  }

  @media (max-width: 780px) {
    .chart-pane {
      border-right: none;
      border-bottom: 1px solid var(--line);
    }
  }

  .list-pane {
    padding: 12px 12px 14px;
    background: var(--header-bg);
    overflow-y: auto;
    max-height: 560px;
  }

  .lh {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-family: var(--mono);
    font-size: 11px;
    color: var(--ink-faint);
    letter-spacing: 1px;
    margin-bottom: 9px;
  }

  .scen {
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 9px 10px;
    margin-bottom: 8px;
    background: var(--tag-bg);
    cursor: pointer;
    transition: border-color 0.12s ease;
  }

  .scen:hover {
    border-color: #3b6080;
  }

  .scen.sel {
    border-color: #2e6f8c;
    background: #0c2433;
    box-shadow: 0 0 0 1px #2e6f8c40;
  }

  .scen:focus-visible {
    outline: 2px solid var(--wave);
    outline-offset: 2px;
  }

  .r1 {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--mono);
    font-size: 11px;
    margin-bottom: 4px;
    flex-wrap: wrap;
  }

  .sid {
    color: var(--ink);
    font-weight: 600;
  }

  .dir {
    font-size: 13px;
    width: 14px;
    text-align: center;
  }

  .dir[data-dir='Up'] {
    color: var(--ok);
  }

  .dir[data-dir='Down'] {
    color: var(--inval);
  }

  .degree {
    color: var(--track2);
    font-size: 10px;
    padding: 1px 6px;
    background: #1c1830;
    border: 1px solid #3a3257;
    border-radius: 10px;
  }

  .pref {
    margin-left: auto;
    color: var(--ink-dim);
    font-size: 10px;
    cursor: help;
  }

  .lbl {
    font-size: 11px;
    color: var(--ink-dim);
    font-family: var(--mono);
    line-height: 1.45;
  }

  .time {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--ink-faint);
    margin-top: 4px;
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
  }

  .time.stale {
    color: var(--fib);
  }

  .t-date {
    color: var(--ink-dim);
  }

  .time.stale .t-date {
    color: var(--fib);
  }

  .t-arrow,
  .t-rec {
    color: var(--ink-faint);
  }

  .footer {
    text-align: center;
    color: var(--ink-faint);
    padding-top: 2px;
    font-family: var(--mono);
    font-size: 10px;
  }

  .inval-bar {
    margin: 0 14px 14px;
    padding: 9px 12px;
    border: 1px dashed #5a3340;
    border-radius: 7px;
    background: #1c0f14;
    font-family: var(--mono);
    font-size: 11.5px;
    color: #ff9aa6;
  }

  .inval-bar b {
    color: #ffd0d6;
    font-weight: 600;
  }

  .inval-bar .src {
    color: #a06b73;
    margin-left: 6px;
  }
</style>
