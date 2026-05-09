"""m3_cores_three_tables

Revision ID: w2x3y4z5a6b7
Revises: v1w2x3y4z5a6
Create Date: 2026-05-09 13:00:00.000000

==============================================================================
M3 Cores 層三表落地(對齊 m2Spec/oldm2Spec/cores_overview.md §七 寫入分流):

| 用途 | 表 | 寫入頻率 |
|---|---|---|
| 時間序列值(MACD/RSI/KD 等每日數值) | `indicator_values` JSONB | 每日 batch |
| 結構性快照(SR / Trendline / Wave Forest)| `structural_snapshots` | 每日 batch,全量重算 |
| 事件式 Fact(golden_cross / breakout / divergence)| `facts` (append-only) | 每日 batch,僅新增當日新事件 |

== 設計 ==

1. `indicator_values`:存 IndicatorCore 每日輸出。PK 含 params_hash 區分同 Core 多個參數組;
   value JSONB 存 Core-specific 序列化後 Output。

2. `structural_snapshots`:存 WaveCore 全量重算的 Forest 快照 + 結構性 Indicator
   (SR / Trendline / candlestick_pattern)。snapshot JSONB 存完整 Scenario Forest(對齊 §17.1)。

3. `facts`:append-only 事件 Fact。Unique constraint 對齊 §6.3:
   `(stock_id, fact_date, timeframe, source_core, COALESCE(params_hash, ''), md5(statement))`
   + INSERT ON CONFLICT DO NOTHING。

== 索引 ==

- 各表 (stock_id, fact_date / snapshot_date / value_date) DESC 索引給「最新 N 筆」查詢
- facts 加 GIN 索引在 metadata 給 metadata 條件查詢

== 不做的事 ==

- 不加 partition(留 P1 後 row count 暴增時再切)
- 不加 retention policy(留 P0 Gate 後 user 決定)

== Rollback ==
downgrade DROP 三表(無生產資料 — M3 PR-7 落地時表為空)。

== 對齊 ==
- m2Spec/oldm2Spec/cores_overview.md §6.2(Fact schema)
- m2Spec/oldm2Spec/cores_overview.md §6.3(Facts unique constraint)
- m2Spec/oldm2Spec/cores_overview.md §7.1(三類資料寫入分流)
- m2Spec/oldm2Spec/cores_overview.md §7.4(params_hash blake3 + canonical JSON 前 16 hex)
- m2Spec/oldm2Spec/neely_core.md §17(對應資料表)
- m2Spec/oldm2Spec/neely_core.md §17.1(structural_snapshots JSONB 範例)
==============================================================================
"""
from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision = "w2x3y4z5a6b7"
down_revision = "v1w2x3y4z5a6"
branch_labels = None
depends_on = None


def upgrade() -> None:
    # ------------------------------------------------------------
    # indicator_values:時間序列值(MACD/RSI/KD/...)
    # ------------------------------------------------------------
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS indicator_values (
            stock_id      TEXT NOT NULL,
            value_date    DATE NOT NULL,
            timeframe     TEXT NOT NULL,
            source_core   TEXT NOT NULL,
            source_version TEXT NOT NULL,
            params_hash   TEXT NOT NULL DEFAULT '',
            value         JSONB NOT NULL,
            created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (stock_id, value_date, timeframe, source_core, params_hash)
        )
        """
    )
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_indicator_values_stock_date_desc "
        "ON indicator_values(stock_id, value_date DESC)"
    )
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_indicator_values_core "
        "ON indicator_values(source_core, value_date DESC)"
    )

    # ------------------------------------------------------------
    # structural_snapshots:結構性快照(Wave Forest / SR / Trendline)
    # ------------------------------------------------------------
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS structural_snapshots (
            stock_id        TEXT NOT NULL,
            snapshot_date   DATE NOT NULL,
            timeframe       TEXT NOT NULL,
            core_name       TEXT NOT NULL,
            source_version  TEXT NOT NULL,
            params_hash     TEXT NOT NULL DEFAULT '',
            snapshot        JSONB NOT NULL,
            derived_from_core TEXT,
            created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            PRIMARY KEY (stock_id, snapshot_date, timeframe, core_name, params_hash)
        )
        """
    )
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_structural_snapshots_stock_date_desc "
        "ON structural_snapshots(stock_id, snapshot_date DESC)"
    )
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_structural_snapshots_core "
        "ON structural_snapshots(core_name, snapshot_date DESC)"
    )

    # ------------------------------------------------------------
    # facts:append-only 事件 Fact(對齊 cores_overview §6.3)
    # ------------------------------------------------------------
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS facts (
            id            BIGSERIAL PRIMARY KEY,
            stock_id      TEXT NOT NULL,
            fact_date     DATE NOT NULL,
            timeframe     TEXT NOT NULL,
            source_core   TEXT NOT NULL,
            source_version TEXT NOT NULL,
            params_hash   TEXT,
            statement     TEXT NOT NULL,
            statement_md5 TEXT GENERATED ALWAYS AS (md5(statement)) STORED,
            metadata      JSONB NOT NULL DEFAULT '{}'::jsonb,
            created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        """
    )
    # Unique constraint 對齊 cores_overview §6.3:
    # (stock_id, fact_date, timeframe, source_core, COALESCE(params_hash, ''), md5(statement))
    op.execute(
        """
        CREATE UNIQUE INDEX IF NOT EXISTS uq_facts_dedup
        ON facts(stock_id, fact_date, timeframe, source_core,
                 COALESCE(params_hash, ''), statement_md5)
        """
    )
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_facts_stock_date_desc "
        "ON facts(stock_id, fact_date DESC)"
    )
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_facts_core "
        "ON facts(source_core, fact_date DESC)"
    )
    op.execute(
        "CREATE INDEX IF NOT EXISTS idx_facts_metadata_gin "
        "ON facts USING GIN (metadata jsonb_path_ops)"
    )


def downgrade() -> None:
    op.execute("DROP TABLE IF EXISTS facts CASCADE")
    op.execute("DROP TABLE IF EXISTS structural_snapshots CASCADE")
    op.execute("DROP TABLE IF EXISTS indicator_values CASCADE")
