"""Dossier builder — 引擎證據卷宗(m3Spec/wave_judgment_loop.md §4)。

讀者面零 primary:三 timeframe 各列 live-edge 候選(引擎證據三區 + anchor_key),
排序只反映結構(degree desc → end desc → start asc),**無分數鍵**;判讀由
wave_judgments 承載,dossier 只附 active judgment 參照。

落位:builder 在 fusion(web_api 不能 import mcp_server,兩者都 import fusion);
`mcp_server/_dossier.py` 為薄轉接(spec §2 檔名)。

payload 紀律:candidates 僅 live-edge(其餘收 `historical.count`),
per-timeframe 上限 `CANDIDATES_CAP`,超出砍尾 + `truncated: true`
(排序決定性,砍掉的是低 degree / 較舊 end)。
"""

from __future__ import annotations

from datetime import date, datetime
from typing import Any

from fusion._picker import canonical_is_invalidated, classify_degree_by_years
from fusion.raw._db import fetch_structural_latest, fetch_traditional_latest

from .anchor_key import pattern_tag, scenario_anchor_key
from .db import fetch_active_judgment

__all__ = ["build_dossier", "CANDIDATES_CAP", "LIVE_EDGE_BARS", "TREE_DEPTH_CAP"]

# live-edge 判定:scenario end bar 距最後 bar ≤ 3(對齊引擎 E4)
LIVE_EDGE_BARS = 3
# per-timeframe 候選上限(MCP payload budget;超出 → truncated: true)
CANDIDATES_CAP = 12
# 候選 wave_tree 序列化深度上限(root + 2 層 children;更深子樹以
# children_omitted 計數收斂 — anchor_key 仍由**完整**樹計算,完整樹走
# /neely/forest)。payload 政策對齊 verify_mcp_toolkit:soft 50KB / hard 1MB
TREE_DEPTH_CAP = 2

_TIMEFRAMES = ("daily", "weekly", "monthly")

# quality caveat:≤ SubMinuette 視為短期 swing(沿用 v3.35.1 既有邏輯)
_SHORT_DEGREE_LABELS = {"SubMicro", "Micro", "SubMinuette"}


def build_dossier(
    conn,
    *,
    stock_id: str,
    as_of: date,
    current_price: float | None = None,
) -> dict[str, Any]:
    """組 §4 dossier JSON(唯讀;不選 primary、不打分)。"""
    rows = fetch_structural_latest(conn, stock_id=stock_id, as_of=as_of, cores=["neely_core"])
    by_tf: dict[str, dict[str, Any]] = {}
    for r in rows:
        if r.get("core_name") == "neely_core":
            by_tf[r.get("timeframe")] = r

    timeframes: dict[str, Any] = {}
    all_scenarios: list[dict] = []
    live_by_tf: dict[str, list[dict]] = {}
    for tf in _TIMEFRAMES:
        row = by_tf.get(tf)
        trad_row = fetch_traditional_latest(conn, stock_id=stock_id, timeframe=tf)
        section, live, scenarios = _timeframe_section(row, trad_row, current_price)
        timeframes[tf] = section
        live_by_tf[tf] = live
        all_scenarios.extend(scenarios)

    daily_row = by_tf.get("daily")
    daily_snap = (daily_row or {}).get("snapshot") or {}

    active_judgment = {
        tf: _judgment_summary(fetch_active_judgment(conn, stock_id=stock_id, timeframe=tf))
        for tf in _TIMEFRAMES
    }

    return {
        "stock_id": stock_id,
        "as_of": as_of.isoformat(),
        "current_price": current_price,
        "engine": {
            "neely": (daily_row or {}).get("source_version"),
            "traditional": _traditional_version(conn, stock_id),
            "assumption_hash": daily_snap.get("assumption_hash"),
        },
        "assumptions": daily_snap.get("assumptions") or [],
        "timeframes": timeframes,
        "cross_timeframe": _cross_timeframe(live_by_tf),
        "active_judgment": active_judgment,
        "quality_caveat": _quality_caveat(all_scenarios, live_by_tf.get("daily") or [], current_price),
    }


# ────────────────────────────────────────────────────────────
# per-timeframe section
# ────────────────────────────────────────────────────────────

