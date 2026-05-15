"""Magic Formula ranked (Greenblatt 2005) — Silver derived table

Cross-stock ranking by combined (EBIT/EV) + (EBIT/Invested Capital).
For each trading date, computes per-stock earnings_yield + roic, then ranks
across the non-financial / non-utility universe (Greenblatt 2005 original).

Refs:
  - Greenblatt, J. (2005). *The Little Book That Beats the Market*. Wiley.
  - Larkin, K. (2009). "Magic Formula investing — the long-term evidence."
    SSRN id=1330551 (OOS 1988-2007 valid)
  - Persson & Selander (2009). Lund Univ. thesis (European markets valid)

Revision ID: y4z5a6b7c8d9
Revises: x3y4z5a6b7c8
Create Date: 2026-05-15
"""
from alembic import op

revision = "y4z5a6b7c8d9"
down_revision = "x3y4z5a6b7c8"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS magic_formula_ranked_derived (
            market            TEXT NOT NULL,
            stock_id          TEXT NOT NULL,
            date              DATE NOT NULL,
            ebit_ttm          NUMERIC(20, 2),        -- 過去 4 季營業利益(損失)加總 NTD
            market_cap        NUMERIC(20, 2),        -- close × total_issued
            total_debt        NUMERIC(20, 2),        -- 估:Total Liabilities(後續若有 short/long debt 細分可拆)
            cash              NUMERIC(20, 2),        -- 現金及約當現金
            enterprise_value  NUMERIC(20, 2),        -- market_cap + total_debt - cash
            invested_capital  NUMERIC(20, 2),        -- total_assets - cash (working-capital proxy)
            earnings_yield    NUMERIC(10, 6),        -- ebit_ttm / enterprise_value
            roic              NUMERIC(10, 6),        -- ebit_ttm / invested_capital
            ey_rank           INTEGER,               -- 1..universe_size; NULL for excluded
            roic_rank         INTEGER,
            combined_rank     INTEGER,               -- ey_rank + roic_rank
            universe_size     INTEGER,
            is_top_30         BOOLEAN NOT NULL DEFAULT FALSE,
            excluded_reason   TEXT,                  -- 'financial' / 'utility' / 'no_ebit_data' / 'no_balance_data' / NULL
            detail            JSONB,
            is_dirty          BOOLEAN NOT NULL DEFAULT FALSE,
            dirty_at          TIMESTAMPTZ,
            PRIMARY KEY (market, stock_id, date)
        )
        """
    )
    op.execute(
        """
        CREATE INDEX IF NOT EXISTS idx_mf_top30
            ON magic_formula_ranked_derived (market, date, combined_rank)
            WHERE is_top_30 = TRUE
        """
    )
    op.execute(
        """
        CREATE INDEX IF NOT EXISTS idx_mf_dirty
            ON magic_formula_ranked_derived (market, stock_id)
            WHERE is_dirty = TRUE
        """
    )

    # 對齊 PR #20 trigger:Bronze upsert → mark Silver dirty
    # financial_statement / valuation_per / price_daily_fwd 三個 source 各加一條
    op.execute(
        """
        CREATE OR REPLACE FUNCTION trg_mark_magic_formula_dirty()
        RETURNS TRIGGER AS $$
        BEGIN
            INSERT INTO magic_formula_ranked_derived
                (market, stock_id, date, is_dirty, dirty_at)
            VALUES
                (NEW.market, NEW.stock_id, NEW.date, TRUE, NOW())
            ON CONFLICT (market, stock_id, date) DO UPDATE
              SET is_dirty = TRUE, dirty_at = NOW();
            RETURN NULL;
        END;
        $$ LANGUAGE plpgsql;
        """
    )
    op.execute(
        """
        DROP TRIGGER IF EXISTS mark_magic_formula_dirty_from_fs
            ON financial_statement;
        CREATE TRIGGER mark_magic_formula_dirty_from_fs
            AFTER INSERT OR UPDATE ON financial_statement
            FOR EACH ROW EXECUTE FUNCTION trg_mark_magic_formula_dirty();
        """
    )
    op.execute(
        """
        DROP TRIGGER IF EXISTS mark_magic_formula_dirty_from_val
            ON valuation_per_tw;
        CREATE TRIGGER mark_magic_formula_dirty_from_val
            AFTER INSERT OR UPDATE ON valuation_per_tw
            FOR EACH ROW EXECUTE FUNCTION trg_mark_magic_formula_dirty();
        """
    )


def downgrade() -> None:
    op.execute("DROP TRIGGER IF EXISTS mark_magic_formula_dirty_from_fs ON financial_statement")
    op.execute("DROP TRIGGER IF EXISTS mark_magic_formula_dirty_from_val ON valuation_per_tw")
    op.execute("DROP FUNCTION IF EXISTS trg_mark_magic_formula_dirty()")
    op.execute("DROP TABLE IF EXISTS magic_formula_ranked_derived")
