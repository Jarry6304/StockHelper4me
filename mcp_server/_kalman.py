"""Tool 5 內部演算法:`kalman_trend` — multi-horizon Kalman trend smoothing。

對齊 v3.4 plan §Phase C(2026-05-15)+ v3.33 multi-horizon refactor(2026-05-18)。

v3.33 變動:
  - Output 加 `kalman_by_horizon` 4 entries(short / medium / long / ultra_long),
    每個含 smoothed_price / velocity / uncertainty / regime / Q / halflife_bars
  - 頂層欄(`smoothed_price` / `velocity` / `regime` / `current_price`)保留 backward
    compat,對齊 primary horizon("medium")
  - 加 `cross_horizon_consistency` field 描述 4 horizon regime 一致性
  - 對齊 v3.30 series-last-entry path fix + v3.31 stock_snapshot graceful degradation

設計:
- 走 agg.as_of(stock_id, cores=["kalman_filter_core"])
- 從 indicator_latest 拉 multi-horizon latest state(優先 `horizons` array,fallback `series[-1]`)
- 從 facts 拉 recent regime transition events(只 primary horizon)
- payload ~ 2.0 KB / ~500 tokens(+0.5 KB for kalman_by_horizon)

呼叫端:`mcp_server.tools.data.kalman_trend()`。

Reference:
  - Kalman, R. E. (1960). "A new approach to linear filtering and prediction
    problems." *Trans. ASME — Journal of Basic Engineering*, 82(1), 35–45.
  - Roncalli, T. (2013). *Lectures on Risk Management*. CRC Press, §11.2.
    Q ∈ [1e-5, 1e-3] 對應不同 horizon(本實作擴 [1e-5, 1e-1] 4 horizons)
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


# v3.34(2026-05-18):Kalman P_t|t 自我估計對長 series 過於樂觀(收斂到極小,
# e.g. 3030 ultra_long uncertainty=0.06 對 smoothed=158.59 → deviation 4138σ)。
# 對 LLM 體驗誤導 — 看起來像極端 outlier 但其實是 P 收斂塌掉。
#
# Fix:對 deviation_sigma / uncertainty_band 計算施 1% noise floor —
# `effective_uncertainty = max(uncertainty, |smoothed_price| × 0.01)`,對齊
# Bork & Petersen (2014) R=(0.01·p)² 量綱;同時保留 Rust 端 P_t|t 原值給其他
# consumer(dashboards / 直接讀 indicator_values 的 script)看真實 Kalman 信心。
_UNCERTAINTY_FLOOR_PCT: float = 0.01      # = 1% of smoothed_price


def _effective_uncertainty(uncertainty: float, smoothed_price: float) -> float:
    """對齊 v3.34:1% smoothed_price floor。避免 P 收斂塌掉導致 deviation 飆天。"""
    floor = abs(smoothed_price) * _UNCERTAINTY_FLOOR_PCT
    return max(uncertainty, floor)


def compute_kalman_trend(
    stock_id: str,
    as_of: date,
    *,
    lookback_days: int = 180,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Multi-horizon Kalman trend + 5-class regime(v3.33)。

    Args:
        stock_id:     股票代號(例 "2330")
        as_of:        查詢日
        lookback_days: facts / indicator 期間。預設 180
        database_url: 可選 PG 連線字串

    Returns:
        dict 結構(~2 KB / ~500 tokens):
          {
            "stock_id": "2330",
            "as_of": "2026-05-15",
            "current_price": 1234.5,

            // primary horizon(medium)— backward compat 頂層
            "smoothed_price": 1220.3,
            "trend_velocity": 0.42,
            "uncertainty_band": [1211.8, 1228.8],
            "deviation_sigma": 1.16,
            "regime": "StableUp",
            "regime_label": "穩定上漲",

            // v3.33:per-horizon multi-resolution state
            "primary_horizon": "medium",
            "kalman_by_horizon": {
              "short":      {"Q": 0.1,   "halflife_bars": 31,   "smoothed_price": 1230.5,
                             "velocity": 1.2, "uncertainty": 5.0, "regime": "Accelerating",
                             "regime_label": "加速上漲", "deviation_sigma": 0.8},
              "medium":     {"Q": 0.01,  "halflife_bars": 99,   "smoothed_price": 1220.3, ...},
              "long":       {"Q": 0.001, "halflife_bars": 310,  "smoothed_price": 1180.0, ...},
              "ultra_long": {"Q": 1e-5,  "halflife_bars": 3100, "smoothed_price": 800.0,  ...}
            },
            "cross_horizon_consistency": {
              "all_aligned": false,
              "majority_regime": "StableUp",
              "majority_count": 3,
              "summary": "short/medium/long 三 horizon 一致 StableUp,ultra_long 差異(長期均衡 anchor 舊)"
            },

            "recent_regime_changes": [{"date":..., "from":..., "to":...}, ...],
            "indicator_staleness": {...},
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

    # v3.30:Rust 寫入 indicator_values.value 整個 KalmanFilterOutput
    # (`{stock_id, timeframe, series: [...KalmanPoint], events: [...]}`),
    # 最新 state 在 `series[-1]`。
    # v3.33:加 `horizons: [...]` array,優先讀 horizons,fallback series[-1]
    series = val.get("series") or []
    latest_state = series[-1] if series else val

    # v3.33:解析 horizons array
    horizons_raw = val.get("horizons") or []
    primary_horizon_label = val.get("primary_horizon") or "medium"

    # raw_close v3.26 修:authoritative source 是 price_daily
    from mcp_server._price import fetch_latest_close_for_tool
    price_info = fetch_latest_close_for_tool(stock_id, as_of, database_url=database_url)
    raw_close = price_info["close"] if price_info else float(latest_state.get("raw_close") or 0.0)

    # primary horizon(對齊 backward compat 頂層)
    smoothed_price  = float(latest_state.get("smoothed_price") or 0.0)
    velocity        = float(latest_state.get("velocity") or 0.0)
    uncertainty     = float(latest_state.get("uncertainty") or 0.0)
    regime          = str(latest_state.get("regime") or "Sideway")

    # v3.34:1% smoothed_price floor 避免 P 收斂塌掉
    eff_unc = _effective_uncertainty(uncertainty, smoothed_price)
    band_lo = smoothed_price - eff_unc
    band_hi = smoothed_price + eff_unc
    deviation_sigma = (
        (raw_close - smoothed_price) / eff_unc
        if eff_unc > 1e-9 else 0.0
    )

    # v3.33:per-horizon 摘要(每個 horizon 重算自己的 deviation_sigma)
    kalman_by_horizon = _build_kalman_by_horizon(horizons_raw, raw_close)

    # v3.33:cross-horizon regime consistency(LLM 看 4 horizon regime 一致性)
    consistency = _compute_cross_horizon_consistency(kalman_by_horizon)

    # 2. recent regime transitions(facts table 內 EnteredXxx events;只 primary)
    recent_changes = _extract_recent_regime_changes(snapshot)

    # v3.28:indicator staleness check
    indicator_staleness = _compute_indicator_staleness(indicator, as_of)

    return {
        "stock_id":         stock_id,
        "as_of":            as_of.isoformat(),
        "current_price":    _round(raw_close, 2),
        # primary horizon(backward compat 頂層)
        "smoothed_price":   _round(smoothed_price, 2),
        "trend_velocity":   _round(velocity, 4),
        "uncertainty_band": [_round(band_lo, 2), _round(band_hi, 2)],
        "deviation_sigma":  _round(deviation_sigma, 2),
        "regime":           regime,
        "regime_label":     _REGIME_LABELS.get(regime, regime),
        # v3.33 multi-horizon
        "primary_horizon":            primary_horizon_label,
        "kalman_by_horizon":          kalman_by_horizon,
        "cross_horizon_consistency":  consistency,
        # 共用
        "recent_regime_changes": recent_changes,
        "indicator_staleness":   indicator_staleness,
        "narrative": _compose_narrative(
            stock_id=stock_id, regime=regime,
            smoothed_price=smoothed_price, raw_close=raw_close,
            velocity=velocity, deviation_sigma=deviation_sigma,
            recent_changes=recent_changes,
            consistency=consistency,
        ),
    }


def _build_kalman_by_horizon(horizons_raw: list, raw_close: float) -> dict[str, dict]:
    """v3.33:把 Rust horizons array 轉成 LLM-friendly dict by label。

    每個 horizon entry 結構(來自 Rust `KalmanHorizonOutput`):
      {
        "label": "short",
        "process_noise_q": 0.1,
        "halflife_bars": 31.0,
        "velocity_threshold_pct": 0.005,
        "min_regime_duration_days": 3,
        "series_last": {date, raw_close, smoothed_price, uncertainty, velocity, regime},
        "event_count": 8
      }
    """
    out: dict[str, dict] = {}
    for h in horizons_raw or []:
        label = h.get("label")
        if not label:
            continue
        last = h.get("series_last") or {}
        smoothed = float(last.get("smoothed_price") or 0.0)
        uncertainty = float(last.get("uncertainty") or 0.0)
        velocity = float(last.get("velocity") or 0.0)
        regime = str(last.get("regime") or "Sideway")
        # v3.34:1% smoothed_price floor(同頂層 deviation 算法)
        eff_unc = _effective_uncertainty(uncertainty, smoothed)
        dev = (
            (raw_close - smoothed) / eff_unc
            if eff_unc > 1e-9 else 0.0
        )
        out[label] = {
            "Q":                _round(float(h.get("process_noise_q") or 0.0), 6),
            "halflife_bars":    int(round(float(h.get("halflife_bars") or 0))),
            "smoothed_price":   _round(smoothed, 2),
            "trend_velocity":   _round(velocity, 4),
            "uncertainty":      _round(uncertainty, 2),
            "regime":           regime,
            "regime_label":     _REGIME_LABELS.get(regime, regime),
            "deviation_sigma":  _round(dev, 2),
            "event_count":      int(h.get("event_count") or 0),
        }
    return out


def _compute_cross_horizon_consistency(
    by_horizon: dict[str, dict],
) -> dict[str, Any]:
    """v3.33:4 horizons regime 一致性摘要。

    LLM 看 majority_regime + all_aligned 就知道訊號是否穩固:
      - all_aligned=True:4 個 horizon 同 regime → 強訊號
      - majority_count=3:3 個 horizon 同向 → 一致但有疑問
      - majority_count=2:雙陣營分裂 → 訊號不明
    """
    if not by_horizon:
        return {
            "all_aligned": None, "majority_regime": None,
            "majority_count": 0, "total_horizons": 0,
            "summary": "無 horizon 資料",
        }

    regimes = [h.get("regime", "Sideway") for h in by_horizon.values()]
    total = len(regimes)
    # 數最多的 regime
    counts: dict[str, int] = {}
    for r in regimes:
        counts[r] = counts.get(r, 0) + 1
    majority_regime, majority_count = max(counts.items(), key=lambda kv: kv[1])
    all_aligned = majority_count == total

    if all_aligned:
        label = _REGIME_LABELS.get(majority_regime, majority_regime)
        summary = f"4 horizon 全部 {label} — 訊號高度一致"
    else:
        align_labels = [
            f"{lbl}={_REGIME_LABELS.get(by_horizon[lbl]['regime'], by_horizon[lbl]['regime'])}"
            for lbl in by_horizon
        ]
        summary = (
            f"horizon 分歧({majority_count}/{total} 同向):"
            + " / ".join(align_labels)
        )

    return {
        "all_aligned":      all_aligned,
        "majority_regime":  majority_regime,
        "majority_count":   majority_count,
        "total_horizons":   total,
        "summary":          summary,
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
        "primary_horizon":            None,
        "kalman_by_horizon":          {},
        "cross_horizon_consistency":  {
            "all_aligned": None, "majority_regime": None,
            "majority_count": 0, "total_horizons": 0,
            "summary": "無資料",
        },
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
    consistency: dict[str, Any] | None = None,
) -> str:
    """1-2 句敘述,~200 chars。v3.33 加 cross-horizon consistency 摘要。"""
    label = _REGIME_LABELS.get(regime, regime)
    dev_dir = "高於" if raw_close > smoothed_price else "低於"
    dev_abs = abs(deviation_sigma)
    recent_phrase = ""
    if recent_changes:
        last = recent_changes[0]
        last_label = _REGIME_LABELS.get(str(last.get("to") or ""), last.get("to") or "")
        recent_phrase = f";最近一次 regime 變化 {last['date']} 進入「{last_label}」"

    base = (
        f"{stock_id} Kalman primary(medium horizon)判讀:「{label}」"
        f"(velocity={velocity:+.3f}/day),current 收盤 {dev_dir} smoothed {dev_abs:.2f}σ"
        f"{recent_phrase}。"
    )

    # v3.33 cross-horizon 摘要(若有)
    if consistency and consistency.get("summary"):
        base += f" 跨 horizon 一致性:{consistency['summary']}。"
    return base


def _compute_indicator_staleness(indicator: Any, as_of: date) -> dict[str, Any]:
    """檢查 kalman indicator value_date 距 as_of 多遠;> 7 天標 stale。

    v3.28(2026-05-17):indicator 過期會讓 regime / smoothed_price / velocity 變舊值
    (user bug 報告 3030 stuck "Sideway" 但實際 price 已動)。
    """
    if indicator is None:
        return {"value_date": None, "age_days": None, "is_stale": None,
                "warning": "no kalman_filter_core indicator(尚未跑 tw_cores)"}

    value_date = getattr(indicator, "value_date", None)
    if not isinstance(value_date, date):
        return {"value_date": None, "age_days": None, "is_stale": None,
                "warning": "value_date 缺失或非 date 物件"}

    age_days = (as_of - value_date).days
    is_stale = age_days > 7
    warning = None
    if is_stale:
        warning = (
            f"kalman state 過期 {age_days} 天(value_date={value_date.isoformat()},"
            f"as_of={as_of.isoformat()})— regime / smoothed_price / velocity 可能是舊值"
            f"。請跑 `tw_cores run-all --write` 重算 kalman_filter_core。"
        )
    return {
        "value_date": value_date.isoformat(),
        "age_days":   age_days,
        "is_stale":   is_stale,
        "warning":    warning,
    }
