<script lang="ts">
  import type { Certainty } from '$contracts/neely/Certainty';

  export let certainty: Certainty | null;

  const labels: Record<Certainty, string> = {
    Primary: 'Primary',
    Possible: 'Possible',
    Rare: 'Rare',
    MissingWaveBundle: 'Missing?'
  };
</script>

{#if certainty}
  <span class="cert {certainty.toLowerCase()}">{labels[certainty]}</span>
{/if}

<style>
  .cert {
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 10px;
    font-family: var(--mono);
    white-space: nowrap;
  }

  .primary {
    background: #0c3a2c;
    color: #54e0a8;
    border: 1px solid #1c6b4f;
  }

  .possible {
    background: #3a300c;
    color: #f3c463;
    border: 1px solid #6b561c;
  }

  .rare {
    background: #2a2f38;
    color: #9fb0c6;
    border: 1px solid #495567;
  }

  .missingwavebundle {
    background: #2a1f3a;
    color: #c094f0;
    border: 1px solid #56386b;
  }
</style>
