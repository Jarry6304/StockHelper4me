"""
post_process.py
----------------
Phase 2 後處理模組：TaiwanStockDividend 合併邏輯。

執行時機：Phase 2 中 dividend_result 與 dividend_policy 都完成後執行。

職責：
  1. 修補「權息」混合事件的 cash_dividend / stock_dividend 拆分
     （field_mapper 在遇到「權息」時設為 NULL，此處補齊）
  2. 偵測純現增事件（TaiwanStockDividendResult 無對應記錄的情況），
     寫入 price_adjustment_events，AF 計算延後至 Rust Phase 4（記憶體版,
     v3.2 PR #17 後不再 UPDATE 寫回 events 表）
  3. 修正 stock_dividend 事件的 volume_factor(P1-17 補丁)

§5.6 短期補丁 `invalidate_fwd_cache` 由 DB trigger `trg_mark_fwd_silver_dirty`
(PR #20 / alembic n3o4p5q6r7s8)接管;Phase 7c orchestrator 從
`price_daily_fwd.is_dirty=TRUE` pull 清單派 Rust。Python 端不再有對應函式
(PR #21 移除 deprecated shim)。
"""

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


# =============================================================================
# Step 1:修補「權息」混合事件(v3.3 batch SQL,對齊 plan §規格 7)
# =============================================================================

def _patch_mixed_dividend(db: DBWriter, stock_id: str) -> None:
    """
    找出 cash_dividend IS NULL 的除權息事件(即「權息」混合事件),
    從 _dividend_policy_staging 查出明細,補齊 cash_dividend / stock_dividend。

    v3.3 改:N+1(每 event 一個 query_one + update)→ 單一 batch UPDATE。
    對 1700+ 股 × 數年股利政策,從 ~5000 DB round-trip 降到 1 個 SQL。

    對齊 v2.x 公式(同 `_dividend_policy_staging` aggregator 內邏輯):
      cash_dividend = CashEarningsDistribution + CashStatutorySurplus
      stock_dividend = (StockEarningsDistribution + StockStatutorySurplus) / 10

    Args:
        db:       DBWriter 連線
        stock_id: 股票代碼
    """
    # NULLIF('','')::numeric → NULL,COALESCE(... , 0) 防 NULL 加總出 NULL
    sql = """
        UPDATE price_adjustment_events pae
           SET cash_dividend  = sub.cash_dividend,
               stock_dividend = sub.stock_dividend
          FROM (
              SELECT
                  pae.market, pae.stock_id, pae.date, pae.event_type,
                  (COALESCE(NULLIF(dps.detail->>'CashEarningsDistribution', '')::numeric, 0)
                   + COALESCE(NULLIF(dps.detail->>'CashStatutorySurplus',  '')::numeric, 0))
                      AS cash_dividend,
                  (COALESCE(NULLIF(dps.detail->>'StockEarningsDistribution', '')::numeric, 0)
                   + COALESCE(NULLIF(dps.detail->>'StockStatutorySurplus',  '')::numeric, 0))
                  / 10.0 AS stock_dividend
              FROM price_adjustment_events pae
              JOIN _dividend_policy_staging dps
                ON dps.market   = pae.market
               AND dps.stock_id = pae.stock_id
               AND (
                   NULLIF(dps.detail->>'CashExDividendTradingDate',  '')::date = pae.date
                OR NULLIF(dps.detail->>'StockExDividendTradingDate', '')::date = pae.date
               )
             WHERE pae.market = %s
               AND pae.stock_id = %s
               AND pae.event_type = 'dividend'
               AND pae.cash_dividend  IS NULL
               AND pae.stock_dividend IS NULL
          ) AS sub
         WHERE pae.market     = sub.market
           AND pae.stock_id   = sub.stock_id
           AND pae.date       = sub.date
           AND pae.event_type = sub.event_type
    """
    affected = db.update(sql, ["TW", stock_id])
    if affected > 0:
        logger.debug(
            f"{stock_id}: 「權息」拆分完成,{affected} 筆 event 從 staging 補齊 "
            f"cash_dividend / stock_dividend"
        )


