"""v3.14: gov_bank_buy_sell_tw 加 bank_name 維度 + buy_amount/sell_amount

User 2026-05-16 升 FinMind sponsor tier 後 smoke gov_bank 揭露真實 schema:
FinMind `TaiwanStockGovernmentBankBuySell` 一日回 11906 row × 2312 stocks,
每 row 帶 `bank_name`(8 大行庫)+ `buy_amount`/`sell_amount`(NTD 金額)+
`buy`/`sell`(股數)。原 v3 Bronze PK (market, stock_id, date) 會踩衝突
(CLAUDE.md v1.21-B §H 預警過)。

本 migration:
- ALTER ADD bank_name TEXT NOT NULL DEFAULT '' (表為空 → 0 row 不受影響)
- ALTER ADD buy_amount NUMERIC, sell_amount NUMERIC(NTD 金額,8 大行庫各自報告)
- DROP 舊 PK (market, stock_id, date) → ADD 新 PK (market, stock_id, date, bank_name)
- 既有 trigger `mark_institutional_derived_from_gov_bank_dirty` 不動(generic
  `trg_mark_silver_dirty`,讀 NEW.market/stock_id/date,bank_name 加進 PK 不影響)

下游 Silver builder `institutional.py._gov_bank_net` 需改成
`SUM(buy) - SUM(sell) GROUP BY stock_id, date`(本 migration 不動,builder
同 PR 接續)。

Revision ID: a6b7c8d9e0f1
Revises: z5a6b7c8d9e0
Create Date: 2026-05-16
"""

from alembic import op


revision = 'a6b7c8d9e0f1'
down_revision = 'z5a6b7c8d9e0'
branch_labels = None
depends_on = None


def upgrade() -> None:
    # 新增三欄(冪等)
    op.execute(
        """
        ALTER TABLE government_bank_buy_sell_tw
            ADD COLUMN IF NOT EXISTS bank_name TEXT NOT NULL DEFAULT ''
        """
    )
    op.execute(
        """
        ALTER TABLE government_bank_buy_sell_tw
            ADD COLUMN IF NOT EXISTS buy_amount NUMERIC
        """
    )
    op.execute(
        """
        ALTER TABLE government_bank_buy_sell_tw
            ADD COLUMN IF NOT EXISTS sell_amount NUMERIC
        """
    )

    # 換 PK:動態查既有 PK 名 → DROP → 加新 PK 含 bank_name
    # 對齊 v1.30 a2a9df3 hotfix pattern(PG 不保證 rename 後 constraint 名同步)
    op.execute(
        """
        DO $$
        DECLARE
            pk_name TEXT;
        BEGIN
            SELECT conname INTO pk_name
            FROM pg_constraint
            WHERE conrelid = 'government_bank_buy_sell_tw'::regclass
              AND contype = 'p';
            IF pk_name IS NOT NULL THEN
                EXECUTE 'ALTER TABLE government_bank_buy_sell_tw DROP CONSTRAINT ' || quote_ident(pk_name);
            END IF;
        END $$;
        """
    )

    op.execute(
        """
        ALTER TABLE government_bank_buy_sell_tw
            ADD CONSTRAINT government_bank_buy_sell_tw_pkey
            PRIMARY KEY (market, stock_id, date, bank_name)
        """
    )

    # 移除 bank_name DEFAULT(新資料一定會帶,DEFAULT 只為冪等加欄用)
    op.execute(
        """
        ALTER TABLE government_bank_buy_sell_tw
            ALTER COLUMN bank_name DROP DEFAULT
        """
    )


def downgrade() -> None:
    op.execute(
        """
        DO $$
        DECLARE
            pk_name TEXT;
        BEGIN
            SELECT conname INTO pk_name
            FROM pg_constraint
            WHERE conrelid = 'government_bank_buy_sell_tw'::regclass
              AND contype = 'p';
            IF pk_name IS NOT NULL THEN
                EXECUTE 'ALTER TABLE government_bank_buy_sell_tw DROP CONSTRAINT ' || quote_ident(pk_name);
            END IF;
        END $$;
        """
    )

    op.execute(
        """
        ALTER TABLE government_bank_buy_sell_tw
            ADD CONSTRAINT government_bank_buy_sell_tw_pkey
            PRIMARY KEY (market, stock_id, date)
        """
    )

    op.execute(
        """
        ALTER TABLE government_bank_buy_sell_tw
            DROP COLUMN IF EXISTS sell_amount,
            DROP COLUMN IF EXISTS buy_amount,
            DROP COLUMN IF EXISTS bank_name
        """
    )
