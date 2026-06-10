"""V2 跨股表 WAVE 欄批次摘要 — 伺服端抽取(對外只回結論,不回 forest)。

對齊 v2-wave 拍版 (a)(2026-06-11,docs/changelog/v2-wave-endpoint.md):
讀既有兩個資料源,每檔抽 8 個欄位,30 檔 2 條 batch SQL:
- `structural_snapshots` neely_core:top scenario(label / certainty / direction)
  + scenario_count + monowave 尾段 sparkline
- `structural_snapshots` resonance_fusion(Golden L3 物化):findings 歸約成
  共振三級 badge(strong / basic / divergence / none)

**top scenario 選法鏡射前端 V1**(`frontend/src/lib/wave/power.ts::pickDefaultScenario`,
含 fb9e166 老化形態修正)— V2 表格 cell 與 V1 個股卡 default focus 必須同一顆
scenario,兩端排序鍵改一邊必同步另一邊:
  1. 未被現價觸發 invalidation 優先
  2. recency tier DESC(≤60d=3 / ≤180d=2 / ≤365d=1 / 其餘 0)
  3. power rank DESC  4. rules_passed_count DESC  5. recency days ASC

若未來本端點變慢(universe 翻倍等),本檔抽取函式可原樣搬進
src/fusion/materialize/ 升級為物化 kind(拍版紀錄的 (b′) 路徑)。
"""

from __future__ import annotations

from datetime import date
from typing import Any

# PowerRating enum 排序鍵(鏡射 frontend power.ts POWER_RANK;正=bullish)
_POWER_RANK: dict[str, int] = {
    "StrongBullish": 3, "Bullish": 2, "SlightBullish": 1,
    "Neutral": 0,
    "SlightBearish": -1, "Bearish": -2, "StrongBearish": -3,
}

# Certainty 排序(鏡射 frontend CERTAINTY_RANK;Primary 最強)
_CERTAINTY_RANK: dict[str, int] = {
    "Primary": 3, "Possible": 2, "Rare": 1, "MissingWaveBundle": 0,
}

_VALID_TIMEFRAMES = ("daily", "weekly", "monthly")

# sparkline 取 monowave 尾段點數(同 placeholder 視覺密度 6-10 點的上界)
_SPARKLINE_POINTS = 10


# ── per-scenario helpers(鏡射 frontend power.ts 同名函式語意)─────────────────

def _power_rank(scenario: dict[str, Any]) -> int:
    return _POWER_RANK.get(str(scenario.get("power_rating")), 0)


def _recency_days(scenario: dict[str, Any], as_of: date) -> float:
    """scenario 結尾距 as_of 天數(無 wave_tree.end → +inf,排最後)。"""
    end = (scenario.get("wave_tree") or {}).get("end")
    if not end:
        return float("inf")
    try:
        end_d = date.fromisoformat(str(end)[:10])
    except ValueError:
        return float("inf")
    return float((as_of - end_d).days)


def _recency_tier(days: float) -> int:
    if days != days or days == float("inf"):  # NaN / inf
        return 0
    if days <= 60:
        return 3
    if days <= 180:
        return 2
    if days <= 365:
        return 1
    return 0


def _is_invalidated(scenario: dict[str, Any], current_price: float | None) -> bool:
    """鏡射 frontend isScenarioInvalidated:只看 InvalidateScenario,0 價 placeholder 忽略。"""
    if current_price is None:
        return False
    for trig in scenario.get("invalidation_triggers") or []:
        if trig.get("on_trigger") != "InvalidateScenario":
            continue
        tt = trig.get("trigger_type")
        if not isinstance(tt, dict):
            continue
        below = tt.get("PriceBreakBelow")
        if isinstance(below, (int, float)) and below > 0 and current_price < below:
            return True
        above = tt.get("PriceBreakAbove")
        if isinstance(above, (int, float)) and above > 0 and current_price > above:
            return True
    return False


def _pick_default_scenario(
    forest: list[dict[str, Any]], as_of: date, current_price: float | None,
) -> dict[str, Any] | None:
    if not forest:
        return None

    def sort_key(s: dict[str, Any]) -> tuple:
        days = _recency_days(s, as_of)
        return (
            _is_invalidated(s, current_price),          # False(未失效)優先
            -_recency_tier(days),                        # tier DESC
            -_power_rank(s),                             # power DESC
            -int(s.get("rules_passed_count") or 0),      # passed DESC
            days,                                        # 最新優先
        )

    return min(forest, key=sort_key)


