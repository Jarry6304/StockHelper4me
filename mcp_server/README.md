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

## Tools(10 個)

### Data tools(回 JSON)

| Tool | 簽章 | 用途 |
|---|---|---|
| `as_of_snapshot` | `(stock_id, date, lookback_days=90, include_market=True, cores?, timeframes?)` | 主路徑 — 單股 as_of 查詢:facts + indicator_latest + structural + market |
| `find_facts` | `(date, source_core?, kind?)` | 跨股搜尋:當日哪些股票觸發某 fact(對齊 §9.4 use case) |
| `list_cores` | `()` | 列出 23 個 cores(分 Wave / Indicator / Chip / Fundamental / Environment) |
| `fetch_ohlc` | `(stock_id, date, lookback_days=90)` | `price_daily_fwd` 後復權 OHLC 序列 |

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

## 對話內用法範例

啟動 Claude Desktop 對話後,自然語言提問,Claude 自動 dispatch tool:

```
你: 列一下 stockhelper 有哪些 tool

Claude: (call list_cores + 用 metadata 看其他 tool)
  4 data tools + 6 render tools — ...

你: 2330 在 2026-05-13 那天有哪些 facts?

Claude: (call as_of_snapshot stock_id="2330" date="2026-05-13")
  facts 共 X 筆,主要訊號:RsiOversold(rsi_core)/ ... 

你: 把那天的 K-line 畫出來

Claude: (call render_kline stock_id="2330" date="2026-05-13")
  [顯示 PNG]
  K-line + bollinger + MA + facts markers 已顯示。最近收盤 590.0...

你: 那天 neely 的第 0 個 scenario 長什麼樣

Claude: (call render_neely stock_id="2330" date="2026-05-13" scenario_idx=0)
  [顯示 PNG]
  scenario_count=N,選定 scenario_0,monowave_count=12,power_rating=...

你: 找出當天觸發 RsiOversold 的所有股票

Claude: (call find_facts date="2026-05-13" source_core="rsi_core" kind="RsiOversold")
  共 5 檔:1101 / 2317 / 2330 / 2884 / 6505 ...
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
