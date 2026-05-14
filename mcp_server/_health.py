"""Tool 2 內部演算法:`stock_health` — 個股 4 維健康度評分。

對齊 plan §Tool 2:
1. 撈 `agg.as_of()` 全 cores 90-day
2. 4 維 score 加權(technical / chip / valuation / fundamental)
3. top_signals 跨 cores 排序取 top 5
4. narrative 規則組裝

呼叫端:`mcp_server.tools.data.stock_health()`。

設計:
- 跨 cores 加權邏輯屬 Aggregation Layer 整合層責任(cores_overview §10.0)
- 4 維 dimension cores list 寫死(不外部化),對齊 NEELY constants pattern
- Bullish/Bearish 分類:explicit dict + keyword fallback
"""

from __future__ import annotations

from datetime import date
from math import exp
from typing import Any

# ────────────────────────────────────────────────────────────
# 4 維 cores 分類(對齊 cores_overview.md §8 + m3Spec/*.md)
# ────────────────────────────────────────────────────────────

_TECHNICAL_CORES: frozenset[str] = frozenset({
    # P1 indicator(8)
    "ma_core", "macd_core", "rsi_core", "kd_core", "adx_core",
    "atr_core", "bollinger_core", "obv_core",
    # P3 indicator(8)
    "williams_r_core", "cci_core", "keltner_core", "donchian_core",
    "vwap_core", "mfi_core", "coppock_core", "ichimoku_core",
    # P2 pattern(3)
    "support_resistance_core", "candlestick_pattern_core", "trendline_core",
    # P0 wave(neely 屬技術結構,但本 health 不重複用 — Tool 1 已專門做)
})

_CHIP_CORES: frozenset[str] = frozenset({
    "institutional_core",
    "margin_core",
    "foreign_holding_core",
    "day_trading_core",
    "shareholder_core",
})

_VALUATION_CORES: frozenset[str] = frozenset({"valuation_core"})
_FUNDAMENTAL_CORES: frozenset[str] = frozenset({
    "revenue_core",
    "financial_statement_core",
})

# 4 維 score → overall_score 權重(對齊 plan 投資決策論述)
_DIM_WEIGHTS: dict[str, float] = {
    "technical":   0.30,
    "chip":        0.25,
    "valuation":   0.20,
    "fundamental": 0.25,
}

# ────────────────────────────────────────────────────────────
# Kind 分類(+1 bullish / -1 bearish / 0 neutral)
# ────────────────────────────────────────────────────────────

