"""judgment forward-only forecast emitter(wave_judgment_loop §8 S3)。

v4.39 起 forward log 切到 judgment:有 active judgment 才發列
(`source_core='judgment'`,依 accepted[preferred] 候選的 expected_fib_zones);
無判讀不發。舊 `neely_fib`(picker 序列)**凍結唯讀** — 寫路徑(picker 函式
+ `emit_neely_fib`)已刪除,歷史列供對照,否則實證量的是 picker 而非判讀。

關鍵 spec 規則(v0.3 區間預測 §「強制規則」,不變):
  - 裁量軌 — 只 forward log,**禁回測**
  - calibrated=False(不可宣稱覆蓋率,fib 帶非統計帶)
  - regime_tag = 候選 pattern_type → regime-conditional scorer 分組
  - internal_only=True:一行外包絡壓掉離散 fib 線,禁上畫面與 MCP
    (dual_track 直讀 wave_judgments / structural_snapshots)

Horizon mapping(對齊 plan phase 6 NeoWave degree → 21/63/126)。
"""

from __future__ import annotations

import json
import logging
from datetime import date
from typing import Any

from forecast._db import upsert_forecast
from fusion._picker import effective_degree as _effective_degree


__all__ = ["emit_judgment_forecast"]

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


def emit_judgment_forecast(
    conn,
    stock_id: str,
    asof: date,
    *,
    timeframe: str = "daily",
    confidence: float = 0.60,
    overwrite_horizon: int | None = None,
    stale_threshold_days: int = _DEFAULT_STALE_THRESHOLD_DAYS,
) -> dict[str, Any]:
    """依 active judgment 發 `source_core='judgment'` forecast_log 列。

    無 active judgment → 不發(`no_active_judgment`;§8:「無 → 不發」)。
    preferred 候選以 anchor_key 對回最新 forest;對不回 → 不發
    (`anchor_not_in_forest`;J2 diff 的責任區,emitter 不代判)。

    Returns:
        {status: "no_active_judgment" | "no_snapshot" | "stale_snapshot" |
                 "anchor_not_in_forest" | "no_fib_zones" | "malformed_zones" |
                 "written", ...}
    """
    from fusion.judgment import fetch_active_judgment, scenario_anchor_key

    judgment = fetch_active_judgment(conn, stock_id=stock_id, timeframe=timeframe)
    if judgment is None:
        return {"status": "no_active_judgment", "zones_emitted": 0}

    accepted = judgment.get("accepted") or []
    preferred_key = next(
        (a.get("anchor_key") for a in accepted
         if isinstance(a, dict) and a.get("role") == "preferred"),
        None,
    )
    if not preferred_key:
        # no_fit 判讀(accepted=[])— 無錨可發
        return {"status": "no_active_judgment", "zones_emitted": 0,
                "judgment_id": judgment.get("id")}

    snap_row = _fetch_latest_neely_snapshot(conn, stock_id, asof, timeframe)
    if snap_row is None:
        return {"status": "no_snapshot", "zones_emitted": 0}

    snapshot = snap_row["snapshot"]
    snapshot_date = snap_row["snapshot_date"]

    # stale snapshot gate(沿用 b1 拍版:跳過寫入 + log 警告)
    if stale_threshold_days > 0 and isinstance(snapshot_date, date):
        age_days = (asof - snapshot_date).days
        if age_days > stale_threshold_days:
            logger.warning(
                "neely_emitter.emit_judgment_forecast: stale snapshot for %s — "
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
    candidate = next(
        (s for s in forest
         if isinstance(s, dict) and scenario_anchor_key(s) == preferred_key),
        None,
    )
    if candidate is None:
        return {"status": "anchor_not_in_forest", "zones_emitted": 0,
                "judgment_id": judgment.get("id"),
                "snapshot_date": str(snapshot_date)}

    zones = candidate.get("expected_fib_zones") or []
    if not zones:
        return {"status": "no_fib_zones", "zones_emitted": 0,
                "judgment_id": judgment.get("id"),
                "snapshot_date": str(snapshot_date)}

    horizon = (
        overwrite_horizon if overwrite_horizon is not None
        else _scenario_horizon_days(candidate)
    )
    pattern_type = candidate.get("pattern_type")
    if isinstance(pattern_type, dict):
        pattern_type = next(iter(pattern_type.keys()))
    regime_tag = str(pattern_type) if pattern_type else None

    # OUTER envelope(單列;對齊「fib 帶非統計帶」+ forecast_log 唯一鍵限制)
    lows = [float(z["low"]) for z in zones if z.get("low") is not None]
    highs = [float(z["high"]) for z in zones if z.get("high") is not None]
    if not lows or not highs:
        return {"status": "malformed_zones", "zones_emitted": 0,
                "snapshot_date": str(snapshot_date)}

    envelope_lower = min(lows)
    envelope_upper = max(highs)
    midpoints = [(float(z["low"]) + float(z["high"])) / 2 for z in zones
                 if z.get("low") is not None and z.get("high") is not None]
    point = sum(midpoints) / len(midpoints) if midpoints else None

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
            "source_core": "judgment",
            "regime_tag": regime_tag,
            "params_hash": (
                f"judgment|id={judgment.get('id')}|tf={timeframe}|"
                f"degree={_effective_degree(candidate) or 'unk'}|"
                f"by={judgment.get('judged_by')}"
            ),
        },
    )
    return {
        "status": "written",
        "judgment_id": judgment.get("id"),
        "judged_by": judgment.get("judged_by"),
        "primary_pattern": regime_tag,
        "horizon_days": horizon,
        "zones_emitted": len(zones),
        "envelope": (round(envelope_lower, 4), round(envelope_upper, 4)),
        "snapshot_date": str(snapshot_date),
    }
