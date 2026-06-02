"""traditional_snapshots — Traditional Core(Frost & Prechter EWP)獨立 vertical 自有表

對齊 Traditional Core v2 `references/storage-and-io.md`「自有表(DDL)」段。

設計(對齊 SPEC,**不**復用 structural_snapshots):
- 專用表,**無** core_name 欄(對比 structural_snapshots)、**無** FK 至 Neely 物 → 完全獨立
- snapshot 即 read model:compute 時 materialize forest 成 JSONB,Web API 純 passthrough 此表
- PK (stock_id, timeframe, params_hash) — latest snapshot per (stock, tf, params),recompute 覆寫
- `forest` JSONB = 整個 TraditionalCoreOutput;`diagnostics` JSONB = output.diagnostics
- 現階段不設 traditional_facts 表(fact 式陳述放 forest payload)

Revision ID: j6k7l8m9n0o1
Revises: i5j6k7l8m9n0
Create Date: 2026-06-02
"""

from alembic import op


revision = 'j6k7l8m9n0o1'
down_revision = 'i5j6k7l8m9n0'
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS traditional_snapshots (
            stock_id     TEXT        NOT NULL,
            timeframe    TEXT        NOT NULL,           -- daily / weekly / monthly
            forest       JSONB       NOT NULL,           -- TraditionalCoreOutput(materialized read model)
            diagnostics  JSONB       NOT NULL,           -- TraditionalDiagnostics
            params_hash  TEXT        NOT NULL,           -- 自有 hasher(blake3 canonical;無 school 維度)
            data_range   TSTZRANGE,
            computed_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (stock_id, timeframe, params_hash)
        )
        """
    )
    op.execute(
        """
        CREATE INDEX IF NOT EXISTS idx_traditional_snapshots_stock_tf
            ON traditional_snapshots (stock_id, timeframe)
        """
    )


def downgrade() -> None:
    # destructive — 對齊 PR #R6 / v4.17 destructive 先例
    op.execute("DROP INDEX IF EXISTS idx_traditional_snapshots_stock_tf")
    op.execute("DROP TABLE IF EXISTS traditional_snapshots")
