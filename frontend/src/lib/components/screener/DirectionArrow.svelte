<script lang="ts">
  import type { WaveDirection } from '$lib/screener/placeholder';

  export let direction: WaveDirection;

  const symbols: Record<WaveDirection, string> = {
    up: '↑',
    down: '↓',
    flat: '→',
    correction: '↘'
  };
</script>

<span class="dir {direction}" aria-label={direction}>{symbols[direction]}</span>

<style>
  .dir {
    font-size: 13px;
    width: 14px;
    text-align: center;
    display: inline-block;
  }

  .up {
    color: var(--ok);
  }

  .down {
    color: var(--inval);
  }

  .flat {
    color: var(--ink-dim);
  }

  .correction {
    color: var(--basic);
  }
</style>
