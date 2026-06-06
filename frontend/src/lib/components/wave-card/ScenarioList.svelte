<script lang="ts">
  import type { Scenario } from '$contracts/neely/Scenario';
  import { createEventDispatcher } from 'svelte';
  import { sortScenarios } from '$lib/wave/power';
  import ScenarioCard from './ScenarioCard.svelte';

  export let scenarios: Scenario[];
  export let selectedId: string | null = null;
  /** 透傳給每張 ScenarioCard,用來算 recency。 */
  export let asOf: string | null = null;

  const dispatch = createEventDispatcher<{ select: { scenarioId: string } }>();

  $: sorted = sortScenarios(scenarios);

  function handleSelect(e: CustomEvent<{ scenarioId: string }>) {
    dispatch('select', e.detail);
  }
</script>

<div class="scenlist">
  <div class="lh">
    <span>情境清單 · scenario_forest</span>
    <span>排序 ▾ power</span>
  </div>

  {#each sorted as scenario, i (scenario.id)}
    <ScenarioCard
      {scenario}
      selected={selectedId === scenario.id}
      displayId={`S${i + 1}`}
      {asOf}
      on:select={handleSelect}
    />
  {/each}

  <div class="footer">
    … 共 {scenarios.length} 條(無 primary 旗標 · 平權)
  </div>
</div>

<style>
  .scenlist {
    padding: 12px 12px 14px;
    background: var(--header-bg);
    overflow-y: auto;
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

  .footer {
    text-align: center;
    color: var(--ink-faint);
    padding-top: 2px;
    font-family: var(--mono);
    font-size: 10px;
  }
</style>
