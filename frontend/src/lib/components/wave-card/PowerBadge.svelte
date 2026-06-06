<script lang="ts">
  import type { PowerRating } from '$contracts/neely/PowerRating';
  import { powerAbsLevel, powerDirection } from '$lib/wave/power';

  export let power: PowerRating;

  $: level = powerAbsLevel(power);
  $: direction = powerDirection(power);
</script>

<span class="badge" data-direction={direction}>Power <b>{level}</b></span>

<style>
  .badge {
    font-family: var(--mono);
    font-size: 11px;
    background: var(--tag-bg);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 3px 9px;
    color: var(--ink-dim);
    white-space: nowrap;
  }

  .badge b {
    color: var(--ink);
    font-weight: 600;
  }

  .badge[data-direction='bullish'] b {
    color: var(--ok);
  }

  .badge[data-direction='bearish'] b {
    color: var(--inval);
  }
</style>
