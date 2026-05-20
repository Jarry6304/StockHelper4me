"""Tool 3 內部演算法:`market_context` — 大盤環境綜合判讀。

對齊 plan §Tool 3:讀 market-level facts → 8 components score → climate
score weighted avg → systemic risks → 1 句 narrative。

v3.25(2026-05-17):加 2 新 components — `commodity_macro`(macro 商品信號,
對齊 environment_cores §十)+ `risk_alert`(per-stock 處置股聚合,對齊
chip_cores §十二)。risk_alert 是 per-stock 資料聚合成 market summary。

呼叫端:`mcp_server.tools.data.market_context()`。

設計:
- 純讀 PG;不改 facts;不改 cores;不改 agg。
- 5 個保留字 stock_id 對映 7 個 environment cores(`_global_` 含
  exchange_rate + fear_greed + **commodity_macro** v3.25):
  * `_index_taiex_`     ↔ `taiex_core`
  * `_index_us_market_` ↔ `us_market_core`
  * `_index_business_`  ↔ `business_indicator_core`
  * `_market_`          ↔ `market_margin_core`
  * `_global_`          ↔ `exchange_rate_core` + `fear_greed_core` + `commodity_macro_core`(v3.25)
- `risk_alert_core`(per-stock,real stock_ids)→ 額外 marketwide aggregation
"""

from __future__ import annotations

from datetime import date, timedelta
from math import exp
from typing import Any

# ────────────────────────────────────────────────────────────
# Component config(對齊 m3Spec/environment_cores.md §三~§八 + §十 + chip §十二)
# ────────────────────────────────────────────────────────────

# climate_score weighted avg 權重(v3.25 拍版:8 components,sum=1.0)
_COMPONENT_WEIGHTS: dict[str, float] = {
    "taiex":           0.22,
    "us_market":       0.17,
    "fear_greed":      0.13,
    "business":        0.17,
    "exchange_rate":   0.08,
    "market_margin":   0.08,
    "commodity_macro": 0.05,    # v3.25 macro 商品(初版 GOLD,weight 低)
    "risk_alert":      0.10,    # v3.25 marketwide 處置股聚合(domestic 風險)
}

# 每個 component 對映的 source_core(從 facts 撈)
_COMPONENT_TO_CORE: dict[str, str] = {
    "taiex":           "taiex_core",
    "us_market":       "us_market_core",
    "fear_greed":      "fear_greed_core",
    "business":        "business_indicator_core",
    "exchange_rate":   "exchange_rate_core",
    "market_margin":   "market_margin_core",
    "commodity_macro": "commodity_macro_core",   # v3.25
    # risk_alert 不在這個 map 內(per-stock 來源,走獨立 aggregation 函式)
}

# Kind → sign 對映(+1 bullish / -1 bearish / 0 neutral)
# 對齊各 env core 的 EventKind enum + 大盤環境語意(注意:fear_greed 是 contrarian;
# commodity GOLD momentum 是 risk-off proxy → 對 equities 反向)
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
    # commodity_macro_core(v3.25):GOLD 是 risk-off proxy,對 equities 反向
    # 設計:gold up = 避險情緒 = 對股市 bearish;gold down = risk-on = 對股市 bullish
    "CommoditySpike":             0,  # 中性:spike 方向需看 metadata(可正可負)
    "CommodityMomentumUp":        -1, # 對股市偏空
    "CommodityMomentumDown":      +1, # 對股市偏多
    "CommodityRegimeBreak":       0,  # 中性警示,計入 systemic_risks
}

