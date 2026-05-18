"""FastMCP server app + tool registration。

對齊 plan Phase D + MCP v3 重構 + v3.22 B-5 + v3.31 consolidation
(`/root/.claude/plans/hashed-foraging-pixel.md`)。

**Public toolkit(v3.31:9 → 4 高度封裝 tools)**:
- `neely_forecast`:Neely NEoWave 預測(Tool 1)
- `kalman_trend`:個股 1-D Kalman trend + 5-class regime(Tool 2)
- `magic_formula_screen`:Greenblatt 2005 跨股篩選(Tool 3,cross-stock)
- `stock_snapshot`:6-in-1 基本資料(health + loan + block + risk + market + commodity)

**Hidden tools(v3.31 從 MCP 隱藏 — 仍可從 Python 直接呼叫供 dashboard 用)**:
- stock_health / market_context / loan_collateral_snapshot /
  block_trade_summary / risk_alert_status / commodity_macro_snapshot
  → 全部進 stock_snapshot 內部呼叫

**Hidden tools(v3.30 暫隱藏 — production silent fail 修好後解開)**:
- render_kline / render_chip / render_fundamental / render_environment /
  render_neely / render_facts_cloud

**Hidden tools(向下兼容,Step 4 才正式從 LLM 介面砍掉)**:
- as_of_snapshot / find_facts / list_cores / fetch_ohlc
"""

from __future__ import annotations

# `from mcp_server import ...` 已透過 __init__.py 觸發 _conn(設 sys.path)
from fastmcp import FastMCP

from mcp_server.tools import data as _data_tools
# v3.30(2026-05-17):render tools 暫隱藏 — production 6 支(render_kline /
# render_chip / render_fundamental / render_environment / render_neely /
# render_facts_cloud)全部回 "outputSchema defined but no structured output
# returned",PNG 生成 pipeline 後端 silent fail。functions 仍留在
# `mcp_server.tools.render` 給 dashboard / direct python 用,只是不曝露 MCP。
# 修好後解開下方 `mcp.tool(_render_tools.*)` 6 行即可。
# from mcp_server.tools import render as _render_tools


mcp = FastMCP(
    name="StockHelper4me",
    instructions=(
        "Taiwan stock M3 aggregation layer。\n\n"
        "**Public toolkit(v3.31:4 個高度封裝 tools)**:\n"
        "  1. `neely_forecast(stock_id, date)` — NEoWave 預測,4 個時間框架"
        "(月/季/半年/年)上漲機率 + 價位區間。\n"
        "  2. `kalman_trend(stock_id, date)` — 1-D Kalman 濾波趨勢平滑 + "
        "5-class regime(stable_up / accelerating / sideway / decelerating / "
        "stable_down)+ recent regime transitions。\n"
        "  3. `magic_formula_screen(date, top_n=30)` — Greenblatt 2005 神奇公式"
        "跨股篩選(排除金融 + 公用後,EBIT/EV + EBIT/IC combined rank top N)。\n"
        "  4. `stock_snapshot(stock_id, date)` — **6-in-1 基本資料當下快照**:\n"
        "     - `health` — 個股 4 維健康度(技術 / 籌碼 / 估值 / 基本面)+ top 5 訊號\n"
        "     - `loan_collateral` — 5 大類借券抵押 + 集中度警示(> 70%)\n"
        "     - `block_trade` — 30 日大宗交易摘要 + 配對交易 spike 日\n"
        "     - `risk_alert` — 處置股當前狀態 + 60 日 escalation 鏈\n"
        "     - `market_context` — 大盤環境(TAIEX / 美股 / VIX / Fear-Greed / 景氣 / 融資 / 商品 / 風險)\n"
        "     - `commodity_macro` — GOLD macro 信號(z-score / momentum / spike)\n\n"
        "設計約束:\n"
        "- 所有 tool 強制 as_of date(回測 / 即時同介面)\n"
        "- facts 已過 look-ahead bias 防衛\n"
        "- stock_snapshot 各 sub-section graceful degradation(1 段壞不影響 5 段)\n"
        "- 內部處理時間區間 / 數字 / 排序;LLM 只看結論"
    ),
)


# Public toolkit(v3.31 consolidation:9 → 4 高度封裝 tools)
#
# 設計拍版(2026-05-17):LLM 只看 4 個 tool 名 + 簡單 args(stock_id / date)。
# 6 個 per-stock / market 基本資料 → 合進 stock_snapshot 1 個 query 拿全。
# 對齊 plan /root/.claude/plans/hashed-foraging-pixel.md v3.31。
mcp.tool(_data_tools.neely_forecast)              # 預測 1:Neely NEoWave
mcp.tool(_data_tools.kalman_trend)                # 預測 2:Kalman 1-D regime
mcp.tool(_data_tools.magic_formula_screen)        # 跨股預測(Greenblatt 2005)
mcp.tool(_data_tools.stock_snapshot)              # v3.31 6-in-1 基本資料快照

# v3.31:以下 6 個仍在 mcp_server.tools.data 內(dashboard / direct python 用),
# 但**不再透過 MCP server.py 註冊**,LLM 看不到。stock_snapshot 內部會呼叫
# 這 6 個 helper 統合輸出。若 user 需要單獨曝露某個,解開對應行即可。
# mcp.tool(_data_tools.stock_health)                # → 進 stock_snapshot.health
# mcp.tool(_data_tools.market_context)              # → 進 stock_snapshot.market_context
# mcp.tool(_data_tools.loan_collateral_snapshot)    # → 進 stock_snapshot.loan_collateral
# mcp.tool(_data_tools.block_trade_summary)         # → 進 stock_snapshot.block_trade
# mcp.tool(_data_tools.risk_alert_status)           # → 進 stock_snapshot.risk_alert
# mcp.tool(_data_tools.commodity_macro_snapshot)    # → 進 stock_snapshot.commodity_macro

# **舊 4 data tools 不再透過 MCP 暴露給 LLM**(as_of_snapshot / find_facts /
# list_cores / fetch_ohlc)— 它們的 function 仍留在 `mcp_server.tools.data` 內,
# 供 dashboard / 既有 unit tests / direct python 呼叫者使用。新 LLM agent 走
# 上方 3 個 public toolkit。

# Render tools(視覺輸出 — PNG image content)
# v3.30(2026-05-17):暫隱藏 — production 6 支全 silent fail
# (outputSchema defined but no structured output returned)。修好後解開:
# mcp.tool(_render_tools.render_kline)
# mcp.tool(_render_tools.render_chip)
# mcp.tool(_render_tools.render_fundamental)
# mcp.tool(_render_tools.render_environment)
# mcp.tool(_render_tools.render_neely)
# mcp.tool(_render_tools.render_facts_cloud)
