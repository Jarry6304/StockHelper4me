"""pae_dedup_par_value_split

Revision ID: v1w2x3y4z5a6
Revises: u0v1w2x3y4z5
Create Date: 2026-05-09 11:00:00.000000

==============================================================================
修正 `price_adjustment_events` 同一公司行動被 par_value_change + split 兩個
event_type 重複記錄(FinMind TaiwanStockParValueChange + TaiwanStockSplitPrice
兩 dataset 同時報告同一面額變更 → vf 累乘 0.1 × 0.1 = 0.01,fwd_volume 多 ×10)。

== 揭露 ==
2026-05-09 user spot check 5278 fwd_volume 異常(64000 raw → 6,927,633 fwd,
× 108.24 而非預期的 × 10.83)。Trace:

| 事件 | event_type | vf | 實際公司行動 |
|---|---|---|---|
| 2024-12-09 | par_value_change | 0.1 | 1000→100 NTD 面額 |
| 2024-12-09 | split            | 0.1 | (同上,重複)|

dev DB query confirm 16 個 (stock, date) pair 中標(2327/3093/4763/5278/5314/
5536/6415/6531/6548×2/6613/6763/6919/8070/8476/8932)。

== 處理 ==
1. **既有 16 dup cleanup**:DELETE 同 (market, stock_id, date) 的 split row,
   保留 par_value_change(主要原因:par_value_change 命名更具體,FinMind
   `TaiwanStockParValueChange` 是 authoritative source;split 在這 case 是
   FinMind `TaiwanStockSplitPrice` 副 source)
2. **未來防衛 trigger**:`trg_pae_dedup_par_value_split` AFTER INSERT OR UPDATE,
   每當 split 或 par_value_change 寫入,DELETE 同 (market, stock_id, date,
   before_price, reference_price, vf) 的 split row(保留 par_value_change)。
3. **Mark fwd dirty**:UPDATE 4 fwd 表的 affected stocks SET is_dirty=TRUE,
   讓 user 跑 `silver phase 7c` 重算正確 fwd_volume。

== 觸發語意設計 ==
trigger 邏輯 invariant:**每當 split + par_value_change 同 (market, stock_id,
date, before_price, reference_price, vf) 同時存在 → DELETE split**。

無論寫入順序(split 先 / par_value_change 先),trigger 都正確 dedup:
- split 先 INSERT → trigger 找 par_value_change → 不存在 → no-op,split 留下
- par_value_change 後 INSERT → trigger 找 split → 找到 → DELETE split ✓
- 反序同理

UPDATE 也涵蓋,UPSERT(`db.upsert` ON CONFLICT DO UPDATE)case 也處理。

== Rollback ==
downgrade(): DROP trigger + function。**不還原 deleted split rows**(lossy);
若需要還原,從 backup 或重跑 collector。
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


revision: str = "v1w2x3y4z5a6"
down_revision: Union[str, Sequence[str], None] = "u0v1w2x3y4z5"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """Cleanup existing duplicates + create dedup trigger + mark fwd dirty。"""

    # ─── Step 1: 清理既有 duplicate split rows ─────────────────────────
    op.execute("""
    DO $$
    DECLARE
        deleted_count INT;
    BEGIN
        WITH cleaned AS (
            DELETE FROM price_adjustment_events s
            WHERE s.event_type = 'split'
              AND EXISTS (
                SELECT 1 FROM price_adjustment_events p
                WHERE p.event_type = 'par_value_change'
                  AND p.market = s.market
                  AND p.stock_id = s.stock_id
                  AND p.date = s.date
                  AND COALESCE(p.before_price, 0) = COALESCE(s.before_price, 0)
                  AND COALESCE(p.reference_price, 0) = COALESCE(s.reference_price, 0)
                  AND COALESCE(p.volume_factor, 1) = COALESCE(s.volume_factor, 1)
              )
            RETURNING 1
        )
        SELECT COUNT(*) INTO deleted_count FROM cleaned;
        RAISE NOTICE 'Cleaned % duplicate split rows', deleted_count;
    END $$;
    """)

    # ─── Step 2: Mark fwd 4 表 is_dirty=TRUE for affected stocks ─────
    # 觸發 7c 重算 fwd_volume(原本被 ×10 的 stock 會修正)
    op.execute("""
    UPDATE price_daily_fwd
       SET is_dirty = TRUE, dirty_at = NOW()
     WHERE (market, stock_id) IN (
        SELECT DISTINCT market, stock_id
          FROM price_adjustment_events
         WHERE event_type = 'par_value_change'
     );
    """)
    op.execute("""
    UPDATE price_weekly_fwd
       SET is_dirty = TRUE, dirty_at = NOW()
     WHERE (market, stock_id) IN (
        SELECT DISTINCT market, stock_id
          FROM price_adjustment_events
         WHERE event_type = 'par_value_change'
     );
    """)
    op.execute("""
    UPDATE price_monthly_fwd
       SET is_dirty = TRUE, dirty_at = NOW()
     WHERE (market, stock_id) IN (
        SELECT DISTINCT market, stock_id
          FROM price_adjustment_events
         WHERE event_type = 'par_value_change'
     );
    """)
    op.execute("""
    UPDATE price_limit_merge_events
       SET is_dirty = TRUE, dirty_at = NOW()
     WHERE (market, stock_id) IN (
        SELECT DISTINCT market, stock_id
          FROM price_adjustment_events
         WHERE event_type = 'par_value_change'
     );
    """)

    # ─── Step 3: CREATE trigger function 防衛未來 dup ─────────────────
    op.execute("""
    CREATE OR REPLACE FUNCTION trg_pae_dedup_par_value_split()
    RETURNS TRIGGER AS $$
    BEGIN
        -- invariant:每當 split + par_value_change 同 (key, before/ref/vf)
        -- 同時存在 → DELETE split row(保留 par_value_change 為 primary)
        DELETE FROM price_adjustment_events s
        WHERE s.event_type = 'split'
          AND s.market = NEW.market
          AND s.stock_id = NEW.stock_id
          AND s.date = NEW.date
          AND EXISTS (
            SELECT 1 FROM price_adjustment_events p
            WHERE p.event_type = 'par_value_change'
              AND p.market = s.market
              AND p.stock_id = s.stock_id
              AND p.date = s.date
              AND COALESCE(p.before_price, 0) = COALESCE(s.before_price, 0)
              AND COALESCE(p.reference_price, 0) = COALESCE(s.reference_price, 0)
              AND COALESCE(p.volume_factor, 1) = COALESCE(s.volume_factor, 1)
          );
        RETURN NULL;  -- AFTER trigger return value 被忽略
    END;
    $$ LANGUAGE plpgsql;
    """)

    # ─── Step 4: CREATE trigger ON price_adjustment_events ───────────
    op.execute("""
    CREATE TRIGGER trg_pae_dedup_par_value_split
        AFTER INSERT OR UPDATE ON price_adjustment_events
        FOR EACH ROW
        WHEN (NEW.event_type IN ('split', 'par_value_change'))
        EXECUTE FUNCTION trg_pae_dedup_par_value_split();
    """)


def downgrade() -> None:
    """DROP trigger + function。**不還原 deleted split rows**(lossy)。"""
    op.execute("DROP TRIGGER IF EXISTS trg_pae_dedup_par_value_split ON price_adjustment_events;")
    op.execute("DROP FUNCTION IF EXISTS trg_pae_dedup_par_value_split();")
