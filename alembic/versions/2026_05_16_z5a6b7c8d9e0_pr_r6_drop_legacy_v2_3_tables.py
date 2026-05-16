"""PR #R6 — 永久 DROP 3 張 v2.0 legacy_v2 表

m2 大重構收尾(R5 觀察期 21~60 天提前結束,user 拍版「直接 DROP」):
  - holding_shares_per_legacy_v2
  - financial_statement_legacy_v2
  - monthly_revenue_legacy_v2

R5 觀察期紀錄(2026-05-09 啟動):本機 user 已驗 Silver builder 12/12 OK +
api_sync_progress.status='failed' = 0 + 3 張 _legacy_v2 row count 與主名表 ±1%
(對齊 v1.10 plan §7.2)。v3.7+v3.8+v3.9 連續 4 個 sprint 累積無觀察到 regression。

關鍵設計:
  - **不可 rollback**(spec plan §六 明文 PR #R6 destructive,downgrade no-op)
  - 對應 collector.toml 5 個 `_legacy` entries 在本 PR 同步移除(不再 dual-write)
  - 對應 schema_pg.sql 3 個 CREATE TABLE + 2 個 INDEX DDL 同步移除
  - 既有 silver builders 不讀 `_legacy_v2`(全部走 `*_tw` 主名 + Bronze),
    Silver pipeline 不受影響
  - api_sync_progress 殘留 5 個 `*_legacy` api_name 的 row(歷史 backfill 進度)
    本 PR **保留**:無害 + 可作為 R5/R6 過程的審計軌跡;若 user 嫌雜,後續單獨 DELETE

落地後狀態(R6 完成):
  - alembic head:`z5a6b7c8d9e0`
  - 0 張 v2.0 `_legacy_v2` 表(主路徑全走 v3 `*_tw` 或主名)
  - collector.toml 27 entries(從 32 - 5 legacy)
  - v3.2 r1 PR sequencing 全部終結 ✅

References:
  - CLAUDE.md v1.23 PR #R2 rename(s8t9u0v1w2x3)
  - CLAUDE.md v1.25 PR #R4 entry name rename(u0v1w2x3y4z5)
  - plan §7.2 觀察期 SLO + 風險「downgrade DROP 後不可回復」明文
"""

from alembic import op


# revision identifiers, used by Alembic.
revision = "z5a6b7c8d9e0"
down_revision = "y4z5a6b7c8d9"
branch_labels = None
depends_on = None


def upgrade() -> None:
    """DROP 3 張 _legacy_v2 表(idempotent via IF EXISTS;CASCADE 自動砍掉所有 INDEX 與 FK 依賴)。"""
    op.execute("DROP TABLE IF EXISTS holding_shares_per_legacy_v2 CASCADE")
    op.execute("DROP TABLE IF EXISTS financial_statement_legacy_v2 CASCADE")
    op.execute("DROP TABLE IF EXISTS monthly_revenue_legacy_v2 CASCADE")


def downgrade() -> None:
    """No-op:R6 永久 DROP 後不可 rollback(對齊 spec plan §六)。

    若需要恢復 v2.0 legacy 表結構,須:
      1. 從備份還原(user 端責任,本 migration 不負責)
      2. 重新跑 alembic upgrade s8t9u0v1w2x3(PR #R2 rename _legacy_v2)的 DDL
      3. 全市場重新 backfill `*_legacy` collector.toml entries(需復原)

    本 downgrade no-op,避免誤觸 alembic downgrade -1 自動建空表造成混淆。
    """
    pass
