# MCP Server — StockHelper4me

把 `src/agg/` aggregation 查詢層 + `dashboards/charts/` Plotly figure builders 包成
[MCP](https://modelcontextprotocol.io/)(Model Context Protocol)server。

對話內讓 Claude 直接 call tools 撈資料 / 看套圖。對齊 plan Phase D。

---

## 安裝

```bash
pip install -e ".[mcp]"

# Kaleido 需要 chromium binary 才能匯出 PNG(首次 ~80MB)
plotly_get_chrome -y
```

或一次裝完 dashboard + mcp:

```bash
pip install -e ".[dashboard,mcp]"
plotly_get_chrome -y
```

DB 連線從 `.env` 取 `DATABASE_URL`(對齊 collector / silver / agg)。

---

## Claude Desktop 設定

編 `%APPDATA%\Claude\claude_desktop_config.json`(Windows)或
`~/Library/Application Support/Claude/claude_desktop_config.json`(macOS):

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

> `cwd` 必填(指向本 repo 根目錄)— 對話啟動時 Desktop 會 spawn 該 process,
> 在那 cwd 才能 import `mcp_server`(走 cwd-default-in-sys.path)+ `dashboards`
> + 載 `.env`。

重啟 Claude Desktop,左下角 🔌 圖示應出現 `stockhelper`,展開可見 10 個 tools。

---

## Tools(9 個 — 3 public + 6 render)

> **v2 重構(2026-05-14)**:從 v1 的 4 data + 6 render = 10 tools,改為 **3 個高度
> 封裝 public toolkit + 6 render tools = 9 tools**。對齊 plan
> `/root/.claude/plans/hashed-foraging-pixel.md`:LLM 只看 3 個 tool,內部處理時間
> 區間 / 數字 / 排序,輸出只回結論(每 tool ≤ 5K tokens,3-tool chain ≤ 15K tokens)。
>
> 舊 4 tools(`as_of_snapshot` / `find_facts` / `list_cores` / `fetch_ohlc`)
> **不再對 LLM 暴露**,但 function 留在 `mcp_server.tools.data` 內供 dashboard /
> direct python 呼叫者使用(舊 9 tests 仍 pass)。

### Public toolkit(LLM 入口,3 tools)

| Tool | 簽章 | 用途 + 輸出尺寸 |
|---|---|---|
| `neely_forecast` | `(stock_id, date)` | NEoWave 預測:4 個時間框架(月 / 季 / 半年 / 年)的上漲機率(`prob_up`)+ 價位區間(`range_high` / `range_low`)+ 主要 scenario + invalidation_price。**~2 KB / ~500 tokens** |
| `stock_health` | `(stock_id, date, lookback_days=90)` | 個股 4 維健康度(技術 / 籌碼 / 估值 / 基本面)各 -100~+100 分 + top 5 訊號 + 1 句中文敘述。**~2 KB / ~500 tokens** |
| `market_context` | `(date, lookback_days=60)` | 大盤環境綜合判讀(TAIEX / 美股 + VIX / Fear-Greed / 景氣 / 匯率 / 融資維持率)6 components 分數 + climate_score + systemic_risks。**~1.5 KB / ~400 tokens** |

3 tools 內部各自寫 `_forecast.py` / `_health.py` / `_climate.py`,不抽共用 base
class(對齊 cores_overview §四 / §十四 零耦合 + 不抽象)。跨 cores 加權算分屬
Aggregation Layer 整合層責任(cores_overview §10.0 列為例外)。

### Render tools(回 PNG image + summary dict)

每個 tool 回 `[Image, dict]`:Image 含 base64 PNG bytes,Desktop 直接 inline 顯示;
dict 是 summary metadata(facts_count / 主要 latest 值),純文字 fallback。

| Tool | 簽章 | 內容 |
|---|---|---|
| `render_kline` | `(stock_id, date, lookback_days=90, indicators=["macd","rsi","kd"], with_volume=True, show_bollinger=True, show_ma=True, show_neely_zigzag=True, show_facts_markers=True)` | K-line + bollinger + MA + neely zigzag + 動態 indicator subplots + facts markers |
| `render_chip` | `(stock_id, date, lookback_days=90)` | 5-row 籌碼:institutional / margin / foreign_holding / day_trading / shareholder |
| `render_fundamental` | `(stock_id, date, view="revenue"|"valuation"|"financial", lookback_days=365)` | revenue / valuation / financial 三 view |
| `render_environment` | `(date, view="taiex"|"us_market"|"global", lookback_days=90)` | TAIEX / US market / global 環境面 |
| `render_neely` | `(stock_id, date, scenario_idx=0, lookback_days=180, show_fib_zones=True)` | Neely Wave deep-dive scenario forest |
| `render_facts_cloud` | `(stock_id, date, lookback_days=90, source_cores?)` | facts 散點圖(x=date, y=source_core, color=kind) |

---

## 對話內用法範例(v2 toolkit)

啟動 Claude Desktop 對話後,自然語言提問,Claude 自動 dispatch tool:

```
你: 幫我分析 2330 接下來 1 年的走勢

Claude: (call neely_forecast stock_id="2330" date="2026-05-13")
  primary_scenario = Impulse W3 of 5 (power=Bullish);
  1 年 prob_up=0.55 / range_high [1500, 1800] / range_low [950, 1100];
  invalidation_price = 880(跌破此價主場景失效)...

你: 2330 現在能買嗎?

Claude: (call stock_health stock_id="2330" date="2026-05-13")
  overall_score=+35:技術面 +50(GoldenCross + RSI 升)+ 籌碼面 +20(法人買超),
  但估值 -5(PER 78% 分位偏高)+ 基本面 +60(ROE 高 + 營收 YoY 強)。
  建議短期觀察回檔買點...

你: 今天大盤環境如何?

Claude: (call market_context date="2026-05-13")
  climate_score=+25 / 整體 neutral_bullish;TAIEX 偏多(+30)+ 景氣指標
  改善(+40),但 Fear-Greed 已到貪婪區(72)+ TAIEX RSI 偏高(65),
  短期注意修正。系統性風險:無。

你: 把 2330 那天的 K-line 畫出來

Claude: (call render_kline stock_id="2330" date="2026-05-13")
  [顯示 PNG]
  K-line + bollinger + MA + facts markers 已顯示...
```

---

## 限制 + Phase 2 follow-up

- ✋ **stdio only** — Desktop / Claude Code CLI 本機跑可。Claude mobile / claude.ai web
  需 remote HTTP server(Phase 2 follow-up)。
- ✋ **Image 在 Desktop 才 inline** — claude.ai web MCP 整合較新,image 渲染體驗
  不保證;tools 仍 work,JSON 部分有純文字 fallback。
- ✋ **首次 render 慢** — kaleido 啟動 chromium 約 ~2-3 秒;後續 ~300-800ms。
- ✋ **長 lookback 慢** — render tools 內部 fetch `lookback_days` 範圍 facts,
  facts 表 4.4M+ rows;90 天 OK,365 天請有耐心。

Phase 2 計畫:
- HTTP+SSE transport(同套 tools 直接 lift)
- OAuth 認證
- Cloudflare Tunnel host(支援 mobile / 多人共享)

---

## 開發

```bash
# Unit tests(mock psycopg / kaleido,不依賴真實 PG / chromium)
pytest tests/mcp_server/ -v

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

## 排錯

- **Desktop 看不到 tools**:檢查 `claude_desktop_config.json` syntax(`cwd` 用雙反斜
  Windows path / macOS 用 forward slash),Desktop log 在 `%APPDATA%\Claude\logs\`
  (或 macOS `~/Library/Logs/Claude/`)有 server stderr。
- **`No module named 'mcp_server'`**:`cwd` 沒設或設錯,Desktop spawn process
  時 cwd 必須是 repo 根目錄(`pip install -e .` 後 mcp_server 同樣 importable from anywhere)。
- **`Kaleido 缺 chromium`**:跑 `plotly_get_chrome -y`。
- **`DATABASE_URL 未設定`**:`.env` 內加 `DATABASE_URL=...`,或在 Desktop config
  的 `env` 欄位設定。
- **render 太慢**:縮 `lookback_days`(預設 90 對應 K-line / 散點圖;180 對應 Neely;
  365 對應 fundamental)。

---

## 對齊參考

- 規格:plan `/root/.claude/plans/squishy-foraging-stroustrup.md` Phase D
- Aggregation layer:`m3Spec/aggregation_layer.md` r1 + `src/agg/`
- Plotly figure builders:`dashboards/charts/`(Phase C 落地)
- Streamlit demo:`dashboards/aggregation.py`(同套 charts/ 的 web UI)
