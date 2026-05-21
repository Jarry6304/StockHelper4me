"""v4.17 — 永久 DROP 5 張 v2.0 orphan 表

PR #18 `_tw` 遷移收尾(CLAUDE.md v4.16):collector 從 dual-write v2.0 表改成直寫
Bronze-raw `_tw` 表。5 張 v2.0 表自此無人寫、無人讀 —— Silver builder 全部走
`_tw` 主名:
  - institutional_daily   → 主路徑 institutional_investors_tw
  - valuation_daily       → 主路徑 valuation_per_tw
  - day_trading           → 主路徑 day_trading_tw
  - margin_daily          → 主路徑 margin_purchase_short_sale_tw
  - foreign_holding       → 主路徑 foreign_investor_share_tw

audit(grep `src/`):0 處讀寫這 5 張表 —— 只有 reverse_pivot_* / verify_pr18 /
verify_pr19b / cleanup_non_trading_days 等 obsolete 遷移工具引用(DROP 後失效
屬已知,本就不再需要)。

關鍵設計(對齊 PR #R6 z5a6b7c8d9e0 destructive 先例):
  - **不可 rollback**(destructive,downgrade no-op)
  - schema_pg.sql 5 個 CREATE TABLE 同步移除(fresh-init 不再重建)
  - scripts/check_all_tables.py 表清單同步移除
  - api_sync_progress 殘留舊 v2.0 entry 進度 row 無害,本 PR 不動

落地後:
  - alembic head:`f1g2h3i4j5k6`
  - 0 張 v2.0 orphan 表

References:
  - CLAUDE.md v4.16 — PR #18 `_tw` 遷移收尾
  - CLAUDE.md v3.10 PR #R6(z5a6b7c8d9e0)— 同款 destructive DROP 先例
"""

from alembic import op


# revision identifiers, used by Alembic.
revision = "f1g2h3i4j5k6"
down_revision = "e0f1g2h3i4j5"
branch_labels = None
depends_on = None


_ORPHAN_TABLES = (
    "institutional_daily",
    "valuation_daily",
    "day_trading",
    "margin_daily",
    "foreign_holding",
)


def upgrade() -> None:
    """DROP 5 張 v2.0 orphan 表(idempotent via IF EXISTS;CASCADE 砍掉 INDEX 依賴)。"""
    for table in _ORPHAN_TABLES:
        op.execute(f"DROP TABLE IF EXISTS {table} CASCADE")


def downgrade() -> None:
    """No-op:v4.17 永久 DROP 後不可 rollback(對齊 PR #R6 先例)。

    若需恢復 v2.0 表結構,須從備份還原(user 端責任,本 migration 不負責)。
    本 downgrade 刻意 no-op,避免 alembic downgrade -1 自動建空表造成混淆。
    """
    pass
