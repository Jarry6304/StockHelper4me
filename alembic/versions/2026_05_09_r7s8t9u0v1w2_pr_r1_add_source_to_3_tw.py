"""pr_r1_add_source_to_3_tw

Revision ID: r7s8t9u0v1w2
Revises: q6r7s8t9u0v1
Create Date: 2026-05-09 00:00:00.000000

==============================================================================
m2 重構 PR #R1 — 補回 source 欄位至 3 張 PR #18.5 Bronze 表。

依 m2Spec/data_refactor_plan.md §三 + m2Spec/layered_schema_post_refactor.md
§3.5 / §3.6:

  1. holding_shares_per_tw — spec §3.5 明文「PR #R1 補回」
  2. financial_statement_tw — spec §3.6 表格漏寫,依 Bronze 全表 source 一致原則補上
  3. monthly_revenue_tw     — spec §3.6 表格漏寫,同上

3 張都 ALTER ADD COLUMN `source TEXT NOT NULL DEFAULT 'finmind'`。default 對既有
資料自動填 'finmind',db.upsert 對 collector.toml 沒指定 source 的 entry 不影響
(schema default 接管)。

對齊既有 19 張 Bronze 慣例:多數表都有 `source TEXT NOT NULL DEFAULT 'finmind'`,
唯一例外是 trading_date_ref(設計上 row 存在 = 交易日,沒 multi-source 可能性
所以無此欄)。

依據:
- m2Spec/layered_schema_post_refactor.md §3.5 line 430(holding_shares_per 表
  source 欄附「PR #R1 補回」標記)
- m2Spec/data_refactor_plan.md §三(本 PR 範圍說明)

風險:🟢 低
- ALTER ADD COLUMN with DEFAULT 是 PG 11+ instant operation(不掃 row,不阻塞)
- collector.toml 既有 5 個 v3 entries(holding_shares_per_v3 / financial_*_v3 /
  monthly_revenue_v3)沒指定 source 欄,db.upsert 自動帶入 schema default
- 既有資料 SELECT count 不變

Rollback:downgrade DROP COLUMN source(各表)。
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


revision: str = "r7s8t9u0v1w2"
down_revision: Union[str, Sequence[str], None] = "q6r7s8t9u0v1"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


TABLES = (
    "holding_shares_per_tw",
    "financial_statement_tw",
    "monthly_revenue_tw",
)


def upgrade() -> None:
    """3 張 Bronze 加 source TEXT NOT NULL DEFAULT 'finmind'。"""
    for table in TABLES:
        op.execute(
            f"ALTER TABLE {table} "
            f"ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'finmind'"
        )


def downgrade() -> None:
    """DROP COLUMN source 反向。既有資料無感(default 值移除等於該欄消失)。"""
    for table in TABLES:
        op.execute(f"ALTER TABLE {table} DROP COLUMN IF EXISTS source")
