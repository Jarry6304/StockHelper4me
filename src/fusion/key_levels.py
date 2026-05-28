"""Fusion Layer · Integration 端口 — key_levels(B 視角)。

對齊 m3Spec/fusion_layer.md §8.1 + api_roadmap_v1.md §4.3。

整合三個來源的支撐 / 壓力價位,以 1% bucket cluster 後依強度排序:
1. `support_resistance_core` facts — Support / Resistance 價位(metadata.price)。
2. `trendline_core` structural snapshot — 有效趨勢線(取最後 anchor pivot 價位)。
3. `neely_core` structural snapshot — `flat_fib_zones`(P1.1,取區間中點)。

不引入新規則 — 只整合 cores 既有輸出(fusion_layer §9 #1)。
"""

from __future__ import annotations

from datetime import date
from typing import Any

from fusion._shared import cluster_price_levels
from fusion.raw._db import fetch_facts, fetch_structural_latest, get_connection


def key_levels(
    stock_id: str,
    as_of: date,
    *,
    lookback_days: int = 120,
    top_n: int = 20,
    database_url: str | None = None,
    conn: Any = None,
) -> dict[str, Any]:
    """個股關鍵支撐 / 壓力價位(整合 SR + 趨勢線 + Neely Fib)。

    Args:
        stock_id: 股票代號。
        as_of: 查詢日(look-ahead 上界)。
        lookback_days: SR facts 回看天數(預設 120)。
        top_n: cap 到前 N 個 levels(預設 20;對齊 LLM context 預算)。0 = 不 cap。
        database_url / conn: 連線。

    Returns:
        {stock_id, as_of, source_point_count, level_count_total, level_count, levels}
        levels 依 price 升序;每筆:{price, low, high, sources, strength, member_count}。
        v4.30 後加 top_n cap:`level_count` = 取的 top N,`level_count_total` = cluster 後總數。
    """
    own_conn = conn is None
    if own_conn:
        conn = get_connection(database_url)
    try:
        sr_facts = fetch_facts(
            conn,
            stock_ids=[stock_id],
            as_of=as_of,
            lookback_days=lookback_days,
            cores=["support_resistance_core"],
        )
        structural = fetch_structural_latest(
            conn, stock_id=stock_id, as_of=as_of, cores=["trendline_core", "neely_core"]
        )
    finally:
        if own_conn:
            conn.close()

    points: list[dict[str, Any]] = []
    points.extend(_sr_points(sr_facts))
    for row in structural:
        snap = row.get("snapshot") or {}
        if row.get("core_name") == "trendline_core":
            points.extend(_trendline_points(snap))
        elif row.get("core_name") == "neely_core":
            tf = row.get("timeframe")
            points.extend(_neely_fib_points(snap, timeframe=str(tf) if tf else ""))

    levels_all = cluster_price_levels(points)
    # v4.30 Finding 4:按 strength * member_count 排序取 top N(LLM context cap)。
    # 排序前先選 top N,再按 price 升序回傳(對齊原 v4.19 API 慣例)。
    if top_n and len(levels_all) > top_n:
        levels_top = sorted(
            levels_all,
            key=lambda lv: (lv.get("strength", 0), lv.get("member_count", 0)),
            reverse=True,
        )[:top_n]
        levels = sorted(levels_top, key=lambda lv: lv["price"])
    else:
        levels = levels_all
    return {
        "stock_id": stock_id,
        "as_of": as_of.isoformat(),
        "source_point_count": len(points),
        "level_count_total": len(levels_all),
        "level_count": len(levels),
        "levels": levels,
    }


def _is_num(v: Any) -> bool:
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _sr_points(facts: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """support_resistance_core facts → 價位點。"""
    out: list[dict[str, Any]] = []
    for f in facts:
        md = f.get("metadata") or {}
        ek = md.get("event_kind")
        price = md.get("price")
        if ek in ("Support", "Resistance") and _is_num(price):
            out.append({
                "price": float(price),
                "source": f"sr_{ek.lower()}",
                "touch_count": md.get("touch_count"),
            })
    return out


def _trendline_points(snapshot: dict[str, Any]) -> list[dict[str, Any]]:
    """trendline_core snapshot → 趨勢線價位點(Valid + Broken 都進)。

    v4.30 Finding 2 後加 Broken trendlines:破前壓力 = 將來支撐(經典 SR roleflip),
    對長期上漲股(如 2330)Valid 趨勢線常 0 個但 Broken 50+ 個都是歷史 SR。
    Broken 用獨立 source `trendline_historical`,跟 Valid `trendline` 區分,讓 1%
    bucket cluster 內形成 strength=2(Valid + Broken 同價位 = 雙重歷史 SR 訊號)。
    """
    out: list[dict[str, Any]] = []
    for tl in snapshot.get("trendlines") or []:
        status = str(tl.get("status"))
        if status not in ("Valid", "Broken"):
            continue
        anchors = tl.get("anchor_pivots") or []
        if anchors and _is_num(anchors[-1].get("price")):
            out.append({
                "price": float(anchors[-1]["price"]),
                "source": "trendline" if status == "Valid" else "trendline_historical",
                "direction": tl.get("direction"),
            })
    return out


def _neely_fib_points(snapshot: dict[str, Any], *, timeframe: str = "daily") -> list[dict[str, Any]]:
    """neely_core snapshot.flat_fib_zones → 取區間中點為價位點。

    v4.30 Finding 4 後 source 加 timeframe 後綴(`neely_fib_daily` / `_weekly` /
    `_monthly`),讓多 timeframe 共振於同 1% bucket 時 cluster strength 真實升高
    (原本 source 都標 `neely_fib` → distinct=1)。
    """
    src = f"neely_fib_{timeframe}" if timeframe else "neely_fib"
    out: list[dict[str, Any]] = []
    for z in snapshot.get("flat_fib_zones") or []:
        lo, hi = z.get("low"), z.get("high")
        if _is_num(lo) and _is_num(hi):
            out.append({
                "price": (float(lo) + float(hi)) / 2.0,
                "source": src,
                "ratio": z.get("source_ratio"),
            })
    return out
