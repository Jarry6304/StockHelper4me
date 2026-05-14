"""Tool 3 內部演算法:`market_context` — 大盤環境綜合判讀。

對齊 plan §Tool 3:讀 market-level facts → 6 components score → climate
score weighted avg → systemic risks → 1 句 narrative。

呼叫端:`mcp_server.tools.data.market_context()`。

設計:
- 純讀 PG;不改 facts;不改 cores;不改 agg。
- 5 個保留字 stock_id 對映 6 個 environment cores:
  * `_index_taiex_`     ↔ `taiex_core`
  * `_index_us_market_` ↔ `us_market_core`
  * `_index_business_`  ↔ `business_indicator_core`
  * `_market_`          ↔ `market_margin_core`
  * `_global_`          ↔ `exchange_rate_core` + `fear_greed_core`(同保留字 2 cores)
"""

from __future__ import annotations

from datetime import date, timedelta
from math import exp
from typing import Any

# ────────────────────────────────────────────────────────────
# Component config(對齊 m3Spec/environment_cores.md §三~§八)
# ────────────────────────────────────────────────────────────

# climate_score weighted avg 權重(plan 拍版)
_COMPONENT_WEIGHTS: dict[str, float] = {
    "taiex":         0.25,
    "us_market":     0.20,
    "fear_greed":    0.15,
    "business":      0.20,
    "exchange_rate": 0.10,
    "market_margin": 0.10,
}

# 每個 component 對映的 source_core(從 facts 撈)
_COMPONENT_TO_CORE: dict[str, str] = {
    "taiex":         "taiex_core",
    "us_market":     "us_market_core",
    "fear_greed":    "fear_greed_core",
    "business":      "business_indicator_core",
    "exchange_rate": "exchange_rate_core",
    "market_margin": "market_margin_core",
}

# Kind → sign 對映(+1 bullish / -1 bearish / 0 neutral)
# 對齊各 env core 的 EventKind enum + 大盤環境語意(注意:fear_greed 是 contrarian)
_KIND_SIGN: dict[str, int] = {
    # us_market_core
    "SpyMacdGoldenCross":        +1,
    "SpyMacdDeathCross":         -1,
    "VixSpike":                  -1,
    "VixHighZoneEntry":          -1,
    "VixLowZoneEntry":           +1,
    "SpyOvernightLargeMove":      0,
    # fear_greed_core(contrarian indicator)
    "EnteredExtremeFear":        +1,
    "ExitedExtremeFear":         +1,
    "EnteredExtremeGreed":       -1,
    "ExitedExtremeGreed":        -1,
    "StreakInZone":               0,
    # business_indicator_core
    "LeadingTurningUp":          +1,
    "LeadingTurningDown":        -1,
    "LeadingStreakUp":           +1,
    "LeadingStreakDown":         -1,
    "MonitoringColorChange":      0,
    "MonitoringStreakInColor":    0,
    # market_margin_core
    "EnteredWarningZone":        -1,
    "EnteredDangerZone":         -1,
    "ExitedDangerZone":          +1,
    "SignificantSingleDayDrop":  -1,
    # exchange_rate_core(中性 by default;意義屬產業面而非系統面)
    "KeyLevelBreakout":           0,
    "KeyLevelBreakdown":          0,
    "SignificantSingleDayMove":   0,
    "MaCross":                    0,
    # taiex_core(spec §3.6 — bullish/bearish trend / MACD / RSI)
    "TaiexBullishTrend":         +1,
    "TaiexBearishTrend":         -1,
    "TaiexMacdGoldenCross":      +1,
    "TaiexMacdDeathCross":       -1,
    "TaiexRsiOverbought":        -1,
    "TaiexRsiOversold":          +1,
    "TaiexNearAllTimeHigh":      -1,  # 偏向警訊
    "TaiexVolumeSpike":           0,
    "TaiexBreakout":             +1,
}

