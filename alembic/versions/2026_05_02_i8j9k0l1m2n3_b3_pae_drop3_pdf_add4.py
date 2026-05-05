"""b3_pae_drop3_pdf_add4

Revision ID: i8j9k0l1m2n3
Revises: h7i8j9k0l1m2
Create Date: 2026-05-02 00:00:00.000000

==============================================================================
m2 collector 重構 PR #17(per blueprint v3.2 r1 §六 #9 B-3 + §5.2 同步 ALTER)。

兩件事一起做(不分兩 migration,因為 Rust 端的對應 code 改動是原子性的):

A. price_adjustment_events 砍 3 欄(blueprint §5.2 + spec v3.2 §3.2):
   - adjustment_factor: 違反 Medallion 原則,改在 Silver 計算(Rust 內現算)
   - after_price:       Beta 不抓(Rust 用 before_price + reference_price 反推)
   - source:            用 event_type 推導,不需獨立欄
   留:market, stock_id, date, event_type, before_price, reference_price,
       cash_dividend, stock_dividend, volume_factor(P0-11 後保留), detail JSONB

B. price_daily_fwd 加 4 欄(blueprint §5.2 amend + §4.4 r3.1):
   - cumulative_adjustment_factor: Wave Cores 反推 raw price 用
   - cumulative_volume_factor:     P0-11 修完後反推 raw volume(對 split 必要)
   - is_adjusted:                  flag 給 Aggregation Layer 判斷該日是否動過
   - adjustment_factor:            單日 AF,除錯用
   ❌ 不加 volume_adjusted(av3 揭露假設不成立)

依據:
- m2Spec/collector_rust_restructure_blueprint_v3_2.md §5.2(本檔 + amend)
- m2Spec/collector_schema_consolidated_spec_v3_2.md §3.2
- m2Spec/unified_alignment_review_r2.md r3.1 段(av3 + P0-11)

對應 code 改動(同 commit):
- rust_compute/src/main.rs:
  · AdjEvent 砍 adjustment_factor 欄,改加 before_price + reference_price
  · load_adj_events SELECT 改用 before_price + reference_price + volume_factor
  · compute_forward_adjusted 內現算 AF(before/reference 反推)
  · patch_capital_increase_af 改成記憶體版(不再 UPDATE events 表)
  · FwdDailyPrice 加 4 欄,upsert_daily_fwd INSERT 補 4 欄
- src/field_mapper.py: 砍 adjustment_factor 計算(events 沒這欄)
- src/post_process.py: _detect_capital_increase insert 砍 after_price /
  adjustment_factor / source key
- config/collector.toml: 4 個 events 來源 field_rename 砍 after_price,
  computed_fields 砍 adjustment_factor

User 操作流程:
  1. git pull
  2. alembic upgrade head
  3. cd rust_compute && cargo build --release && cd ..
  4. python src/main.py backfill --phases 4 --stocks 2330  # smoke test
  5. python src/main.py backfill --phases 4               # 全市場重跑
  6. psql $env:DATABASE_URL -f scripts/av3_spot_check.sql # 驗證新 4 欄

Rollback 風險:
  本 migration downgrade 重建 3 個被砍欄但不會 backfill 內容(adjustment_factor
  / after_price 都是 derived,可由 Rust 重算;source 全 'finmind' default 即可)。
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


revision: str = "i8j9k0l1m2n3"
down_revision: Union[str, Sequence[str], None] = "h7i8j9k0l1m2"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """events 砍 3 欄 + fwd 加 4 欄。"""
    # A. price_adjustment_events 砍 3 欄
    op.execute("ALTER TABLE price_adjustment_events DROP COLUMN IF EXISTS adjustment_factor")
    op.execute("ALTER TABLE price_adjustment_events DROP COLUMN IF EXISTS after_price")
    op.execute("ALTER TABLE price_adjustment_events DROP COLUMN IF EXISTS source")

    # B. price_daily_fwd 加 4 欄
    op.execute(
        """
        ALTER TABLE price_daily_fwd
            ADD COLUMN IF NOT EXISTS cumulative_adjustment_factor NUMERIC(20, 10),
            ADD COLUMN IF NOT EXISTS cumulative_volume_factor     NUMERIC(20, 10),
            ADD COLUMN IF NOT EXISTS is_adjusted                  BOOLEAN NOT NULL DEFAULT FALSE,
            ADD COLUMN IF NOT EXISTS adjustment_factor            NUMERIC(20, 10)
        """
    )


def downgrade() -> None:
    """重建 events 3 欄 + 砍 fwd 4 欄。

    注意:adjustment_factor / after_price 內容不 backfill(derived 欄,可由
    Rust 重算)。source 全填 'finmind' default。
    """
    # B 反向
    op.execute(
        """
        ALTER TABLE price_daily_fwd
            DROP COLUMN IF EXISTS cumulative_adjustment_factor,
            DROP COLUMN IF EXISTS cumulative_volume_factor,
            DROP COLUMN IF EXISTS is_adjusted,
            DROP COLUMN IF EXISTS adjustment_factor
        """
    )

    # A 反向
    op.execute(
        """
        ALTER TABLE price_adjustment_events
            ADD COLUMN IF NOT EXISTS adjustment_factor NUMERIC(20, 10) NOT NULL DEFAULT 1.0,
            ADD COLUMN IF NOT EXISTS after_price       NUMERIC(15, 4),
            ADD COLUMN IF NOT EXISTS source            TEXT NOT NULL DEFAULT 'finmind'
        """
    )
