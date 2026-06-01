"""rename magic_formula is_top_30 → is_top_n (align cross-stock ranked schema)

`magic_formula_ranked_derived`(2026-05-15 最早建)用 `is_top_30` 欄名,但之後
v3.32(d9e0f1g2h3i4)10 表 + v4.26(g3h4i5j6k7l8)wave_impulse 全部用 canonical
`is_top_n`。本 migration 把 magic_formula 對齊,令 12 個 cross-stock ranked 表
schema 統一。

⚠️ 此處的 `is_top_30` 是**實體欄位**(magic_formula_ranked_derived 儲存欄);
與 resonance「cross-stock 旁路升振」的**語意旗標** `is_top_30`(DualTrackResult /
前端契約)完全無關 — 後者一律不動。

⚠️ 部署順序:本 migration 與寫入端 dict key 改名(src/cross_cores/magic_formula.py)
+ 讀欄預設翻 is_top_n(fusion.raw._db / dual_track/resonance / mcp_server/_magic_formula)
必須同次部署。中間若只上 migration、refresh 還在寫 is_top_30 → upsert 找不到欄位炸。

Revision ID: i5j6k7l8m9n0
Revises: h4i5j6k7l8m9
Create Date: 2026-06-01
"""

from alembic import op


revision = 'i5j6k7l8m9n0'
down_revision = 'h4i5j6k7l8m9'
branch_labels = None
depends_on = None

TABLE = "magic_formula_ranked_derived"


def upgrade() -> None:
    # 欄位改名(Postgres 自動更新依賴此欄的 index predicate,但 index 名 idx_mf_top30
    # 語意過時 → 順手重建為 idx_mf_topn)
    op.execute(f"ALTER TABLE {TABLE} RENAME COLUMN is_top_30 TO is_top_n")
    op.execute("DROP INDEX IF EXISTS idx_mf_top30")
    op.execute(
        f"CREATE INDEX IF NOT EXISTS idx_mf_topn "
        f"ON {TABLE} (market, date, combined_rank) WHERE is_top_n = TRUE"
    )


def downgrade() -> None:
    op.execute("DROP INDEX IF EXISTS idx_mf_topn")
    op.execute(f"ALTER TABLE {TABLE} RENAME COLUMN is_top_n TO is_top_30")
    op.execute(
        f"CREATE INDEX IF NOT EXISTS idx_mf_top30 "
        f"ON {TABLE} (market, date, combined_rank) WHERE is_top_30 = TRUE"
    )
