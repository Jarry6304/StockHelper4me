"""Tool 1 內部演算法:`neely_forecast` — 4 個時間框架的 % + 價位區間。

對齊 plan §Tool 1:
1. 撈 Neely structural_snapshots(取 top 5 by `power_rating`)+ indicator_values latest
2. 4 個時間框架的價位區間 from `expected_fib_zones`(Fib ratio scaling)
3. prob_up 跨 cores 加權公式(plan §Tool 1 第 3 點)
4. invalidation_price 從 scenario invalidation_triggers 抽

呼叫端:`mcp_server.tools.data.neely_forecast()`。

設計:
- Forest 不選 primary(對齊 architecture §8.2),top 5 by `power_rating` 取首位作 primary_scenario
- 4 時間框架 fib ratio scaling 寫死(plan §Tool 1 第 2 點)
- prob_up 公式寫死 constants(對齊 NEELY constants 寫死慣例)
- 跨 cores 加權算機率 = Aggregation Layer 整合層責任(cores_overview §10.0)

v3.35(2026-05-18)Neely-C-MCP picker upgrade:
- Picker 加 invalidation filter(scenario PriceBreakBelow > current_price 或
  PriceBreakAbove < current_price 視為失效,過濾掉)
- Picker 排序加 degree-aware preference:(effective_degree DESC,power_rating DESC,
  rules_passed_count DESC)。3030 case 短期 swing scenario 與長期主升 scenario
  power_rating 同 → 改 degree 優先 → primary 變長期主升 corrective phase。
- effective_degree 從 scenario.wave_tree.start/end 推算(對齊 Stage 11 Degree
  Ceiling 表 m3Spec/neely_core_architecture.md §13.3)。
- 對齊 spec「展示式森林」設計(output.rs:5-6 註解):picker 在 Aggregation Layer,
  Rust Core 不選 primary。
"""

from __future__ import annotations

from datetime import date, datetime
from typing import Any

# ────────────────────────────────────────────────────────────
# 4 時間框架 Fibonacci ratio scaling(plan §Tool 1 第 2 點)
# ────────────────────────────────────────────────────────────

# 每個 timeframe 用 fib zones 的 ratio 範圍取 range_high / range_low
# ratio_lo = 預期下界 fib;ratio_hi = 預期上界 fib
_TIMEFRAME_FIB_RANGE: dict[str, tuple[float, float]] = {
    "1_month":   (0.382, 0.618),
    "1_quarter": (0.618, 1.000),
    "6_month":   (1.000, 1.382),
    "1_year":    (1.382, 1.618),
}

# Prob_up 時間衰減(plan §Tool 1 第 3 點)
_TIMEFRAME_DECAY: dict[str, float] = {
    "1_month":   1.00,
    "1_quarter": 0.85,
    "6_month":   0.70,
    "1_year":    0.55,
}

# Power Rating → base prob_up(plan §Tool 1 第 3 點)
_POWER_TO_PROB: dict[str, float] = {
    "StrongBullish":  0.70,
    "Bullish":        0.62,
    "SlightBullish":  0.56,
    "Neutral":        0.50,
    "SlightBearish":  0.44,
    "Bearish":        0.38,
    "StrongBearish":  0.30,
}


# ────────────────────────────────────────────────────────────
# Public API
# ────────────────────────────────────────────────────────────