def _timeframe_section(
    row: dict[str, Any] | None,
    trad_row: dict[str, Any] | None,
    current_price: float | None,
) -> tuple[dict[str, Any], list[dict], list[dict]]:
    """單 timeframe 段。回傳 (section, live_scenarios, all_scenarios)。"""
    if row is None:
        return (
            {
                "snapshot_ref": None,
                "monowave_count": 0,
                "last_bar": None,
                "live_edge": {"ambiguity": None},
                "candidates": [],
                "historical": {"count": 0, "note": "無 snapshot(尚未跑 tw_cores)"},
                "traditional": _traditional_section(trad_row, []),
            },
            [],
            [],
        )

    snap = row.get("snapshot") or {}
    forest = snap.get("scenario_forest") or snap.get("scenarios") or []
    forest = [s for s in forest if isinstance(s, dict)]
    monowaves = snap.get("monowave_series") or []

    # date ↔ bar index 對映(scenario 端點必落在 monowave 端點;Neutral 端點在內,
    # 合成葉端點可反查)。last_bar_index = 序列最大 end bar。
    bar_of: dict[str, int] = {}
    last_bar_index = 0
    last_bar_date: str | None = None
    for m in monowaves:
        if not isinstance(m, dict):
            continue
        idx = m.get("bar_indices") or [0, 0]
        try:
            s_idx, e_idx = int(idx[0]), int(idx[1])
        except (TypeError, ValueError, IndexError):
            continue
        bar_of[str(m.get("start_date"))] = s_idx
        bar_of[str(m.get("end_date"))] = e_idx
        if e_idx >= last_bar_index:
            last_bar_index = e_idx
            last_bar_date = str(m.get("end_date"))

    data_range = snap.get("data_range") or {}
    last_bar = data_range.get("end") or last_bar_date

    live: list[dict] = []
    historical_count = 0
    for s in forest:
        end_bar = bar_of.get(str((s.get("wave_tree") or {}).get("end")))
        if end_bar is not None and end_bar >= last_bar_index - LIVE_EDGE_BARS:
            live.append(s)
        else:
            historical_count += 1

    # 排序:degree desc → end desc → start asc(無分數鍵)
    def _sort_key(s: dict) -> tuple[int, int, int]:
        tree = s.get("wave_tree") or {}
        return (
            -int(tree.get("degree_level") or 0),
            -_date_ord(tree.get("end")),
            _date_ord(tree.get("start")),
        )

    live.sort(key=_sort_key)
    truncated = len(live) > CANDIDATES_CAP
    shown = live[:CANDIDATES_CAP]

    candidates = [_candidate(s, current_price, last_bar_index, bar_of) for s in shown]

    section: dict[str, Any] = {
        "snapshot_ref": {
            "snapshot_date": _iso(row.get("snapshot_date")),
            "params_hash": row.get("params_hash"),
        },
        "monowave_count": len(monowaves),
        "last_bar": last_bar,
        "live_edge": {"ambiguity": snap.get("live_edge_ambiguity")},
        "candidates": candidates,
        "historical": {
            "count": historical_count,
            "note": "end < last_bar − 3;僅供脈絡,完整 forest 走 /neely/forest",
        },
        "traditional": _traditional_section(trad_row, candidates),
    }
    if truncated:
        section["truncated"] = True
        section["truncated_note"] = (
            f"live-edge 候選 {len(live)} 筆,僅列前 {CANDIDATES_CAP}(degree/end 排序尾端截斷)"
        )
    return section, live, forest


def _candidate(
    s: dict,
    current_price: float | None,
    last_bar_index: int,
    bar_of: dict[str, int],
) -> dict[str, Any]:
    """Scenario → 候選三區(身分 / 證據 / 前瞻)+ 機械失效狀態(§4)。

    舊 snapshot 容缺(§11):1.1.1 及以前無 ch6_status / robust →
    補 "Deferred" / null(非 false,判讀 protocol 視 null 為未知)。
    """
    tree = s.get("wave_tree") or {}
    end_bar = bar_of.get(str(tree.get("end")))
    return {
        "id": s.get("id"),
        "anchor_key": scenario_anchor_key(s),
        "pattern_type": pattern_tag(s.get("pattern_type")),
        "structure_label": s.get("structure_label"),
        "degree_level": tree.get("degree_level"),
        "span": {"start": tree.get("start"), "end": tree.get("end")},
        "age_bars": (last_bar_index - end_bar) if end_bar is not None else None,
        "wave_tree": _prune_tree(tree, TREE_DEPTH_CAP),
        "evidence": {
            "passed_rules": s.get("passed_rules") or [],
            "deferred_rules": s.get("deferred_rules") or [],
            "ch6_status": s.get("ch6_status") or "Deferred",
            "robust": s.get("robust", None),
            "advisory_findings": s.get("advisory_findings") or [],
            "complexity_level": s.get("complexity_level"),
            "triplexity_detected": s.get("triplexity_detected"),
        },
        "forward": {
            "power_rating": s.get("power_rating"),
            "post_pattern_behavior": s.get("post_pattern_behavior"),
            "max_retracement": s.get("max_retracement"),
            "invalidation_triggers": s.get("invalidation_triggers") or [],
            "expected_fib_zones": s.get("expected_fib_zones") or [],
            "awaiting_l_label": s.get("awaiting_l_label"),
        },
        "is_invalidated": (
            canonical_is_invalidated(s, current_price) if current_price else False
        ),
    }


