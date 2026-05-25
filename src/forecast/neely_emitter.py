"""neely_fib forward-only forecast emitter.

對齊 user v0.3 區間預測 spec phase 6 + plan 文件 phase 6。

讀最新 structural_snapshots WHERE source_core='neely_core' AND timeframe=daily,
從 primary scenario 的 expected_fib_zones 產 forecast_log row。

關鍵 spec 規則(§「強制規則」):
  - 裁量軌 — 只 forward log,**禁回測**
  - calibrated=False(不可宣稱覆蓋率,fib 帶非統計帶)
  - regime_tag = scenario.pattern_type → 啟動 regime-conditional scorer 分組
  - 不進 fusion gate(只作 decision support)

Picker(對齊 mcp_server/_forecast.py v3.35 degree-aware,但保留簡化版避免
forecast → mcp_server 反向 dep):
  - 排序 by (effective_degree DESC, power_rating_strength DESC, rules_passed DESC)
  - effective_degree 從 wave_tree.start/end span_years 推算(Stage 11 §13.3)

Horizon mapping(對齊 plan phase 6 NeoWave degree → 21/63/126):
  - SubMinuette → 21
  - Minute → 63
  - Minor / Intermediate / Primary / Cycle / Supercycle → 126(cap)
"""

from __future__ import annotations

import json
from datetime import date
from typing import Any

from forecast._db import upsert_forecast


__all__ = ["emit_neely_fib"]


# ─── Degree → horizon mapping ────────────────────────────────────────────────
# 參考 NeoWave §13.3 Degree Ceiling 表
_DEGREE_TO_HORIZON: dict[str, int] = {
    "Subminuette": 21,
    "SubMinuette": 21,
    "Minuette":    21,
    "Minute":      63,
    "Minor":       126,
    "Intermediate": 126,
    "Primary":     126,
    "Cycle":       126,
    "Supercycle":  126,
    "GrandSupercycle": 126,
}

_DEFAULT_HORIZON = 63  # if degree unknown / NULL


def _coerce_date(s: Any) -> date | None:
    if isinstance(s, date):
        return s
    if isinstance(s, str):
        try:
            return date.fromisoformat(s[:10])
        except ValueError:
            return None
    return None


def _scenario_span_days(scenario: dict) -> int | None:
    wt = scenario.get("wave_tree") or {}
    start = _coerce_date(wt.get("start"))
    end = _coerce_date(wt.get("end"))
    if start is None or end is None:
        return None
    delta = (end - start).days
    return delta if delta > 0 else None


def _effective_degree(scenario: dict) -> str | None:
    """Derive NeoWave degree from wave_tree span(對齊 §13.3 Degree Ceiling 表)。"""
    span = _scenario_span_days(scenario)
    if span is None:
        return None
    years = span / 365.0
    if years < 0.3:
        return "Subminuette"
    if years < 1.0:
        return "Minuette"
    if years < 3.0:
        return "Minute"
    if years < 10.0:
        return "Minor"
    if years < 30.0:
        return "Primary"
    if years < 100.0:
        return "Cycle"
    return "Supercycle"


_DEGREE_RANK: dict[str, int] = {
    "Subminuette":     1,  "SubMinuette": 1,  "Minuette": 2,
    "Minute":          3,
    "Minor":           4,  "Intermediate": 4,
    "Primary":         5,  "Cycle": 6,  "Supercycle": 7,  "GrandSupercycle": 8,
}


def _power_rating_strength(p: dict | str | None) -> int:
    """Crude rank — Strong > Moderate > Weak > Unknown."""
    if isinstance(p, dict):
        kind = p.get("kind") or p.get("rating") or ""
    else:
        kind = str(p or "")
    k = kind.lower()
    if "strong" in k:
        return 3
    if "moderate" in k:
        return 2
    if "weak" in k:
        return 1
    return 0


def _pick_primary(forest: list[dict]) -> dict | None:
    """Return the highest-priority scenario per v3.35 picker rules."""
    if not forest:
        return None
    scored = [
        (
            _DEGREE_RANK.get(_effective_degree(s) or "", 0),
            _power_rating_strength(s.get("power_rating")),
            int(s.get("rules_passed_count") or 0),
            s,
        )
        for s in forest
    ]
    scored.sort(key=lambda x: (x[0], x[1], x[2]), reverse=True)
    return scored[0][3]


def _scenario_horizon_days(scenario: dict) -> int:
    degree = _effective_degree(scenario)
    if degree and degree in _DEGREE_TO_HORIZON:
        return _DEGREE_TO_HORIZON[degree]
    return _DEFAULT_HORIZON


# ─── DB lookups ──────────────────────────────────────────────────────────────


def _fetch_latest_neely_snapshot(
    conn,
    stock_id: str,
    asof: date,
    timeframe: str = "daily",
) -> dict | None:
    """讀 structural_snapshots 最後一筆 ≤ asof,parse JSONB 回傳。"""
    sql = """
        SELECT snapshot_date, snapshot
          FROM structural_snapshots
         WHERE stock_id  = %s
           AND core_name = 'neely_core'
           AND timeframe = %s
           AND snapshot_date <= %s
         ORDER BY snapshot_date DESC
         LIMIT 1
    """
    with conn.cursor() as cur:
        cur.execute(sql, (stock_id, timeframe, asof))
        rows = cur.fetchall()
    if not rows:
        return None
    row = rows[0]
    snapshot = row["snapshot"]
    if isinstance(snapshot, str):
        try:
            snapshot = json.loads(snapshot)
        except Exception:
            return None
    return {"snapshot_date": row["snapshot_date"], "snapshot": snapshot}


