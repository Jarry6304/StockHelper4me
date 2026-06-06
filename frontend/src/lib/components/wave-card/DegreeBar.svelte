<script lang="ts">
  import type { Degree } from '$contracts/neely/Degree';
  import type { DegreeCeiling } from '$contracts/neely/DegreeCeiling';

  /** 顯示用 degree(若 forest 內無,可傳 ceiling.max_reachable_degree)。 */
  export let degree: Degree | null = null;
  export let ceiling: DegreeCeiling | null = null;
  export let selectedScenarioId: string | null = null;
</script>

<div class="degree">
  Degree:
  {#if degree}
    <b>{degree}</b>
  {:else}
    <span class="faint">N/A</span>
  {/if}
  {#if ceiling}
    <span class="cap" title={ceiling.reason}>▸ ceiling {ceiling.max_reachable_degree}</span>
  {/if}
  {#if selectedScenarioId}
    <span class="sel">· 選中 {selectedScenarioId}</span>
  {/if}
</div>

<style>
  .degree {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--line);
    font-family: var(--mono);
    font-size: 12px;
    color: var(--ink-dim);
    flex-wrap: wrap;
  }

  b {
    color: var(--ink);
    font-weight: 600;
  }

  .faint {
    color: var(--ink-faint);
  }

  .cap {
    color: var(--ok);
    cursor: help;
  }

  .sel {
    color: var(--ink-faint);
  }
</style>