def compute_neely_forecast(
    stock_id: str,
    as_of: date,
    *,
    database_url: str | None = None,
) -> dict[str, Any]:
    """主入口 — Neely 4 時間框架預測。

    Args:
        stock_id: 股票代號(例 "2330")
        as_of: 查詢日
        database_url: 可選 PG 連線字串

    Returns:
        dict 結構對齊 plan §Tool 1 Output(~2 KB / ~500 tokens)
    """
    from agg import as_of as agg_as_of

    # 撈 Neely structural + 跨 cores 動能 / 籌碼最新值
    relevant_cores = [
        "neely_core",
        "macd_core", "rsi_core", "adx_core",
        "institutional_core", "foreign_holding_core",
    ]
    snapshot = agg_as_of(
        stock_id,
        as_of,
        cores=relevant_cores,
        lookback_days=30,
        include_market=False,
        database_url=database_url,
    )

    # 1. Current price(v3.26 修:直讀 price_daily;v3.35 提前到 picker 之前,
    #    讓 picker 用 current_price 做 invalidation filter)
    from mcp_server._price import fetch_latest_close_for_tool
    price_info = fetch_latest_close_for_tool(stock_id, as_of, database_url=database_url)
    current_price = price_info["close"] if price_info else _extract_current_price(snapshot)

    # 2. Primary scenario:v3.35 picker 走 invalidation filter + degree-aware ordering
    primary, all_scenarios = _extract_primary_and_top_scenarios(
        snapshot, current_price=current_price, limit=5,
    )

    # v3.28(2026-05-17):scenario_forest staleness check — neely_core 沒重 backfill
    # 時 invalidation_price 等欄會是過期 anchor;surface 給 LLM 知道
    scenario_staleness = _compute_scenario_staleness(snapshot, as_of)

    # 3. 4 時間框架價位區間
    forecasts = _build_forecasts(primary, current_price, snapshot, as_of)

    # 4. Key levels:supports / resistances 從 fib zones 推
    key_levels = _extract_key_levels(primary, current_price)

    # 5. Invalidation price 從 primary 的 triggers 抽
    invalidation_price = _extract_invalidation_price(primary, current_price)

    # 6. v3.35.1 quality caveat:
    #    (a) 若所有 scenarios 都是 short-degree → warn LLM 沒長期 anchor
    #    (b) 若 primary fib zones 與 current_price 完全脫節 → forecasts 不可用
    #    對應 v3.35 production verify 揭露:Rust Stage 3 Generator 對長 history 股票
    #    只產短 span candidates,picker 即使選最長也仍是 SubMinuette。
    quality_caveat = _compute_quality_caveat(all_scenarios, primary, current_price)

    # 7. v3.37 multi-timeframe Neely 摘要(LLM 看 daily / weekly / monthly 各自 primary
    #    + 跨 timeframe 一致性)。primary 帶 __timeframe__ tag 告知它來自哪 timeframe。
    neely_by_timeframe = _build_neely_by_timeframe(snapshot, current_price)

    return {
        "stock_id":           stock_id,
        "as_of":              as_of.isoformat(),
        "current_price":      current_price,
        "primary_scenario":   _format_primary_scenario(primary),
        "scenario_count":     len(all_scenarios),
        "scenario_staleness": scenario_staleness,
        "forecasts":          forecasts,
        "key_levels":         key_levels,
        "invalidation_price": invalidation_price,
        "quality_caveat":     quality_caveat,
        "neely_by_timeframe": neely_by_timeframe,    # v3.37 加
    }


def _compute_scenario_staleness(snapshot, as_of: date) -> dict[str, Any]:
    """檢查 neely structural snapshot_date 距 as_of 多遠;> 7 天標 stale。

    v3.28(2026-05-17):scenario_forest 過期會讓 invalidation_price / fib_zones
    用舊 anchor 算,導致預測偏差。surface 給 LLM 知道何時需要 user 跑
    `tw_cores run-all --write` 重算 neely_core。
    """
    structural = snapshot.structural or {}
    neely_row = None
    for key, row in structural.items():
        if key.startswith("neely_core"):
            neely_row = row
            break
    if neely_row is None:
        return {
            "snapshot_date": None,
            "age_days":      None,
            "is_stale":      None,
            "warning":       "no neely_core structural snapshot 資料(尚未跑 tw_cores)",
        }

    snap_date = getattr(neely_row, "snapshot_date", None)
    if not isinstance(snap_date, date):
        return {"snapshot_date": None, "age_days": None, "is_stale": None,
                "warning": "snapshot_date 缺失或非 date 物件"}

    age_days = (as_of - snap_date).days
    is_stale = age_days > 7
    warning = None
    if is_stale:
        warning = (
            f"scenario_forest 過期 {age_days} 天(snapshot_date={snap_date.isoformat()},"
            f"as_of={as_of.isoformat()})— invalidation_price / fib_zones 可能用舊 anchor"
            f"。請跑 `tw_cores run-all --write` 重算 neely_core 對 {snap_date}+ 的新 bars。"
        )
    return {
        "snapshot_date": snap_date.isoformat(),
        "age_days":      age_days,
        "is_stale":      is_stale,
        "warning":       warning,
    }


# ────────────────────────────────────────────────────────────
# Internal helpers
# ────────────────────────────────────────────────────────────

