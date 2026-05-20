"""add_facts_severity

Revision ID: e0f1g2h3i4j5
Revises: d9e0f1g2h3i4
Create Date: 2026-05-20 13:00:00.000000

==============================================================================
Fusion Layer P0.2 — facts.severity 欄位。

對齊 m3Spec/fusion_layer.md §7.1。每筆 Fact 帶嚴重度,給 Fusion Integration 端口
的 market_events 工具做 severity filter。嚴重度由各 Core produce_facts() 寫入時
決定(對齊 fusion_layer §9 #6:severity 由 cores 決定,Fusion 只 filter 不二次判斷)。

severity SMALLINT:1=info / 2=notable / 3=warning / 4=critical
- 預設 1(info)。既有 facts 全部落 info(write_facts 走 ON CONFLICT DO NOTHING,
  歷史 facts 不重寫;新日期 facts 由各 Core 寫入正確值)。
- severity 不在 uq_facts_dedup 內 → Fact identity 不變,dedup 行為不受影響。

idx_facts_severity_date 給 market_events 的 (severity, fact_date) range scan。

== Rollback ==
downgrade DROP index + column。
==============================================================================
"""
from alembic import op


# revision identifiers, used by Alembic.
revision = "e0f1g2h3i4j5"
down_revision = "d9e0f1g2h3i4"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.execute(
        "ALTER TABLE facts "
        "ADD COLUMN IF NOT EXISTS severity SMALLINT NOT NULL DEFAULT 1"
    )
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_facts_severity_date "
        "ON facts(severity, fact_date DESC)"
    )


def downgrade() -> None:
    op.execute("DROP INDEX IF EXISTS idx_facts_severity_date")
    op.execute("ALTER TABLE facts DROP COLUMN IF EXISTS severity")
