"""Golden L3 fusion 物化 — 版本 / params_hash / lineage / 變更偵測 helper。

集中定義 levels_fusion / resonance_fusion / climate_fusion 三個物化輸出的:
- `*_SOURCE_VERSION` 常數(bump → backfill 全量 re-derive)
- `*_params_hash()` canonical 字串(PK 一部分;同 params → 同 hash → ON CONFLICT UPDATE,不會 row 爆炸)
- `*_DERIVED_FROM` CSV(嚴守規格:純上游 core 名 lineage,re-derive **不**靠它當 cache key)
- backfill `fusion_row_exists()` skip-if-exists(daily 走 always-recompute-latest,不呼叫此)

對齊 m3Spec/golden-layers.md「L3 儲存:沿用 structural_snapshots」決議。
"""

from __future__ import annotations

from datetime import date
from typing import Any

# ── core_name / sentinel ──────────────────────────────────────────────────
LEVELS_CORE = "levels_fusion"
RESONANCE_CORE = "resonance_fusion"
CLIMATE_CORE = "climate_fusion"

# levels 是 per-stock(key_levels 把 daily/weekly/monthly 折進 source 標籤)→ 哨兵 tf
LEVELS_TIMEFRAME = "_all_"
# climate 是 marketwide,沿用既有保留字 stock_id `_market_`(對齊 _climate.py)
CLIMATE_STOCK_ID = "_market_"
CLIMATE_TIMEFRAME = "_all_"
# resonance 是 per-(stock, timeframe)
RESONANCE_TIMEFRAMES: tuple[str, ...] = ("daily", "weekly", "monthly")

# ── source_version(per-output 邏輯版本;bump 觸發 backfill 全量重算)──────────
LEVELS_SOURCE_VERSION = "levels_v1"
RESONANCE_SOURCE_VERSION = "resonance_v1"
CLIMATE_SOURCE_VERSION = "climate_v1"

# ── derived_from_core(嚴守規格:上游 core 名 CSV,純 lineage)─────────────────
LEVELS_DERIVED_FROM = "support_resistance_core,trendline_core,neely_core"
RESONANCE_DERIVED_FROM = "neely_core,forecast,magic_formula_ranked_derived"
CLIMATE_DERIVED_FROM = (
    "taiex_core,us_market_core,fear_greed_core,business_indicator_core,"
    "exchange_rate_core,market_margin_core,commodity_macro_core,risk_alert_core"
)

# structural_snapshots PK(寫入時給 DBWriter.upsert 的 ON CONFLICT 欄位)
PK_COLS = ["stock_id", "snapshot_date", "timeframe", "core_name", "params_hash"]


# ── params_hash canonical(編碼預設 knob;非 blake3,對齊 forecast "fusion|..." 慣例)─
def levels_params_hash(*, top_n: int = 20, lookback_days: int = 120) -> str:
    return f"lv|top{top_n}|lb{lookback_days}"


def resonance_params_hash(
    *,
    primary_horizon: int = 63,
    primary_confidence: float = 0.80,
    median_tolerance: float = 0.02,
    horizons: tuple[int, ...] = (21, 63, 126),
) -> str:
    h = "-".join(str(x) for x in horizons)
    return f"rz|h{primary_horizon}|c{primary_confidence:.2f}|tol{median_tolerance}|H{h}"


def climate_params_hash(*, lookback_days: int = 60) -> str:
    return f"cl|lb{lookback_days}"


def build_row(
    *,
    stock_id: str,
    snapshot_date: date,
    timeframe: str,
    core_name: str,
    source_version: str,
    params_hash: str,
    snapshot: dict[str, Any],
    derived_from_core: str,
) -> dict[str, Any]:
    """組一筆 structural_snapshots row(snapshot dict 由 DBWriter.upsert 轉 Jsonb)。"""
    return {
        "stock_id": stock_id,
        "snapshot_date": snapshot_date,
        "timeframe": timeframe,
        "core_name": core_name,
        "source_version": source_version,
        "params_hash": params_hash,
        "snapshot": snapshot,
        "derived_from_core": derived_from_core,
    }


# ── 讀取面 helper(read conn = get_connection,dict_row)────────────────────
def latest_trading_date(conn) -> date | None:
    """price_daily 最新交易日(as_of=None 時的預設;對齊 builder latest-date 慣例)。"""
    with conn.cursor() as cur:
        cur.execute("SELECT MAX(date) AS d FROM price_daily WHERE market = 'TW'")
        row = cur.fetchone()
    return row["d"] if row and row.get("d") else None


def fetch_universe(conn, stocks: list[str] | None = None) -> list[str]:
    """物化 universe:顯式 stocks 或 price_daily_fwd 全市場 distinct stock_id。"""
    if stocks:
        return list(stocks)
    with conn.cursor() as cur:
        cur.execute(
            "SELECT DISTINCT stock_id FROM price_daily_fwd "
            "WHERE market = 'TW' ORDER BY stock_id"
        )
        return [r["stock_id"] for r in cur.fetchall()]


def fusion_row_exists(
    conn,
    *,
    stock_id: str,
    timeframe: str,
    core_name: str,
    snapshot_date: date,
    source_version: str,
) -> bool:
    """backfill skip-if-exists:該 (stock, tf, core, snapshot_date) 已有 row 且 version 相符。

    daily refresh 走 always-recompute-latest 不呼叫此(Option B 每日全市場重跑上游 →
    skip 不划算)。僅 backfill 歷史回填時用此跳過已物化的靜態歷史。source_version bump
    → 既有 row version 不符 → 回 False → 重算(對齊規格「source_version 變即全量重算」)。
    """
    with conn.cursor() as cur:
        cur.execute(
            "SELECT 1 FROM structural_snapshots "
            "WHERE stock_id = %s AND timeframe = %s AND core_name = %s "
            "  AND snapshot_date = %s AND source_version = %s LIMIT 1",
            [stock_id, timeframe, core_name, snapshot_date, source_version],
        )
        return cur.fetchone() is not None


def forecast_log_lag_days(conn, as_of: date) -> int | None:
    """forecast_log 最新 forecast_date 落後 as_of 幾天(供 resonance stale 警告)。

    回 None = forecast_log 無資料(全市場 track2 缺 band);0 = 當日已有;正數 = 落後天數。
    只看非 internal_only(track2 讀統計軌,neely_fib internal_only 不算)。
    """
    with conn.cursor() as cur:
        cur.execute(
            "SELECT MAX(forecast_date) AS d FROM forecast_log "
            "WHERE forecast_date <= %s AND internal_only = FALSE",
            [as_of],
        )
        row = cur.fetchone()
    d = row["d"] if row else None
    if d is None:
        return None
    return (as_of - d).days
