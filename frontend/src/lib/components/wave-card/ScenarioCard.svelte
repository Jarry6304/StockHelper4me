<script lang="ts">
  import type { Scenario } from '$contracts/neely/Scenario';
  import { createEventDispatcher } from 'svelte';
  import { powerAbsLevel, powerDirection, scenarioPrimaryCertainty } from '$lib/wave/power';
  import CertaintyBadge from './CertaintyBadge.svelte';

  export let scenario: Scenario;
  export let selected: boolean = false;
  /** scenario 顯示用 id(若 Scenario.id 太抽象,可外傳「S1 / S2 …」)。 */
  export let displayId: string = scenario.id;

  const dispatch = createEventDispatcher<{ select: { scenarioId: string } }>();

  $: certainty = scenarioPrimaryCertainty(scenario);
  $: power = scenario.power_rating;
  $: powerDir = powerDirection(power);

  function handleClick() {
    dispatch('select', { scenarioId: scenario.id });
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      handleClick();
    }
  }
</script>

<div
  class="scen"
  class:sel={selected}
  role="button"
  tabindex="0"
  aria-pressed={selected}
  on:click={handleClick}
  on:keydown={handleKeydown}
>
  <div class="r1">
    <span class="sid">{displayId}</span>
    <CertaintyBadge {certainty} />
    <span class="pw" data-direction={powerDir}>Power {powerAbsLevel(power)}</span>
  </div>
  <div class="lbl">{scenario.structure_label}</div>
  <div class="cnt">
    passed {scenario.rules_passed_count} · deferred {scenario.deferred_rules_count}
    {#if scenario.complexity_level}
      · {scenario.complexity_level}
    {/if}
  </div>
</div>

<style>
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
  }

  .sid {
    color: var(--ink);
    font-weight: 600;
  }

  .pw {
    margin-left: auto;
    font-size: 10px;
    color: var(--ink-dim);
  }

  .pw[data-direction='bullish'] {
    color: var(--ok);
  }

  .pw[data-direction='bearish'] {
    color: var(--inval);
  }

  .lbl {
    font-size: 11px;
    color: var(--ink-dim);
    font-family: var(--mono);
    line-height: 1.45;
  }

  .cnt {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--ink-faint);
    margin-top: 4px;
  }
</style>
