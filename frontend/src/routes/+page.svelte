<script lang="ts">
  import { goto } from '$app/navigation';

  // 簡單 landing page,引導去 V1 或 V2。下方僅是示範連結 —
  // 任何有 snapshot 的股號都可看(/stocks/<股號>)。
  const sampleStocks = ['2330', '3030', '3363', '1101', '2454'];

  let stockQuery = '';

  function goStock() {
    const id = stockQuery.trim();
    if (id) void goto(`/stocks/${encodeURIComponent(id)}`);
  }
</script>

<section class="hero">
  <h1>StockHelper4me · Web 原型</h1>
  <p class="lead">
    消費 Golden L3 唯讀 API 的兩個視圖原型 — 個股 wave 卡 + 跨股因子排行。
  </p>
</section>

<section class="cards">
  <article class="card">
    <header>
      <span class="card-mark">▟</span>
      <h2>個股 WAVE 卡</h2>
    </header>
    <p>
      Neely 波浪 ∥ 傳統 並排;forest 無 primary、無百分比、無料時顯式「無法判斷」。
    </p>
    <div class="jump">
      <input
        type="text"
        placeholder="輸入任意股號(如 2317)"
        bind:value={stockQuery}
        on:keydown={(e) => e.key === 'Enter' && goStock()}
        aria-label="股號"
      />
      <button type="button" on:click={goStock}>查波浪 →</button>
    </div>
    <div class="samples">
      {#each sampleStocks as id}
        <a href="/stocks/{id}" class="sample">{id}</a>
      {/each}
      <span class="samples-hint">↑ 示範連結;全市場有 snapshot 的股號都可查</span>
    </div>
  </article>

  <article class="card">
    <header>
      <span class="card-mark">▤</span>
      <h2>跨股篩選表</h2>
    </header>
    <p>
      因子排行(magic_formula / f_score …)+ WAVE 狀態 summary 欄(原型 placeholder)。
    </p>
    <div class="samples">
      <a href="/screens/magic_formula" class="sample">magic_formula</a>
      <a href="/screens/f_score" class="sample">f_score</a>
      <a href="/screens/low_volatility" class="sample">low_volatility</a>
    </div>
  </article>
</section>

<style>
  .hero {
    margin: 32px 0;
  }

  h1 {
    font-size: 24px;
    margin: 0 0 8px;
    letter-spacing: 0.3px;
  }

  .lead {
    color: var(--ink-dim);
    font-family: var(--mono);
    font-size: 13px;
    margin: 0;
  }

  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(380px, 1fr));
    gap: 18px;
    margin-top: 24px;
  }

  .card {
    background: var(--panel-solid);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 20px;
    box-shadow:
      0 18px 40px -24px #000,
      inset 0 1px 0 #ffffff08;
  }

  .card header {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 12px;
  }

  .card-mark {
    color: var(--wave);
    font-size: 18px;
  }

  .card h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 700;
  }

  .card p {
    color: var(--ink-dim);
    font-size: 13px;
    margin: 0 0 16px;
  }

  .jump {
    display: flex;
    gap: 8px;
    margin-bottom: 12px;
  }

  .jump input {
    flex: 1;
    min-width: 0;
    font-family: var(--mono);
    font-size: 13px;
    color: var(--ink);
    background: var(--tag-bg);
    border: 1px solid #21466a;
    border-radius: 6px;
    padding: 6px 10px;
  }

  .jump input:focus-visible {
    outline: 2px solid var(--wave);
    outline-offset: 1px;
  }

  .jump button {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--wave);
    background: #0c2030;
    border: 1px solid #21466a;
    border-radius: 6px;
    padding: 6px 14px;
    cursor: pointer;
  }

  .jump button:hover {
    background: #0e2840;
  }

  .samples {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }

  .samples-hint {
    color: var(--ink-faint);
    font-family: var(--mono);
    font-size: 10.5px;
  }

  .sample {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--wave);
    background: var(--tag-bg);
    border: 1px solid #21466a;
    border-radius: 6px;
    padding: 4px 10px;
  }

  .sample:hover {
    background: #0c2030;
    text-decoration: none;
  }
</style>
