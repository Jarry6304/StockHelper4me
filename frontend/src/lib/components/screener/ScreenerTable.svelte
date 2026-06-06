<script lang="ts">
  import type { ScreenRow } from '$lib/api';
  import { goto } from '$app/navigation';
  import { getWaveDigest } from '$lib/screener/placeholder';
  import { factorColumnsFor, formatFactor } from '$lib/screener/factors';
  import type { ActiveToolkit } from '$lib/api';
  import WaveCell from './WaveCell.svelte';
  import ResonanceBadge from './ResonanceBadge.svelte';

  export let rows: ScreenRow[];
  export let toolkit: ActiveToolkit;
  /** dev / ?debug=1 mode — 顯示 placeholder 角標。 */
  export let showPlaceholderBadge: boolean = false;

  $: factorCols = factorColumnsFor(toolkit);
  $: digests = new Map(rows.map((r) => [r.stock_id, getWaveDigest(r.stock_id)]));

  function topThirty(row: ScreenRow): boolean {
    // 後端 v4.35 改名 is_top_30 → is_top_n (concept 名稱仍叫 top30)
    return Boolean(row.is_top_n ?? row.is_top_30);
  }

  function drilldown(stockId: string) {
    void goto(`/stocks/${encodeURIComponent(stockId)}?state=detail`);
  }

  function handleKeydown(e: KeyboardEvent, stockId: string) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      drilldown(stockId);
    }
  }

  function stockName(row: ScreenRow): string {
    const n = row.stock_name;
    return typeof n === 'string' ? n : '';
  }
</script>

<div class="card">
  <table>
    <thead>
      <tr>
        <th class="num">#</th>
        <th>代號 / 名稱</th>
        {#each factorCols as col}
          <th class="num">{col.label}</th>
        {/each}
        <th>top30</th>
        <th class="warnhdr vsep">WAVE 狀態</th>
        <th class="warnhdr">共振</th>
        <th></th>
      </tr>
    </thead>
    <tbody>
      {#each rows as row (row.stock_id)}
        {@const digest = digests.get(row.stock_id)}
        <tr
          on:click={() => drilldown(row.stock_id)}
          on:keydown={(e) => handleKeydown(e, row.stock_id)}
          tabindex="0"
          role="button"
          aria-label="下鑽 {row.stock_id}"
        >
          <td class="num rk">{row.combined_rank}</td>
          <td class="stock">
            <span class="tick">{row.stock_id}</span>
            {#if stockName(row)}<small>{stockName(row)}</small>{/if}
          </td>
          {#each factorCols as col}
            <td class="num factor">{formatFactor(row, col)}</td>
          {/each}
          <td>{#if topThirty(row)}<span class="top30">✓</span>{/if}</td>
          <td class="wavecell vsep">
            {#if digest}
              <WaveCell {digest} {showPlaceholderBadge} />
            {/if}
          </td>
          <td class="rescell">
            {#if digest}
              <ResonanceBadge level={digest.insufficient ? 'none' : digest.resonance} />
            {/if}
          </td>
          <td><span class="drill">→</span></td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .card {
    background: var(--panel-solid);
    border: 1px solid var(--line);
    border-radius: 0 12px 12px 12px;
    box-shadow:
      0 18px 40px -24px #000,
      inset 0 1px 0 #ffffff08;
    overflow-x: auto;
  }

  table {
    border-collapse: collapse;
    width: 100%;
    min-width: 920px;
  }

  thead th {
    font-family: var(--mono);
    font-size: 10.5px;
    font-weight: 500;
    letter-spacing: 0.5px;
    color: var(--ink-faint);
    text-align: left;
    padding: 10px 12px;
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
    background: var(--header-bg);
  }

  th.warnhdr,
  td.wavecell,
  td.rescell {
    background: var(--amber-cell-bg);
  }

  th.num,
  td.num {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  td {
    padding: 9px 12px;
    border-bottom: 1px solid #1e2e46;
    font-family: var(--mono);
    font-size: 12px;
    vertical-align: middle;
  }

  td.stock {
    font-family: var(--sans);
  }

  tbody tr {
    cursor: pointer;
  }

  tbody tr:hover td {
    background: var(--row-hover);
  }

  tbody tr:hover td.wavecell,
  tbody tr:hover td.rescell {
    background: var(--row-hover-amber);
  }

  tbody tr:focus-visible {
    outline: 2px solid var(--wave);
    outline-offset: -2px;
  }

  .vsep {
    border-left: 1px solid var(--line);
  }

  .rk {
    color: var(--ink);
    font-weight: 600;
  }

  .top30 {
    color: var(--ok);
  }

  .tick {
    color: var(--ink);
    font-weight: 600;
  }

  .tick + small {
    color: var(--ink-dim);
    font-weight: 400;
    margin-left: 5px;
  }

  .factor {
    color: var(--ink-dim);
  }

  .drill {
    color: var(--ink-faint);
    font-size: 14px;
  }
</style>
