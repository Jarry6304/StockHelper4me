"""Fix Bronze financial_statement PK: use type instead of origin_name

FinMind TaiwanStockBalanceSheet returns BOTH element values (type='TotalAssets')
AND common-size % values (type='TotalAssets_per') for the same origin_name
(e.g. '資產總額'). With the old PK on origin_name, only the last-written row
survived per (date, event_type, origin_name) — for TSMC and ~2 other stocks,
_per rows were written last, destroying element values needed for ROE/ROA.

New PK uses the FinMind `type` column (e.g. 'TotalAssets' vs 'TotalAssets_per')
which is unique per item, allowing element value and _per row to coexist.
After this migration, re-backfill financial_statement for affected stocks
(at minimum: python src/main.py backfill --stocks 2330 --phases 5).

Revision ID: x3y4z5a6b7c8
Revises: w2x3y4z5a6b7
Create Date: 2026-05-11
"""
from alembic import op

revision = "x3y4z5a6b7c8"
down_revision = "w2x3y4z5a6b7"
branch_labels = None
depends_on = None


def upgrade() -> None:
    # 1. Remove rows where type IS NULL (should be none, but guard for safety)
    op.execute("DELETE FROM financial_statement WHERE type IS NULL OR type = ''")

    # 2. Drop old primary key (keyed on origin_name)
    op.execute("ALTER TABLE financial_statement DROP CONSTRAINT financial_statement_pkey")

    # 3. Make type NOT NULL now that NULLs are cleared
    op.execute("ALTER TABLE financial_statement ALTER COLUMN type SET NOT NULL")

    # 4. Add new primary key using type (allows TotalAssets + TotalAssets_per to coexist)
    op.execute(
        "ALTER TABLE financial_statement "
        "ADD CONSTRAINT financial_statement_pkey "
        "PRIMARY KEY (market, stock_id, date, event_type, type)"
    )


def downgrade() -> None:
    # WARNING: downgrade deletes all _per rows (since they would create duplicate
    # origin_name entries under the old PK). Re-backfill will restore element values.
    op.execute("DELETE FROM financial_statement WHERE type LIKE '%_per'")

    op.execute("ALTER TABLE financial_statement DROP CONSTRAINT financial_statement_pkey")
    op.execute("ALTER TABLE financial_statement ALTER COLUMN type DROP NOT NULL")

    op.execute(
        "ALTER TABLE financial_statement "
        "ADD CONSTRAINT financial_statement_pkey "
        "PRIMARY KEY (market, stock_id, date, event_type, origin_name)"
    )
