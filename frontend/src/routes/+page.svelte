<script lang="ts">
  // 簡單 landing page,引導去 V1 或 V2。
  const sampleStocks = ['2330', '3030', '3363', '1101', '2454'];
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
    <div class="samples">
      {#each sampleStocks as id}
        <a href="/stocks/{id}" class="sample">{id}</a>
      {/each}
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

  .samples {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
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