def _scenario_certainty(scenario: dict[str, Any]) -> str:
    """鏡射 frontend scenarioPrimaryCertainty:monowave_structure_labels 取最強;無 → Possible。"""
    best: str | None = None
    for mw in scenario.get("monowave_structure_labels") or []:
        for lbl in mw.get("labels") or []:
            c = lbl.get("certainty")
            if c in _CERTAINTY_RANK and (
                best is None or _CERTAINTY_RANK[c] > _CERTAINTY_RANK[best]
            ):
                best = c
    return best or "Possible"


def _compact_label(scenario: dict[str, Any]) -> str:
    """cell 用緊湊標籤(對齊 wireframe "Flat·BFailure" 風格,CL5 summary-only)。

    raw `structure_label` 是 Rust Debug 格式長字串
    (例 "Flat { sub_kind: BFailure } Down (3-wave from mw238 to mw240)",
    2026-06-11 production 實測 ~50 字元)— 直接進 11px mono cell 會撐爆表格;
    從 pattern_type 結構化欄位組緊湊型,完整 label 看 V1 個股卡。
    """
    pt = scenario.get("pattern_type")
    if isinstance(pt, str) and pt:
        return pt  # "Impulse" / "RunningCorrection"
    if isinstance(pt, dict) and pt:
        kind = next(iter(pt))
        val = pt.get(kind)
        if isinstance(val, dict):
            sub = val.get("sub_kind")
            if isinstance(sub, str) and sub:
                return f"{kind}·{sub}"
            subs = val.get("sub_kinds")
            if isinstance(subs, list) and subs:
                return f"{kind}·" + "+".join(str(s) for s in subs[:2])
        return str(kind)
    # pattern_type 缺值 fallback:截短 raw label
    return str(scenario.get("structure_label") or "")[:24]


def _scenario_direction(scenario: dict[str, Any]) -> str:
    """WAVE 欄 4 向箭頭語意:修正型結構 → correction(↘);推動型依 power 符號。

    pattern_type 序列化:"Impulse" / "RunningCorrection" 為字串;
    {"Zigzag": {...}} / {"Flat": ...} / {"Triangle": ...} / {"Combination": ...} /
    {"Diagonal": ...} 為單鍵 dict。Diagonal 屬推動家族,其餘 dict + RunningCorrection
    屬修正家族。
    """
    pt = scenario.get("pattern_type")
    corrective = False
    if pt == "RunningCorrection":
        corrective = True
    elif isinstance(pt, dict):
        corrective = next(iter(pt), None) in ("Zigzag", "Flat", "Triangle", "Combination")
    if corrective:
        return "correction"
    rank = _power_rank(scenario)
    if rank > 0:
        return "up"
    if rank < 0:
        return "down"
    return "flat"


def _sparkline(monowaves: list[dict[str, Any]]) -> list[float]:
    """monowave 尾段 end_price 序列 → min-max 歸一化 0..1(平序列 → 全 0.5)。"""
    prices: list[float] = []
    for mw in monowaves[-_SPARKLINE_POINTS:]:
        p = mw.get("end_price")
        if isinstance(p, (int, float)):
            prices.append(float(p))
    if len(prices) < 2:
        return []
    lo, hi = min(prices), max(prices)
    if hi == lo:
        return [0.5] * len(prices)
    return [round((p - lo) / (hi - lo), 4) for p in prices]


def _resonance_level(reso_doc: dict[str, Any] | None) -> str:
    """findings 歸約成 badge 三級:任一 strong > 任一 basic > 非空 divergence > none。

    single_track_mode(A-3 失效閘門,軌道一退場)→ none(共振判定已跳過)。
    """
    if not reso_doc or reso_doc.get("single_track_mode"):
        return "none"
    levels = {f.get("level") for f in reso_doc.get("findings") or []}
    if "strong" in levels:
        return "strong"
    if "basic" in levels:
        return "basic"
    if levels:
        return "divergence"
    return "none"


# ── 批次入口 ──────────────────────────────────────────────────────────────────

