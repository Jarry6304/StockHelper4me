"""J2 錨定 diff — run-all 後對 active 判讀逐筆比對(wave_judgment_loop §6)。

判定順序(per accepted anchor):
1. 命中(anchor_key ∈ 最新 forest)→ intact,不新增列(連續 intact 不寫)
2. 未命中 且 (as_of, now] 有 bar 跨越記錄的 PriceBreakBelow/Above,或
   TimeExceeds / 判讀 time_limit_bar 到期 → invalidated + diff_detail{rule, bar, price}
3. 未命中 且 anchor_key 為某 forest scenario wave_tree 的**嚴格子樹** →
   absorbed + diff_detail{parent_anchor_key}
4. 其餘 vanished:assumption_hash / engine_version 與判讀時不同 → cause=engine_changed;
   相同 → cause=engine_regression **告警**(同假設下丟掉曾接受的合法候選 = 引擎 bug)
5. 多錨各自判定,整體取最差(invalidated > vanished > absorbed > intact)

狀態列 = INSERT 新列(內容欄拷貝原列、status/diff_detail/supersedes_id 換;
judged_by 沿用 — J2 非判讀者,列仍代表同一判讀的存續)。
"""

from __future__ import annotations

import logging
from datetime import date, datetime
from typing import Any

from fusion.raw._db import fetch_structural_latest

from .anchor_key import forest_anchor_keys, is_strict_subtree, scenario_anchor_key
from .db import fetch_all_active_judgments, insert_judgment

logger = logging.getLogger(__name__)

__all__ = ["run_anchor_diff"]

# 最差優先(§6 判定 5)
_SEVERITY = {"invalidated": 3, "vanished": 2, "absorbed": 1, "intact": 0}


def run_anchor_diff(
    conn,
    *,
    stock_ids: list[str] | None = None,
    as_of: date | None = None,
) -> dict[str, int]:
    """對全部(或指定股票的)active 判讀跑 J2 diff。回傳計數摘要。"""
    as_of = as_of or date.today()
    judgments = fetch_all_active_judgments(conn)
    if stock_ids:
        wanted = set(stock_ids)
        judgments = [j for j in judgments if j["stock_id"] in wanted]

    summary = {
        "checked": 0, "intact": 0, "invalidated": 0,
        "absorbed": 0, "vanished": 0, "engine_regression": 0,
    }
    snapshot_cache: dict[tuple[str, str], dict[str, Any] | None] = {}

    for j in judgments:
        summary["checked"] += 1
        key = (j["stock_id"], j["timeframe"])
        if key not in snapshot_cache:
            snapshot_cache[key] = _latest_neely_row(conn, j["stock_id"], j["timeframe"], as_of)
        row = snapshot_cache[key]
        outcome = _diff_one(conn, j, row, as_of)
        status = outcome["status"]
        summary[status] += 1
        if status == "intact":
            continue  # 不新增列(記憶體視圖層次;§6 判定 1)
        if outcome.get("cause") == "engine_regression":
            summary["engine_regression"] += 1
            logger.error(
                "[J2] engine_regression:judgment #%s(%s %s)anchor 於同 "
                "assumption_hash/engine_version 下消失 — 引擎 bug,非市場(§J2 判定 4)",
                j["id"], j["stock_id"], j["timeframe"],
            )
        _insert_status_row(conn, j, status, outcome["diff_detail"])

    return summary


# ────────────────────────────────────────────────────────────
# 單筆判定
# ────────────────────────────────────────────────────────────

def _diff_one(
    conn, judgment: dict[str, Any], row: dict[str, Any] | None, as_of: date,
) -> dict[str, Any]:
    snapshot = (row or {}).get("snapshot") or {}
    forest = [s for s in (snapshot.get("scenario_forest") or []) if isinstance(s, dict)]
    keys = forest_anchor_keys(forest)

    accepted = judgment.get("accepted") or []
    anchors = [a.get("anchor_key") for a in accepted if isinstance(a, dict)]
    if not anchors:
        # no_fit 判讀無錨 — J2 無事可比(重判由 dossier 面驅動)
        return {"status": "intact", "diff_detail": None}

    worst = {"status": "intact", "diff_detail": None, "cause": None}
    bars: list[dict[str, Any]] | None = None  # lazy(僅未命中才撈價)

    for anchor in anchors:
        if anchor in keys:
            continue  # intact

        if bars is None:
            bars = _fetch_bars_since(conn, judgment["stock_id"], judgment["as_of"], as_of)

        breach = _check_invalidation(judgment, anchor, bars, as_of)
        if breach is not None:
            outcome = {"status": "invalidated", "diff_detail": breach, "cause": None}
        else:
            parent = _find_absorbing_parent(anchor, forest)
            if parent is not None:
                outcome = {
                    "status": "absorbed",
                    "diff_detail": {"anchor_key": anchor, "parent_anchor_key": parent},
                    "cause": None,
                }
            else:
                same_engine = (
                    (row or {}).get("source_version") == judgment.get("engine_version")
                    and (snapshot.get("assumption_hash") or "")
                        == (judgment.get("assumption_hash") or "")
                )
                cause = "engine_regression" if same_engine else "engine_changed"
                outcome = {
                    "status": "vanished",
                    "diff_detail": {"anchor_key": anchor, "cause": cause},
                    "cause": cause,
                }

        if _SEVERITY[outcome["status"]] > _SEVERITY[worst["status"]]:
            worst = outcome

    return worst