def _extract_primary_and_top_scenarios(
    snapshot, *, current_price: float | None = None, limit: int = 5,
) -> tuple[dict | None, list[dict]]:
    """從 structural['neely_core@*'] 取 scenario_forest,picker 選 primary。

    v3.37(2026-05-18)multi-timeframe pick:
      - 從 ALL neely_core@{daily,weekly,monthly} structural rows 收集 scenarios
      - 對每 scenario tag `__timeframe__`(daily/weekly/monthly)
      - 跨 timeframe 統一 (degree DESC, power DESC, rules DESC) 排序
      - 對 3030 long-history:weekly/monthly Neely 產 Minor/Primary degree scenarios,
        排序後 promote 到 primary,取代 daily SubMinuette

    v3.35 picker(沿用):
      1. invalidation filter(若 current_price 可用)
      2. degree-aware ordering

    Args:
        current_price: 若 None,跳過 invalidation filter(向下相容 unit test)

    Returns:
        (primary, top_scenarios) — primary 帶 __timeframe__ tag,top 限 `limit` 筆
    """
    structural = snapshot.structural

    # v3.37:iterate ALL neely_core@<tf> rows(不再 break on first)
    all_scenarios: list[dict] = []
    for key, row in structural.items():
        if not key.startswith("neely_core"):
            continue
        # key format "neely_core@daily" / "neely_core@weekly" / "neely_core@monthly"
        tf_str = key.split("@", 1)[1] if "@" in key else "daily"
        snap = row.snapshot or {}
        scenarios = snap.get("scenario_forest") or snap.get("scenarios") or []
        if not isinstance(scenarios, list):
            continue
        for s in scenarios:
            if not isinstance(s, dict):
                continue
            tagged = dict(s)        # shallow copy(不污染 indicator_latest snapshot 內 dict)
            tagged["__timeframe__"] = tf_str
            all_scenarios.append(tagged)

    if not all_scenarios:
        return None, []

    # v3.35 step 1:invalidation filter(若 current_price 可用)
    if current_price is not None and current_price > 0:
        all_scenarios = [
            s for s in all_scenarios
            if not _scenario_is_invalidated(s, current_price)
        ]
        if not all_scenarios:
            return None, []

    # v3.35 step 2:degree-aware ordering(跨 timeframe 統一排序)
    def _score(s: dict) -> tuple[int, int, int]:
        degree_rank = _degree_rank(_compute_scenario_effective_degree(s))
        pr_strength = _power_rating_strength(s.get("power_rating"))
        rules_count = int(s.get("rules_passed_count") or 0)
        return (degree_rank, pr_strength, rules_count)

    sorted_scenarios = sorted(all_scenarios, key=_score, reverse=True)
    top = sorted_scenarios[:limit]
    primary = top[0] if top else None
    return primary, top


# ────────────────────────────────────────────────────────────
# v3.35 picker helpers — degree-aware preference + invalidation filter
# ────────────────────────────────────────────────────────────

# Degree label → rank(Stage 11 §13.3 表 + spec output.rs::Degree enum 順序)。
# 較大 degree → 較高 rank → 排序時優先。
_DEGREE_RANK: dict[str, int] = {
    "GrandSupercycle": 11,
    "Supercycle":      10,
    "Cycle":            9,
    "Primary":          8,
    "Intermediate":     7,
    "Minor":            6,
    "Minute":           5,
    "Minuette":         4,
    "SubMinuette":      3,
    "Micro":            2,
    "SubMicro":         1,
}


def _degree_rank(degree_label: str | None) -> int:
    """Degree string → 整數 rank。None / 未知 → 0(fallback 不影響其他 sort key)。"""
    if not degree_label:
        return 0
    return _DEGREE_RANK.get(degree_label, 0)


def _compute_scenario_effective_degree(
    scenario: dict, *, timeframe: str = "daily",
) -> str | None:
    """對齊 Stage 11 §13.3 Degree Ceiling 表,從 scenario.wave_tree.start/end 推算 degree。

    Daily 閾值(spec rust degree/mod.rs::classify_degree):
      - < 1 年   → SubMinuette
      - 1-3 年   → Minute
      - 3-10 年  → Minor
      - 10-30 年 → Primary
      - 30-100 年→ Cycle
      - > 100 年 → Supercycle

    Weekly / Monthly / Quarterly 走相同年數區間(spec timeframe 已轉成年級判定)。

    Returns:
        Degree string(對齊 Rust output.rs::Degree enum)or None(wave_tree.start/end 缺失)
    """
    wave_tree = scenario.get("wave_tree") or {}
    start_str = wave_tree.get("start")
    end_str   = wave_tree.get("end")
    if not start_str or not end_str:
        return None

    try:
        start = _parse_iso_date(start_str)
        end   = _parse_iso_date(end_str)
    except (ValueError, TypeError):
        return None

    if end < start:
        return None
    span_years = (end - start).days / 365.25

    if span_years < 1.0:
        return "SubMinuette"
    if span_years < 3.0:
        return "Minute"
    if span_years < 10.0:
        return "Minor"
    if span_years < 30.0:
        return "Primary"
    if span_years < 100.0:
        return "Cycle"
    return "Supercycle"


