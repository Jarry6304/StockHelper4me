"""Tool 5 內部演算法:`kalman_trend` — 1-D Kalman trend smoothing。

對齊 v3.4 plan §Phase C(2026-05-15)。

設計:
- 走 agg.as_of(stock_id, cores=["kalman_filter_core"])
- 從 indicator_latest 拉 latest smoothed_price / velocity / uncertainty / regime
- 從 facts 拉 recent regime transition events(對齊 lookback_days)
- payload ~ 1.5 KB / ~400 tokens

呼叫端:`mcp_server.tools.data.kalman_trend()`。

Reference:
  - Kalman, R. E. (1960). "A new approach to linear filtering and prediction
    problems." *Trans. ASME — Journal of Basic Engineering*, 82(1), 35–45.
  - Roncalli, T. (2013). *Lectures on Risk Management*. CRC Press, §11.2.
"""

from __future__ import annotations

from datetime import date
from typing import Any

# 5 regime → 中文標籤
_REGIME_LABELS: dict[str, str] = {
    "StableUp":         "穩定上漲",
    "Accelerating":     "加速上漲",
    "Sideway":          "盤整",
    "Decelerating":     "動能消退",
    "StableDown":       "穩定下跌",
    "stable_up":        "穩定上漲",
    "accelerating":     "加速上漲",
    "sideway":          "盤整",
    "decelerating":     "動能消退",
    "stable_down":      "穩定下跌",
}


def compute_kalman_trend(
    stock_id: str,
    as_of: date,
    *,
    lookback_days: int = 180,
    database_url: str | None = None,
) -> dict[str, Any]:
    """1-D Kalman trend + 5-class regime。

    Args:
        stock_id:     股票代號(例 "2330")
        as_of:        查詢日
        lookback_days: facts / indicator 期間。預設 180
        database_url: 可選 PG 連線字串

    Returns:
        dict 結構(~1.5 KB / ~400 tokens):
          {
            "stock_id": "2330",
            "as_of": "2026-05-15",
            "current_price": 1234.5,
            "smoothed_price": 1220.3,
            "trend_velocity": 0.42,
            "uncertainty_band": [1211.8, 1228.8],
            "deviation_sigma": 1.16,
            "regime": "stable_up",
            "regime_label": "穩定上漲",
            "recent_regime_changes": [
              {"date": "2026-05-10", "from": "Sideway", "to": "StableUp"},
              ...
            ],
            "narrative": "..."
          }
    """
    from agg import as_of as agg_as_of

    snapshot = agg_as_of(
        stock_id, as_of,
        cores=["kalman_filter_core"],
        lookback_days=lookback_days,
        include_market=False,
        database_url=database_url,
    )

    # 1. indicator_latest 拉最新一筆 Kalman state
    indicator = _find_kalman_indicator(snapshot)
    if indicator is None:
        return _empty_result(stock_id, as_of, reason="no_kalman_indicator")

    val = indicator.value or {}
    raw_close       = float(val.get("raw_close") or 0.0)
    smoothed_price  = float(val.get("smoothed_price") or 0.0)
    velocity        = float(val.get("velocity") or 0.0)
    uncertainty     = float(val.get("uncertainty") or 0.0)
    regime          = str(val.get("regime") or "Sideway")

    band_lo = smoothed_price - uncertainty
    band_hi = smoothed_price + uncertainty
    deviation_sigma = (
        (raw_close - smoothed_price) / uncertainty
        if uncertainty > 1e-9 else 0.0
    )

    # 2. recent regime transitions(facts table 內 EnteredXxx events)
    recent_changes = _extract_recent_regime_changes(snapshot)

    return {
        "stock_id":         stock_id,
        "as_of":            as_of.isoformat(),
        "current_price":    _round(raw_close, 2),
        "smoothed_price":   _round(smoothed_price, 2),
        "trend_velocity":   _round(velocity, 4),
        "uncertainty_band": [_round(band_lo, 2), _round(band_hi, 2)],
        "deviation_sigma":  _round(deviation_sigma, 2),
        "regime":           regime,
        "regime_label":     _REGIME_LABELS.get(regime, regime),
        "recent_regime_changes": recent_changes,
        "narrative": _compose_narrative(
            stock_id=stock_id, regime=regime,
            smoothed_price=smoothed_price, raw_close=raw_close,
            velocity=velocity, deviation_sigma=deviation_sigma,
            recent_changes=recent_changes,
        ),
    }


def _find_kalman_indicator(snapshot: Any) -> Any | None:
    """snapshot.indicator_latest 內 source_core='kalman_filter_core' 的 row。"""
    for key, row in (snapshot.indicator_latest or {}).items():
        if row.source_core == "kalman_filter_core":
            return row
    return None


def _extract_recent_regime_changes(
    snapshot: Any, *, limit: int = 5,
) -> list[dict[str, Any]]:
    """從 facts 過濾 kalman_filter_core 的 EnteredXxx events(已 sort 過)。"""
    out: list[dict[str, Any]] = []
    for f in snapshot.facts:
        if f.source_core != "kalman_filter_core":
            continue
        md = f.metadata or {}
        out.append({
            "date":          f.fact_date.isoformat(),
            "from":          md.get("from_regime"),
            "to":            md.get("to_regime"),
        })
    # facts 由 agg 已按 fact_date desc 排序;取最近 limit 個
    return out[:limit]


def _empty_result(stock_id: str, as_of: date, *, reason: str) -> dict[str, Any]:
    return {
        "stock_id":         stock_id,
        "as_of":            as_of.isoformat(),
        "current_price":    None,
        "smoothed_price":   None,
        "trend_velocity":   None,
        "uncertainty_band": [None, None],
        "deviation_sigma":  None,
        "regime":           None,
        "regime_label":     None,
        "recent_regime_changes": [],
        "narrative":        f"{stock_id} 無 kalman_filter_core 資料({reason})。"
                            f"請確認 tw_cores run-all 已對 as_of {as_of.isoformat()} 之前跑過。",
    }


def _round(v: float | None, digits: int) -> float | None:
    if v is None:
        return None
    return round(float(v), digits)


def _compose_narrative(
    *, stock_id: str, regime: str, smoothed_price: float, raw_close: float,
    velocity: float, deviation_sigma: float,
    recent_changes: list[dict[str, Any]],
) -> str:
    """1 句敘述,~140 chars。"""
    label = _REGIME_LABELS.get(regime, regime)
    dev_dir = "高於" if raw_close > smoothed_price else "低於"
    dev_abs = abs(deviation_sigma)
    recent_phrase = ""
    if recent_changes:
        last = recent_changes[0]
        last_label = _REGIME_LABELS.get(str(last.get("to") or ""), last.get("to") or "")
        recent_phrase = f";最近一次 regime 變化 {last['date']} 進入「{last_label}」"

    return (
        f"{stock_id} 1-D Kalman 趨勢判讀:目前處於「{label}」(velocity={velocity:+.3f}/day),"
        f"current 收盤 {dev_dir} smoothed {dev_abs:.2f}σ{recent_phrase}。"
    )
