<script lang="ts">
  export let label: string = '';
  export let count: number | string;
  /** 第二個計數(例如 deferred);若有就顯示 N/M。 */
  export let secondary: number | string | null = null;
</script>

<span class="badge">
  {#if label}<span class="lbl">{label}</span>{/if}
  <b>{count}</b>{#if secondary !== null}<span class="sep">/{secondary}</span>{/if}
</span>

<style>
  .badge {
    font-family: var(--mono);
    font-size: 11px;
    background: var(--tag-bg);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 3px 9px;
    color: var(--ink-dim);
    display: inline-flex;
    align-items: baseline;
    gap: 4px;
    white-space: nowrap;
  }

  .lbl {
    color: var(--ink-dim);
  }

  b {
    color: var(--ink);
    font-weight: 600;
  }

  .sep {
    color: var(--ink-faint);
  }
</style>