def _parse_iso_date(s: str | Any) -> date:
    """ISO date string → date object(包含 date object pass-through)。"""
    if isinstance(s, date):
        return s
    return datetime.fromisoformat(str(s)).date()


def _scenario_is_invalidated(scenario: dict, current_price: float) -> bool:
    """v3.35:檢查 scenario 是否已被 current_price 觸發 InvalidateScenario trigger。

    對齊 Rust triggers/mod.rs:
      - PriceBreakBelow(price): bullish scenario 跌破 price → invalidated
      - PriceBreakAbove(price): bearish scenario 漲破 price → invalidated

    只看 OnTriggerAction == "InvalidateScenario"(WeakenScenario / PromoteAlternative 不算)。
    """
    triggers = scenario.get("invalidation_triggers") or []
    for t in triggers:
        action = t.get("on_trigger")
        # serde tagged enum 可能是 dict 或 str
        if isinstance(action, dict):
            action = next(iter(action.keys()), None)
        if action != "InvalidateScenario":
            continue

        trigger_type = t.get("trigger_type")
        if not isinstance(trigger_type, dict):
            continue

        if "PriceBreakBelow" in trigger_type:
            try:
                threshold = float(trigger_type["PriceBreakBelow"])
                if current_price < threshold:
                    return True
            except (TypeError, ValueError):
                continue
        elif "PriceBreakAbove" in trigger_type:
            try:
                threshold = float(trigger_type["PriceBreakAbove"])
                if current_price > threshold:
                    return True
            except (TypeError, ValueError):
                continue
    return False


def _power_rating_strength(rating: Any) -> int:
    """PowerRating → 強度級別(0-3)。"""
    if not rating:
        return 0
    if isinstance(rating, dict):
        # serde tagged enum,取唯一 key
        rating = next(iter(rating.keys()), None)
    if not isinstance(rating, str):
        return 0
    mapping = {
        "StrongBullish": 3, "StrongBearish": 3,
        "Bullish":       2, "Bearish":       2,
        "SlightBullish": 1, "SlightBearish": 1,
        "Neutral":       0,
    }
    return mapping.get(rating, 0)


def _power_rating_sign(rating: Any) -> int:
    """+1 bull / -1 bear / 0 neutral。"""
    if isinstance(rating, dict):
        rating = next(iter(rating.keys()), None)
    if not isinstance(rating, str):
        return 0
    if rating.endswith("Bullish"):
        return +1
    if rating.endswith("Bearish"):
        return -1
    return 0


def _power_rating_label(rating: Any) -> str:
    """正規化 PowerRating 字串。"""
    if isinstance(rating, dict):
        return next(iter(rating.keys()), "Neutral")
    if isinstance(rating, str):
        return rating
    return "Neutral"


def _format_primary_scenario(scenario: dict | None) -> dict[str, Any]:
    """壓縮 primary scenario 成最小 LLM-friendly 摘要(避免回 raw scenario blob)。

    v3.35 加 `effective_degree` + `wave_span_years`(picker 用)— LLM 看 primary 是
    哪個 degree 級的 scenario(對 3030 預期看到 Minor / Primary,而非 SubMinuette)。
    """
    if scenario is None:
        return {
            "label":              None,
            "pattern_type":       None,
            "power_rating":       "Neutral",
            "wave_count":         0,
            "effective_degree":   None,
            "wave_span_years":    None,
        }
    pattern_type = scenario.get("pattern_type")
    if isinstance(pattern_type, dict):
        pattern_label = next(iter(pattern_type.keys()), "Unknown")
    elif isinstance(pattern_type, str):
        pattern_label = pattern_type
    else:
        pattern_label = "Unknown"

    # v3.28 修(2026-05-17):wave_count 從 structure_label parse(`"5-wave from mw27..."`)
    label = scenario.get("structure_label") or scenario.get("id") or ""
    wave_count = 0
    import re
    m = re.search(r"(\d+)-wave", label)
    if m:
        wave_count = int(m.group(1))

    # v3.35:degree + span surface
    degree = _compute_scenario_effective_degree(scenario)
    span_years = _scenario_span_years(scenario)

    return {
        "label":              label or None,
        "pattern_type":       pattern_label,
        "power_rating":       _power_rating_label(scenario.get("power_rating")),
        "wave_count":         wave_count,
        "effective_degree":   degree,
        "wave_span_years":    round(span_years, 2) if span_years is not None else None,
        # v3.37:scenario 來自哪 timeframe(daily / weekly / monthly)
        "timeframe":          scenario.get("__timeframe__"),
    }


