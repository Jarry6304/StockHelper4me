"""個股價位域 tools — 支撐壓力 / 止損止盈 / K 線型態 + stock_levels 整併入口。

實作層:src/fusion/key_levels.py / stop_loss.py / pattern_scan.py。
key_levels / stop_loss_calc / pattern_scan 預設不註冊 MCP(v4.19 整併進
stock_levels),留給 dashboard / direct python;註冊見 server.py mcp.tool() 區塊。
"""

from __future__ import annotations

from typing import Any

from mcp_server.tools._shared import _parse_date, _read_materialized_snapshot, _section_error


def key_levels(
    stock_id: str,
    date: str,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion B 視角:個股關鍵支撐 / 壓力價位。

    整合三來源並以 1% bucket cluster:support_resistance_core(SR 價位)、
    trendline_core(有效趨勢線)、neely_core flat_fib_zones(Fibonacci 區)。
    strength = 該價位被幾個來源確認(越多越強)。

    Returns:
        {stock_id, as_of, source_point_count, level_count,
         levels: [{price, low, high, sources, strength, member_count}, ...]}
        levels 依 price 升序。
    """
    from fusion.key_levels import key_levels as _key_levels

    return _key_levels(stock_id, _parse_date(date), database_url=database_url)


def stop_loss_calc(
    stock_id: str,
    entry_price: float,
    date: str,
    direction: str = "long",
    atr_mult: float = 2.0,
    reward_risk_ratio: float = 2.0,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion B 視角:止損 / 止盈計算。

    給定進場價,整合 ATR(atr_core)+ key_levels(SR / 趨勢線 / Neely Fib)算出
    止損、止盈候選。純計算 — 同時呈現 ATR-based 與 level-based 候選 + 距離百分比,
    不替你抉擇(由 LLM 判讀)。

    direction:long(預設)或 short。atr_mult 為止損 ATR 倍數;reward_risk_ratio
    為 ATR 止盈相對止損的報酬風險比。

    Returns:
        {stock_id, as_of, direction, entry_price, atr, stops, targets}
        stops/targets 各含 atr_based + nearest_level,每筆 {price, distance,
        distance_pct}。
    """
    from fusion.stop_loss import stop_loss as _stop_loss

    return _stop_loss(
        stock_id, entry_price, _parse_date(date),
        direction=direction, atr_mult=atr_mult,
        reward_risk_ratio=reward_risk_ratio, database_url=database_url,
    )


def pattern_scan(
    stock_id: str,
    date: str,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion B 視角:近期 K 線型態 + 支撐 / 壓力 context。

    撈 candlestick_pattern_core 近期偵測到的 K 線型態,為每個型態補上
    key_levels context(型態發生價是否貼近支撐 / 壓力 — 同型態在支撐附近
    與在中段意義不同)。型態本身由 core 偵測,本層只整合 key_levels。

    Returns:
        {stock_id, as_of, pattern_count,
         patterns: [{date, pattern, trend_context, strength, price,
                     level_context}, ...]}  依 date 降序。
    """
    from fusion.pattern_scan import pattern_scan as _pattern_scan

    return _pattern_scan(stock_id, _parse_date(date), database_url=database_url)


# ────────────────────────────────────────────────────────────
# Fusion Layer · Consolidated 入口(v4.19 — key_levels + pattern_scan + stop_loss → 1)
# ────────────────────────────────────────────────────────────


def stock_levels(
    stock_id: str,
    date: str,
    entry_price: float | None = None,
    direction: str = "long",
    atr_mult: float = 2.0,
    reward_risk_ratio: float = 2.0,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion B 視角:個股價位總覽(整併 key_levels + pattern_scan + stop_loss_calc)。

    - key_levels:支撐 / 壓力(SR + 趨勢線 + Neely Fib,1% cluster)
    - patterns:近期 K 線型態 + 支撐 / 壓力 context
    - stop_loss:止損 / 止盈計算 — **僅當給 entry_price 才算**,否則 None

    Returns:
        {stock_id, as_of, key_levels, patterns, stop_loss} — 某段失敗 → 該段
        = {"error": ..., "section": ...};未給 entry_price → stop_loss = None。
    """
    as_of = _parse_date(date)
    out: dict[str, Any] = {"stock_id": stock_id, "as_of": as_of.isoformat()}

    try:
        # v4.32 Golden L3:先讀物化 levels_fusion(哨兵 tf _all_);缺 → compute fallback。
        # pattern_scan / stop_loss 維持 read-time(非物化範圍,且仍內部呼叫 key_levels)。
        doc = _read_materialized_snapshot(
            stock_id, as_of, "levels_fusion", timeframe="_all_",
            database_url=database_url,
        )
        if doc is not None:
            out["key_levels"] = doc
        else:
            from fusion.key_levels import key_levels as _kl
            out["key_levels"] = _kl(stock_id, as_of, database_url=database_url)
    except Exception as e:  # noqa: BLE001
        out["key_levels"] = _section_error("key_levels", e)

    try:
        from fusion.pattern_scan import pattern_scan as _ps
        out["patterns"] = _ps(stock_id, as_of, database_url=database_url)
    except Exception as e:  # noqa: BLE001
        out["patterns"] = _section_error("patterns", e)

    if entry_price is None:
        out["stop_loss"] = None
    else:
        try:
            from fusion.stop_loss import stop_loss as _sl
            out["stop_loss"] = _sl(
                stock_id, entry_price, as_of,
                direction=direction, atr_mult=atr_mult,
                reward_risk_ratio=reward_risk_ratio, database_url=database_url,
            )
        except Exception as e:  # noqa: BLE001
            out["stop_loss"] = _section_error("stop_loss", e)

    return out
