"""FastMCP server app + tool registration。

對齊 plan Phase D + MCP v3 重構 + v3.22 B-5 + v3.31 consolidation
(`/root/.claude/plans/hashed-foraging-pixel.md`)。

**Public toolkit(v3.32:4 + 4 cross-stock factor screens = 8 tools)**:
- `neely_forecast`:Neely NEoWave 預測(Tool 1)
- `kalman_trend`:個股 1-D Kalman trend + 5-class regime(Tool 2)
- `magic_formula_screen`:Greenblatt 2005 跨股篩選(Tool 3,cross-stock)
- `stock_snapshot`:6-in-1 基本資料(health + loan + block + risk + market + commodity)
- `monthly_screen`:Toolkit A 月度 — Persistent Mom + Rev Mom + Inst Concert + vol overlay
- `quarterly_screen`:Toolkit B 季度 — F-Score + Low Vol + Industry-Adj GP
- `annual_low_risk_screen`:Toolkit C 年度 — Long-Term Low Vol + Dividend + 12-1 Mom
- `monthly_trigger_scan`:Layer 5 — Positive/Negative trigger overlay(conviction adjustment)

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
        "Taiwan stock M3 aggregation layer + cross-stock factor toolkit。\n\n"
        "**個股 / 整合 tools(4 個)**:\n"
        "  1. `neely_forecast(stock_id, date)` — NEoWave 預測,4 個時間框架。\n"
        "  2. `kalman_trend(stock_id, date)` — 1-D Kalman 趨勢 + 5-class regime。\n"
        "  3. `magic_formula_screen(date, top_n=30)` — Greenblatt 2005 跨股篩選。\n"
        "  4. `stock_snapshot(stock_id, date)` — 6-in-1 基本資料(health + loan + "
        "block + risk + market + commodity)。\n\n"
        "**v3.32 Cross-Stock Factor Screens(4 toolkit,對齊量化研究)**:\n"
        "  5. `monthly_screen(date, top_n=30)` — Toolkit A 月度:"
        "Persistent Momentum(Chen-Chou-Hsieh 2023)+ Revenue Momentum"
        "(Hung-Lu-Yang 2025)+ Institutional Concert(Sias 2004)+ vol "
        "overlay(Barroso-Santa-Clara 2015)。\n"
        "  6. `quarterly_screen(date, top_n=30)` — Toolkit B 季度:"
        "Piotroski F-Score ≥ 7(Piotroski 2000)+ Low Volatility 252d"
        "(Ang 2009)+ Industry-Adjusted GP(Novy-Marx 2013)。\n"
        "  7. `annual_low_risk_screen(date, top_n=30)` — Toolkit C 年度:"
        "Long-Term Low Vol 36M + Dividend Yield(yield trap filter)+ "
        "12-1 Momentum。\n"
        "  8. `monthly_trigger_scan(date)` — Layer 5:Positive(YoY > +30% + "
        "法人買超)/ Negative(YoY < -20% + 法人賣超 > 1%)triggers。\n\n"
        "設計約束:\n"
        "- 所有 tool 強制 as_of date(回測 / 即時同介面)\n"
        "- facts 已過 look-ahead bias 防衛\n"
        "- 4 個 factor screen 各 toolkit graceful degradation(1 factor 壞不影響其他)\n"
        "- 內部處理時間區間 / 數字 / 排序;LLM 只看結論\n"
        "- 量化 toolkit 僅作 screening reference,不替代 walk-forward backtest"
    ),
)


# Public toolkit(v3.32:4 個個股 / 跨股 + 4 個 cross-stock factor screen = 8 tools)
#
# 設計拍版(2026-05-17 v3.32):v3.31 consolidation 砍到 4 個 + v3.32 加 4 個
# cross-stock factor toolkit screens(對齊提案 v1.1 §六)。
# 對齊 plan /root/.claude/plans/hashed-foraging-pixel.md v3.32。
mcp.tool(_data_tools.neely_forecast)              # 預測 1:Neely NEoWave
mcp.tool(_data_tools.kalman_trend)                # 預測 2:Kalman 1-D regime
mcp.tool(_data_tools.magic_formula_screen)        # 跨股預測(Greenblatt 2005)
mcp.tool(_data_tools.stock_snapshot)              # v3.31 6-in-1 基本資料快照

# v3.32:4 個 cross-stock factor toolkit screens
mcp.tool(_data_tools.monthly_screen)              # Toolkit A:Persistent Mom + Rev Mom + Inst Concert + vol overlay
mcp.tool(_data_tools.quarterly_screen)            # Toolkit B:F-Score + Low Vol + Industry-Adj GP
mcp.tool(_data_tools.annual_low_risk_screen)      # Toolkit C:Long-Term Low Vol + Dividend Yield + 12-1 Mom
mcp.tool(_data_tools.monthly_trigger_scan)        # Layer 5:Positive/Negative trigger overlay

# Fusion Layer · Integration 端口 tools(P1.4)
mcp.tool(_data_tools.market_events)               # D 視角:大盤環境事件時間軸(severity filter)
mcp.tool(_data_tools.market_dashboard)            # D 視角:大盤環境快照(7 cores headline metric)
mcp.tool(_data_tools.key_levels)                  # B 視角:個股支撐/壓力(SR + 趨勢線 + Neely Fib)

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
