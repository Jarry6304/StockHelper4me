"""pr_r2_rename_v2_legacy_3_tables

Revision ID: s8t9u0v1w2x3
Revises: r7s8t9u0v1w2
Create Date: 2026-05-09 00:00:00.000000

==============================================================================
m2 重構 PR #R2 — 3 張 v2.0 舊 Bronze 表 rename `_legacy_v2` 進入觀察期。

依 m2Spec/data_refactor_plan.md §四 + §1.3 退場時序:

  1. holding_shares_per  → holding_shares_per_legacy_v2
  2. financial_statement → financial_statement_legacy_v2
  3. monthly_revenue     → monthly_revenue_legacy_v2

連帶 rename financial_statement 的 2 個索引(_legacy 後綴對齊):
  - idx_financial_type_date     → idx_financial_legacy_type_date
  - idx_financial_detail_gin    → idx_financial_legacy_detail_gin

(holding_shares_per / monthly_revenue 無 explicit index;PK 自動跟著表 rename。)

PR #R5 觀察期 21~60 天後,PR #R6 才會 DROP `_legacy_v2`。

依據:
- m2Spec/data_refactor_plan.md §四(本 PR 範圍 + collector.toml 5 個 v2.0
  entry target_table 改 _legacy_v2)
- m2Spec/layered_schema_post_refactor.md §3.5 / §3.6
  (3 張表「PR #R4 後升格」標記;v2.0 表 rename 是升格 prerequisite)

idempotent 設計(對齊 baseline schema_pg.sql 同步更新):
  baseline 走 src/schema_pg.sql,upgrade 後 schema_pg.sql 已是 _legacy_v2 命名。
  本 migration 用 DO $$...IF EXISTS$$ 防衛 — 對既有 DB(舊名)走 rename,對
  fresh DB(新名)走 no-op。

風險:🟡 中
- collector.toml 5 個 v2.0 entries(`holding_shares_per` / `financial_income` /
  `financial_balance` / `financial_cashflow` / `monthly_revenue`)的 target_table
  必須同步改 `_legacy_v2`,否則 dual-write 會 INSERT 進不存在的舊名 → upsert 炸
- Silver builders 不讀 v2.0 legacy(讀 _tw),不影響 Silver pipeline

Rollback:downgrade rename 反向。
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


revision: str = "s8t9u0v1w2x3"
down_revision: Union[str, Sequence[str], None] = "r7s8t9u0v1w2"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


# 表名 + 索引 rename 對映
TABLE_RENAMES = (
    ("holding_shares_per",  "holding_shares_per_legacy_v2"),
    ("financial_statement", "financial_statement_legacy_v2"),
    ("monthly_revenue",     "monthly_revenue_legacy_v2"),
)

INDEX_RENAMES = (
    # financial_statement 的 2 個 explicit 索引
    ("idx_financial_type_date",   "idx_financial_legacy_type_date"),
    ("idx_financial_detail_gin",  "idx_financial_legacy_detail_gin"),
)


def upgrade() -> None:
    """3 張 v2.0 表 rename `_legacy_v2`(idempotent IF EXISTS 防衛)。"""
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
    """rename 反向。observation 期未滿時退回原名。"""
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
