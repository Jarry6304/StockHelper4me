"""Traditional Core(Frost & Prechter EWP)波浪 forest MCP helper。

讀自有表 `traditional_snapshots`(獨立 vertical),回 LLM-friendly 摘要。與 Neely **並排、
不整合、無共識比對**。forest **不選 primary**;`top_scenarios` 依 `preference_score`
(指引 + 限定語客觀計數)降序供 UI/LLM 參考(非主情境標記)。
"""

from __future__ import annotations

from typing import Any

_TOP_N = 10

_CAVEAT = (
    "傳統派(Frost & Prechter EWP)獨立引擎,與 Neely 並排、不整合、無共識比對。"
    "forest 不選 primary;top_scenarios 依 preference_score(指引+限定語客觀計數)降序供參考。"
    "v1:R6/R7/R8/R11 子浪細分標 Deferred(需遞迴子浪分解);degree 為 bar 跨度相對啟發。"
)


def _narrative(stock_id: str, timeframe: str, scenarios: list[dict[str, Any]]) -> str:
    if not scenarios:
        return f"{stock_id}/{timeframe}:traditional forest 為空(資料不足或無合法數法)。"
    s0 = scenarios[0]
    return (
        f"{stock_id}/{timeframe}:{len(scenarios)} 個合法數法,"
        f"首選(preference_score 最高)= {s0.get('structure_label')} "
        f"(pref={s0.get('preference_score')})。"
    )


def compute_traditional_forest(
    stock_id: str,
    timeframe: str = "daily",
    *,
    top_n: int = _TOP_N,
    database_url: str | None = None,
) -> dict[str, Any]:
    """回 traditional forest 摘要(讀 traditional_snapshots 最新 row)。"""
    from fusion.raw import get_connection

    conn = get_connection(database_url)
    try:
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT forest, diagnostics, computed_at,
                       lower(data_range)::date AS data_start,
                       upper(data_range)::date AS data_end
                  FROM traditional_snapshots
                 WHERE stock_id = %s AND timeframe = %s
                 ORDER BY computed_at DESC
                 LIMIT 1
                """,
                [stock_id, timeframe],
            )
            row = cur.fetchone()
    finally:
        conn.close()

    if not row:
        return {
            "stock_id": stock_id,
            "timeframe": timeframe,
            "has_snapshot": False,
            "scenario_count": 0,
            "top_scenarios": [],
            "narrative": (
                f"{stock_id}/{timeframe} 無 traditional_snapshots — 先跑 "
                "`tw_cores run-all --write`(或 `traditional-debug` 驗)寫入。"
            ),
            "caveat": _CAVEAT,
        }

    forest = row.get("forest") or {}
    diag = row.get("diagnostics") or {}
    scenarios = forest.get("scenario_forest") or []

    top: list[dict[str, Any]] = []
    for s in scenarios[: max(top_n, 0)]:
        trig = s.get("invalidation_triggers") or []
        wt = s.get("wave_tree") or {}
        top.append(
            {
                "id": s.get("id"),
                "structure_label": s.get("structure_label"),
                "pattern_type": s.get("pattern_type"),
                "direction": s.get("direction"),
                "degree": s.get("degree"),
                "preference_score": s.get("preference_score"),
                "guidelines_satisfied": s.get("guidelines_satisfied", []),
                "qualifiers_met": s.get("qualifiers_met", []),
                "invalidation_price": (trig[0].get("price") if trig else None),
                "fib_zone_count": len(s.get("expected_fib_zones") or []),
                "wave_start": wt.get("start"),
                "wave_end": wt.get("end"),
                "start_price": wt.get("start_price"),
                "end_price": wt.get("end_price"),
            }
        )

    return {
        "stock_id": stock_id,
        "timeframe": timeframe,
        "has_snapshot": True,
        "computed_at": str(row.get("computed_at")),
        "data_range": {
            "start": str(row["data_start"]) if row.get("data_start") else None,
            "end": str(row["data_end"]) if row.get("data_end") else None,
        },
        "scenario_count": len(scenarios),
        "pivot_count": diag.get("pivot_count"),
        "candidate_count": diag.get("candidate_count"),
        "validator_pass": diag.get("validator_pass_count"),
        "validator_reject": diag.get("validator_reject_count"),
        "forest_overflow_triggered": diag.get("forest_overflow_triggered", False),
        "insufficient_data": diag.get("insufficient_data", False),
        "top_scenarios": top,
        "narrative": _narrative(stock_id, timeframe, scenarios),
        "caveat": _CAVEAT,
    }
