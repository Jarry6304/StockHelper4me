"""v3.21: 3 Silver derived 表 for loan_collateral / block_trade / commodity_macro cores

接 v3.20 Bronze 5 datasets 落地 + v3.21 user 拍版 4 cores spec 後,新增 3 張 Silver
derived 表(對齊 PR #20 dirty-driven pattern):

1. `loan_collateral_balance_derived` — 5 大類 current_balance + 5 change_pct + JSONB
2. `block_trade_derived` — SUM by trade_type per (stock, date)
3. `commodity_price_daily_derived` — z-score / streak / momentum per commodity

risk_alert_core 不需 Silver derived(對齊 fear_greed_core 例外,直讀 Bronze)。
market_value 不需 Silver derived(純資料層,valuation_core 直接消費 Bronze)。

Dirty triggers:對齊 v1.20 PR #20 pattern,Bronze upsert → mark Silver dirty。
本次只加表,trigger 等 Silver builder 上線時補(避免 Bronze backfill 期間 trigger
噪音重置)。

Revision ID: c8d9e0f1g2h3
Revises: b7c8d9e0f1g2
Create Date: 2026-05-17
"""

from alembic import op


revision = 'c8d9e0f1g2h3'
down_revision = 'b7c8d9e0f1g2'
branch_labels = None
depends_on = None


def upgrade() -> None:
    # ─────────────────────────────────────────────────────────────────────
    # 1. loan_collateral_balance_derived — 5 主欄 + 5 change_pct + JSONB
    #    對齊 chip_cores.md §十 拍版:5 主欄 + 5 衍生欄 + JSONB pack 其他 25
    # ─────────────────────────────────────────────────────────────────────
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS loan_collateral_balance_derived (
            market                              TEXT NOT NULL,
            stock_id                            TEXT NOT NULL,
            date                                DATE NOT NULL,
            -- 5 主欄:5 大類 current_balance
            margin_current_balance              BIGINT,
            firm_loan_current_balance           BIGINT,
            unrestricted_loan_current_balance   BIGINT,
            finance_loan_current_balance        BIGINT,
            settlement_margin_current_balance   BIGINT,
            -- 5 衍生欄:對前一交易日 % 變化
            margin_change_pct                   DOUBLE PRECISION,
            firm_loan_change_pct                DOUBLE PRECISION,
            unrestricted_loan_change_pct        DOUBLE PRECISION,
            finance_loan_change_pct             DOUBLE PRECISION,
            settlement_margin_change_pct        DOUBLE PRECISION,
            -- 跨類:總合 + dominant category info(Concentration EventKind 用)
            total_balance                       BIGINT,
            dominant_category                   TEXT,                 -- 'margin' | 'firm_loan' | ...
            dominant_category_ratio             DOUBLE PRECISION,     -- 0..1
            -- detail JSONB pack 其他 25 cols(Previous/Buy/Sell/CashRedemption/Replacement/NextDayQuota × 5)
            detail                              JSONB,
            is_dirty                            BOOLEAN NOT NULL DEFAULT FALSE,
            dirty_at                            TIMESTAMPTZ,
            PRIMARY KEY (market, stock_id, date)
        )
        """
    )
    op.execute(
        """
        CREATE INDEX IF NOT EXISTS idx_loan_collateral_derived_dirty
            ON loan_collateral_balance_derived (market, stock_id)
            WHERE is_dirty = TRUE
        """
    )

    # ─────────────────────────────────────────────────────────────────────
    # 2. block_trade_derived — SUM by trade_type per (stock, date)
    #    對齊 chip_cores.md §十一 拍版:per-stock per-day 聚合
    # ─────────────────────────────────────────────────────────────────────
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS block_trade_derived (
            market                          TEXT NOT NULL,
            stock_id                        TEXT NOT NULL,
            date                            DATE NOT NULL,
            -- 全 trade_type SUM
            total_volume                    BIGINT,
            total_trading_money             BIGINT,
            -- 配對交易專屬(MatchingTradeSpike EventKind 用)
            matching_volume                 BIGINT,
            matching_trading_money          BIGINT,
            matching_share                  DOUBLE PRECISION,        -- matching_volume / total_volume
            -- 大單痕跡
            largest_single_trade_money      BIGINT,
            trade_type_count                INTEGER,                 -- distinct trade_type 數
            -- detail JSONB pack:per-trade_type breakdown
            detail                          JSONB,
            is_dirty                        BOOLEAN NOT NULL DEFAULT FALSE,
            dirty_at                        TIMESTAMPTZ,
            PRIMARY KEY (market, stock_id, date)
        )
        """
    )
    op.execute(
        """
        CREATE INDEX IF NOT EXISTS idx_block_trade_derived_dirty
            ON block_trade_derived (market, stock_id)
            WHERE is_dirty = TRUE
        """
    )

    # ─────────────────────────────────────────────────────────────────────
    # 3. commodity_price_daily_derived — z-score / streak / momentum per commodity
    #    對齊 environment_cores.md §十 拍版:GROUP BY commodity 算 macro signal
    # ─────────────────────────────────────────────────────────────────────
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS commodity_price_daily_derived (
            market                  TEXT NOT NULL,
            commodity               TEXT NOT NULL,                  -- GOLD | (future SILVER/OIL)
            date                    DATE NOT NULL,
            price                   NUMERIC(15, 4),
            return_pct              DOUBLE PRECISION,               -- vs t-1
            return_z_score          DOUBLE PRECISION,               -- rolling 60d z-score
            momentum_state          TEXT,                            -- 'up' | 'down' | 'neutral'
            streak_days             INTEGER,                         -- 連續同向天數
            detail                  JSONB,                           -- {lookback_days, mean, std, ...}
            is_dirty                BOOLEAN NOT NULL DEFAULT FALSE,
            dirty_at                TIMESTAMPTZ,
            PRIMARY KEY (market, commodity, date)
        )
        """
    )
    op.execute(
        """
        CREATE INDEX IF NOT EXISTS idx_commodity_macro_derived_dirty
            ON commodity_price_daily_derived (market, commodity)
            WHERE is_dirty = TRUE
        """
    )


def downgrade() -> None:
    op.execute("DROP TABLE IF EXISTS commodity_price_daily_derived")
    op.execute("DROP TABLE IF EXISTS block_trade_derived")
    op.execute("DROP TABLE IF EXISTS loan_collateral_balance_derived")
