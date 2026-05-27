"""neely_fib forward-only forecast emitter.

對齊 user v0.3 區間預測 spec phase 6 + plan 文件 phase 6。

讀最新 structural_snapshots WHERE source_core='neely_core' AND timeframe=daily,
從 primary scenario 的 expected_fib_zones 產 forecast_log row。

關鍵 spec 規則(§「強制規則」):
  - 裁量軌 — 只 forward log,**禁回測**
  - calibrated=False(不可宣稱覆蓋率,fib 帶非統計帶)
  - regime_tag = scenario.pattern_type → 啟動 regime-conditional scorer 分組
  - 不進 fusion gate(只作 decision support)

Picker(B1 後 — degree / strength / rules 排序 + canonical_is_invalidated filter):
  - 排序 by (degree_rank DESC, power_rating_strength DESC, rules_passed_count DESC)
  - effective_degree / degree_rank / canonical_is_invalidated 全部 import from
    src/fusion/_picker.py(single source,對齊 Rust output.rs::Degree)
  - current_price 提供時:先 canonical_is_invalidated filter 再 sort
  - current_price=None 時:跳過 filter 但 log warning(不靜默放行失效)
  - 寫入面 stale snapshot gate:snapshot_date 距 asof > 7 calendar days → skip
    write + log warning(對齊 user 拍版 b1 spec)

Horizon mapping(對齊 plan phase 6 NeoWave degree → 21/63/126):
  - SubMinuette → 21(canonical bracket <1y)
  - Minute → 63(1-3y)
  - Minor / Intermediate / Primary / Cycle / Supercycle / GrandSupercycle → 126(cap)
"""

from __future__ import annotations

import json
import logging
from datetime import date
from typing import Any

from forecast._db import upsert_forecast
# B1:degree / picker / invalidation 共用 helpers single source。
from fusion._picker import (
    canonical_is_invalidated,
    degree_rank as _degree_rank,
    effective_degree as _effective_degree,
    power_rating_strength as _power_rating_strength,
)


__all__ = ["emit_neely_fib"]

logger = logging.getLogger("forecast.neely_emitter")


# ─── Degree → horizon mapping ────────────────────────────────────────────────
# 對齊 plan phase 6 + Rust degree/mod.rs::classify_degree producer 死碼:
# classify 永不回 Minuette / Micro / SubMicro,但 map 仍寫死防禦(若 future
# Degree Ceiling override 或外部 caller 傳 raw enum 名稱)。
_DEGREE_TO_HORIZON: dict[str, int] = {
    "SubMicro":         21,
    "Micro":            21,
    "SubMinuette":      21,
    "Minuette":         21,
    "Minute":           63,
    "Minor":            126,
    "Intermediate":     126,
    "Primary":          126,
    "Cycle":            126,
    "Supercycle":       126,
    "GrandSupercycle":  126,
}

_DEFAULT_HORIZON = 63  # if degree unknown / NULL

# B1:user 拍版 stale 門檻 7 calendar days(對齊 v3.28 MCP staleness 警告)
_DEFAULT_STALE_THRESHOLD_DAYS = 7


def _pick_primary(
    forest: list[dict],
    current_price: float | None = None,
) -> dict | None:
    """寫入面 picker — degree-aware sort + 可選 invalidation filter。

    Args:
        forest: scenario list from structural_snapshots
        current_price: 當下 close。提供時:先 canonical_is_invalidated filter
            再 sort;None 時跳過 filter 但 log warning(loud bypass,不靜默)

    Returns:
        primary scenario or None(forest 空 / 全部失效)
    """
    if not forest:
        return None

    # B1 step 1:invalidation filter(若 current_price 可用)
    if current_price is not None:
        candidates = [s for s in forest if not canonical_is_invalidated(s, current_price)]
        if not candidates:
            logger.warning(
                "neely_emitter._pick_primary: all %d scenarios filtered as invalidated "
                "at current_price=%s — returning None(caller should narrate)",
                len(forest), current_price,
            )
            return None
    else:
        logger.warning(
            "neely_emitter._pick_primary: current_price is None — skipping invalidation "
            "filter step(may pick already-invalidated scenario as primary)"
        )
        candidates = forest

    # B1 step 2:degree-aware sort(對齊 v3.35 規則)
    scored = [
        (
            _degree_rank(_effective_degree(s)),
            _power_rating_strength(s.get("power_rating")),
            int(s.get("rules_passed_count") or 0),
            s,
        )
        for s in candidates
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
    current_price: float | None = None,
    stale_threshold_days: int = _DEFAULT_STALE_THRESHOLD_DAYS,
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
        current_price: B1 — 當下 close,用於 _pick_primary 的 canonical
                       invalidation filter。None → 跳過 filter + log warning。
        stale_threshold_days: B1 — snapshot_date 距 asof 超過此天數視為過期 →
                             skip write + log warning。預設 7(對齊 v3.28 MCP
                             staleness)。0 / 負數 → disable gate(intra-day 用)。

    Returns:
        - {status: "no_snapshot" | "stale_snapshot" | "empty_forest" |
                   "no_fib_zones" | "malformed_zones" | "written", ...}
        - "stale_snapshot" 多 fields:snapshot_date / asof / age_days
        - "written" 多 fields:envelope / primary_pattern / horizon_days
    """
    snap_row = _fetch_latest_neely_snapshot(conn, stock_id, asof, timeframe)
    if snap_row is None:
        return {"status": "no_snapshot", "zones_emitted": 0}

    snapshot = snap_row["snapshot"]
    snapshot_date = snap_row["snapshot_date"]

    # B1:stale snapshot gate(user 拍版「跳過寫入 + log 警告」)
    if stale_threshold_days > 0 and isinstance(snapshot_date, date):
        age_days = (asof - snapshot_date).days
        if age_days > stale_threshold_days:
            logger.warning(
                "neely_emitter.emit_neely_fib: stale snapshot for %s — "
                "snapshot_date=%s asof=%s age_days=%d > threshold=%d → skip write",
                stock_id, snapshot_date, asof, age_days, stale_threshold_days,
            )
            return {
                "status": "stale_snapshot",
                "skipped": True,
                "snapshot_date": str(snapshot_date),
                "asof": str(asof),
                "age_days": age_days,
                "stale_threshold_days": stale_threshold_days,
                "zones_emitted": 0,
            }

    forest = snapshot.get("scenario_forest") or []
    primary = _pick_primary(forest, current_price=current_price)
    if primary is None:
        # 區分「forest 空」與「全部 filtered as invalidated」
        status = "all_invalidated" if (forest and current_price is not None) else "empty_forest"
        return {"status": status, "zones_emitted": 0,
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