# Explicit per-kind 標籤(優先級高);對齊 spec 各 EventKind
_KIND_SIGN_EXPLICIT: dict[str, int] = {
    # macd_core
    "GoldenCross":               +1,
    "DeathCross":                -1,
    "BullishDivergence":         +1,
    "BearishDivergence":         -1,
    "HistogramZeroCross":         0,  # neutral(direction 看上下文)
    # rsi_core
    "RsiOversold":               +1,  # contrarian
    "RsiOverbought":             -1,
    "OversoldExit":              +1,
    "OverboughtExit":            -1,
    "FailureSwing":               0,  # neutral
    # kd_core(類似 rsi 邏輯;指標出現名詞同)
    "KdOversold":                +1,
    "KdOverbought":              -1,
    # ma_core
    "MaBullishCross":            +1,
    "MaBearishCross":            -1,
    "MaGoldenCross":             +1,
    "MaDeathCross":              -1,
    "AboveMaStreak":             +1,
    # adx_core
    "AdxStrongTrendStart":       +1,
    "DiCrossover":                0,
    # bollinger_core(Round 4 transition pattern)
    "EnteredUpperBand":          -1,  # overbought
    "ExitedUpperBand":           +1,
    "EnteredLowerBand":          +1,
    "ExitedLowerBand":           -1,
    "SqueezeStart":               0,
    "BandwidthExpand":            0,
    # atr_core
    "AtrExpansion":              -1,  # 波動放大 = warning
    "AtrContraction":             0,
    # obv_core
    "ObvBullishDivergence":      +1,
    "ObvBearishDivergence":      -1,
    # williams_r_core
    "OversoldStreak":            +1,
    "OverboughtStreak":          -1,
    # cci_core
    "ExtremeHigh":               -1,
    "ExtremeLow":                +1,
    "OverboughtEntry":           -1,
    "OversoldEntry":             +1,
    "ZeroCrossPositive":         +1,
    "ZeroCrossNegative":         -1,
    # ichimoku_core
    "TkBullishCross":            +1,
    "TkBearishCross":            -1,
    "CloudBreakoutUp":           +1,
    "CloudBreakoutDown":         -1,
    # candlestick_pattern_core(對齊型態語意)
    "Hammer":                    +1,
    "BullishEngulfing":          +1,
    "MorningStar":               +1,
    "ThreeWhiteSoldiers":        +1,
    "MarubozuBullish":           +1,
    "ShootingStar":              -1,
    "BearishEngulfing":          -1,
    "EveningStar":               -1,
    "ThreeBlackCrows":           -1,
    "HangingMan":                -1,
    "MarubozuBearish":           -1,
    "Doji":                       0,
    # institutional_core
    "LargeNetBuy":               +1,
    "LargeNetSell":              -1,
    "DivergenceWithinInstitution": 0,
    # margin_core
    "MarginNetBuy":              +1,
    "MarginNetSell":             -1,
    "MaintenanceLow":            -1,
    # foreign_holding_core
    "HoldingMilestoneHigh":      +1,
    "HoldingMilestoneLow":       -1,
    "HoldingMilestoneHighAnnual": +1,
    "HoldingMilestoneLowAnnual": -1,
    "SignificantSingleDayChange": 0,
    "LimitNearAlert":            -1,
    # shareholder_core
    "ConcentrationUp":           +1,  # 大戶集中 = 籌碼穩
    "ConcentrationDown":         -1,
    "RetailExit":                +1,  # 散戶退場 = 反指標
    # day_trading_core
    "RatioExtremeHigh":          -1,  # 當沖比過高 = 投機
    "RatioExtremeLow":           +1,
    # financial_statement_core
    "RoeHigh":                   +1,
    "GrossMarginRising":         +1,
    "GrossMarginFalling":        -1,
    "DebtRatioRising":           -1,
    "OperatingCashFlowNegative": -1,
    "FreeCashFlowNegativeStreak": -1,
    "EpsTurnPositive":           +1,
    "EpsTurnNegative":           -1,
    # revenue_core
    "RevenueYoyStrong":          +1,
    "RevenueYoyWeak":            -1,
}


def _kind_sign(kind: str | None) -> int:
    """+1 bullish / -1 bearish / 0 neutral。Explicit dict 優先,fallback keyword 啟發式。"""
    if not kind:
        return 0
    if kind in _KIND_SIGN_EXPLICIT:
        return _KIND_SIGN_EXPLICIT[kind]
    k = kind.lower()
    bullish_kw = ("bullish", "golden", "rising", "turningup", "breakoutup",
                  "oversold", "buy", "milestonehigh", "epsturnpositive")
    bearish_kw = ("bearish", "death", "falling", "turningdown", "breakdown",
                  "overbought", "sell", "milestonelow", "epsturnnegative",
                  "negative", "danger")
    if any(s in k for s in bullish_kw):
        return +1
    if any(s in k for s in bearish_kw):
        return -1
    return 0


# 時間 decay 半衰期
_DECAY_DAYS = 14.0


# ────────────────────────────────────────────────────────────
# Public API
# ────────────────────────────────────────────────────────────

