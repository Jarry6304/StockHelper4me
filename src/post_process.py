"""
post_process.py
----------------
Phase 2 後處理模組：TaiwanStockDividend 合併邏輯。

執行時機：Phase 2 中 dividend_result 與 dividend_policy 都完成後執行。

兩個職責：
  1. 修補「權息」混合事件的 cash_dividend / stock_dividend 拆分
     （field_mapper 在遇到「權息」時設為 NULL，此處補齊）
  2. 偵測純現增事件（TaiwanStockDividendResult 無對應記錄的情況），
     寫入 price_adjustment_events，AF 計算延後至 Rust Phase 4（step 1.5）
"""

import json
import logging

from db import DBWriter

logger = logging.getLogger("collector.post_process")


def dividend_policy_merge(db: DBWriter, stock_id: str) -> None:
    """
    合併股利政策資料到 price_adjustment_events。

    Args:
        db:       DBWriter 連線
        stock_id: 要處理的股票代碼
    """
    _patch_mixed_dividend(db, stock_id)
    _detect_capital_increase(db, stock_id)
    _recompute_stock_dividend_vf(db, stock_id)
    invalidate_fwd_cache(db, stock_id)


def _recompute_stock_dividend_vf(db: DBWriter, stock_id: str) -> None:
    """對 stock_dividend > 0 的 dividend 事件,根據實際 stock_dividend 值重算
    volume_factor。

    P1-17 修正:`field_mapper.py:194-198` 把 stock_dividend 跟 cash_dividend
    統一寫 vf=1.0(只對 cash 對),導致 stock_dividend 事件 fwd_volume 沒對
    股本變化調整,違反 spec `indicator_cores_volume.md §2.4` 假設。

    公式(Taiwan 標準面額 10 元):
        vf = 1 / (1 + stock_dividend / 10)
    例:stock_dividend = 2.64 → vf ≈ 0.791;Rust 後復權 fwd_volume = raw / vf
    = raw × 1.264(post-split equivalent shares)。

    限制:面額非 10 元的個股 par_value_change 後本式不精確。本次接受此近似,
    後續 P1 issue 處理(可改成查 stock_info_ref.par_value 動態算)。
    """
    affected = db.update(
        "UPDATE price_adjustment_events "
        "SET volume_factor = 1.0 / (1.0 + stock_dividend / 10.0) "
        "WHERE market = %s AND stock_id = %s "
        "AND event_type = 'dividend' "
        "AND COALESCE(stock_dividend, 0) > 0 "
        "AND ABS(volume_factor - 1.0) < 0.0001",
        ['TW', stock_id],
    )
    if affected > 0:
        logger.info(
            f"[stock_dividend_vf] stock={stock_id} 修正 {affected} 筆 stock_dividend "
            f"事件的 volume_factor(P1-17)"
        )


def invalidate_fwd_cache(db: DBWriter, stock_id: str) -> None:
    """price_adjustment_events 改動後 reset stock_sync_status.fwd_adj_valid=0,
    讓 Rust Phase 4 下次跑時重算這支股票。

    P0-7 短期補丁:r3.1 av3 Test 3 揭露 staleness 實機證據(3363 / 1312
    stock_dividend 事件 fwd 沒處理)。長期 dirty queue 完整契約落地後可移除。
    """
    db.update(
        "INSERT INTO stock_sync_status (market, stock_id, fwd_adj_valid) "
        "VALUES (%s, %s, 0) "
        "ON CONFLICT (market, stock_id) DO UPDATE SET fwd_adj_valid = 0",
        ['TW', stock_id],
    )
    logger.info(f"[fwd_cache_invalidate] stock={stock_id} → fwd_adj_valid=0(Phase 4 將重算)")


# =============================================================================
# Step 1：修補「權息」混合事件
# =============================================================================

