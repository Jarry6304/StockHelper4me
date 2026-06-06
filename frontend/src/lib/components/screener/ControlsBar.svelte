<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let date: string;
  export let topN: number;

  const dispatch = createEventDispatcher<{
    'date-change': { date: string };
    'top-n-change': { topN: number };
  }>();

  function handleDateChange(e: Event) {
    const v = (e.target as HTMLInputElement).value;
    if (v && v !== date) dispatch('date-change', { date: v });
  }

  function handleTopNChange(e: Event) {
    const v = parseInt((e.target as HTMLInputElement).value, 10);
    if (!Number.isNaN(v) && v > 0 && v !== topN) dispatch('top-n-change', { topN: v });
  }
</script>

<div class="controls">
  <label class="ctl">
    date
    <input type="date" value={date} on:change={handleDateChange} />
  </label>
  <label class="ctl">
    top_n
    <input type="number" value={topN} min="1" max="500" on:change={handleTopNChange} />
  </label>
</div>

<style>
  .controls {
    display: flex;
    gap: 14px;
    align-items: center;
    flex-wrap: wrap;
  }

  .ctl {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--ink-dim);
    display: flex;
    align-items: center;
    gap: 6px;
  }

  input {
    background: var(--tag-bg);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 4px 8px;
    color: var(--ink);
    font-family: var(--mono);
    font-size: 12px;
    color-scheme: dark;
  }

  input[type='number'] {
    width: 70px;
  }

  input:focus {
    outline: 1px solid var(--wave);
    outline-offset: -1px;
  }
</style>