def _build_neely_by_timeframe(snapshot, current_price: float) -> dict[str, Any]:
    """v3.37:per-timeframe primary scenario 摘要,LLM 看跨 timeframe 一致性。

    output schema:
      {
        "daily":   {"timeframe_present": True,  "scenario_count": 7,
                    "primary_scenario": {...},  "primary_effective_degree": "SubMinuette"},
        "weekly":  {"timeframe_present": True,  "scenario_count": 5,
                    "primary_scenario": {...},  "primary_effective_degree": "Minute"},
        "monthly": {"timeframe_present": True,  "scenario_count": 2,
                    "primary_scenario": {...},  "primary_effective_degree": "Minor"},
        "cross_timeframe_summary": "monthly 給 Minor degree anchor / daily / weekly 給短中期 momentum confirm"
      }

    Backward compat:若 snapshot 內無 weekly / monthly 的 neely_core entry(v3.37 前的
    舊 production data),該 entry 走 `timeframe_present=False`,primary_scenario 為 None。
    """
    structural = snapshot.structural or {}
    out: dict[str, Any] = {}

    for tf in ("daily", "weekly", "monthly"):
        key = f"neely_core@{tf}"
        row = structural.get(key)
        if row is None:
            out[tf] = {
                "timeframe_present":         False,
                "scenario_count":            0,
                "primary_scenario":          None,
                "primary_effective_degree":  None,
            }
            continue

        snap = row.snapshot or {}
        scenarios = snap.get("scenario_forest") or snap.get("scenarios") or []
        if not isinstance(scenarios, list):
            scenarios = []

        # tag + invalidation filter + sort 同款邏輯(per-timeframe 獨立)
        tagged = []
        for s in scenarios:
            if not isinstance(s, dict):
                continue
            t = dict(s)
            t["__timeframe__"] = tf
            tagged.append(t)

        if current_price is not None and current_price > 0:
            tagged = [s for s in tagged if not _scenario_is_invalidated(s, current_price)]

        def _score(s: dict) -> tuple[int, int, int]:
            return (
                _degree_rank(_compute_scenario_effective_degree(s)),
                _power_rating_strength(s.get("power_rating")),
                int(s.get("rules_passed_count") or 0),
            )
        tagged.sort(key=_score, reverse=True)
        primary = tagged[0] if tagged else None
        out[tf] = {
            "timeframe_present":         True,
            "scenario_count":            len(tagged),
            "primary_scenario":          _format_primary_scenario(primary),
            "primary_effective_degree":  _compute_scenario_effective_degree(primary) if primary else None,
        }

    # cross-timeframe summary(narrative 給 LLM)
    out["cross_timeframe_summary"] = _compose_cross_timeframe_summary(out)
    return out


def _compose_cross_timeframe_summary(by_tf: dict[str, Any]) -> str:
    """v3.37:多 timeframe degree 對照敘述。LLM 看到哪個 timeframe 給最強 anchor。"""
    parts: list[str] = []
    for tf in ("daily", "weekly", "monthly"):
        info = by_tf.get(tf, {})
        if not info.get("timeframe_present"):
            parts.append(f"{tf}=無資料")
            continue
        degree = info.get("primary_effective_degree") or "Unknown"
        count = info.get("scenario_count", 0)
        parts.append(f"{tf}={degree}({count} scenarios)")
    return " / ".join(parts)


def _scenario_span_years(scenario: dict) -> float | None:
    """scenario.wave_tree.start ~ end 年數(LLM-friendly,_format_primary_scenario 用)。"""
    wave_tree = scenario.get("wave_tree") or {}
    start_str = wave_tree.get("start")
    end_str = wave_tree.get("end")
    if not start_str or not end_str:
        return None
    try:
        start = _parse_iso_date(start_str)
        end = _parse_iso_date(end_str)
    except (ValueError, TypeError):
        return None
    if end < start:
        return None
    return (end - start).days / 365.25


