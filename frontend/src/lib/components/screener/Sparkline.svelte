<script lang="ts">
  /** Sparkline 純 inline SVG — 不需要 chart lib(對齊 plan Phase 4 「不用 Plotly」)。 */
  export let points: number[];
  export let width: number = 46;
  export let height: number = 16;
  export let stroke: string = '#56d4f0';
  export let strokeWidth: number = 1.4;

  $: pathPoints = pointsToPolyline(points, width, height);

  function pointsToPolyline(pts: number[], w: number, h: number): string {
    if (pts.length < 2) return '';
    const stepX = w / (pts.length - 1);
    // y 軸 SVG 由上往下;sparkline 0=底 / 1=頂 → 翻轉
    return pts
      .map((v, i) => {
        const x = (i * stepX).toFixed(1);
        const y = (h - v * (h - 2) - 1).toFixed(1);
        return `${x},${y}`;
      })
      .join(' ');
  }
</script>

{#if points.length >= 2}
  <svg
    {width}
    {height}
    viewBox="0 0 {width} {height}"
    xmlns="http://www.w3.org/2000/svg"
    class="spark"
    role="img"
    aria-label="sparkline"
  >
    <polyline points={pathPoints} fill="none" {stroke} stroke-width={strokeWidth} />
  </svg>
{/if}

<style>
  .spark {
    flex: 0 0 auto;
    display: block;
  }
</style>
