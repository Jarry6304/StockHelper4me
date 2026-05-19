"""
mcp_server/_screens.py
======================
v3.32 4 個 cross-stock screen toolkit MCP helper:

  - compute_monthly_screen   — Toolkit A(月)3 factors + vol overlay
  - compute_quarterly_screen — Toolkit B(季)3 factors:F-Score / Low Vol / Industry-Adj GP
  - compute_annual_low_risk_screen — Toolkit C(年)3 factors
  - compute_monthly_trigger_scan   — Layer 5 positive/negative triggers

對齊 v3.31 stock_snapshot graceful degradation 模式:某 sub-factor 失敗 →
該 section 變 error,其他仍 surface。

對齊既有 mcp_server/_magic_formula.py 風格 + agg._db.fetch_cross_stock_ranked
共用 helper。
"""

from __future__ import annotations

import logging
from datetime import date
from typing import Any

from agg._db import fetch_cross_stock_ranked, get_connection

logger = logging.getLogger("collector.mcp_server.screens")


# ────────────────────────────────────────────────────────────
# Common helpers
# ────────────────────────────────────────────────────────────


def _to_float(v: Any) -> float | None:
    if v is None:
        return None
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


def _to_int(v: Any) -> int | None:
    if v is None:
        return None
    try:
        return int(v)
    except (TypeError, ValueError):
        return None


def _safe_screen(label: str, fn) -> dict[str, Any]:
    """try/except wrap for one sub-factor screen → graceful error key 對齊 stock_snapshot。"""
    try:
        return fn()
    except Exception as e:
        return {"error": f"{type(e).__name__}: {e}", "section": label}


def _fetch_top_rows(
    conn,
    *,
    source_table: str,
    as_of: date,
    rank_col: str,
    top_n: int,
    is_top_col: str = "is_top_n",
) -> tuple[Any | None, list[dict[str, Any]]]:
    """Wrap fetch_cross_stock_ranked(對齊新 schema 的 is_top_n column)。"""
    return fetch_cross_stock_ranked(
        conn,
        source_table=source_table, as_of=as_of, top_n=top_n,
        rank_col=rank_col, is_top_col=is_top_col,
    )


def _format_top_rows(
    rows: list[dict[str, Any]],
    *,
    rank_col: str,
    extra_metric_keys: list[str],
) -> list[dict[str, Any]]:
    """壓縮 row → LLM-friendly dict(name / industry / rank / metrics)。"""
    out: list[dict[str, Any]] = []
    for r in rows:
        item: dict[str, Any] = {
            "stock_id": r.get("stock_id"),
            "name":     r.get("stock_name"),
            "industry": r.get("industry_category"),
            "rank":     _to_int(r.get(rank_col)),
        }
        for k in extra_metric_keys:
            v = r.get(k)
            if isinstance(v, dict):
                item[k] = v
            else:
                item[k] = _to_float(v)
        out.append(item)
    return out


# ────────────────────────────────────────────────────────────
# Toolkit A:Monthly screen
# ────────────────────────────────────────────────────────────