# Systemic risks 觸發條件:某 kind 在 lookback 內出現 → 加 risk flag
_RISK_KINDS: dict[str, str] = {
    "VixHighZoneEntry":     "us_vix_high_alert",
    "VixSpike":             "us_vix_spike",
    "EnteredDangerZone":    "tw_margin_maintenance_danger",
    "LeadingStreakDown":    "tw_business_leading_indicator_down",
    "EnteredExtremeFear":   "us_fear_extreme",
    # v3.25 macro / 處置股相關
    "CommoditySpike":       "macro_commodity_spike",
    "CommodityRegimeBreak": "macro_commodity_regime_shift",
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
    """主入口 — 從 PG 撈 market facts + risk_alert 聚合 → 8 components → narrative。

    Args:
        as_of: 查詢日(預測 / 即時都用同介面)
        lookback_days: facts 期間。預設 60(對齊月頻 + daily 雙重 cover)
        database_url: 可選的 PG 連線字串

    Returns:
        dict 結構對齊 plan §Tool 3 Output(v3.25 後 ~2 KB / ~500 tokens)
    """
    from fusion.raw._db import get_connection
    from fusion.raw._market import fetch_market_facts

    conn = get_connection(database_url)
    try:
        # Step 1:撈 5 保留字的 market facts(內建 look-ahead filter)
        grouped = fetch_market_facts(
            conn,
            as_of=as_of,
            lookback_days=lookback_days,
        )
        # Step 1b(v3.25):per-stock risk_alert 聚合(marketwide summary)
        risk_alert_summary = _aggregate_risk_alert_marketwide(
            conn, as_of=as_of, lookback_days=lookback_days,
        )
    finally:
        conn.close()

    # Step 2:把 facts 按 source_core 分組(_global_ 含 3 cores,需要拆)
    facts_by_core: dict[str, list[dict]] = {core: [] for core in _COMPONENT_TO_CORE.values()}
    for _sid, facts in grouped.items():
        for f in facts:
            core = f.get("source_core")
            if core in facts_by_core:
                facts_by_core[core].append(f)

    # Step 3:每 component 算 score(7 個走 facts;risk_alert 走 summary)
    components: dict[str, dict[str, Any]] = {}
    for comp_name, core_name in _COMPONENT_TO_CORE.items():
        score = _score_component(facts_by_core[core_name], as_of)
        components[comp_name] = {
            "score": score,
            "fact_count": len(facts_by_core[core_name]),
        }
    # risk_alert component(v3.25):從 marketwide aggregation 直接算 score
    components["risk_alert"] = {
        "score":                       _score_risk_alert(risk_alert_summary),
        "active_disposition_stocks":   risk_alert_summary["active_count"],
        "escalations_60d":             risk_alert_summary["escalations_60d"],
        "announced_14d":               risk_alert_summary["announced_14d"],
    }

    # Step 4:climate_score 加權平均(v3.25 8 components)
    climate_score = sum(
        components[c]["score"] * _COMPONENT_WEIGHTS[c]
        for c in _COMPONENT_WEIGHTS
    )

    # Step 5:Systemic risks 檢測
    systemic_risks = _detect_risks(facts_by_core, as_of, lookback_days=lookback_days)
    # v3.25:risk_alert 聚合也可加 systemic flag
    if risk_alert_summary["active_count"] >= 5:
        systemic_risks.append("tw_disposition_cluster")
    if risk_alert_summary["escalations_60d"] >= 3:
        systemic_risks.append("tw_disposition_escalation_cluster")
    systemic_risks = sorted(set(systemic_risks))

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
# v3.25 risk_alert marketwide aggregation
# ────────────────────────────────────────────────────────────

def _aggregate_risk_alert_marketwide(
    conn,
    *,
    as_of: date,
    lookback_days: int,
) -> dict[str, int]:
    """聚合全市場 risk_alert_core facts(per-stock 來源)→ 3 個 marketwide 指標。

    1. active_count:當下在處置期內的 distinct stocks(DispositionEntered 且
       period_end >= as_of)
    2. announced_14d:近 14 天 DispositionAnnounced distinct stocks 數
    3. escalations_60d:近 60 天 DispositionEscalation 總次數
    """
    short_cutoff = as_of - timedelta(days=14)
    long_cutoff = as_of - timedelta(days=60)
    earliest_cutoff = as_of - timedelta(days=lookback_days)

    out = {"active_count": 0, "announced_14d": 0, "escalations_60d": 0}

    try:
        with conn.cursor() as cur:
            # 1. active_count:Entered 事件 + metadata.period_end >= as_of
            cur.execute(
                """
                SELECT COUNT(DISTINCT stock_id) AS n
                  FROM facts
                 WHERE source_core = 'risk_alert_core'
                   AND metadata->>'event_kind' = 'DispositionEntered'
                   AND fact_date <= %s
                   AND fact_date >= %s
                   AND (metadata->>'period_end')::date >= %s
                """,
                [as_of, earliest_cutoff, as_of],
            )
            r = cur.fetchone() or {}
            out["active_count"] = int(r.get("n") or 0)

            # 2. announced_14d
            cur.execute(
                """
                SELECT COUNT(DISTINCT stock_id) AS n
                  FROM facts
                 WHERE source_core = 'risk_alert_core'
                   AND metadata->>'event_kind' = 'DispositionAnnounced'
                   AND fact_date >= %s
                   AND fact_date <= %s
                """,
                [short_cutoff, as_of],
            )
            r = cur.fetchone() or {}
            out["announced_14d"] = int(r.get("n") or 0)

            # 3. escalations_60d
            cur.execute(
                """
                SELECT COUNT(*) AS n
                  FROM facts
                 WHERE source_core = 'risk_alert_core'
                   AND metadata->>'event_kind' = 'DispositionEscalation'
                   AND fact_date >= %s
                   AND fact_date <= %s
                """,
                [long_cutoff, as_of],
            )
            r = cur.fetchone() or {}
            out["escalations_60d"] = int(r.get("n") or 0)
    except Exception:
        # facts 表或 risk_alert 還沒上線時 graceful return zeros
        pass

    return out


def _score_risk_alert(summary: dict[str, int]) -> int:
    """處置股聚合 → bearish score(對股市風險意義)。

    Thresholds:
      active_count: 0=0 / 1-4=-15 / 5-9=-50 / 10+=-100
      escalations_60d:  0=0 / 1-2=-15 額外 / 3+=-30 額外(系統性)
      announced_14d:    0=0 / 5+=-15 額外(近期密集警示)
    最後 clamp [-100, +100]。
    """
    active = summary.get("active_count", 0)
    esc = summary.get("escalations_60d", 0)
    ann = summary.get("announced_14d", 0)

    score = 0
    if active >= 10:
        score -= 100
    elif active >= 5:
        score -= 50
    elif active >= 1:
        score -= 15

    if esc >= 3:
        score -= 30
    elif esc >= 1:
        score -= 15

    if ann >= 5:
        score -= 15

    return max(-100, min(100, score))


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
        kind = (f.get("metadata") or {}).get("event_kind") \
            or (f.get("metadata") or {}).get("kind") \
            or _extract_kind_from_statement(f.get("statement", ""))
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
            kind = (f.get("metadata") or {}).get("event_kind") \
                or (f.get("metadata") or {}).get("kind") \
                or _extract_kind_from_statement(f.get("statement", ""))
            risk_label = _RISK_KINDS.get(kind)
            if not risk_label:
                continue

            # VIX / margin danger / commodity spike 屬短期警示;其他屬中期
            fact_date = f.get("fact_date")
            if isinstance(fact_date, str):
                try:
                    fact_date = date.fromisoformat(fact_date)
                except ValueError:
                    continue
            if not isinstance(fact_date, date):
                continue

            if kind in ("VixSpike", "VixHighZoneEntry", "EnteredDangerZone",
                        "CommoditySpike", "CommodityRegimeBreak"):
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
    "taiex":           "台股大盤",
    "us_market":       "美股 + VIX",
    "fear_greed":      "Fear-Greed",
    "business":        "景氣指標",
    "exchange_rate":   "匯率",
    "market_margin":   "融資維持率",
    "commodity_macro": "商品(GOLD)",   # v3.25
    "risk_alert":      "處置股聚合",     # v3.25
}

_CLIMATE_LABEL: dict[str, str] = {
    "extreme_bullish": "極度偏多",
    "bullish":         "偏多",
    "neutral":         "中性",
    "bearish":         "偏空",
    "extreme_bearish": "極度偏空",
}
