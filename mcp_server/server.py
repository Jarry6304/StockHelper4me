"""FastMCP server app + tool registration。

對齊 plan Phase D + MCP v3 重構 plan
(`/root/.claude/plans/hashed-foraging-pixel.md`)。

**Public toolkit v3(LLM 預設曝露,5 個高度封裝 tools)**:
- `neely_forecast`:Neely NEoWave 預測(Tool 1)
- `stock_health`:個股 4 維健康度(Tool 2)
- `market_context`:大盤環境綜合判讀(Tool 3)
- `magic_formula_screen`:Greenblatt 2005 跨股篩選(Tool 4 — v3.4)
- `kalman_trend`:個股 1-D Kalman trend + 5-class regime(Tool 5 — v3.4)

**Render tools(LLM 視覺輸出 PNG + 短摘要)**:
- render_kline / render_chip / render_fundamental / render_environment /
  render_neely / render_facts_cloud

**Hidden tools(向下兼容,Step 4 才正式從 LLM 介面砍掉)**:
- as_of_snapshot / find_facts / list_cores / fetch_ohlc
"""

from __future__ import annotations

# `from mcp_server import ...` 已透過 __init__.py 觸發 _conn(設 sys.path)
from fastmcp import FastMCP

from mcp_server.tools import data as _data_tools
from mcp_server.tools import render as _render_tools


mcp = FastMCP(
    name="StockHelper4me",
    instructions=(
        "Taiwan stock M3 aggregation layer。\n\n"
        "**Public toolkit v3(5 個高度封裝 tools)**:\n"
        "  1. `neely_forecast(stock_id, date)` — NEoWave 預測,4 個時間框架"
        "(月/季/半年/年)上漲機率 + 價位區間。\n"
        "  2. `stock_health(stock_id, date)` — 個股 4 維健康度(技術 / 籌碼 / "
        "估值 / 基本面)+ top 5 訊號 + 1 句敘述。\n"
        "  3. `market_context(date)` — 大盤環境綜合判讀(TAIEX / 美股 / VIX / "
        "Fear-Greed / 景氣 / 融資維持率)+ systemic risks。\n"
        "  4. `magic_formula_screen(date, top_n=30)` — Greenblatt 2005 神奇公式"
        "跨股篩選(排除金融 + 公用後,EBIT/EV + EBIT/IC combined rank top N)。\n"
        "  5. `kalman_trend(stock_id, date)` — 1-D Kalman 濾波趨勢平滑 + "
        "5-class regime(stable_up / accelerating / sideway / decelerating / "
        "stable_down)+ recent regime transitions。\n\n"
        "**Render tools(視覺輸出 PNG + 短摘要)**:render_kline / render_chip / "
        "render_fundamental / render_environment / render_neely / render_facts_cloud。\n\n"
        "設計約束:\n"
        "- 所有 tool 強制 as_of date(回測 / 即時同介面)\n"
        "- facts 已過 look-ahead bias 防衛\n"
        "- 每 tool 輸出限縮 ≤ 5K tokens(MCP context 友善)\n"
        "- 內部處理時間區間 / 數字 / 排序;LLM 只看結論"
    ),
)


# Public toolkit v3(LLM 預設曝露 — 5 高度封裝 tools)
#
# 設計拍版:LLM 只看 5 個 tool 名 + 簡單 args(stock_id / date)。MCP server
# 內部處理時間區間 / 數字 / 排序 / 過濾,輸出只回結論。對齊 plan
# /root/.claude/plans/hashed-foraging-pixel.md。
mcp.tool(_data_tools.neely_forecast)
mcp.tool(_data_tools.stock_health)
mcp.tool(_data_tools.market_context)
mcp.tool(_data_tools.magic_formula_screen)   # v3.4(Greenblatt 2005)
mcp.tool(_data_tools.kalman_trend)           # v3.4(Kalman 1960 1-D smoothing)

# **舊 4 data tools 不再透過 MCP 暴露給 LLM**(as_of_snapshot / find_facts /
# list_cores / fetch_ohlc)— 它們的 function 仍留在 `mcp_server.tools.data` 內,
# 供 dashboard / 既有 unit tests / direct python 呼叫者使用。新 LLM agent 走
# 上方 3 個 public toolkit。

# Render tools(視覺輸出 — PNG image content)
mcp.tool(_render_tools.render_kline)
mcp.tool(_render_tools.render_chip)
mcp.tool(_render_tools.render_fundamental)
mcp.tool(_render_tools.render_environment)
mcp.tool(_render_tools.render_neely)
mcp.tool(_render_tools.render_facts_cloud)