# ─── Public API ──────────────────────────────────────────────────────────────


def emit_neely_fib(
    conn,
    stock_id: str,
    asof: date,
    *,
    timeframe: str = "daily",
    confidence: float = 0.60,
    overwrite_horizon: int | None = None,
) -> dict[str, Any]:
    """Read latest neely_core snapshot ≤ asof and emit forecast_log rows.

    Args:
        conn: PG conn (dict_row factory)
        stock_id: e.g. "2330"
        asof: forecast_date used for the emitted rows
        timeframe: 'daily' / 'weekly' / 'monthly' (matches structural_snapshots key)
        confidence: NEoWave fib zones are not statistical bands — fixed nominal
                    confidence(default 0.60 ≈ moderate)。calibrated stays False.
        overwrite_horizon: if set, write all zones at this horizon instead of
                           degree-derived(useful for one-off probes)。

    Returns:
        {status, primary_pattern, horizon, zones_emitted, snapshot_date}
    """
    snap_row = _fetch_latest_neely_snapshot(conn, stock_id, asof, timeframe)
    if snap_row is None:
        return {"status": "no_snapshot", "zones_emitted": 0}

    snapshot = snap_row["snapshot"]
    snapshot_date = snap_row["snapshot_date"]

    forest = snapshot.get("scenario_forest") or []
    primary = _pick_primary(forest)
    if primary is None:
        return {"status": "empty_forest", "zones_emitted": 0,
                "snapshot_date": str(snapshot_date)}

    zones = primary.get("expected_fib_zones") or []
    fallback_used = False
    if not zones:
        # v4.11 起 neely_core top-level 加 `flat_fib_zones`:全 forest scenario
        # expected_fib_zones 去重聯集(Fusion Layer P1.1)。primary picker 選的
        # scenario 可能沒填 zones(spec 允許);此時退而求其次用 union 仍可給
        # LLM 看一個粗略 envelope。
        flat = snapshot.get("flat_fib_zones") or []
        if flat:
            zones = flat
            fallback_used = True
    if not zones:
        return {"status": "no_fib_zones", "zones_emitted": 0,
                "snapshot_date": str(snapshot_date)}

    horizon = overwrite_horizon if overwrite_horizon is not None else _scenario_horizon_days(primary)
    pattern_type = primary.get("pattern_type")
    if isinstance(pattern_type, dict):
        # Some patterns are nested e.g. {"Triangle": {"sub_kind": "Contracting"}}
        pattern_type = next(iter(pattern_type.keys()))
    regime_tag = str(pattern_type) if pattern_type else None

    n_emitted = 0
    for zone in zones:
        label = zone.get("label", "")
        low = zone.get("low")
        high = zone.get("high")
        if low is None or high is None:
            continue
        # Hash includes scenario index + zone label for uniqueness within
        # ON CONFLICT (which only keys on source_core,not zone label).
        # We mitigate by writing one row per (stock, asof, horizon, source_core)
        # — the LAST zone wins for that key.  Since fib zones often overlap and
        # we want the LARGEST envelope as the visible interval, take that.
        n_emitted += 1

    # Strategy: emit the OUTER envelope of all fib zones as a single row.
    # 對齊 spec「fib 帶非統計帶」+ forecast_log 唯一鍵限制(stock, date, horizon, source_core)。
    lows = [float(z["low"]) for z in zones if z.get("low") is not None]
    highs = [float(z["high"]) for z in zones if z.get("high") is not None]
    if not lows or not highs:
        return {"status": "malformed_zones", "zones_emitted": 0,
                "snapshot_date": str(snapshot_date)}

    envelope_lower = min(lows)
    envelope_upper = max(highs)
    # Approximate centroid as mean of (low+high)/2 across zones
    midpoints = [(float(z["low"]) + float(z["high"])) / 2 for z in zones
                 if z.get("low") is not None and z.get("high") is not None]
    point = sum(midpoints) / len(midpoints) if midpoints else None

    source_tag = "flat_union" if fallback_used else "primary"
    # internal_only=True 對齊 m3Spec/dual_track_resonance.md §六 — neely_fib 行
    # 為「一行外包絡」壓掉了離散 fib 線資訊,dual_track 軌道一直接讀
    # structural_snapshots(完整未壓縮);本行降級為 audit / 對齊影子,**禁止
    # 上畫面與 MCP 輸出**(B-4 機制丙)。
    upsert_forecast(
        conn,
        {
            "stock_id": stock_id,
            "forecast_date": asof,
            "horizon_days": horizon,
            "lower": round(envelope_lower, 4),
            "upper": round(envelope_upper, 4),
            "point": round(point, 4) if point is not None else None,
            "confidence": confidence,
            "calibrated": False,
            "internal_only": True,
            "source_core": "neely_fib",
            "regime_tag": regime_tag,
            "params_hash": (
                f"neely_fib|n_zones={n_emitted}|"
                f"degree={_effective_degree(primary) or 'unk'}|"
                f"source={source_tag}"
            ),
        },
    )
    return {
        "status": "written",
        "primary_pattern": regime_tag,
        "horizon_days": horizon,
        "zones_emitted": n_emitted,
        "envelope": (round(envelope_lower, 4), round(envelope_upper, 4)),
        "snapshot_date": str(snapshot_date),
        "fallback_to_flat_union": fallback_used,
    }