def compute_stock_health(
    stock_id: str,
    as_of: date,
    *,
    lookback_days: int = 90,
    database_url: str | None = None,
) -> dict[str, Any]:
    """主入口 — 個股 4 維健康度。

    Args:
        stock_id: 股票代號(例 "2330")
        as_of: 查詢日
        lookback_days: facts 期間。預設 90
        database_url: 可選 PG 連線字串

    Returns:
        dict 結構對齊 plan §Tool 2(~2 KB / ~500 tokens)
    """
    from agg import as_of as agg_as_of

    snapshot = agg_as_of(
        stock_id,
        as_of,
        lookback_days=lookback_days,
        include_market=False,
        database_url=database_url,
    )

    facts_dicts = [f.to_dict() for f in snapshot.facts]

    # 4 維 score 算
    technical = _score_dim(facts_dicts, _TECHNICAL_CORES, as_of)
    chip      = _score_dim(facts_dicts, _CHIP_CORES, as_of)
    valuation = _score_dim(facts_dicts, _VALUATION_CORES, as_of)
    fundamental = _score_dim(facts_dicts, _FUNDAMENTAL_CORES, as_of)

    # 副 metadata(各維 latest 訊號)
    technical_meta = _dim_meta(facts_dicts, _TECHNICAL_CORES)
    chip_meta      = _dim_meta(facts_dicts, _CHIP_CORES)
    valuation_meta = _dim_meta(facts_dicts, _VALUATION_CORES)
    fundamental_meta = _dim_meta(facts_dicts, _FUNDAMENTAL_CORES)

    # overall_score 加權平均
    overall = (
        technical * _DIM_WEIGHTS["technical"]
        + chip * _DIM_WEIGHTS["chip"]
        + valuation * _DIM_WEIGHTS["valuation"]
        + fundamental * _DIM_WEIGHTS["fundamental"]
    )

    # Top 5 signals(by weight × decay,desc)
    top_signals = _top_signals(facts_dicts, as_of, limit=5)

    # current_price(從 indicator_latest 拿;若沒有 fallback 0.0)
    current_price = _extract_current_price(snapshot)

    # Narrative 組裝
    narrative = _compose_narrative(
        technical=technical,
        chip=chip,
        valuation=valuation,
        fundamental=fundamental,
        overall=overall,
    )

    return {
        "stock_id":      stock_id,
        "as_of":         as_of.isoformat(),
        "current_price": current_price,
        "overall_score": round(overall, 1),
        "dimensions": {
            "technical":   {"score": technical,   **technical_meta},
            "chip":        {"score": chip,        **chip_meta},
            "valuation":   {"score": valuation,   **valuation_meta},
            "fundamental": {"score": fundamental, **fundamental_meta},
        },
        "top_signals": top_signals,
        "narrative":   narrative,
    }


# ────────────────────────────────────────────────────────────
# Internal helpers
# ────────────────────────────────────────────────────────────

def _score_dim(facts: list[dict], cores: frozenset[str], as_of: date) -> int:
    """Weighted sum 該維 cores 的 facts → clamp [-100, +100]。

    每 fact 貢獻 = `sign * 20 * decay`。
    """
    score = 0.0
    for f in facts:
        if f.get("source_core") not in cores:
            continue
        kind = (f.get("metadata") or {}).get("kind") or _extract_kind_from_statement(f.get("statement", ""))
        sign = _kind_sign(kind)
        if sign == 0:
            continue

        days_ago = _days_ago(f.get("fact_date"), as_of)
        if days_ago is None:
            continue
        decay = exp(-days_ago / _DECAY_DAYS)
        score += sign * 20 * decay

    return int(max(-100, min(100, score)))


def _dim_meta(facts: list[dict], cores: frozenset[str]) -> dict[str, str]:
    """組合該維的 metadata 摘要:trend / latest_signal 等。

    規則:取近期 fact 中 sign 不為 0 的最大絕對值,描述方向。
    """
    bullish_count = 0
    bearish_count = 0
    for f in facts:
        if f.get("source_core") not in cores:
            continue
        kind = (f.get("metadata") or {}).get("kind") or _extract_kind_from_statement(f.get("statement", ""))
        sign = _kind_sign(kind)
        if sign > 0:
            bullish_count += 1
        elif sign < 0:
            bearish_count += 1

    if bullish_count > bearish_count * 1.5:
        trend = "bullish"
    elif bearish_count > bullish_count * 1.5:
        trend = "bearish"
    else:
        trend = "mixed" if (bullish_count + bearish_count) > 0 else "quiet"

    return {
        "trend":          trend,
        "bullish_count":  bullish_count,
        "bearish_count":  bearish_count,
    }