# ────────────────────────────────────────────────────────────
# traditional 並列(讀層 juxtaposition;引擎 crate 零耦合不變)
# ────────────────────────────────────────────────────────────

def _traditional_section(
    trad_row: dict[str, Any] | None, neely_candidates: list[dict],
) -> dict[str, Any]:
    if trad_row is None:
        return {"candidates": [], "concordance": []}
    forest = trad_row.get("forest") or []
    if not isinstance(forest, list):
        forest = []

    candidates: list[dict] = []
    for t in forest:
        if not isinstance(t, dict):
            continue
        tree = t.get("wave_tree") or {}
        candidates.append({
            "id": t.get("id"),
            "pattern": pattern_tag(t.get("pattern_type")),
            "direction": t.get("direction"),
            "span": {"start": tree.get("start"), "end": tree.get("end")},
            # traditional v3 無 per-scenario fails(diagnostics.rejections 恆空)
            # → 恆 [],不造假;forest 成員已過硬規則
            "rules_failed": [],
            "guidelines": (t.get("guidelines_satisfied") or []) + (t.get("qualifiers_met") or []),
        })

    # concordance:端點同(start+end 皆同日)→ "endpoints";
    # 僅起始方向同(trad.direction vs neely initial_direction)→ "direction";
    # 其餘不列(全配對 × 全候選會炸 payload)
    concordance: list[dict] = []
    for nc in neely_candidates:
        n_span = nc.get("span") or {}
        for tc in candidates:
            t_span = tc.get("span") or {}
            if (
                str(n_span.get("start")) == str(t_span.get("start"))
                and str(n_span.get("end")) == str(t_span.get("end"))
            ):
                concordance.append({
                    "neely": nc.get("anchor_key"),
                    "traditional": tc.get("id"),
                    "shared": "endpoints",
                })
    return {"candidates": candidates, "concordance": concordance}


def _traditional_version(conn, stock_id: str) -> str | None:
    """traditional 引擎版本:daily row 的 diagnostics.engine_version(3.0.0 起;
    舊 row 無此欄 → None,讀取端容缺)。"""
    row = fetch_traditional_latest(conn, stock_id=stock_id, timeframe="daily")
    if row is None:
        return None
    diag = row.get("diagnostics") or {}
    return diag.get("engine_version")


# ────────────────────────────────────────────────────────────
# cross-timeframe / active judgment / quality caveat
# ────────────────────────────────────────────────────────────

def _dominant_directions(live: list[dict]) -> set[str]:
    return {
        str(s.get("initial_direction"))
        for s in live
        if s.get("initial_direction") in ("Up", "Down")
    }


def _cross_timeframe(live_by_tf: dict[str, list[dict]]) -> dict[str, Any]:
    """方向衝突:兩 timeframe 皆有候選且 initial_direction 集合不相交 → conflict。
    weekly/monthly 無候選 = 資料窗不足(§11),記 note 不視為衝突。"""
    notes: list[str] = []
    daily_dirs = _dominant_directions(live_by_tf.get("daily") or [])
    conflict = False
    for tf in ("weekly", "monthly"):
        live = live_by_tf.get(tf) or []
        if not live:
            notes.append(f"{tf} 無 live-edge 候選(資料窗不足,degree 脈絡不可用)")
            continue
        dirs = _dominant_directions(live)
        if daily_dirs and dirs and daily_dirs.isdisjoint(dirs):
            conflict = True
            notes.append(
                f"daily 候選方向 {sorted(daily_dirs)} 與 {tf} 候選方向 {sorted(dirs)} 互斥"
            )
    return {"direction_conflict": conflict, "notes": notes}


