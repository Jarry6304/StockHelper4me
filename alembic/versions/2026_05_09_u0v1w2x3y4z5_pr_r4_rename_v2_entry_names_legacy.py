"""pr_r4_rename_v2_entry_names_legacy

Revision ID: u0v1w2x3y4z5
Revises: t9u0v1w2x3y4
Create Date: 2026-05-09 00:00:02.000000

==============================================================================
m2 重構 PR #R4(simplified per plan §6.3)— collector.toml v2.0 entry name
加 `_legacy` 後綴,api_sync_progress.api_name 同步遷移。

依 m2Spec/data_refactor_plan.md §6.3 推薦簡化選項:
- ❌ 不收 v3 entry name 回主名 — `_v3` 永久作為「重抓 spec 來源」標籤
- ✅ 只把 5 個 v2.0 entry 加 `_legacy` 後綴(target_table 已在 R2 改 _legacy_v2)
- 結果:v3 spec entry 永久叫 `_v3`(命名一致),v2.0 entry 顯式 `_legacy`

5 個 entry name rename(已落 collector.toml):
  - holding_shares_per   → holding_shares_per_legacy
  - monthly_revenue      → monthly_revenue_legacy
  - financial_income     → financial_income_legacy
  - financial_balance    → financial_balance_legacy
  - financial_cashflow   → financial_cashflow_legacy

------------------------------------------------------------------------------
api_sync_progress 遷移邏輯
------------------------------------------------------------------------------
api_sync_progress PK = (api_name, stock_id, segment_start)。entry name rename
要 UPDATE 對應 row 的 api_name 欄,讓既有 backfill 進度紀錄跟新 entry name 對齊
(否則 incremental 會以為這些 entry 從未跑過,踩重抓全市場)。

5 條 UPDATE(idempotent — 只更新舊名;新名若已存在不踩 PK 衝突,因為 R4 前
新名 row 不存在):
  UPDATE api_sync_progress SET api_name = '<new>' WHERE api_name = '<old>';

無 collision 風險:R4 前 5 個新名 (`*_legacy`) row count = 0(沒任何地方用過),
新名 row 由本 migration UPDATE 寫入。

------------------------------------------------------------------------------
配套改動(本 PR 同步落地):
------------------------------------------------------------------------------
1. config/collector.toml:5 個 v2.0 entry name 加 `_legacy` 後綴 + notes 更新
2. CLAUDE.md:v1.25 段 + alembic head 更新

(scripts/verify_pr19c2_silver.py 的 R2 follow-up legacy_table refs fix
 也在本 PR 同步收;3 處 `holding_shares_per` / `monthly_revenue` /
 `financial_statement` → `*_legacy_v2`。)

------------------------------------------------------------------------------
idempotent 設計
------------------------------------------------------------------------------
DO $$ ... IF EXISTS rows with old name ... THEN UPDATE ... END $$
- 既有 DB(舊 entry name 紀錄)→ rename 走起來
- fresh DB(無紀錄)→ no-op pass through
- 二次跑 migration → no-op(舊名已不存在,EXISTS check fail)

------------------------------------------------------------------------------
風險:🟡 中
------------------------------------------------------------------------------
- 若 user 跑 migration 時剛好有 backfill in-flight 且 phase 5 跑到一半,
  舊 entry name 的 row UPDATE 後該 backfill 會繼續完成寫入但用新 entry name;
  推薦 R4 落地時不要有 active backfill(下節 verify 流程會提)
- v3 entry 不動,主路徑寫入 100% 不受影響
- Rollback:downgrade UPDATE 反向(新名 → 舊名)
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


revision: str = "u0v1w2x3y4z5"
down_revision: Union[str, Sequence[str], None] = "t9u0v1w2x3y4"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


# v2.0 entry name rename 對映(舊 → 新)
ENTRY_RENAMES = (
    ("holding_shares_per",   "holding_shares_per_legacy"),
    ("monthly_revenue",      "monthly_revenue_legacy"),
    ("financial_income",     "financial_income_legacy"),
    ("financial_balance",    "financial_balance_legacy"),
    ("financial_cashflow",   "financial_cashflow_legacy"),
)


def upgrade() -> None:
    """5 個 v2.0 entry name 加 `_legacy` 後綴,api_sync_progress.api_name 同步遷移。"""
    for old_name, new_name in ENTRY_RENAMES:
        op.execute(f"""
        DO $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM api_sync_progress WHERE api_name = '{old_name}'
            ) THEN
                UPDATE api_sync_progress
                   SET api_name = '{new_name}'
                 WHERE api_name = '{old_name}';
            END IF;
        END $$;
        """)


def downgrade() -> None:
    """rename 反向:_legacy 後綴砍掉,回原 v2.0 entry name。"""
    for old_name, new_name in ENTRY_RENAMES:
        op.execute(f"""
        DO $$
        BEGIN
            IF EXISTS (
                SELECT 1 FROM api_sync_progress WHERE api_name = '{new_name}'
            ) THEN
                UPDATE api_sync_progress
                   SET api_name = '{old_name}'
                 WHERE api_name = '{new_name}';
            END IF;
        END $$;
        """)
