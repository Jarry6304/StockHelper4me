"""Fusion Layer · Integration 端口 — stock_snapshot(A 視角,10-in-1)。

對齊 m3Spec/fusion_layer.md §5 / §8.1 + api_roadmap_v1.md §三。

把個股當下可知資訊組成單一快照。6 個既有 section 重用 mcp_server 的 compute_*
helper(對齊 fusion_layer §5「reuses existing mcp_server.compute_* helpers」);
4 個新 section(fundamentals / institutional / shareholder / technical_summary)
直接讀 Silver / facts。各 section 獨立 try/except — 某段壞不影響其他。
"""

from __future__ import annotations

from datetime import date
from decimal import Decimal
from typing import Any

from fusion._shared import fact_to_event
from fusion.raw._db import fetch_facts, get_connection

# technical_summary 摘要級覆蓋的 indicator cores
_TECHNICAL_CORES = ["macd_core", "rsi_core", "kd_core", "ma_core", "bollinger_core"]


def stock_snapshot(
    stock_id: str,
    as_of: date,
    *,
    database_url: str | None = None,
    conn: Any = None,
) -> dict[str, Any]:
    """個股 10-in-1 當下快照。

    Sections:health / loan_collateral / block_trade / risk_alert /
    market_context / commodity_macro(6 既有,重用 mcp_server helper)+
    fundamentals / institutional / shareholder / technical_summary(4 新)。

    Returns:
        {stock_id, as_of, <10 sections>, narrative}
        某 section 失敗 → 該 section = {"error": "...", "section": "..."}。
    """

    def _safe(label: str, fn):
        try:
            return fn()
        except Exception as e:  # noqa: BLE001 — graceful degradation,對齊既有 stock_snapshot
            return {"error": f"{type(e).__name__}: {e}", "section": label}

    # ── 6 既有 section:重用 mcp_server compute_* helper ──────────────────
    from mcp_server._block_trade import compute_block_trade_summary
    from mcp_server._climate import compute_market_context
    from mcp_server._commodity_macro import compute_commodity_macro_snapshot
    from mcp_server._health import compute_stock_health
    from mcp_server._loan_collateral import compute_loan_collateral_snapshot
    from mcp_server._risk_alert import compute_risk_alert_status

    health = _safe("health", lambda: compute_stock_health(stock_id, as_of))
    loan = _safe("loan_collateral", lambda: compute_loan_collateral_snapshot(
        stock_id, as_of, database_url=database_url))
    block = _safe("block_trade", lambda: compute_block_trade_summary(
        stock_id, as_of, lookback_days=30, database_url=database_url))
    risk = _safe("risk_alert", lambda: compute_risk_alert_status(
        stock_id, as_of, database_url=database_url))
    def _market_context():
        # v4.32 Golden L3:先讀物化 climate_fusion,缺 / 失敗 → compute_market_context fallback。
        # isinstance(dict) 守門 + try/except → 連線失敗或 mock conn 都安全降級為 compute。
        from fusion.materialize.read import fetch_fusion_doc
        try:
            _c = get_connection(database_url)
            try:
                _row = fetch_fusion_doc(
                    _c, stock_id="_market_", as_of=as_of,
                    core_name="climate_fusion", timeframe="_all_",
                )
            finally:
                _c.close()
            _snap = _row.get("snapshot") if hasattr(_row, "get") else None
            if isinstance(_snap, dict):
                return _snap
        except Exception:  # noqa: BLE001
            pass
        return compute_market_context(as_of)

    market = _safe("market_context", _market_context)
    commodity = _safe("commodity_macro", lambda: compute_commodity_macro_snapshot(
        as_of, commodities=["GOLD"], database_url=database_url))

    # ── 4 新 section:直接讀 Silver / facts(共用一條 conn)───────────────
    # 連線取得失敗 → 4 個 section 一起降級為 error,6 個既有 section 仍各自獨立。
    own_conn = conn is None
    conn_err: dict[str, Any] | None = None
    if own_conn:
        try:
            conn = get_connection(database_url)
        except Exception as e:  # noqa: BLE001 — graceful degradation
            conn = None
            conn_err = {"error": f"{type(e).__name__}: {e}", "section": "db_connection"}
    if conn is not None:
        try:
            fundamentals = _safe("fundamentals", lambda: _fundamentals(conn, stock_id, as_of))
            institutional = _safe("institutional", lambda: _fact_section(
                conn, stock_id, as_of, "institutional_core"))
            shareholder = _safe("shareholder", lambda: _fact_section(
                conn, stock_id, as_of, "shareholder_core"))
            technical = _safe("technical_summary", lambda: _technical_summary(
                conn, stock_id, as_of))
        finally:
            if own_conn:
                conn.close()
    else:
        fundamentals = institutional = shareholder = technical = conn_err

    return {
        "stock_id": stock_id,
        "as_of": as_of.isoformat(),
        "health": health,
        "loan_collateral": loan,
        "block_trade": block,
        "risk_alert": risk,
        "market_context": market,
        "commodity_macro": commodity,
        "fundamentals": fundamentals,
        "institutional": institutional,
        "shareholder": shareholder,
        "technical_summary": technical,
        "narrative": _narrative(stock_id, health, market, risk),
    }