# Systemic risks 觸發條件:某 kind 在 lookback 內出現 → 加 risk flag
_RISK_KINDS: dict[str, str] = {
    "VixHighZoneEntry":   "us_vix_high_alert",
    "VixSpike":           "us_vix_spike",
    "EnteredDangerZone":  "tw_margin_maintenance_danger",
    "LeadingStreakDown":  "tw_business_leading_indicator_down",
    "EnteredExtremeFear": "us_fear_extreme",
}

# 時間 decay:近期 fact 權重高;14 天半衰期
_DECAY_DAYS = 14.0

# climate_score → overall_climate 5 級分類
def _score_to_climate(score: float) -> str:
    if score >= 50:
        return "extreme_bullish"
    if score >= 15:
        return "bullish"
    if score >= -15:
        return "neutral"
    if score >= -50:
        return "bearish"
    return "extreme_bearish"


# ────────────────────────────────────────────────────────────
# Public API
# ────────────────────────────────────────────────────────────

def compute_market_context(
    as_of: date,
    *,
    lookback_days: int = 60,
    database_url: str | None = None,
) -> dict[str, Any]:
    """主入口 — 從 PG 撈 market facts → 6 components score → narrative。

    Args:
        as_of: 查詢日(預測 / 即時都用同介面)
        lookback_days: facts 期間。預設 60(對齊月頻 + daily 雙重 cover)
        database_url: 可選的 PG 連線字串

    Returns:
        dict 結構對齊 plan §Tool 3 Output(~1.5 KB / ~400 tokens)
    """
    from agg._db import get_connection
    from agg._market import fetch_market_facts

    conn = get_connection(database_url)
    try:
        # Step 1:撈 5 保留字的 market facts(內建 look-ahead filter)
        grouped = fetch_market_facts(
            conn,
            as_of=as_of,
            lookback_days=lookback_days,
        )
    finally:
        conn.close()

    # Step 2:把 facts 按 source_core 分組(_global_ 含 2 cores,需要拆)
    facts_by_core: dict[str, list[dict]] = {core: [] for core in _COMPONENT_TO_CORE.values()}
    for _sid, facts in grouped.items():
        for f in facts:
            core = f.get("source_core")
            if core in facts_by_core:
                facts_by_core[core].append(f)

    # Step 3:每 component 算 score
    components: dict[str, dict[str, Any]] = {}
    for comp_name, core_name in _COMPONENT_TO_CORE.items():
        score = _score_component(facts_by_core[core_name], as_of)
        components[comp_name] = {
            "score": score,
            "fact_count": len(facts_by_core[core_name]),
        }

    # Step 4:climate_score 加權平均
    climate_score = sum(
        components[c]["score"] * _COMPONENT_WEIGHTS[c]
        for c in _COMPONENT_WEIGHTS
    )

    # Step 5:Systemic risks 檢測
    systemic_risks = _detect_risks(facts_by_core, as_of, lookback_days=lookback_days)

    # Step 6:Narrative 組裝
    narrative = _compose_narrative(components, climate_score, systemic_risks)

    return {
        "as_of":           as_of.isoformat(),
        "overall_climate": _score_to_climate(climate_score),
        "climate_score":   round(climate_score, 1),
        "components":      components,
        "systemic_risks":  systemic_risks,
        "narrative":       narrative,
    }


# ────────────────────────────────────────────────────────────
# Internal helpers
# ────────────────────────────────────────────────────────────

def _score_component(facts: list[dict], as_of: date) -> int:
    """Weighted sum:每 fact 的 sign × time decay → 累積 score → 縮到 [-100, +100]。

    時間 decay 公式:`weight = exp(-days_ago / 14)`。
    每 fact 對 score 貢獻 = `sign * 25 * decay`(25 是每筆 fact 的最大絕對值)。
    最後 sum clamp 到 [-100, +100]。
    """
    if not facts:
        return 0

    score = 0.0
    for f in facts:
        kind = (f.get("metadata") or {}).get("kind") or _extract_kind_from_statement(f.get("statement", ""))
        sign = _KIND_SIGN.get(kind, 0)
        if sign == 0:
            continue

        # Date 解析(支援 ISO str / date object)
        fact_date = f.get("fact_date")
        if isinstance(fact_date, str):
            try:
                fact_date = date.fromisoformat(fact_date)
            except ValueError:
                continue
        if not isinstance(fact_date, date):
            continue

        days_ago = max(0, (as_of - fact_date).days)
        decay = exp(-days_ago / _DECAY_DAYS)
        score += sign * 25 * decay

    # Clamp [-100, +100]
    return int(max(-100, min(100, score)))


