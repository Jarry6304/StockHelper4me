"""schema_metadata_bump_v3_2

Revision ID: c2d3e4f5g6h7
Revises: a1b2c3d4e5f6
Create Date: 2026-05-02 00:00:00.000000

==============================================================================
schema_metadata.schema_version 從 '2.0' bump 到 '3.2',m2 collector 重構動工
入口(blueprint v3.2 §六 PR 切法 #1 對應)。

背景：
    blueprint v3.2 r1(`m2Spec/collector_rust_restructure_blueprint_v3_2.md`)
    描述 Bronze / Reference / Silver / M3 四層 Medallion 重構,涉及:
      - Bronze 4 張新增 raw 表(market_ohlcv_tw / stock_suspension_events /
        securities_lending_tw / business_indicator_tw)
      - Reference 2 張改名精簡(trading_calendar → trading_date_ref;
        stock_info → stock_info_ref)
      - Silver 14 張 derived(全 Bronze raw 計算後寫 *_derived)
      - M3 三表 + 3 view + dirty queue

    本 migration 是這一系列改動的「入口」,把 schema_metadata 標記為 v3.2,
    後續 PR(R-1 / R-2 / B-1 ~ B-6 / Silver builders)依此版本繼續加 migration。

協同改動(同 PR):
    - src/schema_pg.sql:35 baseline INSERT 改成 '3.2'(fresh-install fallback)
    - rust_compute/src/main.rs:13 EXPECTED_SCHEMA_VERSION 改成 "3.2"
    - rust_compute/src/main.rs:158 doc comment 同步

User 操作流程:
    1. git pull
    2. cd rust_compute && cargo build --release(產生 v3.2 expected binary)
    3. alembic upgrade head(DB schema_metadata 從 2.0 → 3.2)
    4. 後續任何 Rust binary 啟動會 assert v3.2,符合預期

⚠️ Rollback 風險:
    若 user 已用 v3.2 schema 跑過 m2 後續 PR(例如 R-1 改名 trading_calendar
    → trading_date_ref),downgrade 本 migration 不會自動 rename 回去。
    本 migration 只負責改 metadata 字串,實際 schema 物件 rollback 由各
    後續 migration 自己負責。
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


# revision identifiers, used by Alembic.
revision: str = "c2d3e4f5g6h7"
down_revision: Union[str, Sequence[str], None] = "a1b2c3d4e5f6"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """schema_version 2.0 → 3.2,標記 m2 重構動工開始。"""
    op.execute(
        "UPDATE schema_metadata SET value = '3.2', updated_at = NOW() "
        "WHERE key = 'schema_version'"
    )
    # Defensive:若 baseline 沒寫進去(理論上不會,但 fresh DB 走 schema_pg.sql
    # 不經 alembic baseline 的話可能漏),補一筆 INSERT
    op.execute(
        "INSERT INTO schema_metadata (key, value) VALUES ('schema_version', '3.2') "
        "ON CONFLICT (key) DO NOTHING"
    )


def downgrade() -> None:
    """退回 v2.0 schema_metadata。

    ⚠️ 警告:若已套用任何 v3.2 後續 migration(R-1 / R-2 / B-1~B-6 / Silver
    builders),downgrade 本 migration 後 metadata 顯示 2.0 但實際 schema 已含
    v3.2 物件 → schema 跟 metadata 不一致。生產環境 downgrade 前須先把所有
    v3.2 後續 migration 也 downgrade 掉。
    """
    op.execute(
        "UPDATE schema_metadata SET value = '2.0', updated_at = NOW() "
        "WHERE key = 'schema_version'"
    )
