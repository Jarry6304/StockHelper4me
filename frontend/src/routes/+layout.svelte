<script lang="ts">
  import '../app.css';
</script>

<div class="layout">
  <header class="topnav">
    <a href="/" class="brand">
      <span class="brand-mark">▟</span>
      <span class="brand-text">StockHelper4me</span>
      <span class="brand-stamp">WIREFRAME · 原型</span>
    </a>
    <nav class="nav-links">
      <a href="/stocks/2330">個股 WAVE 卡</a>
      <a href="/screens/magic_formula">跨股篩選表</a>
    </nav>
  </header>

  <main>
    <slot />
  </main>
</div>

<style>
  .layout {
    max-width: 1120px;
    margin: 0 auto;
    padding: 24px 28px 64px;
  }

  .topnav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    padding-bottom: 16px;
    border-bottom: 1px solid var(--line);
    margin-bottom: 20px;
    flex-wrap: wrap;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    text-decoration: none;
    color: var(--ink);
  }

  .brand:hover {
    text-decoration: none;
  }

  .brand-mark {
    color: var(--wave);
    font-size: 20px;
  }

  .brand-text {
    font-size: 17px;
    font-weight: 700;
    letter-spacing: 0.3px;
  }

  .brand-stamp {
    font-family: var(--mono);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 2px;
    color: var(--stamp);
    border: 1.5px dashed var(--stamp);
    border-radius: 5px;
    padding: 3px 8px;
    transform: rotate(-1.5deg);
  }

  .nav-links {
    display: flex;
    gap: 18px;
    font-family: var(--mono);
    font-size: 12px;
  }

  .nav-links a {
    color: var(--ink-dim);
  }

  .nav-links a:hover {
    color: var(--wave);
    text-decoration: none;
  }

  main {
    min-height: 60vh;
  }
</style>