def _extract_kind_from_statement(statement: str) -> str | None:
    """Fallback:從 fact.statement 字串猜 kind(若 metadata 沒給)。

    Statement 一般長:`{EventKind:?} on {date}: value=...`,kind 是第一 token。
    """
    if not statement:
        return None
    head = statement.split(" ", 1)[0]
    # 簡單 sanity:全字母 + 第一字大寫 = 像 enum variant
    return head if head and head[0].isupper() and head.isalpha() else None


def _detect_risks(
    facts_by_core: dict[str, list[dict]],
    as_of: date,
    *,
    lookback_days: int,
) -> list[str]:
    """掃描 lookback 期間出現過的 risk-trigger kinds → 累積 systemic_risks 標籤。

    對齊 plan §Tool 3 systemic_risks 觸發條件。重複 risk 去重。
    """
    risks: set[str] = set()
    # 重要 risks 只看最近 14 天,長期影響的看全 lookback
    short_window_days = 14
    short_cutoff = as_of - timedelta(days=short_window_days)

    for core_facts in facts_by_core.values():
        for f in core_facts:
            kind = (f.get("metadata") or {}).get("kind") or _extract_kind_from_statement(f.get("statement", ""))
            risk_label = _RISK_KINDS.get(kind)
            if not risk_label:
                continue

            # VIX / margin danger 屬短期警示;Business / Fear Greed 屬中期
            fact_date = f.get("fact_date")
            if isinstance(fact_date, str):
                try:
                    fact_date = date.fromisoformat(fact_date)
                except ValueError:
                    continue
            if not isinstance(fact_date, date):
                continue

            if kind in ("VixSpike", "VixHighZoneEntry", "EnteredDangerZone"):
                if fact_date < short_cutoff:
                    continue
            risks.add(risk_label)

    return sorted(risks)


def _compose_narrative(
    components: dict[str, dict[str, Any]],
    climate_score: float,
    systemic_risks: list[str],
) -> str:
    """從各 component score 組裝 1-2 句 narrative。

    規則:
    1. 找 top 2 主導因子(score 絕對值最大,且 != 0)
    2. systemic_risks 非空 → 加警示句
    3. 最後綴 climate 級別判讀
    """
    # 過濾非 0 component
    non_zero = [
        (c, info["score"])
        for c, info in components.items()
        if info["score"] != 0
    ]
    # 絕對值由大到小排序
    non_zero.sort(key=lambda x: abs(x[1]), reverse=True)

    parts: list[str] = []
    for comp, score in non_zero[:2]:
        direction = "偏多" if score > 0 else "偏空"
        parts.append(f"{_COMP_LABEL[comp]} {direction}({score})")

    if not parts:
        body = "各 environment cores 近 60 日無顯著事件"
    else:
        body = "、".join(parts)

    if systemic_risks:
        body += f";風險警示:{'、'.join(systemic_risks)}"

    climate = _score_to_climate(climate_score)
    return f"{body}。整體 {_CLIMATE_LABEL[climate]}({climate_score:+.0f}/100)。"


_COMP_LABEL: dict[str, str] = {
    "taiex":         "台股大盤",
    "us_market":     "美股 + VIX",
    "fear_greed":    "Fear-Greed",
    "business":      "景氣指標",
    "exchange_rate": "匯率",
    "market_margin": "融資維持率",
}

_CLIMATE_LABEL: dict[str, str] = {
    "extreme_bullish": "極度偏多",
    "bullish":         "偏多",
    "neutral":         "中性",
    "bearish":         "偏空",
    "extreme_bearish": "極度偏空",
}
