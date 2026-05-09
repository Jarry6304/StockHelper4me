"""
silver/builders/day_trading.py
==============================
day_trading_tw + price_daily_fwd (Bronze + Silver) → day_trading_derived (Silver)。

PR #19b 階段:1:1 直拷 raw 部分(2 stored + detail JSONB 重 pack)。
PR #21-A:加 day_trading_ratio 衍生欄(% 單位)— 對齊 chip_cores.md §7.4。
PR #21-A hotfix:formula 改用 day_trading_tw.volume / pd.volume × 100
              (原本誤用 (buy+sell)×100/volume,但 buy/sell 是金額不是股數,
              撞 4000+ 倍的 bug;見 v1.18 user verify spot-check)。
v1.27:**改 LEFT JOIN price_daily_fwd 而非 price_daily**(對齊 spec
       layered_schema_post_refactor.md §6.4 + chip_cores.md §7.2 明文要求)。
       實質影響:歷史 pre-event(stock_dividend / split 前)日期的 ratio 改
       對齊「後復權 scale」,跨歷史 cross-comparison 一致性提升。
       依賴:**S1_adjustment(7c Rust)必須先跑過**才有 fwd 資料;
       初次 silver phase 7a 跑時 fwd 空 → ratio NULL,7c 跑完下一輪 7a
       自然填入。

Bronze 欄位語意(per FinMind TaiwanStockDayTrading + collector.toml notes):
  day_trading_buy    = BuyAmount(當沖買進金額,NTD)
  day_trading_sell   = SellAmount(當沖賣出金額,NTD)
  day_trading_flag   = BuyAfterSale(可否當沖旗標,str)
  day_trading_tw.volume = Volume(當沖成交股數)— 是當沖部分的股數,不是全日總量

公式:`day_trade_ratio = 當沖股數 / 全日成交股數 × 100`
  分子:day_trading_tw.volume(Bronze raw)
  分母:price_daily_fwd.volume(Silver,後復權 scale)
LEFT JOIN price_daily_fwd — 沒對應日的 fwd row(該股那天沒交易,或 7c 未跑過)
                            → ratio NULL。

- 2 stored cols(1:1):day_trading_buy / day_trading_sell
- detail JSONB 從 2 個 unpack 欄重 pack:day_trading_flag / volume(當沖股數)

Round-trip:Silver 2 stored + detail JSONB 應與 v2.0 day_trading 等值;
day_trading_ratio 是 PR #21-A 新增,不在 v2.0 legacy 比對範圍(verifier skip)。
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import upsert_silver


logger = logging.getLogger("collector.silver.builders.day_trading")


NAME          = "day_trading"
SILVER_TABLE  = "day_trading_derived"
BRONZE_TABLES = ["day_trading_tw", "price_daily_fwd"]

STORED_COLS = ("day_trading_buy", "day_trading_sell")
DETAIL_KEYS = ("day_trading_flag", "volume")


def _compute_ratio(day_trade_volume: Any, total_volume: Any) -> float | None:
    """day_trade_ratio = day_trade_volume / total_volume × 100(per spec §7.4)。

    任一欄 NULL → None。total_volume <= 0 → None(避免除以零)。
    回 float(NUMERIC 由 psycopg 端轉)。
    """
    if day_trade_volume is None or total_volume is None:
        return None
    try:
        tv = float(total_volume)
        if tv <= 0:
            return None
        return float(day_trade_volume) * 100.0 / tv
    except (TypeError, ValueError):
        return None


def _fetch_joined_rows(
    db: Any, stock_ids: list[str] | None,
) -> list[dict[str, Any]]:
    """SELECT day_trading_tw LEFT JOIN price_daily_fwd,拿當沖股數(raw)+ 全日股數(fwd)。

    v1.27 起改 join price_daily_fwd(spec §6.4):後復權 volume 跨歷史一致性提升,
    歷史 pre-event 日期的 ratio 對齊現在 scale。fwd 不存在(7c 未跑)→ pd_volume NULL
    → ratio NULL。
    """
    where = ""
    params: list[Any] = []
    if stock_ids:
        placeholders = ",".join(["%s"] * len(stock_ids))
        where = f"WHERE dt.stock_id IN ({placeholders})"
        params = list(stock_ids)
    sql = (
        "SELECT dt.market, dt.stock_id, dt.date, "
        "       dt.day_trading_buy, dt.day_trading_sell, "
        "       dt.day_trading_flag, "
        "       dt.volume AS dt_volume, "
        "       pd.volume AS pd_volume "
        "FROM day_trading_tw dt "
        "LEFT JOIN price_daily_fwd pd "
        "  ON dt.market = pd.market AND dt.stock_id = pd.stock_id AND dt.date = pd.date "
        f"{where} "
        "ORDER BY dt.market, dt.stock_id, dt.date"
    )
    return db.query(sql, params if params else None)


def _build_silver_rows(joined_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for row in joined_rows:
        s: dict[str, Any] = {
            "market":   row.get("market"),
            "stock_id": row.get("stock_id"),
            "date":     row.get("date"),
            "day_trading_buy":  row.get("day_trading_buy"),
            "day_trading_sell": row.get("day_trading_sell"),
        }
        s["day_trading_ratio"] = _compute_ratio(
            row.get("dt_volume"), row.get("pd_volume"),
        )
        s["detail"] = {
            "day_trading_flag": row.get("day_trading_flag"),
            "volume":           row.get("dt_volume"),  # 維持 v2.0 detail key 名(當沖股數)
        }
        out.append(s)
    return out


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    start = time.monotonic()

    joined = _fetch_joined_rows(db, stock_ids)
    silver = _build_silver_rows(joined)
    written = upsert_silver(
        db, SILVER_TABLE, silver,
        pk_cols=["market", "stock_id", "date"],
    )

    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(f"[{NAME}] read={len(joined)} → wrote={written}({elapsed_ms}ms)")
    return {
        "name":         NAME,
        "rows_read":    len(joined),
        "rows_written": written,
        "elapsed_ms":   elapsed_ms,
    }