def _top_signals(facts: list[dict], as_of: date, *, limit: int = 5) -> list[dict[str, Any]]:
    """跨 cores 按 weight × decay 排序取 top-N(只回 sign != 0 的 facts)。"""
    weighted: list[tuple[float, dict]] = []
    for f in facts:
        kind = (f.get("metadata") or {}).get("kind") or _extract_kind_from_statement(f.get("statement", ""))
        sign = _kind_sign(kind)
        if sign == 0:
            continue
        days_ago = _days_ago(f.get("fact_date"), as_of)
        if days_ago is None:
            continue
        decay = exp(-days_ago / _DECAY_DAYS)
        weight = abs(sign) * decay
        weighted.append((weight, {
            "date":   _date_str(f.get("fact_date")),
            "core":   f.get("source_core"),
            "kind":   kind,
            "sign":   sign,
            "weight": round(weight, 3),
        }))
    weighted.sort(key=lambda x: x[0], reverse=True)
    return [w[1] for w in weighted[:limit]]


def _compose_narrative(
    *,
    technical: int,
    chip: int,
    valuation: int,
    fundamental: int,
    overall: float,
) -> str:
    """規則組裝 1 句敘述。"""
    parts: list[str] = []
    dims = [
        ("技術面", technical),
        ("籌碼面", chip),
        ("估值",   valuation),
        ("基本面", fundamental),
    ]
    # 依絕對值排序找主導因素
    dims.sort(key=lambda x: abs(x[1]), reverse=True)
    for label, score in dims[:2]:
        if abs(score) < 10:
            continue
        direction = "強勢" if score > 30 else ("偏多" if score > 0 else ("偏弱" if score > -30 else "弱勢"))
        parts.append(f"{label}{direction}({score:+d})")

    if not parts:
        body = "各維度均衡"
    else:
        body = "、".join(parts)

    if overall > 50:
        verdict = "整體看多"
    elif overall > 15:
        verdict = "整體略多"
    elif overall > -15:
        verdict = "整體中性"
    elif overall > -50:
        verdict = "整體略空"
    else:
        verdict = "整體看空"

    return f"{body};{verdict}({overall:+.0f}/100)。"


def _extract_current_price(snapshot) -> float:
    """從 indicator_latest 推 current_price。

    優先 ma_core(因 ma_core 必含當日 close);找不到 fallback 0.0。
    """
    indicator_latest = snapshot.indicator_latest
    for key, row in indicator_latest.items():
        if not key.startswith("ma_core"):
            continue
        value = row.value or {}
        series = value.get("series")
        if not isinstance(series, list) or not series:
            continue
        last = series[-1]
        if isinstance(last, dict) and "close" in last:
            try:
                return float(last["close"])
            except (TypeError, ValueError):
                pass
    return 0.0


def _extract_kind_from_statement(statement: str) -> str | None:
    """Fallback 從 fact.statement 第一 token 抽 kind。"""
    if not statement:
        return None
    head = statement.split(" ", 1)[0]
    return head if head and head[0].isupper() and head.isalpha() else None


def _days_ago(fact_date: Any, as_of: date) -> int | None:
    """Robust date 解析 + days_ago 計算。"""
    if isinstance(fact_date, str):
        try:
            fact_date = date.fromisoformat(fact_date)
        except ValueError:
            return None
    if not isinstance(fact_date, date):
        return None
    return max(0, (as_of - fact_date).days)


def _date_str(fact_date: Any) -> str:
    if isinstance(fact_date, str):
        return fact_date
    if isinstance(fact_date, date):
        return fact_date.isoformat()
    return ""
