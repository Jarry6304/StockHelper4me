"""Point-in-time (PIT) reconstruction layer.

Sibling to bronze/silver/fusion — its job is to provide as-of-T views over Bronze
data, computed as pure functions from raw price_daily + price_adjustment_events
(or Bronze fundamental tables for non-OHLCV PIT views).

Used by `src/forecast/` backtest harness and any future Rust forecast cores that
need an as-of-T OHLCV slice without lookahead.

Do NOT read price_daily_fwd for backtest paths — that table bakes in future
adjustment events.  Use `asof_close_series` / `asof_ohlc` instead.
"""

from pit.ohlcv import asof_close_series, asof_ohlc
from pit._calendar import trading_days_between
from pit.fundamental import asof_revenue, asof_financial, asof_business_indicator

__all__ = [
    "asof_close_series",
    "asof_ohlc",
    "asof_revenue",
    "asof_financial",
    "asof_business_indicator",
    "trading_days_between",
]
