"""
aggregators.py
---------------
Phase E 特殊資料聚合模組。

處理三類需要跨列合併的 FinMind 回傳資料：

1. pivot_institutional（三大法人 pivot）：
   API 每日回傳 3 筆（外資/投信/自營商），合併為 1 筆，欄位拆分

2. pack_financial（財報科目打包）：
   API 每日回傳 N 筆（各財務科目），依 (date, stock_id) 合併為 1 筆，
   所有科目打包進 detail JSON

3. pack_holding_shares（股權分散表打包）：
   API 每日回傳多筆（各持股級距），依 (date, stock_id) 合併為 1 筆，
   各級距打包進 detail JSON
"""

import json
import logging
from typing import Any

logger = logging.getLogger("collector.aggregators")

# ──────────────────────────────────────────────────────────────────────────────
# 三大法人機構名稱 → DB 欄位前綴 對應表
# FinMind 的 name 欄位值（中文，可能有多種寫法）
# ──────────────────────────────────────────────────────────────────────────────
INSTITUTIONAL_NAME_MAP: dict[str, tuple[str, str]] = {
    # 外資（不含外資自營商）
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
    # 自營商（自行買賣）
    "自營商":                 ("dealer_buy",            "dealer_sell"),
    "自營商(自行買賣)":        ("dealer_buy",            "dealer_sell"),
    "Dealer":                ("dealer_buy",            "dealer_sell"),
    "Dealer_self":           ("dealer_buy",            "dealer_sell"),
    # 自營商（避險）
    "自營商(避險)":           ("dealer_hedging_buy",   "dealer_hedging_sell"),
    "Dealer_Hedging":        ("dealer_hedging_buy",   "dealer_hedging_sell"),
}

# 已知會出現但可從 5 類法人自己加總得到的「合計」列，靜默略過不發 warning
INSTITUTIONAL_NAME_IGNORED: set[str] = {
    "total", "Total",      # TaiwanStockTotalInstitutionalInvestors 多回的合計列
    "合計",                 # 中文版本
}