# ────────────────────────────────────────────────────────────
# v3.35.1 quality caveat — LLM 警示 picker 給的 primary 是否可用
# ────────────────────────────────────────────────────────────

# Degree label → is_short_degree?(<= SubMinuette 視為短期 swing,不適合長 history 股票)
_SHORT_DEGREE_LABELS: set[str] = {"SubMicro", "Micro", "SubMinuette"}


def _compute_quality_caveat(
    all_scenarios: list[dict], primary: dict | None, current_price: float,
) -> dict[str, Any]:
    """v3.35.1 picker quality 警示:幫 LLM 識別何時 primary 不適合 long-anchor 判讀。

    對應 v3.35 production verify 揭露的 root cause:
      Rust Stage 3 Generator 對長 history 股票(e.g. 3030 7 yr)只產短 span
      candidates(全 SubMinuette < 1 yr)→ picker 即使 degree DESC 也只能選最長的 SubMinuette
      → forecasts / invalidation_price / key_levels 對齊 short swing,LLM 若不知情會誤判。

    回兩種警示:
      (a) is_short_degree_only:所有 scenarios 都 ≤ SubMinuette → 無長期 anchor 可用
      (b) fib_zones_decoupled_from_price:primary fib zones 與 current_price 完全脫節
          (zones bracket 外 50% buffer 以上)→ forecasts 區間不可用

    若 (a) 或 (b) 命中,narrative 出警示 + 設旗 is_usable=False。
    """
    warnings: list[str] = []

    # ── (a) short-degree only ─────────────────────────────────
    is_short_only = False
    max_span = 0.0
    max_degree: str | None = None
    if all_scenarios:
        for s in all_scenarios:
            span = _scenario_span_years(s)
            if span is not None and span > max_span:
                max_span = span
                max_degree = _compute_scenario_effective_degree(s)
        is_short_only = (max_degree is None) or (max_degree in _SHORT_DEGREE_LABELS)
        if is_short_only and max_span > 0:
            warnings.append(
                f"所有 {len(all_scenarios)} 個 scenarios 都是 short-degree "
                f"(最長 span {max_span:.2f} yr,{max_degree or 'unknown'})— Rust Stage 3 Generator "
                f"對長期 history 沒產 Minor+ degree candidates,LLM 無長期 anchor 可用。"
                f"參考 invalidation_price 時請注意該值對應近期短 swing。"
            )

    # ── (b) primary fib zones 與 current_price 脫節 ───────────
    is_decoupled = False
    if primary is not None and current_price > 0:
        fib_zones = primary.get("expected_fib_zones") or []
        all_prices: list[float] = []
        for zone in fib_zones:
            if not isinstance(zone, dict):
                continue
            for key in ("low", "high"):
                v = zone.get(key)
                if isinstance(v, (int, float)):
                    all_prices.append(float(v))

        if all_prices:
            fib_max = max(all_prices)
            fib_min = min(all_prices)
            buffer = (fib_max - fib_min) * 0.5 if fib_max > fib_min else fib_max * 0.1
            if current_price > fib_max + buffer or current_price < fib_min - buffer:
                is_decoupled = True
                warnings.append(
                    f"current_price={current_price:.2f} 在 primary scenario fib zones "
                    f"[{fib_min:.2f}, {fib_max:.2f}] 之外(+/- 50% buffer)— forecasts 區間"
                    f"基於短期 swing anchor 投影,不適用當前 price level。"
                )

    return {
        "is_short_degree_only":           is_short_only,
        "max_scenario_span_years":        round(max_span, 2) if max_span > 0 else None,
        "max_scenario_degree":            max_degree,
        "fib_zones_decoupled_from_price": is_decoupled,
        "is_usable":                      not (is_short_only or is_decoupled),
        "warnings":                       warnings,
    }


def _extract_current_price(snapshot) -> float:
    """從 ma_core series 最後一筆 close 取;沒有 fallback 0.0。"""
    for key, row in snapshot.indicator_latest.items():
        if not key.startswith("ma_core"):
            continue
        value = row.value or {}
        series = value.get("series")
        if isinstance(series, list) and series:
            last = series[-1]
            if isinstance(last, dict) and "close" in last:
                try:
                    return float(last["close"])
                except (TypeError, ValueError):
                    pass
    return 0.0


