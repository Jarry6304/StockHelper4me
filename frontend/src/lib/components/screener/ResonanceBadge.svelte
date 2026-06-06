<script lang="ts">
  import type { ResonanceLevel } from '$lib/screener/placeholder';

  export let level: ResonanceLevel;

  const labels: Record<ResonanceLevel, string> = {
    strong: '強',
    basic: '基本',
    divergence: '背離',
    none: '—'
  };
</script>

<span class="res {level}" aria-label="resonance {level}">{labels[level]}</span>

<style>
  .res {
    font-size: 10.5px;
    padding: 2px 9px;
    border-radius: 12px;
    display: inline-block;
    font-family: var(--mono);
  }

  .strong {
    background: #3a0f15;
    color: #ff8e9a;
    border: 1px solid #6b1f29;
  }

  .basic {
    background: #3a2810;
    color: #f5b86a;
    border: 1px solid #6b4d1c;
  }

  .divergence {
    background: #23292f;
    color: #aab8c9;
    border: 1px solid #41505f;
  }

  .none {
    color: var(--ink-faint);
    background: transparent;
  }
</style>
