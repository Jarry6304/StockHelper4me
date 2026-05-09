"""pr_r3_promote_tw_bronze_3_tables

Revision ID: t9u0v1w2x3y4
Revises: s8t9u0v1w2x3
Create Date: 2026-05-09 00:00:01.000000

==============================================================================
m2 重構 PR #R3 — 3 張 `_tw` Bronze 表去 `_tw` suffix 升格成主名。

依 m2Spec/data_refactor_plan.md §五:

  1. holding_shares_per_tw   → holding_shares_per
  2. financial_statement_tw  → financial_statement
  3. monthly_revenue_tw      → monthly_revenue

連帶 rename 3 個 explicit index(去 `_tw` 後綴):
  - idx_holding_shares_per_tw_stock_date_desc   → idx_holding_shares_per_stock_date_desc
  - idx_financial_statement_tw_stock_date_desc  → idx_financial_statement_stock_date_desc
  - idx_monthly_revenue_tw_stock_date_desc      → idx_monthly_revenue_stock_date_desc

(PK 由 PG 自動跟著表 rename。)

PR 順序強約束:必須在 PR #R2(主名空出來 → `_legacy_v2`)落地後才能執行,
否則 rename 會撞既有 v2.0 表 PK 衝突。

------------------------------------------------------------------------------
Trigger 重綁:**不需手動 DROP + 重建**
------------------------------------------------------------------------------
PG trigger 透過 OID 綁 table,不是綁名字 — ALTER TABLE RENAME 後 trigger 自動
跟著表走,event_object_table 自動更新到新名。已於 2026-05-09 sandbox smoke
test 驗證(_smoke_t → _smoke_t2 後 information_schema.triggers.event_object_table
回 _smoke_t2)。

3 個受影響的 dirty trigger:
  - mark_holding_shares_per_derived_dirty   ON holding_shares_per_tw  → 自動跟到 holding_shares_per
  - mark_financial_stmt_derived_dirty       ON financial_statement_tw → 自動跟到 financial_statement
  - mark_monthly_revenue_derived_dirty      ON monthly_revenue_tw     → 自動跟到 monthly_revenue

→ 本 migration 不動 trigger DDL;`src/schema_pg.sql` 仍要把 CREATE TRIGGER ...
   ON *_tw 改成主名(給 fresh DB 走 schema_pg.sql 初始化用)。

------------------------------------------------------------------------------
配套改動(本 PR 同步落地):
------------------------------------------------------------------------------
1. `src/schema_pg.sql`:3 個 CREATE TABLE 改名 + 3 個 CREATE INDEX 改名 +
   3 個 CREATE TRIGGER ON 表名同步;comment 標 PR #R3
2. `config/collector.toml`:5 個 v3 entry 的 target_table 從 `*_tw` 改主名
   (`holding_shares_per_v3` / `financial_income_v3` / `financial_balance_v3` /
    `financial_cashflow_v3` / `monthly_revenue_v3`);entry name 仍留 `_v3`,等 R4 收
3. `src/silver/builders/{holding_shares_per,financial_statement,monthly_revenue}.py`:
   `BRONZE_TABLES` + `fetch_bronze()` 表名同步主名
4. `scripts/inspect_db.py` / `scripts/verify_pr20_triggers.py`:表名同步
5. `CLAUDE.md`:v1.24 段 + alembic head 更新

------------------------------------------------------------------------------
idempotent 設計
------------------------------------------------------------------------------
DO $$ ... IF EXISTS old AND NOT EXISTS new ... THEN RENAME ... END $$
- 既有 DB(舊名 `*_tw`)→ rename 走起來
- fresh DB(schema_pg.sql 已是主名)→ no-op pass through

------------------------------------------------------------------------------
風險:🟡 中
------------------------------------------------------------------------------
- collector.toml 5 個 v3 entries 的 `target_table` 必須同步改主名,否則
  dual-write 寫到舊名 → 表不存在 → upsert 炸
- 3 個 Silver builder 的 `BRONZE_TABLES` + `fetch_bronze()` 必須同步主名,
  否則讀空表 → Silver pipeline 斷
- v2.0 entry 的 target_table 已在 PR #R2 改 `_legacy_v2`,**不要動**

Rollback:downgrade rename 反向(主名 → `*_tw`,索引同步反向)。
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


revision: str = "t9u0v1w2x3y4"
down_revision: Union[str, Sequence[str], None] = "s8t9u0v1w2x3"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


# 表名 + 索引 rename 對映
TABLE_RENAMES = (
    ("holding_shares_per_tw",  "holding_shares_per"),
    ("financial_statement_tw", "financial_statement"),
    ("monthly_revenue_tw",     "monthly_revenue"),
)

INDEX_RENAMES = (
    ("idx_holding_shares_per_tw_stock_date_desc",  "idx_holding_shares_per_stock_date_desc"),
    ("idx_financial_statement_tw_stock_date_desc", "idx_financial_statement_stock_date_desc"),
    ("idx_monthly_revenue_tw_stock_date_desc",     "idx_monthly_revenue_stock_date_desc"),
)


def upgrade() -> None:
    """3 張 `_tw` 表升格主名(idempotent IF EXISTS 防衛)。"""
    for old_name, new_name in TABLE_RENAMES:
        op.execute(f"""
        DO $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM information_schema.tables
                WHERE table_name = '{old_name}' AND table_schema = 'public'
            ) AND NOT EXISTS (
                SELECT 1 FROM information_schema.tables
                WHERE table_name = '{new_name}' AND table_schema = 'public'
            ) THEN
                ALTER TABLE {old_name} RENAME TO {new_name};
            END IF;
        END $$;
        """)

    for old_idx, new_idx in INDEX_RENAMES:
        op.execute(f"""
        DO $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM pg_indexes
                WHERE indexname = '{old_idx}' AND schemaname = 'public'
            ) AND NOT EXISTS (
                SELECT 1 FROM pg_indexes
                WHERE indexname = '{new_idx}' AND schemaname = 'public'
            ) THEN
                ALTER INDEX {old_idx} RENAME TO {new_idx};
            END IF;
        END $$;
        """)


def downgrade() -> None:
    """rename 反向:主名 → `*_tw`,索引同步反向。"""
    for old_idx, new_idx in INDEX_RENAMES:
        op.execute(f"""
        DO $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM pg_indexes
                WHERE indexname = '{new_idx}' AND schemaname = 'public'
            ) AND NOT EXISTS (
                SELECT 1 FROM pg_indexes
                WHERE indexname = '{old_idx}' AND schemaname = 'public'
            ) THEN
                ALTER INDEX {new_idx} RENAME TO {old_idx};
            END IF;
        END $$;
        """)

    for old_name, new_name in TABLE_RENAMES:
        op.execute(f"""
        DO $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM information_schema.tables
                WHERE table_name = '{new_name}' AND table_schema = 'public'
            ) AND NOT EXISTS (
                SELECT 1 FROM information_schema.tables
                WHERE table_name = '{old_name}' AND table_schema = 'public'
            ) THEN
                ALTER TABLE {new_name} RENAME TO {old_name};
            END IF;
        END $$;
        """)