def _build_forecasts(
    primary: dict | None,
    current_price: float,
    snapshot,
    as_of: date,
) -> dict[str, dict[str, Any]]:
    """4 個 timeframe 各算 prob_up + range_high + range_low。"""
    if primary is None or current_price <= 0:
        # 無 primary scenario / 無價格 → 全 neutral
        return {
            tf: {"prob_up": 0.50, "range_high": None, "range_low": None}
            for tf in _TIMEFRAME_FIB_RANGE
        }

    fib_zones = primary.get("expected_fib_zones") or []
    pr_label = _power_rating_label(primary.get("power_rating"))
    base_prob = _POWER_TO_PROB.get(pr_label, 0.50)
    pr_sign = _power_rating_sign(primary.get("power_rating"))

    # 跨 cores 動能 / 籌碼 adjustments(plan §Tool 1 第 3 點公式)
    momentum_adj = _compute_momentum_adj(snapshot)
    chip_adj     = _compute_chip_adj(snapshot)

    forecasts: dict[str, dict[str, Any]] = {}
    for tf, (ratio_lo, ratio_hi) in _TIMEFRAME_FIB_RANGE.items():
        decay = _TIMEFRAME_DECAY[tf]
        # prob_up = base + decay * (momentum + chip) ×(sign 對齊 bullish/bearish 方向)
        raw_prob = base_prob + decay * pr_sign * (momentum_adj + chip_adj)
        prob_up = max(0.10, min(0.90, raw_prob))

        # 價位區間從 fib zones 拉
        range_low, range_high = _project_range(fib_zones, ratio_lo, ratio_hi, current_price, pr_sign)

        forecasts[tf] = {
            "prob_up":    round(prob_up, 2),
            "range_high": _round_range(range_high),
            "range_low":  _round_range(range_low),
        }

    return forecasts


def _compute_momentum_adj(snapshot) -> float:
    """從 macd / rsi / adx 算 momentum adjustment(-0.13 ~ +0.13)。"""
    adj = 0.0
    indicator_latest = snapshot.indicator_latest

    # macd histogram sign(+0.05 / -0.05 / 0)
    for key, row in indicator_latest.items():
        if not key.startswith("macd_core"):
            continue
        value = row.value or {}
        last = _get_last_series_point(value)
        if last:
            histogram = last.get("histogram") or 0
            try:
                if float(histogram) > 0:
                    adj += 0.05
                elif float(histogram) < 0:
                    adj -= 0.05
            except (TypeError, ValueError):
                pass
        break

    # rsi: (rsi - 50) / 100 × 0.05
    for key, row in indicator_latest.items():
        if not key.startswith("rsi_core"):
            continue
        value = row.value or {}
        last = _get_last_series_point(value)
        if last:
            rsi = last.get("rsi") or last.get("value") or 50
            try:
                adj += (float(rsi) - 50.0) / 100.0 * 0.05
            except (TypeError, ValueError):
                pass
        break

    # adx +DI / -DI 比較(or adx 趨勢強度)
    for key, row in indicator_latest.items():
        if not key.startswith("adx_core"):
            continue
        value = row.value or {}
        last = _get_last_series_point(value)
        if last:
            try:
                plus_di = float(last.get("plus_di") or 0)
                minus_di = float(last.get("minus_di") or 0)
                if plus_di > minus_di:
                    adj += 0.03
                elif minus_di > plus_di:
                    adj -= 0.03
            except (TypeError, ValueError):
                pass
        break

    return 0.5 * adj


def _compute_chip_adj(snapshot) -> float:
    """從 institutional / foreign_holding 算 chip adjustment(-0.07 ~ +0.07)。"""
    adj = 0.0
    indicator_latest = snapshot.indicator_latest

    # institutional net_5d sign
    for key, row in indicator_latest.items():
        if not key.startswith("institutional_core"):
            continue
        value = row.value or {}
        last = _get_last_series_point(value)
        if last:
            try:
                inst_net = float(last.get("total_net") or last.get("net") or 0)
                if inst_net > 0:
                    adj += 0.04
                elif inst_net < 0:
                    adj -= 0.04
            except (TypeError, ValueError):
                pass
        break

    # foreign_holding change_pct
    for key, row in indicator_latest.items():
        if not key.startswith("foreign_holding_core"):
            continue
        value = row.value or {}
        last = _get_last_series_point(value)
        if last:
            try:
                change = float(last.get("change_pct") or last.get("holding_change") or 0)
                if change > 0:
                    adj += 0.03
                elif change < 0:
                    adj -= 0.03
            except (TypeError, ValueError):
                pass
        break

    return 0.5 * adj


