"""大盤環境域 tools — events 時間軸 / dashboard 快照 / market_overview 整併入口。

實作層:src/fusion/market_events.py / market_dashboard.py。
market_events / market_dashboard 預設不註冊 MCP(v4.19 整併進 market_overview),
留給 dashboard / direct python;註冊見 server.py mcp.tool() 區塊。
"""

from __future__ import annotations

from typing import Any

from mcp_server.tools._shared import _MAX_LOOKBACK_DAYS, _clamp, _parse_date, _section_error


def market_events(
    start_date: str,
    end_date: str,
    severity_min: str = "info",
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion D 視角:大盤環境事件時間軸。

    撈 7 個 environment cores(taiex / us_market / exchange_rate / fear_greed /
    market_margin / business_indicator / commodity_macro)寫進 facts 的事件,
    依日期區間 [start_date, end_date] + 最低嚴重度 filter,以統一 Event schema
    回傳時間軸。

    severity_min:info / notable / warning / critical(預設 info = 全收)。
    嚴重度由各 core 寫入 fact 時決定,本層只 filter 不二次判斷。

    Returns:
        {start_date, end_date, severity_min, event_count, by_severity,
         events: [{date, source, kind, severity, statement, value, metadata}, ...]}
        events 依 (date DESC, severity DESC) 排序。
    """
    from fusion.market_events import market_events as _market_events

    return _market_events(
        _parse_date(start_date),
        _parse_date(end_date),
        severity_min=severity_min,
        database_url=database_url,
    )


def market_dashboard(
    date: str,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion D 視角:大盤環境快照。

    讀 7 個 environment cores(taiex / us_market / exchange_rate / fear_greed /
    market_margin / business_indicator / commodity_macro)的最新一筆,抽出各核心
    headline metric + 歷史百分位(percentile_252)+ 短期變化。

    純資料快照 — 不打主觀標籤,由 LLM 自行判讀大盤環境。

    Returns:
        {as_of, component_count, components, missing}
        每個 component:{latest_date, value, change_pct, percentile_252, state, ...}
    """
    from fusion.market_dashboard import market_dashboard as _market_dashboard

    return _market_dashboard(_parse_date(date), database_url=database_url)


# ────────────────────────────────────────────────────────────
# Fusion Layer · Consolidated 入口(v4.19 — market_dashboard + market_events → 1)
# ────────────────────────────────────────────────────────────


def market_overview(
    date: str,
    events_lookback_days: int = 30,
    severity_min: str = "notable",
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion D 視角:大盤環境總覽(整併 market_dashboard + market_events)。

    - dashboard:7 個 environment cores 最新 headline metric + 歷史百分位
    - events:[date - events_lookback_days, date] 區間環境事件時間軸

    輸出大小:events 預設 severity_min="notable"(濾掉 info 噪音)+ 30 天窗。
    要更早 / 更全 → 調大 events_lookback_days 或 severity_min="info"。

    Returns:
        {as_of, dashboard, events} — 某段失敗 → 該段 = {"error": ..., "section": ...}
    """
    from datetime import timedelta

    as_of = _parse_date(date)
    out: dict[str, Any] = {"as_of": as_of.isoformat()}

    try:
        from fusion.market_dashboard import market_dashboard as _md
        out["dashboard"] = _md(as_of, database_url=database_url)
    except Exception as e:  # noqa: BLE001
        out["dashboard"] = _section_error("dashboard", e)

    try:
        from fusion.market_events import market_events as _me
        start = as_of - timedelta(days=_clamp(events_lookback_days, 1, _MAX_LOOKBACK_DAYS))
        out["events"] = _me(start, as_of, severity_min=severity_min,
                            database_url=database_url)
    except Exception as e:  # noqa: BLE001
        out["events"] = _section_error("events", e)

    return out
