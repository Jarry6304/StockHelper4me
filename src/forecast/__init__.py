"""Interval-forecast spine.

Provides:
- `forecast_log` table writers/readers (`_db`)
- pinball / sharpness / coverage scorer (`scorer`)
- settlement (resolve_pending) — fills realized_price + hit + pinball_loss
- forecast cores (baseline; later: kalman_cqr, log_channel, neely_fib, fusion)
- backtest harness (`backtest.run_backtest`)

Spec source: user v0.3 interval-forecast spine spec (this session, 2026-05-23).
Plan: /root/.claude/plans/stockhelper4me-serene-thacker.md
"""

from forecast._db import (
    get_connection,
    upsert_forecast,
    fetch_unresolved,
    fetch_resolved,
)
from forecast.scorer import score
from forecast.settlement import resolve_pending
from forecast.baseline import make_baseline_forecast
from forecast.log_channel import make_log_channel_forecast
from forecast.backtest import run_backtest
from forecast.calibration import (
    conformalize_one,
    conformalize_batch,
    nonconformity_score,
    cqr_quantile,
)
from forecast.neely_emitter import emit_neely_fib
from forecast.manual import write_manual_forecast

__all__ = [
    "get_connection",
    "upsert_forecast",
    "fetch_unresolved",
    "fetch_resolved",
    "score",
    "resolve_pending",
    "make_baseline_forecast",
    "make_log_channel_forecast",
    "run_backtest",
    "conformalize_one",
    "conformalize_batch",
    "nonconformity_score",
    "cqr_quantile",
    "emit_neely_fib",
    "write_manual_forecast",
]