def compute_monthly_screen(
    as_of: date,
    *,
    top_n: int = 30,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Toolkit A:Persistent Momentum + Revenue Momentum + Institutional Concert + vol overlay。"""
    conn = get_connection(database_url)
    try:
        def _pm():
            rd, rows = _fetch_top_rows(
                conn, source_table="persistent_momentum_ranked_derived",
                as_of=as_of, rank_col="momentum_rank", top_n=top_n,
            )
            top = _format_top_rows(rows, rank_col="momentum_rank",
                                   extra_metric_keys=["return_6m", "return_12m_1m",
                                                       "persistent_months", "detail"])
            return {"ranking_date": rd.isoformat() if rd else None,
                    "top_stocks":   top,
                    "narrative":    _narr("Persistent Momentum", len(top), top)}

        def _rm():
            rd, rows = _fetch_top_rows(
                conn, source_table="revenue_momentum_ranked_derived",
                as_of=as_of, rank_col="revenue_rank", top_n=top_n,
            )
            top = _format_top_rows(rows, rank_col="revenue_rank",
                                   extra_metric_keys=["revenue_yoy_latest",
                                                       "consecutive_positive"])
            return {"ranking_date": rd.isoformat() if rd else None,
                    "top_stocks":   top,
                    "narrative":    _narr("Revenue Momentum", len(top), top)}

        def _ic():
            rd, rows = _fetch_top_rows(
                conn, source_table="institutional_concert_ranked_derived",
                as_of=as_of, rank_col="concert_rank", top_n=top_n,
            )
            top = _format_top_rows(rows, rank_col="concert_rank",
                                   extra_metric_keys=["concert_days",
                                                       "foreign_cumulative_20d",
                                                       "cumulative_pct"])
            return {"ranking_date": rd.isoformat() if rd else None,
                    "top_stocks":   top,
                    "narrative":    _narr("Institutional Concert", len(top), top)}

        pm = _safe_screen("persistent_momentum", _pm)
        rm = _safe_screen("revenue_momentum",    _rm)
        ic = _safe_screen("institutional_concert", _ic)

        # vol overlay hint(從 pm 內第一個 stock 的 detail 摘要,跨股 cross_mean_vol 相同)
        vol_overlay = {"scale": 1.0, "rationale": "no_data"}
        if isinstance(pm, dict) and isinstance(pm.get("top_stocks"), list) and pm["top_stocks"]:
            d = pm["top_stocks"][0].get("detail") or {}
            scale = d.get("vol_managed_scale", 1.0)
            vol_overlay = {
                "scale":     scale,
                "rationale": ("Barroso-Santa-Clara 2015:6M realized vol > 歷史均值 × 1.5"
                              if scale < 1.0 else "vol 正常,full scale"),
            }
    finally:
        conn.close()

    return {
        "as_of":         as_of.isoformat(),
        "top_n":         top_n,
        "toolkit":       "A_monthly",
        "factors": {
            "persistent_momentum":  pm,
            "revenue_momentum":     rm,
            "institutional_concert": ic,
        },
        "vol_managed_overlay": vol_overlay,
        "narrative":     _compose_toolkit_narrative("A", pm, rm, ic),
    }


# ────────────────────────────────────────────────────────────
# Toolkit B:Quarterly screen
# ────────────────────────────────────────────────────────────


def compute_quarterly_screen(
    as_of: date,
    *,
    top_n: int = 30,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Toolkit B:F-Score + Low Vol + Industry-Adj GP。"""
    conn = get_connection(database_url)
    try:
        def _fs():
            rd, rows = _fetch_top_rows(
                conn, source_table="f_score_ranked_derived",
                as_of=as_of, rank_col="score_rank", top_n=top_n,
            )
            top = _format_top_rows(rows, rank_col="score_rank",
                                   extra_metric_keys=["f_score", "profitability",
                                                       "leverage", "efficiency"])
            return {"ranking_date": rd.isoformat() if rd else None,
                    "top_stocks":   top,
                    "narrative":    _narr("F-Score", len(top), top)}

        def _lv():
            rd, rows = _fetch_top_rows(
                conn, source_table="low_volatility_ranked_derived",
                as_of=as_of, rank_col="vol_rank", top_n=top_n,
            )
            top = _format_top_rows(rows, rank_col="vol_rank",
                                   extra_metric_keys=["std_252d"])
            return {"ranking_date": rd.isoformat() if rd else None,
                    "top_stocks":   top,
                    "narrative":    _narr("Low Volatility 252D", len(top), top)}

        def _gp():
            rd, rows = _fetch_top_rows(
                conn, source_table="industry_adj_gp_ranked_derived",
                as_of=as_of, rank_col="gp_rank", top_n=top_n,
            )
            top = _format_top_rows(rows, rank_col="gp_rank",
                                   extra_metric_keys=["gross_profitability",
                                                       "industry_median_gp",
                                                       "industry_adj_gp"])
            return {"ranking_date": rd.isoformat() if rd else None,
                    "top_stocks":   top,
                    "narrative":    _narr("Industry-Adj GP", len(top), top)}

        fs = _safe_screen("f_score",          _fs)
        lv = _safe_screen("low_volatility",   _lv)
        gp = _safe_screen("industry_adj_gp",  _gp)
    finally:
        conn.close()

    return {
        "as_of":     as_of.isoformat(),
        "top_n":     top_n,
        "toolkit":   "B_quarterly",
        "factors": {
            "f_score":           fs,
            "low_volatility":    lv,
            "industry_adj_gp":   gp,
        },
        "narrative": _compose_toolkit_narrative("B", fs, lv, gp),
    }


# ────────────────────────────────────────────────────────────
# Toolkit C:Annual low-risk screen
# ────────────────────────────────────────────────────────────


def compute_annual_low_risk_screen(
    as_of: date,
    *,
    top_n: int = 30,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Toolkit C:Long-Term Low Vol + Dividend Yield(yield trap filter)+ 12-1 Momentum。"""
    conn = get_connection(database_url)
    try:
        def _ltlv():
            rd, rows = _fetch_top_rows(
                conn, source_table="long_term_low_vol_ranked_derived",
                as_of=as_of, rank_col="vol_rank", top_n=top_n,
            )
            top = _format_top_rows(rows, rank_col="vol_rank",
                                   extra_metric_keys=["std_36m"])
            return {"ranking_date": rd.isoformat() if rd else None,
                    "top_stocks":   top,
                    "narrative":    _narr("Long-Term Low Vol 36M", len(top), top)}

        def _dy():
            rd, rows = _fetch_top_rows(
                conn, source_table="dividend_yield_ranked_derived",
                as_of=as_of, rank_col="yield_rank", top_n=top_n,
            )
            top = _format_top_rows(rows, rank_col="yield_rank",
                                   extra_metric_keys=["dividend_yield_pct",
                                                       "return_12m_pct",
                                                       "payout_years_5y"])
            return {"ranking_date": rd.isoformat() if rd else None,
                    "top_stocks":   top,
                    "narrative":    _narr("Dividend Yield(已過 yield trap filter)",
                                          len(top), top)}

        def _mom():
            rd, rows = _fetch_top_rows(
                conn, source_table="mom_12_1_ranked_derived",
                as_of=as_of, rank_col="mom_rank", top_n=top_n,
            )
            top = _format_top_rows(rows, rank_col="mom_rank",
                                   extra_metric_keys=["return_12m_1m"])
            return {"ranking_date": rd.isoformat() if rd else None,
                    "top_stocks":   top,
                    "narrative":    _narr("12-1 Momentum", len(top), top)}

        ltlv = _safe_screen("long_term_low_vol", _ltlv)
        dy   = _safe_screen("dividend_yield",    _dy)
        mom  = _safe_screen("mom_12_1",          _mom)
    finally:
        conn.close()

    return {
        "as_of":   as_of.isoformat(),
        "top_n":   top_n,
        "toolkit": "C_annual_low_risk",
        "factors": {
            "long_term_low_vol": ltlv,
            "dividend_yield":    dy,
            "mom_12_1":          mom,
        },
        "narrative": _compose_toolkit_narrative("C", ltlv, dy, mom),
    }


# ────────────────────────────────────────────────────────────
# Layer 5:Monthly trigger scan
# ────────────────────────────────────────────────────────────


def compute_monthly_trigger_scan(
    as_of: date,
    *,
    stock_id: str | None = None,
    top_n_per_type: int = 20,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Layer 5:Positive(YoY > 30% + 法人買超)/ Negative(YoY < -20% + 法人賣超) triggers。

    v3.32 hotfix(2026-05-18 production):原預設全攤 464 triggers → ~94KB,超 MCP
    context 限制被 Claude Desktop 自動轉檔。修法:
      - 加 stock_id kwarg → 只回該股 trigger(典型 0-2 個)
      - 不傳 stock_id 時走 summary 模式:預設只回 top N(按 |revenue_yoy_pct| 排序)
        per trigger_type + counts,避免 payload 爆量
    """
    conn = get_connection(database_url)
    try:
        with conn.cursor() as cur:
            cur.execute(
                "SELECT MAX(date) AS d FROM monthly_trigger_signals_derived WHERE market = 'TW' AND date <= %s",
                [as_of],
            )
            row = cur.fetchone()
        signal_date = row["d"] if row else None
        if signal_date is None:
            return {
                "as_of":   as_of.isoformat(),
                "toolkit": "Layer_5_monthly_trigger",
                "signal_date": None,
                "stock_filter": stock_id,
                "counts": {"positive_total": 0, "negative_total": 0},
                "positive_triggers": [],
                "negative_triggers": [],
                "narrative": "無 Layer 5 trigger 訊號(尚未跑 monthly_trigger builder 或 backfill)。",
            }

        # SQL:可選 stock_id filter + 統計 count
        sql_filter = "AND t.stock_id = %s" if stock_id else ""
        params: list[Any] = [signal_date]
        if stock_id:
            params.append(stock_id)

        with conn.cursor() as cur:
            cur.execute(
                f"""
                SELECT t.stock_id, t.trigger_type,
                       t.revenue_yoy_pct, t.institutional_20d,
                       t.shares_outstanding, t.institutional_pct,
                       t.action_hint, t.detail,
                       s.stock_name, s.industry_category
                  FROM monthly_trigger_signals_derived t
                  LEFT JOIN stock_info_ref s
                    ON s.market = t.market AND s.stock_id = t.stock_id
                 WHERE t.market = 'TW' AND t.date = %s {sql_filter}
                """,
                params,
            )
            rows = cur.fetchall()

        positives_all = []
        negatives_all = []
        for r in rows:
            item = {
                "stock_id":           r["stock_id"],
                "name":               r.get("stock_name"),
                "industry":           r.get("industry_category"),
                "revenue_yoy_pct":    _to_float(r.get("revenue_yoy_pct")),
                "institutional_pct":  _to_float(r.get("institutional_pct")),
                "action_hint":        r.get("action_hint"),
                "rationale":          (r.get("detail") or {}).get("rationale"),
            }
            if r["trigger_type"] == "positive":
                positives_all.append(item)
            else:
                negatives_all.append(item)

        pos_total = len(positives_all)
        neg_total = len(negatives_all)

        # 若指定 stock_id → 全回(典型 0-2 筆,payload 小);否則按 yoy 排序取 top N
        if stock_id:
            positives = positives_all
            negatives = negatives_all
        else:
            positives = sorted(
                positives_all, key=lambda x: x.get("revenue_yoy_pct") or 0, reverse=True,
            )[:top_n_per_type]
            negatives = sorted(
                negatives_all, key=lambda x: x.get("revenue_yoy_pct") or 0,
            )[:top_n_per_type]
    finally:
        conn.close()

    narrative = (
        f"Layer 5 trigger 偵測:positive {pos_total} 個 / negative {neg_total} 個"
    )
    if stock_id:
        in_pos = any(t.get("stock_id") == stock_id for t in positives_all)
        in_neg = any(t.get("stock_id") == stock_id for t in negatives_all)
        if in_pos:
            narrative += f";{stock_id} 命中 positive trigger"
        elif in_neg:
            narrative += f";{stock_id} 命中 negative trigger"
        else:
            narrative += f";{stock_id} 未命中任何 trigger"
    else:
        narrative += f";已 truncate 至 top {top_n_per_type} per type(完整清單請帶 stock_id 過濾)"
    narrative += "(僅 conviction adjustment hint,不獨立配資)。"

    return {
        "as_of":             as_of.isoformat(),
        "signal_date":       signal_date.isoformat(),
        "toolkit":           "Layer_5_monthly_trigger",
        "stock_filter":      stock_id,
        "counts":            {"positive_total": pos_total, "negative_total": neg_total},
        "positive_triggers": positives,
        "negative_triggers": negatives,
        "narrative":         narrative,
    }


# ────────────────────────────────────────────────────────────
# Narrative helpers
# ────────────────────────────────────────────────────────────


def _narr(label: str, n: int, top: list[dict[str, Any]]) -> str:
    if n == 0:
        return f"{label}:無 top stocks(尚未跑 builder 或 universe 全被排除)。"
    first = top[0]
    return (
        f"{label} top {n}:首位 {first.get('stock_id')} "
        f"{first.get('name') or ''} ({first.get('industry') or '未分類'})。"
    )


def _compose_toolkit_narrative(toolkit: str, *factors: dict) -> str:
    """每 toolkit 主要 1-3 句 overall view。"""
    ok_count = sum(1 for f in factors if isinstance(f, dict) and "error" not in f)
    err_count = len(factors) - ok_count
    base = f"Toolkit {toolkit}: {ok_count}/{len(factors)} factor screens 正常。"
    if err_count > 0:
        errors = [
            f.get("section") for f in factors
            if isinstance(f, dict) and "error" in f
        ]
        base += f"異常 {err_count} 個:{','.join([e for e in errors if e])}。"
    return base