def _fetch_latest_docs(
    conn, stock_ids: list[str], as_of: date, core_name: str, timeframe: str,
) -> dict[str, dict[str, Any]]:
    """批次取每檔 `snapshot_date <= as_of` 最新 snapshot(DISTINCT ON,1 條 SQL)。

    對齊 fetch_fusion_doc(materialize/read.py)的單檔語意,ANY(%s) 批次化。
    """
    sql = (
        "SELECT DISTINCT ON (stock_id) stock_id, snapshot, snapshot_date "
        "FROM structural_snapshots "
        "WHERE stock_id = ANY(%s) AND core_name = %s "
        "  AND timeframe = %s AND snapshot_date <= %s "
        "ORDER BY stock_id, snapshot_date DESC"
    )
    with conn.cursor() as cur:
        cur.execute(sql, [stock_ids, core_name, timeframe, as_of])
        rows = cur.fetchall()
    return {r["stock_id"]: r for r in rows}


def _insufficient_row(stock_id: str) -> dict[str, Any]:
    return {
        "stock_id": stock_id,
        "insufficient": True,
        "label": "",
        "direction": "flat",
        "scenario_count": 0,
        "certainty": "Possible",
        "sparkline": [],
        "resonance": "none",
        "staleness_days": None,
        "scenario_age_days": None,
    }


def digest_from_docs(
    stock_id: str,
    neely_row: dict[str, Any] | None,
    reso_row: dict[str, Any] | None,
    as_of: date,
) -> dict[str, Any]:
    """單檔抽取(純函式,canned doc 可單測)。

    無 neely snapshot / insufficient_data / 空 forest → insufficient row
    (對齊 WaveCell「— 無法判斷」視覺)。
    """
    neely = (neely_row or {}).get("snapshot")
    if not isinstance(neely, dict) or neely.get("insufficient_data"):
        return _insufficient_row(stock_id)
    forest = neely.get("scenario_forest") or []
    if not forest:
        return _insufficient_row(stock_id)

    monowaves = neely.get("monowave_series") or []
    current_price: float | None = None
    if monowaves:
        last_p = monowaves[-1].get("end_price")
        if isinstance(last_p, (int, float)):
            current_price = float(last_p)

    top = _pick_default_scenario(forest, as_of, current_price)
    if top is None:
        return _insufficient_row(stock_id)

    sd = (neely_row or {}).get("snapshot_date")
    staleness = (as_of - sd).days if isinstance(sd, date) else None

    # 形態年齡:picked scenario 的 wave_tree.end 距 as_of 天數。與 staleness 是兩回事 —
    # staleness = snapshot 新鮮度,age = 形態結尾距今(V1 stale 視覺門檻 >365d 用它)。
    age = _recency_days(top, as_of)
    age_days = int(age) if age != float("inf") else None

    reso = (reso_row or {}).get("snapshot")
    return {
        "stock_id": stock_id,
        "insufficient": False,
        "label": _compact_label(top),
        "direction": _scenario_direction(top),
        "scenario_count": len(forest),
        "certainty": _scenario_certainty(top),
        "sparkline": _sparkline(monowaves),
        "resonance": _resonance_level(reso if isinstance(reso, dict) else None),
        "staleness_days": staleness,
        "scenario_age_days": age_days,
    }


def wave_summary_rows(
    conn, stock_ids: list[str], as_of: date, timeframe: str = "daily",
) -> list[dict[str, Any]]:
    """批次 WAVE 摘要:輸入順序 = 輸出順序;單檔抽取失敗回 insufficient,不炸整批。

    Raises:
        ValueError: timeframe 不在 daily / weekly / monthly。
    """
    if timeframe not in _VALID_TIMEFRAMES:
        raise ValueError(f"timeframe {timeframe!r} 不在 {_VALID_TIMEFRAMES}")
    if not stock_ids:
        return []

    neely_docs = _fetch_latest_docs(conn, stock_ids, as_of, "neely_core", timeframe)
    reso_docs = _fetch_latest_docs(conn, stock_ids, as_of, "resonance_fusion", timeframe)

    out: list[dict[str, Any]] = []
    for sid in stock_ids:
        try:
            out.append(digest_from_docs(sid, neely_docs.get(sid), reso_docs.get(sid), as_of))
        except Exception:  # noqa: BLE001 — 單檔資料異常不擋整批(graceful degradation)
            out.append(_insufficient_row(sid))
    return out