def _patch_mixed_dividend(db: DBWriter, stock_id: str) -> None:
    """
    找出 cash_dividend IS NULL 的除權息事件（即「權息」混合事件），
    從 _dividend_policy_staging 查出明細，補齊 cash_dividend / stock_dividend。

    Args:
        db:       DBWriter 連線
        stock_id: 股票代碼
    """
    # 查詢尚未拆分的「權息」混合事件
    mixed_events = db.query(
        """
        SELECT * FROM price_adjustment_events
        WHERE market = %s AND stock_id = %s AND event_type = 'dividend'
          AND cash_dividend IS NULL AND stock_dividend IS NULL
        """,
        ["TW", stock_id],
    )

    if not mixed_events:
        return

    logger.debug(f"{stock_id}: 找到 {len(mixed_events)} 筆待拆分的「權息」混合事件")

    for event in mixed_events:
        ex_date = event["date"]

        # 從暫存表查對應的股利政策
        policy = db.query_one(
            """
            SELECT * FROM _dividend_policy_staging
            WHERE market = %s AND stock_id = %s
              AND (
                  detail->>'CashExDividendTradingDate' = %s
                  OR detail->>'StockExDividendTradingDate' = %s
              )
            """,
            ["TW", stock_id, ex_date, ex_date],
        )

        if policy is None:
            logger.warning(
                f"{stock_id}: 找不到 ex_date={ex_date} 的股利政策記錄，"
                f"無法拆分「權息」事件"
            )
            continue

        # JSONB 已自動 deserialize 為 dict
        detail = policy["detail"] if policy["detail"] else {}

        # 計算現金股利（現金股利 + 法定盈餘公積現金）
        cash_earnings   = _safe_float(detail.get("CashEarningsDistribution", 0))
        cash_statutory  = _safe_float(detail.get("CashStatutorySurplus", 0))
        cash_dividend   = cash_earnings + cash_statutory

        # 計算股票股利（每股盈餘轉增資 + 法定盈餘公積轉增資）/ 10（轉換為元/股）
        stock_earnings  = _safe_float(detail.get("StockEarningsDistribution", 0))
        stock_statutory = _safe_float(detail.get("StockStatutorySurplus", 0))
        stock_dividend  = (stock_earnings + stock_statutory) / 10.0

        # 更新 price_adjustment_events
        db.update(
            """
            UPDATE price_adjustment_events
            SET cash_dividend = %s, stock_dividend = %s
            WHERE market = 'TW' AND stock_id = %s AND date = %s AND event_type = 'dividend'
            """,
            [cash_dividend, stock_dividend, stock_id, ex_date],
        )

        logger.debug(
            f"{stock_id} {ex_date}: 「權息」拆分完成 "
            f"cash={cash_dividend}, stock={stock_dividend}"
        )


# =============================================================================
# Step 2：偵測純現增事件
# =============================================================================

def _detect_capital_increase(db: DBWriter, stock_id: str) -> None:
    """
    找出 _dividend_policy_staging 中有現金增資（CashIncreaseSubscriptionRate > 0）
    但 price_adjustment_events 中無對應日期記錄的情況，
    將其插入為 capital_increase 事件。

    ⚠️ AF 計算完全移交 Rust Phase 4（step 1.5）處理：
    此時 Phase 3（price_daily）尚未入庫，無法計算 AF。
    Python 只寫入原始訂閱資料，Rust 在 Phase 4 補算。

    Args:
        db:       DBWriter 連線
        stock_id: 股票代碼
    """
    # 查詢有現金增資的股利政策
    capital_increases = db.query(
        """
        SELECT * FROM _dividend_policy_staging
        WHERE market = %s AND stock_id = %s
          AND (detail->>'CashIncreaseSubscriptionpRrice')::numeric > 0
        """,
        ["TW", stock_id],
    )

    for ci in capital_increases:
        detail = ci["detail"] if ci["detail"] else {}

        # 取得除權日（優先使用股票除權日，其次現金除息日）
        ex_date = (
            detail.get("StockExDividendTradingDate")
            or detail.get("CashExDividendTradingDate")
        )

        if not ex_date:
            logger.warning(f"{stock_id}: 現增事件無 ex_date，跳過")
            continue

        # 檢查 price_adjustment_events 中是否已有對應記錄
        existing = db.query_one(
            """
            SELECT 1 FROM price_adjustment_events
            WHERE market = %s AND stock_id = %s AND date = %s
            """,
            ["TW", stock_id, ex_date],
        )

        if existing:
            # 已有記錄（可能是 dividend_result 已涵蓋），不重複插入
            continue

        # 純現增事件，TaiwanStockDividendResult 中無對應記錄
        subscription_price = _safe_float(detail.get("CashIncreaseSubscriptionpRrice", 0))
        subscription_rate  = _safe_float(detail.get("CashIncreaseSubscriptionRate", 0))
        total_new_shares   = detail.get("TotalNumberOfCashCapitalIncrease")
        participating_shares = detail.get("ParticipateDistributionOfTotalShares")

        logger.warning(
            f"Pure capital increase detected: {stock_id} on {ex_date}, "
            f"subscription_price={subscription_price}, "
            f"subscription_rate={subscription_rate}. "
            f"AF deferred to Rust Phase 4 (step 1.5)."
        )

        # 插入暫時記錄，AF=1.0 為佔位符，待 Rust Phase 4 補算
        db.insert(
            "price_adjustment_events",
            {
                "market":            "TW",
                "stock_id":          stock_id,
                "date":              ex_date,
                "event_type":        "capital_increase",
                "before_price":      None,     # 需從 price_daily 補查
                "after_price":       None,     # 需計算
                "adjustment_factor": 1.0,      # 暫用，待 Rust Phase 4 修正
                "volume_factor":     1.0,      # 暫用
                "detail":            json.dumps(
                    {
                        "subscription_price":          subscription_price,
                        "subscription_rate_raw":       subscription_rate,
                        "total_new_shares":            total_new_shares,
                        "total_participating_shares":  participating_shares,
                        "source":                      "TaiwanStockDividend",
                        "status":                      "pending_rust_phase4",
                    },
                    ensure_ascii=False,
                ),
                "source": "finmind",
            },
        )


# =============================================================================
# 工具函式
# =============================================================================

def _safe_float(value) -> float:
    """安全地將值轉換為 float，無法轉換時回傳 0.0"""
    try:
        return float(value) if value is not None else 0.0
    except (TypeError, ValueError):
        return 0.0