def aggregate_institutional(
    rows: list[dict[str, Any]],
    trading_dates: set[str] | None = None,
) -> list[dict[str, Any]]:
    """
    將三大法人 API 回傳的多筆資料（每日 3 筆）pivot 為 1 筆。

    輸入（3 筆 / 日 / 股票）：
      [{date, stock_id, name="外資及陸資", buy=100000, sell=50000, market, source},
       {date, stock_id, name="投信",       buy=5000,  sell=3000,  market, source},
       {date, stock_id, name="自營商",     buy=2000,  sell=1000,  market, source}]

    輸出（1 筆 / 日 / 股票）：
      [{date, stock_id, foreign_buy=100000, foreign_sell=50000,
        investment_trust_buy=5000, investment_trust_sell=3000,
        dealer_buy=2000, dealer_sell=1000, market, source}]

    Args:
        rows:          field_mapper 輸出的資料列（已含 market/source）
        trading_dates: 若提供，會過濾掉 date 不在此集合內的 rows，避免
                       FinMind institutional API 在週六回鬼資料（內容為某筆
                       殘留值，date 是非交易日）

    Returns:
        pivot 後的資料列
    """
    if trading_dates is not None:
        rows = _filter_to_trading_days(rows, trading_dates, label="institutional")

    grouped: dict[tuple, dict[str, Any]] = {}

    for row in rows:
        key = (row.get("date"), row.get("stock_id"))
        if key not in grouped:
            grouped[key] = {
                "date":     row.get("date"),
                "stock_id": row.get("stock_id"),
                "market":   row.get("market", "TW"),
                "source":   row.get("source", "finmind"),
                # 初始化所有欄位為 None（部分機構可能缺漏）
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
            pass  # 已知合計列，靜默略過
        else:
            logger.warning(f"未知的法人機構名稱：'{name}'，無法 pivot，已略過")

    result = list(grouped.values())
    logger.debug(f"institutional pivot：{len(rows)} 筆 → {len(result)} 筆")
    return result


def aggregate_institutional_market(
    rows: list[dict[str, Any]],
    trading_dates: set[str] | None = None,
) -> list[dict[str, Any]]:
    """
    將全市場三大法人 API 回傳的多筆資料 pivot 為 1 筆（無 stock_id）。

    輸入（3 筆 / 日）：
      [{date, name="外資及陸資", buy=..., sell=..., market, source}, ...]

    輸出（1 筆 / 日）：
      [{date, foreign_buy=..., foreign_sell=..., ..., market, source}]

    Args:
        trading_dates: 若提供，會過濾掉 date 不在此集合內的 rows（同上）
    """
    if trading_dates is not None:
        rows = _filter_to_trading_days(rows, trading_dates, label="institutional_market")

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
            pass  # 已知合計列，靜默略過
        else:
            logger.warning(f"未知的法人機構名稱（market）：'{name}'，無法 pivot，已略過")

    result = list(grouped.values())
    logger.debug(f"institutional_market pivot：{len(rows)} 筆 → {len(result)} 筆")
    return result


def aggregate_financial(
    rows: list[dict[str, Any]],
    stmt_type: str,
) -> list[dict[str, Any]]:
    """
    將財報科目 API（損益表/資負表/現金流量）的多筆資料打包為 1 筆 / 日。

    FinMind 財報 API 格式（每筆 = 1 個財務科目）：
      {date, stock_id, type=科目名稱, value=金額, origin_name=英文科目名}

    打包後格式：
      {date, stock_id, type=stmt_type, detail=JSON{origin_name: value, ...}}

    Args:
        rows:      field_mapper 輸出的資料列
        stmt_type: 報表類型識別字串，"income" | "balance" | "cashflow"

    Returns:
        每 (date, stock_id) 一筆的打包資料
    """
    grouped: dict[tuple, dict[str, Any]] = {}

    for row in rows:
        key = (row.get("date"), row.get("stock_id"))
        if key not in grouped:
            grouped[key] = {
                "date":     row.get("date"),
                "stock_id": row.get("stock_id"),
                "type":     stmt_type,
                "detail":   {},           # 先用 dict，最後序列化
                "market":   row.get("market", "TW"),
                "source":   row.get("source", "finmind"),
            }

        # origin_name 為英文科目名，type 為中文科目名
        # 優先用 origin_name 作為 detail 的 key，fallback 用 type
        item_key   = row.get("origin_name") or row.get("type") or "unknown"
        item_value = row.get("value")
        grouped[key]["detail"][item_key] = item_value

    # 將 detail dict 序列化為 JSON
    result = []
    for agg in grouped.values():
        agg["detail"] = json.dumps(agg["detail"], ensure_ascii=False)
        result.append(agg)

    logger.debug(f"financial pack（{stmt_type}）：{len(rows)} 筆 → {len(result)} 筆")
    return result


def aggregate_holding_shares(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """
    將股權分散表 API 的多筆資料（每筆 = 1 個持股級距）打包為 1 筆 / 日。

    FinMind TaiwanStockHoldingSharesPer 格式（每筆 = 1 個持股級距）：
      {date, stock_id, HoldingSharesLevel=級距, people=人數, percent=佔比, unit=股數}

    打包後格式：
      {date, stock_id, detail=JSON{HoldingSharesLevel: {people, percent, unit}, ...}}

    Args:
        rows: field_mapper 輸出的資料列

    Returns:
        每 (date, stock_id) 一筆的打包資料
    """
    grouped: dict[tuple, dict[str, Any]] = {}

    for row in rows:
        key = (row.get("date"), row.get("stock_id"))
        if key not in grouped:
            grouped[key] = {
                "date":     row.get("date"),
                "stock_id": row.get("stock_id"),
                "detail":   {},
                "market":   row.get("market", "TW"),
                "source":   row.get("source", "finmind"),
            }

        level = row.get("HoldingSharesLevel") or row.get("holding_shares_level", "unknown")
        grouped[key]["detail"][level] = {
            "people":  row.get("people"),
            "percent": row.get("percent"),
            "unit":    row.get("unit"),
        }

    result = []
    for agg in grouped.values():
        agg["detail"] = json.dumps(agg["detail"], ensure_ascii=False)
        result.append(agg)

    logger.debug(f"holding_shares pack：{len(rows)} 筆 → {len(result)} 筆")
    return result


# ──────────────────────────────────────────────────────────────────────────────
# 分派入口
# ──────────────────────────────────────────────────────────────────────────────

def apply_aggregation(
    strategy: str,
    rows: list[dict[str, Any]],
    stmt_type: str | None = None,
    trading_dates: set[str] | None = None,
) -> list[dict[str, Any]]:
    """
    依 strategy 名稱分派到對應的聚合函式。

    Args:
        strategy:      聚合策略名稱（對應 collector.toml 的 aggregation 欄位）
        rows:          field_mapper 輸出的原始資料列
        stmt_type:     財報類型（僅 pack_financial 需要）
        trading_dates: 交易日集合（僅 institutional 兩個策略會用到，過濾掉 FinMind
                       週六回的鬼資料）

    Returns:
        聚合後的資料列

    Raises:
        ValueError: 未知的 strategy 名稱
    """
    if strategy == "pivot_institutional":
        return aggregate_institutional(rows, trading_dates=trading_dates)

    if strategy == "pivot_institutional_market":
        return aggregate_institutional_market(rows, trading_dates=trading_dates)

    if strategy == "pack_financial":
        if stmt_type is None:
            raise ValueError("pack_financial 需要 stmt_type 參數")
        return aggregate_financial(rows, stmt_type)

    if strategy == "pack_holding_shares":
        return aggregate_holding_shares(rows)

    raise ValueError(f"未知的聚合策略：'{strategy}'")


# ──────────────────────────────────────────────────────────────────────────────
# 通用工具
# ──────────────────────────────────────────────────────────────────────────────

def _filter_to_trading_days(
    rows: list[dict[str, Any]],
    trading_dates: set[str],
    label: str,
) -> list[dict[str, Any]]:
    """過濾掉 date 不在 trading_dates 集合內的 rows，並記錄被丟掉的日期。"""
    # 安全閥：trading_dates 為空（trading_calendar 還沒灌資料）時不過濾，
    # 避免把整批資料都當鬼資料丟掉
    if not trading_dates:
        logger.warning(
            f"[{label}] trading_dates 為空（trading_calendar 表未填充？）"
            f"，跳過非交易日過濾"
        )
        return rows

    kept: list[dict[str, Any]] = []
    dropped_dates: set[str] = set()
    for row in rows:
        d = row.get("date")
        if d is None or d in trading_dates:
            kept.append(row)
        else:
            dropped_dates.add(d)
    if dropped_dates:
        logger.warning(
            f"[{label}] FinMind 回了 {len(dropped_dates)} 個非交易日的資料，"
            f"已過濾：{sorted(dropped_dates)}"
        )
    return kept