# =============================================================================
# Step 2:偵測純現增事件(v3.3 batch SQL,對齊 plan §規格 7)
# =============================================================================

def _detect_capital_increase(db: DBWriter, stock_id: str) -> None:
    """
    找出 _dividend_policy_staging 中有現金增資
    (CashIncreaseSubscriptionpRrice > 0)但 price_adjustment_events 無對應
    日期記錄的情況,批次插入為 capital_increase 事件。

    v3.3 改:N+1(每 ci 一個 query_one + insert)→ 單一 batch
    INSERT ... SELECT ... WHERE NOT EXISTS ... ON CONFLICT DO NOTHING。

    ⚠️ AF 計算完全移交 Rust Phase 4 處理(此時 Phase 3 price_daily 尚未入庫,
    無法計算 AF)。Python 只寫入原始訂閱資料,Rust 在 Phase 4 從
    detail.subscription_price + raw_prices 反推 AF(在記憶體現算,不寫回 DB)。

    Args:
        db:       DBWriter 連線
        stock_id: 股票代碼
    """
    # 取除權日:優先 StockExDividendTradingDate,fallback CashExDividendTradingDate
    # detail JSON 寫成 status='pending_rust_phase4' 觸發 Rust patch_capital_increase_af
    sql = """
        INSERT INTO price_adjustment_events
            (market, stock_id, date, event_type, before_price, volume_factor, detail)
        SELECT
            'TW', %s,
            COALESCE(
                NULLIF(dps.detail->>'StockExDividendTradingDate', '')::date,
                NULLIF(dps.detail->>'CashExDividendTradingDate',  '')::date
            ) AS ex_date,
            'capital_increase',
            NULL,
            1.0,
            jsonb_build_object(
                'subscription_price',          NULLIF(dps.detail->>'CashIncreaseSubscriptionpRrice', '')::numeric,
                'subscription_rate_raw',       NULLIF(dps.detail->>'CashIncreaseSubscriptionRate',   '')::numeric,
                'total_new_shares',            dps.detail->>'TotalNumberOfCashCapitalIncrease',
                'total_participating_shares',  dps.detail->>'ParticipateDistributionOfTotalShares',
                'source',                      'TaiwanStockDividend',
                'status',                      'pending_rust_phase4'
            )
          FROM _dividend_policy_staging dps
         WHERE dps.market = 'TW'
           AND dps.stock_id = %s
           AND COALESCE(NULLIF(dps.detail->>'CashIncreaseSubscriptionpRrice', '')::numeric, 0) > 0
           AND COALESCE(
                NULLIF(dps.detail->>'StockExDividendTradingDate', '')::date,
                NULLIF(dps.detail->>'CashExDividendTradingDate',  '')::date
           ) IS NOT NULL
           AND NOT EXISTS (
               SELECT 1 FROM price_adjustment_events pae
                WHERE pae.market = 'TW'
                  AND pae.stock_id = dps.stock_id
                  AND pae.date = COALESCE(
                        NULLIF(dps.detail->>'StockExDividendTradingDate', '')::date,
                        NULLIF(dps.detail->>'CashExDividendTradingDate',  '')::date
                      )
           )
         ON CONFLICT DO NOTHING
    """
    affected = db.update(sql, [stock_id, stock_id])
    if affected > 0:
        logger.warning(
            f"Pure capital increase detected: stock={stock_id}, "
            f"inserted {affected} capital_increase events. "
            f"AF deferred to Rust Phase 4 (in-memory)."
        )


# =============================================================================
# 工具函式
# =============================================================================
# v3.3:_safe_float / _patch_mixed_dividend 內 Python 端拆分邏輯整段砍 —
# batch SQL 把計算 inline 進 UPDATE FROM (...) sub。CashEarningsDistribution
# 等 detail key 走 NULLIF('','')::numeric + COALESCE(... , 0),語意對齊
# 既有 _safe_float(value or 0)。
