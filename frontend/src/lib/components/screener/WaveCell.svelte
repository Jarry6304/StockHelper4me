<script lang="ts">
  import type { WaveDigest } from '$lib/screener/placeholder';
  import CertaintyBadge from '$lib/components/wave-card/CertaintyBadge.svelte';
  import Sparkline from './Sparkline.svelte';
  import DirectionArrow from './DirectionArrow.svelte';

  export let digest: WaveDigest;
  /** dev / ?debug=1 模式才顯示 placeholder 角標。 */
  export let showPlaceholderBadge: boolean = false;

  // 形態年齡 >365d → stale 變暗(鏡射 V1 卡 stale 視覺;CL5 summary-only 不變)
  $: stale = digest.scenarioAgeDays !== null && digest.scenarioAgeDays > 365;
</script>

{#if digest.insufficient}
  <span class="na">— 無法判斷(insufficient_data)</span>
{:else}
  <div class="wv" data-placeholder={digest.isPlaceholder ? 'true' : 'false'}>
    <Sparkline points={digest.sparkline} />
    <span
      class="wlabel"
      class:stale
      title={stale ? `形態結尾 ${digest.scenarioAgeDays}d 前(> 1y,historical anchor)` : undefined}
    >{digest.label}</span>
    <DirectionArrow direction={digest.direction} />
    <span class="wcnt">{digest.scenarioCount}</span>
    <CertaintyBadge certainty={digest.certainty} />
    {#if showPlaceholderBadge && digest.isPlaceholder}
      <span class="ph" title="placeholder data — 等真實 wave-summary 端點(CL4)">PH</span>
    {/if}
  </div>
{/if}

<style>
  .wv {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .wlabel {
    color: var(--wave);
    font-size: 11px;
    font-family: var(--mono);
  }

  .wlabel.stale {
    color: var(--fib);
    opacity: 0.8;
  }

  .wcnt {
    color: var(--ink-faint);
    font-size: 10.5px;
    font-family: var(--mono);
  }

  .na {
    color: var(--new);
    font-size: 11px;
    font-family: var(--mono);
  }

  .ph {
    font-family: var(--mono);
    font-size: 9px;
    padding: 1px 5px;
    border-radius: 3px;
    background: #2a1f07;
    color: var(--new);
    border: 1px dashed #5e4a1e;
    margin-left: 4px;
    letter-spacing: 1px;
  }
</style>
