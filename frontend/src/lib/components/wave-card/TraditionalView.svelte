<script lang="ts">
  import type { TraditionalForestOutput, TraditionalScenario } from '$lib/api/traditional';
  import {
    extractTradInvalidationLines,
    sortTradScenarios,
    type TradPivot,
    type TradWaveNode
  } from '$lib/wave/traditional-build';
  import TraditionalChart from './TraditionalChart.svelte';

  export let traditional: TraditionalForestOutput;
  export let asOf: string | null = null;

  let selectedId: string | null = null;
  let layers = { fib: true, waveMarkers: true, invalidation: true };

  $: scenarios = (traditional.scenario_forest ?? []) as TraditionalScenario[];
  $: pivots = ((traditional as unknown as { pivot_series?: TradPivot[] }).pivot_series ?? []);
  $: sorted = sortTradScenarios(scenarios);
  $: defaultSelected = sorted[0]?.id ?? null;
  $: effectiveSelected = selectedId ?? defaultSelected;
  $: selectedScenario =
    sorted.find((s) => s.id === effectiveSelected) ?? sorted[0] ?? null;
  $: invalidationLines = extractTradInvalidationLines(selectedScenario);
  $: displayMap = new Map(sorted.map((s, i) => [s.id, `T${i + 1}`]));
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

  $: sortedAugmented = sorted.map(augment);

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
    傳統(Frost & Prechter EWP)forest:**{scenarios.length}** 個 scenarios — 與 Neely
    並排,**不合成**。<code>as_of</code> 僅作用於 Neely 側,傳統永遠取 latest computed。
  </div>

  <div class="body">
    <div class="chart-pane">
      <TraditionalChart
        {pivots}
        {selectedScenario}
        {asOf}
        {layers}
        height={300}
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

      <div class="footer">… 共 {scenarios.length} 條(無 primary 旗標 · 並排不整合 Neely)</div>
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
