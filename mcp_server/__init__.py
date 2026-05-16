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

# v3.5 R5 C12:連線 single entry 全走 agg._db.get_connection,DELETE mcp_server/_conn.py
# 對齊 dashboards/aggregation.py 同一 sys.path 模式 — 確保從 repo root 跑
# `python -m mcp_server` 時 src/(放 agg/ silver/ bronze/ cross_cores/)+ repo root
# (放 dashboards/)都在 sys.path。
import sys as _sys
from pathlib import Path as _Path

_REPO_ROOT = _Path(__file__).resolve().parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
if str(_SRC_ROOT) not in _sys.path:
    _sys.path.insert(0, str(_SRC_ROOT))
if str(_REPO_ROOT) not in _sys.path:
    _sys.path.insert(0, str(_REPO_ROOT))

__version__ = "0.2.0"
