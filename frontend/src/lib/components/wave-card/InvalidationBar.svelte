<script lang="ts">
  import type { Trigger } from '$contracts/neely/Trigger';

  export let triggers: Trigger[];
  export let selectedScenarioId: string | null = null;

  // 過濾出 InvalidateScenario 類型(其他 Weaken/Promote 不在 InvalidationBar 顯示)
  $: invalidations = triggers.filter((t) => t.on_trigger === 'InvalidateScenario');

  function describeTrigger(t: Trigger): string {
    const tt = t.trigger_type;
    if (typeof tt === 'object') {
      if ('PriceBreakBelow' in tt) return `跌破 ${tt.PriceBreakBelow.toFixed(2)}`;
      if ('PriceBreakAbove' in tt) return `漲破 ${tt.PriceBreakAbove.toFixed(2)}`;
      if ('TimeExceeds' in tt) return `時間 > ${tt.TimeExceeds} 未轉折`;
      if ('VolumeAnomaly' in tt) return `量能 z > ${tt.VolumeAnomaly.z_threshold}`;
      if ('OverlapWith' in tt) return `重疊 ${tt.OverlapWith.wave_id}`;
    }
    return String(tt);
  }
</script>

{#if invalidations.length > 0}
  <div class="inval-bar" role="status">
    {#if selectedScenarioId}選中 {selectedScenarioId} → {/if}失效條件:
    {#each invalidations as t, i}
      {#if i > 0} ｜ {/if}
      <b>{describeTrigger(t)}</b>
    {/each}
    <span class="src">◂ invalidation_triggers[]</span>
  </div>
{/if}

<style>
  .inval-bar {
    margin: 0 14px 14px;
    padding: 9px 12px;
    border: 1px dashed #5a3340;
    border-radius: 7px;
    background: #1c0f14;
    font-family: var(--mono);
    font-size: 11.5px;
    color: #ff9aa6;
  }

  b {
    color: #ffd0d6;
    font-weight: 600;
  }

  .src {
    color: #a06b73;
    margin-left: 6px;
  }
</style>