def _get_last_series_point(value: dict) -> dict | None:
    """從 indicator value JSONB 拿最後一筆 series point。"""
    series = value.get("series")
    if isinstance(series, list) and series:
        last = series[-1]
        if isinstance(last, dict):
            return last
    return None


def _project_range(
    fib_zones: list[dict],
    ratio_lo: float,
    ratio_hi: float,
    current_price: float,
    sign: int,
) -> tuple[list[float] | None, list[float] | None]:
    """從 fib_zones 中找最接近 ratio_lo / ratio_hi 的 zone,回 (range_low, range_high)。

    每個 range 用 [low, high] list 表達(對應 FibZone.low / FibZone.high)。
    若 fib_zones 為空 → fallback 用 current_price × ratio 估算。
    """
    if not fib_zones:
        # Fallback:用 current_price × ratio scaling
        if sign >= 0:
            # bullish:預期上漲,target 高於 current
            return (
                [current_price * (1 - ratio_lo / 10), current_price * (1 - ratio_lo / 20)],
                [current_price * (1 + ratio_lo / 10), current_price * (1 + ratio_hi / 10)],
            )
        else:
            # bearish:預期下跌,target 低於 current
            return (
                [current_price * (1 - ratio_hi / 10), current_price * (1 - ratio_lo / 10)],
                [current_price * (1 + ratio_lo / 20), current_price * (1 + ratio_lo / 10)],
            )

    # 從 fib_zones 找 ratio_lo / ratio_hi 對應的 zone
    zone_lo = _find_closest_zone(fib_zones, ratio_lo)
    zone_hi = _find_closest_zone(fib_zones, ratio_hi)

    range_low = [zone_lo["low"], zone_lo["high"]] if zone_lo else None
    range_high = [zone_hi["low"], zone_hi["high"]] if zone_hi else None

    # 確保 range_high 真的 ≥ current,range_low 真的 ≤ current(bullish 場景)
    # bearish 場景 swap
    if sign < 0 and range_high and range_low:
        # bearish 反過來
        range_low, range_high = range_high, range_low

    return range_low, range_high


def _find_closest_zone(fib_zones: list[dict], target_ratio: float) -> dict | None:
    """找 source_ratio 最接近 target 的 fib zone。"""
    best = None
    best_diff = float("inf")
    for z in fib_zones:
        try:
            r = float(z.get("source_ratio") or 0)
            diff = abs(r - target_ratio)
            if diff < best_diff:
                best_diff = diff
                best = z
        except (TypeError, ValueError):
            continue
    return best


def _round_range(rng: list[float] | None) -> list[float] | None:
    """範圍 round 到 2 位小數。"""
    if rng is None:
        return None
    return [round(float(v), 2) for v in rng]


def _extract_key_levels(primary: dict | None, current_price: float) -> dict[str, list[float]]:
    """從 primary scenario 的 fib_zones + invalidation triggers 抽 support / resistance。"""
    if primary is None or current_price <= 0:
        return {"support": [], "resistance": []}

    fib_zones = primary.get("expected_fib_zones") or []
    supports: list[float] = []
    resistances: list[float] = []

    for z in fib_zones:
        try:
            low = float(z["low"])
            high = float(z["high"])
            mid = (low + high) / 2
            if mid < current_price:
                supports.append(round(mid, 2))
            elif mid > current_price:
                resistances.append(round(mid, 2))
        except (KeyError, TypeError, ValueError):
            continue

    # 排序:supports 大 → 小(近 current),resistances 小 → 大
    supports.sort(reverse=True)
    resistances.sort()

    # 限制各 3 個避免噪音
    return {
        "support":    supports[:3],
        "resistance": resistances[:3],
    }


def _extract_invalidation_price(primary: dict | None, current_price: float) -> float | None:
    """從 primary scenario 的 invalidation_triggers 抽價格(PriceBreakBelow / PriceBreakAbove)。"""
    if primary is None:
        return None
    triggers = primary.get("invalidation_triggers") or []
    pr_sign = _power_rating_sign(primary.get("power_rating"))

    # bullish 看 break_below 失效;bearish 看 break_above 失效
    target_key = "PriceBreakBelow" if pr_sign >= 0 else "PriceBreakAbove"

    for t in triggers:
        trigger_type = t.get("trigger_type")
        if isinstance(trigger_type, dict) and target_key in trigger_type:
            try:
                return round(float(trigger_type[target_key]), 2)
            except (TypeError, ValueError):
                continue
    return None