def _check_invalidation(
    judgment: dict[str, Any], anchor: str, bars: list[dict[str, Any]], as_of: date,
) -> dict[str, Any] | None:
    """記錄的 triggers(提交時拷貝)+ 判讀 time_limit_bar;命中回 diff_detail。"""
    invalidation = judgment.get("invalidation") or {}
    recorded = invalidation.get("recorded_triggers") or []
    triggers = []
    for entry in recorded:
        if entry.get("anchor_key") == anchor:
            triggers = entry.get("triggers") or []
            break

    for t in triggers:
        ttype = t.get("trigger_type")
        if not isinstance(ttype, dict):
            continue
        rule = t.get("rule_reference")
        if "PriceBreakBelow" in ttype:
            level = float(ttype["PriceBreakBelow"])
            for b in bars:
                if b["low"] is not None and float(b["low"]) <= level:
                    return {"anchor_key": anchor, "rule": rule,
                            "bar": b["date"].isoformat(), "price": float(b["low"])}
        elif "PriceBreakAbove" in ttype:
            level = float(ttype["PriceBreakAbove"])
            for b in bars:
                if b["high"] is not None and float(b["high"]) >= level:
                    return {"anchor_key": anchor, "rule": rule,
                            "bar": b["date"].isoformat(), "price": float(b["high"])}
        elif "TimeExceeds" in ttype:
            deadline = _parse_date(ttype["TimeExceeds"])
            if deadline is not None and as_of > deadline:
                return {"anchor_key": anchor, "rule": rule,
                        "bar": deadline.isoformat(), "price": None}

    # 判讀者自訂時限(與 triggers 並行;更嚴的一側)
    time_limit = _parse_date(invalidation.get("time_limit_bar"))
    if time_limit is not None and as_of > time_limit:
        return {"anchor_key": anchor, "rule": "judgment.time_limit_bar",
                "bar": time_limit.isoformat(), "price": None}
    return None


def _find_absorbing_parent(anchor: str, forest: list[dict]) -> str | None:
    for s in forest:
        if is_strict_subtree(anchor, s):
            return scenario_anchor_key(s)
    return None


def _insert_status_row(conn, judgment: dict[str, Any], status: str, diff_detail: Any) -> None:
    row = {
        k: judgment.get(k)
        for k in (
            "stock_id", "timeframe", "as_of", "judged_by",
            "snapshot_date", "params_hash", "engine_version", "assumption_hash",
            "accepted", "degree_read", "rationale", "invalidation",
            "confidence_class",
        )
    }
    row["status"] = status
    row["supersedes_id"] = judgment["id"]
    row["diff_detail"] = diff_detail
    insert_judgment(conn, row)


# ────────────────────────────────────────────────────────────
# 資料存取
# ────────────────────────────────────────────────────────────

def _latest_neely_row(conn, stock_id: str, timeframe: str, as_of: date) -> dict[str, Any] | None:
    rows = fetch_structural_latest(conn, stock_id=stock_id, as_of=as_of, cores=["neely_core"])
    for r in rows:
        if r.get("timeframe") == timeframe:
            return r
    return None


def _fetch_bars_since(conn, stock_id: str, since: date, until: date) -> list[dict[str, Any]]:
    """(as_of, now] 的日 bars(§6 判定 2;triggers 產於 daily 價位語意)。"""
    sql = """
        SELECT date, low, high FROM price_daily
        WHERE stock_id = %s AND date > %s AND date <= %s
        ORDER BY date
    """
    with conn.cursor() as cur:
        cur.execute(sql, [stock_id, since, until])
        return cur.fetchall()


def _parse_date(v: Any) -> date | None:
    if isinstance(v, date):
        return v
    if not v:
        return None
    try:
        return datetime.fromisoformat(str(v)).date()
    except (TypeError, ValueError):
        return None
