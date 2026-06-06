<script lang="ts">
  /** 對映 V2 wireframe 三段 grid header(A 排名身分 / B 因子 / C WAVE+共振 amber)。 */
  export let factorGroupLabel: string = '';
</script>

<div class="grouprow">
  <div class="grp a">A · 排名/身分 ◂ /screens.rows</div>
  <div class="grp b">
    B · 因子欄 ◂ /screens (toolkit){#if factorGroupLabel} · {factorGroupLabel}{/if}
  </div>
  <div class="grp c">C · WAVE + 共振 ◂⚠ <b>需新端點</b>(原型用 placeholder)</div>
</div>

<style>
  .grouprow {
    display: grid;
    grid-template-columns: 282px 260px 1fr;
    gap: 0;
    margin-bottom: -1px;
  }

  @media (max-width: 920px) {
    .grouprow {
      display: none;
    }
  }

  .grp {
    font-family: var(--mono);
    font-size: 10.5px;
    letter-spacing: 1px;
    padding: 6px 12px;
    border: 1px solid var(--line);
    border-bottom: none;
    border-radius: 8px 8px 0 0;
    text-transform: uppercase;
  }

  .grp.a {
    color: var(--ink-faint);
    background: #101d30;
  }

  .grp.b {
    color: var(--ink-faint);
    background: #101d30;
  }

  .grp.c {
    color: var(--new);
    background: var(--amber-banner-bg);
    border-color: #5e4a1e;
  }

  .grp.c b {
    color: #ffd28a;
  }
</style>
