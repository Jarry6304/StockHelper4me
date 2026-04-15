"""
stock_resolver.py
------------------
股票清單解析模組。

負責根據 stock_list.toml 的設定，回傳要處理的股票代碼清單。
支援三種來源模式：db（動態）、file（靜態）、both（聯集）。

先雞後蛋問題：
首次執行時 stock_info 表是空的。
phase_executor 在 Phase 1 完成後重新呼叫 resolve()，
Phase 2 起才能使用從 DB 取得的清單。
"""

import logging

from config_loader import StockListConfig
from db import DBWriter

logger = logging.getLogger("collector.stock_resolver")


def resolve(stock_list_cfg: StockListConfig, db: DBWriter) -> list[str]:
    """
    依設定決定股票清單來源並回傳排序後的代碼清單。

    優先順序：
    1. dev.enabled = true → 直接回傳靜態清單，忽略其他設定
    2. source.mode = "file" → 回傳 [stocks].ids
    3. source.mode = "db"   → 從 stock_info 查詢並套用篩選條件
    4. source.mode = "both" → db 結果 ∪ file 靜態清單（取聯集）

    Args:
        stock_list_cfg: stock_list.toml 設定物件
        db:             DBWriter 連線（mode=db 或 both 時使用）

    Returns:
        排序後的股票代碼清單（str list）
    """
    # 開發模式：強制使用靜態清單，避免全市場耗費 API 配額
    if stock_list_cfg.dev_enabled:
        ids = sorted(stock_list_cfg.static_ids)
        logger.info(f"[開發模式] 使用靜態股票清單，共 {len(ids)} 檔")
        return ids

    mode = stock_list_cfg.source_mode

    if mode == "file":
        ids = sorted(stock_list_cfg.static_ids)
        logger.info(f"股票清單來源：file，共 {len(ids)} 檔")
        return ids

    if mode == "db":
        ids = _query_from_db(stock_list_cfg, db)
        logger.info(f"股票清單來源：db，共 {len(ids)} 檔")
        return ids

    if mode == "both":
        db_ids   = set(_query_from_db(stock_list_cfg, db))
        file_ids = set(stock_list_cfg.static_ids)
        ids      = sorted(db_ids | file_ids)  # 取聯集
        logger.info(
            f"股票清單來源：both，db={len(db_ids)} + file={len(file_ids)} "
            f"→ 聯集 {len(ids)} 檔"
        )
        return ids

    # 未知 mode，回退為空清單並記錄警告
    logger.warning(f"未知的 source.mode='{mode}'，回傳空清單")
    return []


def _query_from_db(stock_list_cfg: StockListConfig, db: DBWriter) -> list[str]:
    """
    從 stock_info 表查詢符合篩選條件的股票代碼。

    篩選條件（全部來自 stock_list.toml [filter]）：
    - market_type：只保留指定市場（twse / otc / emerging）
    - exclude_etf：排除 ETF
    - exclude_warrant：排除權證
    - exclude_tdr：排除 TDR
    - exclude_delisted：排除已下市/下櫃
    - min_listing_days：排除上市未滿 N 天的新股

    Args:
        stock_list_cfg: 設定物件
        db:             DBWriter 連線

    Returns:
        排序後的股票代碼列表
    """
    conditions: list[str] = []
    params: list = []

    # 市場類型篩選
    if stock_list_cfg.market_type:
        placeholders = ", ".join(["?"] * len(stock_list_cfg.market_type))
        conditions.append(f"market_type IN ({placeholders})")
        params.extend(stock_list_cfg.market_type)

    # 排除已下市/下櫃
    if stock_list_cfg.exclude_delisted:
        conditions.append("(delist_date IS NULL OR delist_date = '')")

    # 排除 ETF（依命名慣例：股票代碼為 4 碼數字的通常是 ETF，
    # 但更可靠的方式是看 industry 欄位）
    if stock_list_cfg.exclude_etf:
        conditions.append("(industry != 'ETF' OR industry IS NULL)")

    # 排除權證（台灣權證代碼通常為 6 碼，第 1 碼為 0）
    if stock_list_cfg.exclude_warrant:
        conditions.append("length(stock_id) < 6")

    # 排除 TDR（台灣存託憑證，代碼通常以 9 開頭且為 5 碼）
    if stock_list_cfg.exclude_tdr:
        conditions.append("NOT (length(stock_id) = 5 AND stock_id LIKE '9%')")

    # 排除上市未滿 N 天的新股
    if stock_list_cfg.min_listing_days > 0:
        conditions.append(
            f"(listing_date IS NULL OR "
            f"julianday('now') - julianday(listing_date) >= ?)"
        )
        params.append(stock_list_cfg.min_listing_days)

    # 組裝 SQL
    where_clause = " AND ".join(conditions) if conditions else "1=1"
    sql = f"""
        SELECT stock_id
        FROM stock_info
        WHERE {where_clause}
        ORDER BY stock_id
    """

    rows = db.query(sql, params)
    return [row["stock_id"] for row in rows]
