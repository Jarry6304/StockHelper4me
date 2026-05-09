"""
silver/builders/market_margin.py
================================
market_margin_maintenance + total_margin_purchase_short_sale_tw (Bronze) →
                            market_margin_maintenance_derived (Silver)。

PK = (market, date)— 市場級表,無 stock_id 欄。

Silver 衍生欄(per spec §2.6.3):
  - total_margin_purchase_balance(整體市場融資餘額)
  - total_short_sale_balance(整體市場融券餘額)
PR #21-B 落地:從 total_margin_purchase_short_sale_tw Bronze
(FinMind dataset TaiwanStockTotalMarginPurchaseShortSale)讀取後 LEFT JOIN
by (market, date) 補進 Silver。

⚠️ 2026-05-08 hotfix(alembic q6r7s8t9u0v1):FinMind 是 pivoted-by-row 格式,
1 個 (date) 對應 2 row(name='MarginPurchase' + name='ShortSale'),各帶
today_balance / yes_balance / buy / sell / return_amount。Bronze PK 加 `name`,
builder 走 pivot:
    name='MarginPurchase' 那 row 的 today_balance → total_margin_purchase_balance
    name='ShortSale'      那 row 的 today_balance → total_short_sale_balance

Bronze 缺對應 (market, date) → 兩欄 NULL(UNION 行為)。

Bronze 欄位:market / date / ratio
Silver 1:1 直拷 ratio + 2 衍生欄(UNION total_margin Bronze pivot)+ dirty 欄。

v1.26 nice-to-have:builder 改 iterate UNION(主, 副) keys,避免 PR #20 trigger
mark stub row(total_margin Bronze 寫入觸發)後 builder 沒 iterate 副 Bronze 留下
永久 stub row 10 筆。

UNION 行為:
- 主有 + 副有:full row(ratio + 2 衍生欄 = total_margin pivot 值)
- 主有 + 副無:ratio + 2 衍生欄 = NULL
- 主無 + 副有:ratio = NULL,2 衍生欄 = total_margin pivot 值(避免 PR #20 stub row)
"""

from __future__ import annotations

import logging
import time
from typing import Any

from .._common import fetch_bronze, upsert_silver


logger = logging.getLogger("collector.silver.builders.market_margin")


NAME          = "market_margin"
SILVER_TABLE  = "market_margin_maintenance_derived"
BRONZE_TABLES = ["market_margin_maintenance", "total_margin_purchase_short_sale_tw"]


# Bronze name 欄 → Silver 衍生欄
NAME_TO_SILVER_COL: dict[str, str] = {
    "MarginPurchase": "total_margin_purchase_balance",
    "ShortSale":      "total_short_sale_balance",
}

# 已知但不採用的 name(silently skip,不噴 warning)
# - MarginPurchaseMoney:2026-04-29 起 FinMind 新增的「融資金額(NTD)」,spec
#   §2.6.3 只要「融資餘額(shares)」即 MarginPurchase,本 metric 用不到
KNOWN_SKIP_NAMES: set[str] = {"MarginPurchaseMoney"}


def _build_total_margin_lookup(
    bronze_rows: list[dict[str, Any]],
) -> dict[tuple, dict[str, Any]]:
    """Pivot by name:{(market, date): {total_margin_purchase_balance, total_short_sale_balance}}。

    Bronze 1 (market, date) 對應 2~3 row(name=MarginPurchase / ShortSale 必有,
    2026-04-29 起 FinMind 新增 MarginPurchaseMoney 第 3 row),pivot 進 Silver 的 2 欄。
    任一 name 缺 row → 對應 Silver 欄 None;兩個 name 都缺 → key 不在 lookup。
    KNOWN_SKIP_NAMES 內的 name silently skip(不污染 log);其餘未知 name → warning。
    """
    out: dict[tuple, dict[str, Any]] = {}
    for row in bronze_rows:
        key = (row.get("market"), row.get("date"))
        if key not in out:
            out[key] = {
                "total_margin_purchase_balance": None,
                "total_short_sale_balance":      None,
            }
        name = row.get("name", "")
        silver_col = NAME_TO_SILVER_COL.get(name)
        if silver_col:
            out[key][silver_col] = row.get("today_balance")
        elif name in KNOWN_SKIP_NAMES:
            continue
        else:
            logger.warning(
                f"未知 name='{name}' "
                f"(market={key[0]}, date={key[1]}),已略過"
            )
    return out


def _build_silver_rows(
    bronze_rows: list[dict[str, Any]],
    total_margin_lookup: dict[tuple, dict[str, Any]],
) -> list[dict[str, Any]]:
    """v1.26 起 iterate UNION(主, 副) keys,避免 total_margin-only stub row 殘留。"""
    main_lookup: dict[tuple, dict[str, Any]] = {
        (r.get("market"), r.get("date")): r for r in bronze_rows
    }
    all_keys: set[tuple] = set(main_lookup.keys()) | set(total_margin_lookup.keys())

    out: list[dict[str, Any]] = []
    for key in sorted(all_keys, key=lambda k: (k[0] or "", k[1] or "")):
        market, date = key
        main = main_lookup.get(key)
        tm   = total_margin_lookup.get(key, {})
        out.append({
            "market": market,
            "date":   date,
            "ratio":  main.get("ratio") if main is not None else None,
            "total_margin_purchase_balance": tm.get("total_margin_purchase_balance"),
            "total_short_sale_balance":      tm.get("total_short_sale_balance"),
        })
    return out


def run(
    db: Any,
    stock_ids: list[str] | None = None,
    full_rebuild: bool = False,
) -> dict[str, Any]:
    """注意 stock_ids 對 market-level 表無效,一律全讀。"""
    start = time.monotonic()

    bronze = fetch_bronze(db, "market_margin_maintenance", order_by="market, date")
    total_margin = fetch_bronze(
        db, "total_margin_purchase_short_sale_tw",
        order_by="market, date, name",
    )
    total_margin_lookup = _build_total_margin_lookup(total_margin)

    silver = _build_silver_rows(bronze, total_margin_lookup)
    written = upsert_silver(
        db, SILVER_TABLE, silver,
        pk_cols=["market", "date"],
    )

    elapsed_ms = int((time.monotonic() - start) * 1000)
    logger.info(
        f"[{NAME}] read={len(bronze)} margin + {len(total_margin)} total_margin "
        f"(pivot to {len(total_margin_lookup)} dates,union to {len(silver)} silver rows) → "
        f"wrote={written}({elapsed_ms}ms)"
    )
    return {
        "name":         NAME,
        "rows_read":    len(bronze),
        "rows_written": written,
        "elapsed_ms":   elapsed_ms,
    }
