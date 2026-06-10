"""技術指標域 tools — 4 子類 + preset 組合 + indicators 整併入口。

實作層:src/fusion/indicator_assembly.py。
indicator_momentum / volatility / volume / pattern / stack 預設不註冊 MCP
(v4.19 整併進 indicators),留給 dashboard / direct python;
註冊見 server.py mcp.tool() 區塊。
"""

from __future__ import annotations

from typing import Any

from mcp_server.tools._shared import _MAX_LOOKBACK_DAYS, _clamp, _parse_date


def _assemble(category: str, stock_id: str, date: str,
              indicators: list[str] | None, lookback_days: int,
              database_url: str | None) -> dict[str, Any]:
    """E 視角 4 個子類工具共用:依 category 過濾 indicators 後組裝。"""
    from fusion.indicator_assembly import assemble_indicators, category_indicators

    return assemble_indicators(
        stock_id, _parse_date(date),
        category_indicators(category, indicators),
        lookback_days=lookback_days, database_url=database_url,
    )


def indicator_momentum(
    stock_id: str, date: str, indicators: list[str] | None = None,
    lookback_days: int = 60, *, database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion E 視角:動量 / 趨勢 / 強度類指標(series + events)。

    可選 indicators:macd / rsi / kd / adx / ma / ichimoku / williams_r /
    cci / coppock(可帶或不帶 `_core` 後綴);省略 = 全部。

    Returns:
        {stock_id, as_of, indicator_count, indicators, missing}
        indicators[<core>] = {value_date, series, events}。
    """
    return _assemble("momentum", stock_id, date, indicators, lookback_days, database_url)


def indicator_volatility(
    stock_id: str, date: str, indicators: list[str] | None = None,
    lookback_days: int = 60, *, database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion E 視角:波動 / 通道類指標(series + events)。

    可選 indicators:bollinger / keltner / donchian / atr;省略 = 全部。
    """
    return _assemble("volatility", stock_id, date, indicators, lookback_days, database_url)


def indicator_volume(
    stock_id: str, date: str, indicators: list[str] | None = None,
    lookback_days: int = 60, *, database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion E 視角:量能類指標(series + events)。

    可選 indicators:obv / vwap / mfi;省略 = 全部。
    """
    return _assemble("volume", stock_id, date, indicators, lookback_days, database_url)


def indicator_pattern(
    stock_id: str, date: str, indicators: list[str] | None = None,
    lookback_days: int = 60, *, database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion E 視角:型態 / 價位類指標(series + events)。

    可選 indicators:candlestick_pattern / support_resistance / trendline;
    省略 = 全部。
    """
    return _assemble("pattern", stock_id, date, indicators, lookback_days, database_url)


def indicator_stack(
    stock_id: str, date: str, preset: str = "default",
    lookback_days: int = 60, *, database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion E 視角:預設指標組合(series + events)。

    preset:default(MACD+RSI+KD+Bollinger+MA)/ day_trade(KD+RSI+VWAP+
    Bollinger)/ swing(MACD+MA+ADX+ATR)/ position(MA+Ichimoku+OBV+SR)。

    Returns:
        {stock_id, as_of, indicator_count, indicators, missing}
    """
    from fusion.indicator_assembly import INDICATOR_STACK_PRESETS, assemble_indicators

    cores = INDICATOR_STACK_PRESETS.get(preset, INDICATOR_STACK_PRESETS["default"])
    return assemble_indicators(
        stock_id, _parse_date(date), cores,
        lookback_days=lookback_days, database_url=database_url,
    )


# ────────────────────────────────────────────────────────────
# Fusion Layer · Consolidated 入口(v4.19 — 5 個 indicator_* 工具 → 1)
# ────────────────────────────────────────────────────────────


def indicators(
    stock_id: str,
    date: str,
    groups: list[str] | None = None,
    cores: list[str] | None = None,
    preset: str | None = None,
    lookback_days: int = 60,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """Fusion E 視角:技術指標 series + events(整併 5 個 indicator_* 工具)。

    整併 indicator_momentum / volatility / volume / pattern / stack。選擇優先序:
      1. cores  — 明確 core 清單(如 ["macd","rsi","atr"];可省略 `_core` 後綴)
      2. groups — 子類清單,取自 {momentum, volatility, volume, pattern}
      3. preset — {default, day_trade, swing, position}
      4. 皆未給 → preset="default"(MACD+RSI+KD+Bollinger+MA,5 cores)

    輸出大小:預設只回 5 cores。`groups` 多選會把整類 cores 攤開(momentum 一類
    就 9 cores),series 隨之放大 — 多 group 請求請自行斟酌。

    Returns:
        {stock_id, as_of, selection, indicator_count, indicators, missing}
    """
    from fusion.indicator_assembly import (
        INDICATOR_STACK_PRESETS, assemble_indicators, category_indicators,
    )

    as_of = _parse_date(date)

    if cores:
        selected = [
            c if str(c).strip().lower().endswith("_core")
            else f"{str(c).strip().lower()}_core"
            for c in cores
        ]
        selection: dict[str, Any] = {"mode": "cores", "value": selected}
    elif groups:
        selected = []
        seen: set[str] = set()
        for g in groups:
            for core in category_indicators(str(g).strip().lower(), None):
                if core not in seen:
                    seen.add(core)
                    selected.append(core)
        selection = {"mode": "groups", "value": [str(g) for g in groups]}
    else:
        key = preset if preset in INDICATOR_STACK_PRESETS else "default"
        selected = list(INDICATOR_STACK_PRESETS[key])
        selection = {"mode": "preset", "value": key}

    result = assemble_indicators(
        stock_id, as_of, selected,
        lookback_days=_clamp(lookback_days, 1, _MAX_LOOKBACK_DAYS), database_url=database_url,
    )
    result["selection"] = selection
    return result
