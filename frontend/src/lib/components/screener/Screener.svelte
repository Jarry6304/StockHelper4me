<script lang="ts">
  import type { ScreenRow, ActiveToolkit } from '$lib/api';
  import { toolkitFactorSummary } from '$lib/screener/factors';
  import ToolkitTabs from './ToolkitTabs.svelte';
  import ControlsBar from './ControlsBar.svelte';
  import ColumnGroupBanner from './ColumnGroupBanner.svelte';
  import ScreenerTable from './ScreenerTable.svelte';
  import { createEventDispatcher } from 'svelte';

  export let toolkit: ActiveToolkit;
  export let date: string;
  export let topN: number;
  export let rows: ScreenRow[];
  export let rankingDate: string | null = null;
  export let showPlaceholderBadge: boolean = false;

  const dispatch = createEventDispatcher<{
    'toolkit-change': { toolkit: ActiveToolkit };
    'date-change': { date: string };
    'top-n-change': { topN: number };
    'disabled-toolkit-click': { toolkit: string; reason: string };
  }>();

  $: factorSummary = toolkitFactorSummary(toolkit);

  function onToolkit(e: CustomEvent<{ toolkit: ActiveToolkit }>) {
    dispatch('toolkit-change', e.detail);
  }
  function onDate(e: CustomEvent<{ date: string }>) {
    dispatch('date-change', e.detail);
  }
  function onTopN(e: CustomEvent<{ topN: number }>) {
    dispatch('top-n-change', e.detail);
  }
  function onDisabled(e: CustomEvent<{ toolkit: string; reason: string }>) {
    dispatch('disabled-toolkit-click', e.detail);
  }
</script>

<section class="screener" aria-label="跨股篩選表 · {toolkit}">
  <div class="controls">
    <ToolkitTabs current={toolkit} on:select={onToolkit} on:disabled-click={onDisabled} />
    <ControlsBar {date} {topN} on:date-change={onDate} on:top-n-change={onTopN} />
  </div>

  {#if rankingDate && rankingDate !== date}
    <div class="ranking-note">
      ⓘ ranking_date = <code>{rankingDate}</code>(&le; date <code>{date}</code>)
    </div>
  {/if}

  <ColumnGroupBanner factorGroupLabel={factorSummary} />

  {#if rows.length === 0}
    <div class="empty">
      <strong>無資料</strong> · ranking_date &le; {date} 在 toolkit <code>{toolkit}</code> 沒有 ranked rows。
      <div class="hint">提示:跑 <code>python src/main.py cross_cores phase 8</code> 重算 ranking。</div>
    </div>
  {:else}
    <ScreenerTable {rows} {toolkit} {showPlaceholderBadge} />
  {/if}
</section>

<style>
  .screener {
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 14px;
    flex-wrap: wrap;
  }

  .ranking-note {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--ink-faint);
    margin-bottom: 12px;
    padding: 6px 10px;
    background: var(--tag-bg);
    border: 1px solid var(--line);
    border-radius: 6px;
  }

  .ranking-note code {
    color: var(--ink);
  }

  .empty {
    background: var(--panel-solid);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 24px;
    color: var(--ink-dim);
    text-align: center;
  }

  .empty strong {
    color: var(--ink);
  }

  .empty .hint {
    margin-top: 8px;
    font-family: var(--mono);
    font-size: 12px;
  }

  .empty code {
    color: var(--wave);
  }
</style>
