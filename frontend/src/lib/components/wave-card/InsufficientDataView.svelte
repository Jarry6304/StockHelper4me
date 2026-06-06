<script lang="ts">
  /** 觸發原因:資料不足 / compaction timeout / Neely 引擎對近期結構無有效 scenario。 */
  export let reason: 'insufficient_data' | 'compaction_timeout' | 'empty_forest' = 'insufficient_data';
  /** 額外說明文字(可選)。 */
  export let detail: string | null = null;
</script>

<div class="empty" role="alert">
  <div class="big">⚠ 資料不足 · 無法判斷</div>
  <div class="sm">
    {#if reason === 'insufficient_data'}
      insufficient_data = true(歷史過短,大量 candidate 被 reject)
    {:else if reason === 'compaction_timeout'}
      compaction_timeout = true(Three Rounds compaction 超時,無法收斂)
    {:else}
      scenario_forest = []  Neely 對當前結構無有效 scenario
    {/if}
  </div>
  {#if detail}
    <div class="sm">{detail}</div>
  {/if}
  <div class="sm note">→ 不畫任何波浪 / 投影圖,避免假確定性(L6)</div>
</div>

<style>
  .empty {
    margin: 24px 14px;
    padding: 26px;
    border: 1px dashed #5a4a2a;
    border-radius: 8px;
    background: #1c160a;
    text-align: center;
  }

  .big {
    font-family: var(--mono);
    font-size: 13px;
    color: #f3c463;
    letter-spacing: 1px;
  }

  .sm {
    font-size: 11.5px;
    color: var(--ink-faint);
    margin-top: 6px;
  }

  .note {
    color: var(--fib);
    opacity: 0.65;
  }
</style>
