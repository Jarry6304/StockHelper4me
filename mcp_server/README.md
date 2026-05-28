# MCP Server — StockHelper4me

把 `src/agg/`(read-only aggregation 查詢層)+ `src/fusion/`(跨 cores 整合層)
+ `src/cross_cores/`(跨股 ranking)+ Rust M3 cores facts 包成
[MCP](https://modelcontextprotocol.io/)(Model Context Protocol)server。

對話內讓 Claude 直接 call tools 撈台股資料 / 整合判讀 / 跨股篩選。

**對齊版本**:v4.31(2026-05-29 後);PR #110 主幹 + 後續 v4.30/v4.31 commits。

---

## 安裝

```bash
pip install -e ".[mcp]"
```

DB 連線從 `.env` 取 `DATABASE_URL`(對齊 collector / silver / agg)。

> ⚠️ v3.30(2026-05-17)起 PNG render tools 暫關;**不需裝 kaleido / chromium**。

---

## 啟動 — 一鍵 wrapper(推薦)

對齊 `scripts/refresh_daily.ps1` 同款結構,wrapper 自動處理 venv / .env / UTF-8 /
fastmcp sanity check / setup log:

**Windows**(PowerShell):
```powershell
# Smoke test(啟動 2 秒後 SIGTERM,只確認 server 起得來)
.\scripts\start_mcp.ps1 -Smoke

# Production(stdio mode,給 Claude Desktop 接 stdin/stdout)
.\scripts\start_mcp.ps1
```

**Linux / macOS**(bash):
```bash
./scripts/start_mcp.sh --smoke
./scripts/start_mcp.sh
```

Setup logs(venv activate / .env load / fastmcp version / import check)寫到
`logs/mcp_YYYY-MM-DD.log`,**不會污染 stdout**(MCP stdio 留給 JSON-RPC)。

---

## Claude Desktop 設定

編 `%APPDATA%\Claude\claude_desktop_config.json`(Windows)或
`~/Library/Application Support/Claude/claude_desktop_config.json`(macOS)。

**方案 A — 走 wrapper**(推薦,自動處理 venv + .env):

```json
{
  "mcpServers": {
    "stockhelper": {
      "command": "powershell.exe",
      "args": [
        "-NoProfile", "-ExecutionPolicy", "Bypass",
        "-File", "C:\\path\\to\\StockHelper4me\\scripts\\start_mcp.ps1"
      ]
    }
  }
}
```

macOS / Linux:
```json
{
  "mcpServers": {
    "stockhelper": {
      "command": "/path/to/StockHelper4me/scripts/start_mcp.sh"
    }
  }
}
```

**方案 B — 直接呼 python**(需手動處理 cwd + env):

```json
{
  "mcpServers": {
    "stockhelper": {
      "command": "python",
      "args": ["-m", "mcp_server"],
      "cwd": "C:\\path\\to\\StockHelper4me",
      "env": {
        "DATABASE_URL": "postgresql://twstock:twstock@localhost:5432/twstock"
      }
    }
  }
}
```

方案 A 跟 B 互斥;wrapper(A)推薦,因為 venv 升版 / .env 改路徑時不需動 Desktop config。

重啟 Claude Desktop,左下角 🔌 圖示應出現 `stockhelper`,展開可見 13 個 tools。

---

## Tools(13 個 active,分 4 cohort)

```
個股 / 跨股(4)            cross-stock factor screens(4)
  1. neely_forecast          5. monthly_screen(Toolkit A)
  2. kalman_trend            6. quarterly_screen(Toolkit B)
  3. magic_formula_screen    7. annual_low_risk_screen(Toolkit C)
  4. stock_snapshot          8. monthly_trigger_scan(Layer 5)

Fusion 整合(3,v4.19)      Wave + Resonance(2)
  9. market_overview         12. dual_track_resonance(v4.25)
 10. stock_levels            13. scan_wave_impulse(v4.26)
 11. indicators
```

### A. 個股 / 跨股(4 tools)

| Tool | 簽章 | 用途 + 來源 |
|---|---|---|
| `neely_forecast` | `(stock_id, date)` | NEoWave 5 axis alternation + Three Rounds compaction + 3 horizon(1m/3m/6m)forecast + 主要 scenario forest + invalidation_price + quality_caveat(v3.35.1)。**~3 KB** |
| `kalman_trend` | `(stock_id, date)` | 1-D Kalman LLT + 5-class regime(StableUp / Accelerating / StableDown / Decelerating / Sideway)+ smoothed_price + velocity + deviation_sigma。v3.34 deviation σ floor 1% 避免 long-series uncertainty 塌陷。**~2 KB** |
| `magic_formula_screen` | `(date, top_n=30)` | Greenblatt 2005 跨股 EBIT/EV + ROIC 雙排名;Universe 約 1700 stocks 過排除金融保險 + 公用 + 4 季 EBIT 全填。**~2 KB / 30 stocks** |
| `stock_snapshot` | `(stock_id, date)` | **10-in-1 個股快照**(v3.31 + v4.19):health(4 維) + loan_collateral + block_trade + risk_alert + market_context + commodity_macro + fundamentals + institutional + shareholder + technical_summary;某段失敗 graceful → `{"error", "section"}`。**~10 KB** |

### B. Cross-stock factor screens(4 tools,v3.32)

對齊量化研究 v1.1 §六;**僅作 LLM screening reference,不替代 walk-forward backtest**
(對齊 McLean-Pontiff 2016 published factor 平均衰減 58%)。

| Tool | 簽章 | 內含 factors |
|---|---|---|
| `monthly_screen` | `(date, top_n=30)` | **Toolkit A 月度**:Persistent Momentum(Chen-Chou-Hsieh 2023 JFM)+ Revenue Momentum(Hung-Lu-Yang 2025 RQFA)+ Institutional Concert(Sias 2004)+ vol-managed overlay(Barroso-Santa-Clara 2015 JFE) |
| `quarterly_screen` | `(date, top_n=30)` | **Toolkit B 季度**:Piotroski F-Score ≥ 7(Piotroski 2000)+ Low Volatility 252d(Ang 2009 JFE)+ Industry-Adjusted GP(Novy-Marx 2013) |
| `annual_low_risk_screen` | `(date, top_n=30)` | **Toolkit C 年度**:Long-Term Low Vol 36M(Blitz-van Vliet 2007)+ Dividend Yield + yield-trap filter(Boudoukh 2007)+ 12-1 Momentum(Jegadeesh-Titman 1993) |
| `monthly_trigger_scan` | `(date)` | **Layer 5 overlay**:Positive triggers(YoY > +30% + 法人買超)/ Negative triggers(YoY < -20% + 法人賣超 > 1%)。v4.29 Bug A 修法後 positive_total 從 387 假象 → 真實 ~430(對齊 ~19% universe YoY > 30%) |

### C. Fusion Layer consolidated(3 tools,v4.19 整併)

把分散的 10 個 fusion helper 整併成 3 個高密度 LLM 入口。

| Tool | 簽章 | 整併內容 |
|---|---|---|
| `market_overview` | `(date, events_lookback_days=30, severity_min='notable')` | **D 視角大盤**:7 環境 cores dashboard(TAIEX + US market + Fear-Greed + Business indicator + Exchange rate + Market margin + Commodity macro)+ 環境事件時間軸(Drawdown / NewHigh / EnterPanic / Ma20SlopeFlip 等)。預設 severity 過濾 ≥ notable |
| `stock_levels` | `(stock_id, date, entry_price=None)` | **B 視角個股價位**:key_levels(SR + 趨勢線 + Neely Fib,v4.30 後加 Broken trendline roleflip + neely_fib_daily/weekly 區分 + top_n=20 cap)+ K 線型態 + stop_loss/take_profit(僅 entry_price 給才算)。2330 production 約 122 → 20 levels |
| `indicators` | `(stock_id, date, groups, cores, preset, lookback_days=60)` | **E 視角技術指標**:選擇優先序 `cores > groups > preset`;預設 `preset='default'` 5 cores(MACD/RSI/KD/Bollinger/MA);v4.29 Bug B 修 series tail-slice 後 5-core payload ~108 KB(soft WARN 但 < 1MB MCP limit) |

### D. Wave + Resonance(2 tools)

| Tool | 簽章 | 設計 |
|---|---|---|
| `dual_track_resonance` | `(stock_id, date, primary_horizon=63, primary_confidence=0.80)` | **v4.25 雙軌共振決策**:軌道一(neely 結構 fib 線)× 軌道二(fusion / kalman_cqr 統計帶);A-3 失效閘門(現價跌破 invalidation → single_track_mode)+ A-1 三級共振(逐 fib 線:divergence / basic / strong)+ cross_stock is_top_30 旁路升振 + T1 命中 horizon + T2 多 horizon 剖面(21/63/126)。v4.29 Bug C 修 batch query 後 2330 從 timeout > 240s → < 2s |
| `scan_wave_impulse` | `(date, timeframe='daily', top_n=30, include_observe=True)` | **v4.26 跨股 W3 主升段掃選**:neely_core forest 雙軸驗證(wave_tree `W(\d+)` regex + Pass-2 `:L5/:c3` label)+ R/R 1.5 + bullish direction gate + cross_tf_aligned(daily/weekly/monthly 同向)+ Phase enum(CORRECTION_DONE_DOWN candidate / CORRECTION_DONE_UP observe / IMPULSE_COMPLETE warn)。v4.28 calibration buffer 0.05 + RR_MAX 20 cap |

---

## Hidden tools(仍可從 Python 呼,LLM 看不到)

### v3.31 stock_snapshot 內部 helper(6 個)

`stock_health` / `market_context` / `loan_collateral_snapshot` /
`block_trade_summary` / `risk_alert_status` / `commodity_macro_snapshot` →
全部由 `stock_snapshot` 內部呼叫 + graceful degradation 包裝。

### v4.19 fusion 內部 helper(5 個)

`market_events` / `market_dashboard` → `market_overview`
`key_levels` / `pattern_scan` / `stop_loss_calc` → `stock_levels`
`indicator_momentum` / `indicator_volatility` / `indicator_volume` /
`indicator_pattern` / `indicator_stack` → `indicators`

### v3.30 render tools(6 個,暫關)

`render_kline` / `render_chip` / `render_fundamental` / `render_environment` /
`render_neely` / `render_facts_cloud` — PNG 生成 pipeline silent fail
(`outputSchema defined but no structured output returned`)。functions 留在
`mcp_server.tools.render` 給 dashboards 用,**MCP 註冊暫關**;修好後解開
`mcp_server/server.py:161-166` 6 行即可。

---

## 對話內用法範例

啟動 Claude Desktop 對話後,自然語言提問,Claude 自動 dispatch 合適 tool:

```
你:幫我分析 2330 接下來半年的走勢
Claude:(call neely_forecast stock_id="2330" date="2026-05-29")
  primary_scenario = Impulse W3(power=Bullish, degree=Minute);
  3m forecast prob_up=0.62 / range_high [2400, 2650] / range_low [2050, 2200];
  invalidation_price = 1980(跌破此價主場景失效);
  quality_caveat.is_usable = True(degree+span 充足)。
```

```
你:2330 現在能買嗎?
Claude:(call stock_snapshot stock_id="2330" date="2026-05-29")
  health.overall_score = +35:技術 +50(GoldenCross + RSI 升)
  + 籌碼 +20(法人連 3 日買超 + foreign_holding 升 0.4 pct)
  + 估值 -5(PER 78% 分位偏高)+ 基本面 +60(ROE 高 + 營收 YoY +17.5%)。
  loan_collateral.concentration_alert = False;
  risk_alert.severity = None;
  technical_summary 收斂 MACD 多頭 + KD 高檔鈍化。
  建議短期觀察回檔買點 ~2200。
```

```
你:今天大盤如何 + 哪些是月度因子強的?
Claude:(call market_overview date="2026-05-29")
  climate_score = +25 / overall = neutral_bullish;
  components 7/7 OK;systemic_risks = []。
  TAIEX 偏多(+30)+ business +40,但 Fear-Greed 72(貪婪)+ TAIEX RSI 65
  → 短期注意修正。

Claude:(call monthly_screen date="2026-05-29")
  factors = {persistent_momentum, revenue_momentum, institutional_concert};
  top 5:2330(三 factor 全名)/ 2603(rev mom + inst concert)/
  6789 / 1234 / 5678(各 2-3 factor 命中)。
```

```
你:2330 現在的關鍵價位是什麼?
Claude:(call stock_levels stock_id="2330" date="2026-05-29")
  level_count = 20(從 122 cluster 後取 top by strength × member_count);
  source_point_count = 214(2 SR + 50 trendline_historical + 162 neely_fib);
  支撐 cluster:[2050, 2080, 2120, 2200];
  壓力 cluster:[2380, 2450, 2520, 2650]。
  Broken trendline roleflip(v4.30 新)— 2380 / 2520 為前壓力轉支撐 hint。
```

```
你:2330 雙軌共振判什麼?
Claude:(call dual_track_resonance stock_id="2330" date="2026-05-29")
  軌道一(neely)primary scenario 仍 valid;A-3 閘門未觸發。
  軌道二 horizon 63d band [2150, 2480] confidence 80%;
  A-1 共振:30 個 fib 線中 3 basic + 27 divergence;
  is_top_30 = False(未進 cross_stock 升振);
  primary horizon 63d 結論:basic 共振 + single_track_mode = False
  → 結構偏多 + 統計帶寬於門檻不過嚴。
```

---

## 限制 + Phase 2 follow-up

- ✋ **stdio only** — Desktop / Claude Code CLI 本機跑可。Claude mobile /
  claude.ai web 需 remote HTTP server(Phase 2 follow-up)。
- ✋ **render tools 暫關** — 視覺化由 Streamlit dashboards 提供
  (`dashboards/aggregation.py`)。
- ✋ **指定 as_of 為交易日** — 否則查詢可能 fallback 到上一個 trading day,
  facts staleness 透過各 tool output `*_staleness` 欄揭露(v3.28 後)。
- ✋ **`indicators` 預設 5 cores ~108 KB payload** — soft WARN 但 < 1 MB MCP
  limit。需小 payload 改傳 `lookback_days=20` 或 `cores=['macd_core']`。

Phase 2 計畫:
- HTTP+SSE transport(同套 tools 直接 lift)
- OAuth 認證
- Cloudflare Tunnel host(支援 mobile / 多人共享)
- PNG render pipeline 修復後解開 6 個 render tools

---

## Verify harness

`scripts/verify_mcp_toolkit_v4_29.py` 全覆蓋 13 個 public tool 健康度檢查
(v4.30 stock_levels summary key 修正後):

```powershell
python scripts/verify_mcp_toolkit_v4_29.py                              # 預設 2330 + today
python scripts/verify_mcp_toolkit_v4_29.py --stocks 2330,3030 --verbose
python scripts/verify_mcp_toolkit_v4_29.py --as-of 2026-05-15
```

退碼 0=PASS(可含 WARN)/ 1=FAIL。輸出每 tool:status + elapsed_s + payload_kb
+ per-tool summary。Payload budget:soft `> 50KB` WARN / hard `> 1MB` FAIL。

v4.30 production state:**22 OK + 3 WARN + 0 FAIL of 25**(stocks=2330,3030,1101
× per-stock 6 + market 7;3 WARN 全是 `indicators` payload ~108 KB)。

---

## 開發

```bash
# Unit tests(mock psycopg,不依賴真實 PG)
pytest tests/mcp_server/ tests/fusion/ tests/agg/ -v

# 列 tools 清單(smoke test FastMCP 啟動 + 註冊)
python -c "
import asyncio
from mcp_server.server import mcp
async def show():
    for t in sorted(await mcp.list_tools(), key=lambda x: x.name):
        print(' ', t.name)
asyncio.run(show())
"

# stdio 啟動 smoke(Ctrl+C 中斷)
python -m mcp_server
```

---

## 排錯

- **Desktop 看不到 tools**:檢查 `claude_desktop_config.json` syntax
  (`cwd` 用雙反斜 Windows path / macOS 用 forward slash),Desktop log 在
  `%APPDATA%\Claude\logs\`(或 macOS `~/Library/Logs/Claude/`)有 server stderr。
- **`No module named 'mcp_server'`**:`cwd` 沒設或設錯,Desktop spawn process
  時 cwd 必須是 repo 根目錄(`pip install -e .` 後 `mcp_server` 同樣 importable)。
- **`DATABASE_URL 未設定`**:`.env` 內加 `DATABASE_URL=...`,或在 Desktop config
  的 `env` 欄位設定。
- **`dual_track_resonance` timeout** — v4.29 Bug C 修法後對 2330 < 2s。若仍
  timeout(30s)→ batch query EXPLAIN ANALYZE 看 plan regression。
- **`indicators` 5-core payload 爆 MCP limit** — 改傳 `cores=['macd_core']`
  單核或 `lookback_days=20` 縮 series。
- **某 tool empty / stale** — Bronze / Silver / M3 cores 對應 stock 沒新資料,
  跑 `python src/main.py refresh`(v4.30 後 Silver 7c + M3 cores 全市場
  full-rebuild,daily wall time ~33 min)。

---

## 對齊參考

- **規格**:`m3Spec/dual_track_resonance.md`(雙軌共振)+
  `m3Spec/aggregation_layer.md` r3(per-timeframe lookback)+
  `m3Spec/fusion_layer.md`(D/B/E 視角整併)
- **MCP toolkit 歷程**:`CLAUDE.md` v3.31(consolidation 9 → 4)+ v3.32
  (cross-stock factor screens 4)+ v4.19(fusion 10 → 3)+ v4.25(dual track)+
  v4.26(wave impulse)+ v4.29(Bug A/B/C 修)+ v4.30(stock_levels audit)
- **v4.x bug fixes 對 LLM 體驗影響**:
  - v4.29 Bug A:`monthly_trigger.positive_total` 從 387 假象 → 真實 ~430;
    `stock_snapshot.fundamentals.revenue_yoy` 從 calendar year → 真實 YoY%
  - v4.29 Bug B:`indicators` 5-core payload 910 KB → 109 KB(避 MCP 1MB limit)
  - v4.29 Bug C:`dual_track_resonance` 2330 timeout > 240s → 2s
  - v4.30 Finding 2:`stock_levels` 加 Broken trendline roleflip(2330 0 valid
    → 50 historical SR)
  - v4.30 Finding 4:`stock_levels` top_n=20 cap + neely_fib 區分 timeframe
- **MCP server.py**:v4.x 後 **13 active + 17 hidden**(server.py header
  docstring 完整 list)
- **Aggregation layer**:`src/agg/` + `src/fusion/`
- **Cross-stock builders**:`src/cross_cores/`(Phase 8 排程,Layer 2.5)
