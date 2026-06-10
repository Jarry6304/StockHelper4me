"""Tool surface façade — 實作在同層域檔,本檔僅 re-export 維持既有 import 路徑。

新增 tool:寫入對應域檔(wave / snapshot / screens / market / levels /
indicators / raw),本檔補一行 import;不要在本檔寫實作。
註冊清單與分類見 mcp_server/server.py 的 mcp.tool() 區塊(單一真相源)。
"""

from __future__ import annotations

from mcp_server.tools._shared import (
    _clamp,
    _parse_date,
    _read_materialized_snapshot,
    _section_error,
)
from mcp_server.tools.indicators import (
    indicator_momentum,
    indicator_pattern,
    indicator_stack,
    indicator_volatility,
    indicator_volume,
    indicators,
)
from mcp_server.tools.levels import key_levels, pattern_scan, stock_levels, stop_loss_calc
from mcp_server.tools.market import market_dashboard, market_events, market_overview
from mcp_server.tools.raw import as_of_snapshot, fetch_ohlc, find_facts, kalman_trend, list_cores
from mcp_server.tools.screens import (
    annual_low_risk_screen,
    magic_formula_screen,
    monthly_screen,
    monthly_trigger_scan,
    quarterly_screen,
)
from mcp_server.tools.snapshot import (
    block_trade_summary,
    commodity_macro_snapshot,
    loan_collateral_snapshot,
    market_context,
    risk_alert_status,
    stock_health,
    stock_snapshot,
)
from mcp_server.tools.wave import (
    dual_track_resonance,
    neely_forecast,
    scan_wave_impulse,
    traditional_wave_forest,
)

__all__ = [
    # _shared(tests 有直取的底線 helper)
    "_clamp", "_parse_date", "_read_materialized_snapshot", "_section_error",
    # wave
    "traditional_wave_forest", "neely_forecast", "scan_wave_impulse", "dual_track_resonance",
    # snapshot
    "stock_health", "market_context", "loan_collateral_snapshot", "block_trade_summary",
    "risk_alert_status", "commodity_macro_snapshot", "stock_snapshot",
    # screens
    "magic_formula_screen", "monthly_screen", "quarterly_screen",
    "annual_low_risk_screen", "monthly_trigger_scan",
    # market
    "market_events", "market_dashboard", "market_overview",
    # levels
    "key_levels", "stop_loss_calc", "pattern_scan", "stock_levels",
    # indicators
    "indicator_momentum", "indicator_volatility", "indicator_volume",
    "indicator_pattern", "indicator_stack", "indicators",
    # raw / hidden
    "as_of_snapshot", "find_facts", "list_cores", "kalman_trend", "fetch_ohlc",
]
