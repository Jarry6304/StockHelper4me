"""StockHelper4me MCP Server.

對齊 plan `/root/.claude/plans/hashed-foraging-pixel.md`(MCP v2 重構)。

把 src/agg/ aggregation layer + dashboards/charts/ plotly figure builders 包成
MCP(Model Context Protocol)server,讓 Claude Desktop 對話內直接 call tools
查資料 + 看套圖。

Transport: stdio(本機 process)。Phase 2 再 lift 同套 tools 到 HTTP transport
給 mobile / 多人共享。

**Public tools(LLM 入口,v2 拍版:3 個高度封裝)**:
- `neely_forecast(stock_id, date)` — NEoWave 4 時間框架預測(月 / 季 / 半年 / 年)
- `stock_health(stock_id, date)` — 個股 4 維健康度評分 + top 5 訊號
- `market_context(date)` — 大盤環境綜合判讀 + systemic risks

**Render tools(視覺輸出 PNG)**:render_kline / render_chip / render_fundamental
/ render_environment / render_neely / render_facts_cloud。

**Hidden / backward-compat**:as_of_snapshot / find_facts / list_cores / fetch_ohlc
(function 留在 `mcp_server.tools.data`,LLM 不可見)。
"""

# 首次 import mcp_server(或其子模組)就觸發 sys.path 設定,讓 `from agg import ...`
# 能正常解析(無論 import 進入點是 server.py 還是 tools.data 直接被 caller import)。
from mcp_server import _conn as _conn  # noqa: F401

__version__ = "0.2.0"