def _judgment_summary(row: dict[str, Any] | None) -> dict[str, Any] | None:
    """active judgment → dossier 附載摘要(判讀 protocol 步 4 最小修改基準)。"""
    if row is None:
        return None
    return {
        "id": row.get("id"),
        "as_of": _iso(row.get("as_of")),
        "judged_by": row.get("judged_by"),
        "accepted": row.get("accepted"),
        "degree_read": row.get("degree_read"),
        "confidence_class": row.get("confidence_class"),
        "invalidation": row.get("invalidation"),
        "status": row.get("status"),
        "assumption_hash": row.get("assumption_hash"),
        "engine_version": row.get("engine_version"),
    }


def _quality_caveat(
    all_scenarios: list[dict], daily_live: list[dict], current_price: float | None,
) -> dict[str, Any]:
    """沿用 v3.35.1 既有邏輯,改吃候選集(無 primary):
    (a) 全 forest short-degree only;(b) daily live-edge 候選 fib zones 聯集
    與 current_price 脫節。"""
    warnings: list[str] = []

    is_short_only = False
    max_span = 0.0
    max_degree: str | None = None
    for s in all_scenarios:
        span = _span_years(s)
        if span is not None and span > max_span:
            max_span = span
            max_degree = classify_degree_by_years(span)
    if all_scenarios:
        is_short_only = (max_degree is None) or (max_degree in _SHORT_DEGREE_LABELS)
        if is_short_only and max_span > 0:
            warnings.append(
                f"全 forest {len(all_scenarios)} 個 scenarios 皆 short-degree"
                f"(最長 span {max_span:.2f} yr,{max_degree or 'unknown'})— 無長期 anchor 可用。"
            )

    is_decoupled = False
    if current_price and current_price > 0 and daily_live:
        prices: list[float] = []
        for s in daily_live:
            for zone in s.get("expected_fib_zones") or []:
                if not isinstance(zone, dict):
                    continue
                for k in ("low", "high"):
                    v = zone.get(k)
                    if isinstance(v, (int, float)):
                        prices.append(float(v))
        if prices:
            fib_max, fib_min = max(prices), min(prices)
            buffer = (fib_max - fib_min) * 0.5 if fib_max > fib_min else fib_max * 0.1
            if current_price > fib_max + buffer or current_price < fib_min - buffer:
                is_decoupled = True
                warnings.append(
                    f"current_price={current_price:.2f} 落在 live-edge 候選 Fib 投影聯集 "
                    f"[{fib_min:.2f}, {fib_max:.2f}] 外(±50% buffer)— Neely 對現況無有效"
                    f"投影(spec §7.2 合法結果),前瞻區間不可用。"
                )

    return {
        "is_short_degree_only": is_short_only,
        "max_scenario_span_years": round(max_span, 2) if max_span > 0 else None,
        "max_scenario_degree": max_degree,
        "fib_zones_decoupled_from_price": is_decoupled,
        "is_usable": not (is_short_only or is_decoupled),
        "warnings": warnings,
    }


def _count_nodes(children: list) -> int:
    n = 0
    for c in children:
        if isinstance(c, dict):
            n += 1 + _count_nodes(c.get("children") or [])
    return n


def _prune_tree(node: dict, depth: int) -> dict:
    """wave_tree 序列化深度收斂(payload 護欄):depth 用盡後子樹改
    `children_omitted` 計數。anchor_key 由完整樹算,不受此影響。"""
    out = {
        "label": node.get("label"),
        "base_label": node.get("base_label"),
        "start": node.get("start"),
        "end": node.get("end"),
        "degree_level": node.get("degree_level"),
    }
    children = node.get("children") or []
    if depth <= 0 and children:
        out["children"] = []
        out["children_omitted"] = _count_nodes(children)
        return out
    out["children"] = [_prune_tree(c, depth - 1) for c in children if isinstance(c, dict)]
    return out


# ────────────────────────────────────────────────────────────
# 小工具
# ────────────────────────────────────────────────────────────

def _iso(d: Any) -> Any:
    if isinstance(d, date):
        return d.isoformat()
    return d


def _date_ord(v: Any) -> int:
    d = _parse_date(v)
    return d.toordinal() if d else 0


def _parse_date(v: Any) -> date | None:
    if isinstance(v, date):
        return v
    if not v:
        return None
    try:
        return datetime.fromisoformat(str(v)).date()
    except (TypeError, ValueError):
        return None


def _span_years(s: dict) -> float | None:
    tree = s.get("wave_tree") or {}
    start = _parse_date(tree.get("start"))
    end = _parse_date(tree.get("end"))
    if start is None or end is None or end < start:
        return None
    return (end - start).days / 365.25