def _latest_row(
    conn: Any, table: str, stock_id: str, as_of: date, cols: list[str]
) -> dict[str, Any] | None:
    """撈某 Silver 表 `date <= as_of` 最新一筆(table / cols 為內部固定值)。"""
    select = ", ".join(["date", *cols])
    sql = (
        f"SELECT {select} FROM {table} "
        "WHERE market = 'TW' AND stock_id = %s AND date <= %s "
        "ORDER BY date DESC LIMIT 1"
    )
    with conn.cursor() as cur:
        cur.execute(sql, [stock_id, as_of])
        row = cur.fetchone()
    if not row:
        return None
    d = row.get("date")
    out: dict[str, Any] = {"date": d.isoformat() if hasattr(d, "isoformat") else d}
    for c in cols:
        v = row.get(c)
        # Silver NUMERIC 欄 psycopg 回 Decimal — 轉 float 確保 json.dumps 可序列化。
        if isinstance(v, (int, float, Decimal)) and not isinstance(v, bool):
            out[c] = float(v)
        else:
            out[c] = v
    return out


def _fundamentals(conn: Any, stock_id: str, as_of: date) -> dict[str, Any]:
    """基本面:最新估值(PER/PBR/殖利率)+ 最新月營收(MoM/YoY)。"""
    return {
        "valuation": _latest_row(
            conn, "valuation_daily_derived", stock_id, as_of,
            ["per", "pbr", "dividend_yield", "market_value_weight"],
        ),
        "monthly_revenue": _latest_row(
            conn, "monthly_revenue_derived", stock_id, as_of,
            ["revenue", "revenue_mom", "revenue_yoy"],
        ),
    }


def _fact_section(
    conn: Any, stock_id: str, as_of: date, core: str, lookback_days: int = 60
) -> dict[str, Any]:
    """某 core 近期 facts → 統一 Event list + 計數。"""
    facts = fetch_facts(
        conn, stock_ids=[stock_id], as_of=as_of,
        lookback_days=lookback_days, cores=[core],
    )
    events = [fact_to_event(f) for f in facts]
    return {
        "source_core": core,
        "lookback_days": lookback_days,
        "event_count": len(events),
        "recent_events": events[:15],
    }


def _technical_summary(conn: Any, stock_id: str, as_of: date) -> dict[str, Any]:
    """技術面摘要:數個 key indicator cores 近 30 日的訊號 facts。"""
    facts = fetch_facts(
        conn, stock_ids=[stock_id], as_of=as_of,
        lookback_days=30, cores=_TECHNICAL_CORES,
    )
    events = [fact_to_event(f) for f in facts]
    by_core: dict[str, int] = {}
    for e in events:
        by_core[e["source"]] = by_core.get(e["source"], 0) + 1
    return {
        "cores_covered": _TECHNICAL_CORES,
        "lookback_days": 30,
        "signal_count": len(events),
        "by_core": by_core,
        "recent_signals": events[:15],
    }


def _narrative(stock_id: str, health: dict, market: dict, risk: dict) -> str:
    """取 health / market / risk 各 1 訊號串成 1-3 句 overall view。"""
    parts: list[str] = []
    if isinstance(health, dict) and isinstance(health.get("overall_score"), (int, float)):
        s = health["overall_score"]
        tone = "偏多" if s > 20 else "偏空" if s < -20 else "中性"
        parts.append(f"{stock_id} 個股健康度 {s:+.0f}({tone})")
    if isinstance(market, dict) and market.get("overall_climate"):
        parts.append(f"大盤環境 {market['overall_climate']}")
    if isinstance(risk, dict):
        cur = risk.get("current_status")
        if isinstance(cur, dict) and cur.get("in_disposition_period"):
            parts.append(f"處置警示中({cur.get('severity_label') or cur.get('severity')})")
    return ";".join(parts) if parts else f"{stock_id} 快照已組裝(各 section 詳見內容)。"
