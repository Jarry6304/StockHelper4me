"""pr18_5_create_time_text

Revision ID: m2n3o4p5q6r7
Revises: l1m2n3o4p5q6
Create Date: 2026-05-04 00:00:00.000000

==============================================================================
hotfix:monthly_revenue_tw.create_time TIMESTAMPTZ → TEXT(PR #18.5 補丁)。

User 本機跑 backfill --phases 5 --stocks 1101,2317,2330 撞:
  psycopg.errors.InvalidDatetimeFormat: invalid input syntax for type
  timestamp with time zone: "" (CONTEXT: unnamed portal parameter $7 = '')

根因:FinMind 對 monthly_revenue.create_time 某些 row 回**空字串 ""**(不是
JSON null)。psycopg 把 "" 送 PG TIMESTAMPTZ 欄會直接拒(只接受 valid timestamp
或 NULL)。FieldMapper 也沒做空字串 → None 轉換。

修法:把 create_time 從 TIMESTAMPTZ 改 TEXT。對齊 Medallion 原則 Bronze
raw layer 不做 type conversion;Silver builder 階段才 cast(用 NULLIF(create_time,
'')::TIMESTAMPTZ 處理空字串)。

實作:單一 ALTER COLUMN TYPE TEXT — 因為 monthly_revenue_tw 之前因這個 bug
寫不進去任何 row(0 筆),所以沒有現存資料需要轉換。USING 子句加 `::TEXT`
(理論上 TIMESTAMPTZ → TEXT 自然降級,有資料也安全)。

User 操作:
  1. git pull
  2. alembic upgrade head                                     # 執行此 migration
  3. python src/main.py backfill --phases 5 --stocks 1101,2317,2330
     # monthly_revenue_v3 之前 failed/pending 的 segment 會重試,寫入應通

Rollback:downgrade ALTER 回 TIMESTAMPTZ + USING NULLIF(create_time, '')::TIMESTAMPTZ
(把空字串先轉 NULL 再轉型,避免 downgrade 失敗)。

依據:
- m2Spec/collector_rust_restructure_blueprint_v3_2.md §三 Medallion 原則
  「Bronze raw, no transformation」
- 對應 src/silver/builders/monthly_revenue.py PR #19c 動工時要 cast create_time
  TEXT → TIMESTAMPTZ
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


revision: str = "m2n3o4p5q6r7"
down_revision: Union[str, Sequence[str], None] = "l1m2n3o4p5q6"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """create_time TIMESTAMPTZ → TEXT(Bronze raw 不轉型)。"""
    op.execute(
        "ALTER TABLE monthly_revenue_tw "
        "ALTER COLUMN create_time TYPE TEXT USING create_time::TEXT"
    )


def downgrade() -> None:
    """create_time TEXT → TIMESTAMPTZ(空字串先轉 NULL 避免轉型失敗)。"""
    op.execute(
        "ALTER TABLE monthly_revenue_tw "
        "ALTER COLUMN create_time TYPE TIMESTAMPTZ "
        "USING NULLIF(create_time, '')::TIMESTAMPTZ"
    )
