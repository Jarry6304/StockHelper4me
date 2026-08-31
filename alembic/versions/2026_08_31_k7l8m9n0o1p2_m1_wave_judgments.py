"""M1:wave_judgments — 判讀 PIT 表(m3Spec/wave_judgment_loop.md §5)。

設計:
- append-only PIT:狀態變更一律 INSERT 新列 + supersedes_id 指回(與 facts 同紀律);
  **repo 首個 RAISE EXCEPTION trigger** — UPDATE / DELETE 一律拒絕,訊息與
  SQLSTATE(P0001)為契約,測試與 runbook probe 依此比對。
- accepted 限定 dossier 候選集(應用層驗證;DB 只管形狀與 PIT)。
- 「active judgment」語意 = status='active' 且無子列(supersedes 鏈最新);
  無 partial index — 查詢走 (stock_id, timeframe, status) 複合索引 + NOT EXISTS。
- 本 migration 不動 forecast_log(source_core='judgment' 白名單另案 l8m9n0o1p2q3)。

同步:src/schema_pg.sql 已 append 同 DDL(fresh-init path)。
"""

from alembic import op

revision = 'k7l8m9n0o1p2'
down_revision = 'j6k7l8m9n0o1'
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.execute("""
    CREATE TABLE IF NOT EXISTS wave_judgments (
        id              BIGSERIAL PRIMARY KEY,
        stock_id        TEXT NOT NULL,
        timeframe       TEXT NOT NULL CHECK (timeframe IN ('daily','weekly','monthly')),
        as_of           DATE NOT NULL,
        judged_by       TEXT NOT NULL,
        snapshot_date   DATE NOT NULL,
        params_hash     TEXT NOT NULL,
        engine_version  TEXT NOT NULL,
        assumption_hash TEXT NOT NULL,
        accepted        JSONB NOT NULL,
        degree_read     TEXT,
        rationale       JSONB NOT NULL,
        invalidation    JSONB NOT NULL,
        confidence_class TEXT NOT NULL CHECK (confidence_class IN ('single','contested','no_fit')),
        status          TEXT NOT NULL DEFAULT 'active'
                        CHECK (status IN ('active','intact','invalidated','absorbed','vanished','superseded')),
        supersedes_id   BIGINT REFERENCES wave_judgments(id),
        diff_detail     JSONB,
        created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
    )
    """)
    op.execute("""
    CREATE INDEX IF NOT EXISTS idx_wave_judgments_stock_tf_status
        ON wave_judgments(stock_id, timeframe, status)
    """)
    op.execute("""
    CREATE INDEX IF NOT EXISTS idx_wave_judgments_supersedes
        ON wave_judgments(supersedes_id)
    """)
    op.execute("""
    CREATE OR REPLACE FUNCTION wave_judgments_append_only() RETURNS TRIGGER AS $$
    BEGIN
        RAISE EXCEPTION
            'wave_judgments is append-only (PIT): UPDATE/DELETE forbidden — write a superseding row'
            USING ERRCODE = 'P0001';
    END;
    $$ LANGUAGE plpgsql
    """)
    op.execute("""
    CREATE TRIGGER trg_wave_judgments_append_only
        BEFORE UPDATE OR DELETE ON wave_judgments
        FOR EACH ROW
        EXECUTE FUNCTION wave_judgments_append_only()
    """)


def downgrade() -> None:
    # destructive — 對齊 PR #R6 / v4.17 destructive 先例
    op.execute("DROP TRIGGER IF EXISTS trg_wave_judgments_append_only ON wave_judgments")
    op.execute("DROP FUNCTION IF EXISTS wave_judgments_append_only()")
    op.execute("DROP INDEX IF EXISTS idx_wave_judgments_supersedes")
    op.execute("DROP INDEX IF EXISTS idx_wave_judgments_stock_tf_status")
    op.execute("DROP TABLE IF EXISTS wave_judgments")
