"""
bronze/aggregators/pivot_institutional.py
=========================================
三大法人 pivot:每日 N 列(per investor)→ 1 寬列 + 10 法人欄。

per-stock 與 market-level 兩個變體共用 INSTITUTIONAL_NAME_MAP。
"""

import logging
from typing import Any

from bronze._common import filter_to_trading_days

logger = logging.getLogger("collector.bronze.aggregators.pivot_institutional")

# ──────────────────────────────────────────────────────────────────────────────
# 三大法人機構名稱 → DB 欄位前綴 對應表
# FinMind 的 name 欄位值(中文,可能有多種寫法)
# ──────────────────────────────────────────────────────────────────────────────
INSTITUTIONAL_NAME_MAP: dict[str, tuple[str, str]] = {
    # 外資(不含外資自營商)
    "外資及陸資":             ("foreign_buy",          "foreign_sell"),
    "外資及陸資(不含外資自營商)": ("foreign_buy",          "foreign_sell"),
    "外資":                  ("foreign_buy",          "foreign_sell"),
    "Foreign_Investor":      ("foreign_buy",          "foreign_sell"),
    # 外資自營商
    "外資自營商":             ("foreign_dealer_self_buy", "foreign_dealer_self_sell"),
    "Foreign_Dealer_Self":   ("foreign_dealer_self_buy", "foreign_dealer_self_sell"),
    # 投信
    "投信":                  ("investment_trust_buy",  "investment_trust_sell"),
    "Investment_Trust":      ("investment_trust_buy",  "investment_trust_sell"),
    # 自營商(自行買賣)
    "自營商":                 ("dealer_buy",            "dealer_sell"),
    "自營商(自行買賣)":        ("dealer_buy",            "dealer_sell"),
    "Dealer":                ("dealer_buy",            "dealer_sell"),
    "Dealer_self":           ("dealer_buy",            "dealer_sell"),
    # 自營商(避險)
    "自營商(避險)":           ("dealer_hedging_buy",   "dealer_hedging_sell"),
    "Dealer_Hedging":        ("dealer_hedging_buy",   "dealer_hedging_sell"),
}

# 已知會出現但可從 5 類法人自己加總得到的「合計」列,靜默略過不發 warning
INSTITUTIONAL_NAME_IGNORED: set[str] = {
    "total", "Total",      # TaiwanStockTotalInstitutionalInvestors 多回的合計列
    "合計",                 # 中文版本
}


def aggregate_institutional(
    rows: list[dict[str, Any]],
    trading_dates: set[str] | None = None,
) -> list[dict[str, Any]]:
    """三大法人 API 每日 N 筆(per investor)→ 1 寬筆(per stock)。

    輸入(3 筆 / 日 / 股票):
      [{date, stock_id, name="外資及陸資", buy=100000, sell=50000, market, source},
       {date, stock_id, name="投信",       buy=5000,  sell=3000,  market, source},
       {date, stock_id, name="自營商",     buy=2000,  sell=1000,  market, source}]

    輸出(1 筆 / 日 / 股票):
      [{date, stock_id, foreign_buy=100000, foreign_sell=50000,
        investment_trust_buy=5000, investment_trust_sell=3000,
        dealer_buy=2000, dealer_sell=1000, market, source}]

    Args:
        rows:          field_mapper 輸出的資料列(已含 market/source)
        trading_dates: 若提供,會過濾掉 date 不在此集合內的 rows,避免
                       FinMind institutional API 在週六回鬼資料
    """
    if trading_dates is not None:
        rows = filter_to_trading_days(rows, trading_dates, label="institutional")

    grouped: dict[tuple, dict[str, Any]] = {}

    for row in rows:
        key = (row.get("date"), row.get("stock_id"))
        if key not in grouped:
            grouped[key] = {
                "date":     row.get("date"),
                "stock_id": row.get("stock_id"),
                "market":   row.get("market", "TW"),
                "source":   row.get("source", "finmind"),
                # 初始化所有欄位為 None(部分機構可能缺漏)
                "foreign_buy":               None,
                "foreign_sell":              None,
                "foreign_dealer_self_buy":   None,
                "foreign_dealer_self_sell":  None,
                "investment_trust_buy":      None,
                "investment_trust_sell":     None,
                "dealer_buy":                None,
                "dealer_sell":               None,
                "dealer_hedging_buy":        None,
                "dealer_hedging_sell":       None,
            }

        name = row.get("name", "")
        cols = INSTITUTIONAL_NAME_MAP.get(name)
        if cols:
            buy_col, sell_col = cols
            grouped[key][buy_col]  = row.get("buy")
            grouped[key][sell_col] = row.get("sell")
        elif name in INSTITUTIONAL_NAME_IGNORED:
            pass  # 已知合計列,靜默略過
        else:
            logger.warning(f"未知的法人機構名稱:'{name}',無法 pivot,已略過")

    result = list(grouped.values())
    logger.debug(f"institutional pivot:{len(rows)} 筆 → {len(result)} 筆")
    return result


def aggregate_institutional_market(
    rows: list[dict[str, Any]],
    trading_dates: set[str] | None = None,
) -> list[dict[str, Any]]:
    """全市場版本(無 stock_id):每日 N 筆 → 1 寬筆。"""
    if trading_dates is not None:
        rows = filter_to_trading_days(rows, trading_dates, label="institutional_market")

    grouped: dict[str, dict[str, Any]] = {}

    for row in rows:
        date = row.get("date", "")
        if date not in grouped:
            grouped[date] = {
                "date":   date,
                "market": row.get("market", "TW"),
                "source": row.get("source", "finmind"),
                "foreign_buy":               None,
                "foreign_sell":              None,
                "foreign_dealer_self_buy":   None,
                "foreign_dealer_self_sell":  None,
                "investment_trust_buy":      None,
                "investment_trust_sell":     None,
                "dealer_buy":                None,
                "dealer_sell":               None,
                "dealer_hedging_buy":        None,
                "dealer_hedging_sell":       None,
            }

        name = row.get("name", "")
        cols = INSTITUTIONAL_NAME_MAP.get(name)
        if cols:
            buy_col, sell_col = cols
            grouped[date][buy_col]  = row.get("buy")
            grouped[date][sell_col] = row.get("sell")
        elif name in INSTITUTIONAL_NAME_IGNORED:
            pass
        else:
            logger.warning(f"未知的法人機構名稱(market):'{name}',無法 pivot,已略過")

    result = list(grouped.values())
    logger.debug(f"institutional_market pivot:{len(rows)} 筆 → {len(result)} 筆")
    return result
