"""pr21_b_total_margin_pivot_fix

Revision ID: q6r7s8t9u0v1
Revises: p5q6r7s8t9u0
Create Date: 2026-05-08 00:00:00.000000

==============================================================================
PR #21-B hotfix(per 2026-05-08 user 全市場 spot check 揭露 bug)。

問題:
    PR #21-B 上線後 user 跑全市場 backfill,total_margin_purchase_short_sale_tw
    Bronze 拿到 1778 row 但 total_margin_purchase_balance / total_short_sale_balance
    兩欄全 NULL,Silver 衍生欄 fill rate = 0%。

Probe FinMind /api/v4/data?dataset=TaiwanStockTotalMarginPurchaseShortSale
揭露真實格式 — pivoted by row,1 個 (date) 對應 2 row:

    [
      {"date": "2025-01-02", "name": "MarginPurchase", "TodayBalance": 8738210, ...},
      {"date": "2025-01-02", "name": "ShortSale",      "TodayBalance": 6543210, ...},
    ]

不是我們原本預期的 wide row(date / TotalMarginPurchase* / TotalShortSale*)。

修法(本 migration):
    Bronze 加 `name` 進 PK + 改 raw FinMind 欄名,Silver builder 走 pivot
    (MarginPurchase 那 row 的 TodayBalance → total_margin_purchase_balance,
     ShortSale 那 row 的 TodayBalance → total_short_sale_balance)。

------------------------------------------------------------------------------
變更
------------------------------------------------------------------------------

1. DROP + 重建 total_margin_purchase_short_sale_tw 表(保留 trigger 補回):
   - PK 從 (market, date) → (market, date, name)
   - 砍 total_margin_purchase_balance / total_short_sale_balance 兩欄
     (這 2 個是 Silver 衍生欄,不該在 Bronze)
   - 加 name TEXT (MarginPurchase / ShortSale)
   - 加 today_balance / yes_balance / buy / sell / return_amount(raw FinMind 欄名,
     `Return` 是 SQL 保留字,rename 成 return_amount)

2. 重建 trigger mark_market_margin_derived_from_total_dirty(DROP TABLE 連帶
   drop trigger,需重建);函式 body 不動(只讀 NEW.market / NEW.date,
   加 name 進 PK 不影響)

3. 不改 trigger function 本身(generic 路徑)。

------------------------------------------------------------------------------
配套 user 操作流程
------------------------------------------------------------------------------

    git pull
    alembic upgrade head        # q6r7s8t9u0v1

    # collector.toml 已同步改 field_rename;reset api_sync_progress + 清舊 Bronze 重抓
    psql $DATABASE_URL -c "DELETE FROM api_sync_progress
                            WHERE api_name = 'total_margin_purchase_short_sale_v3'"

    # 全市場重抓(8 segments × all_market,~30 秒)
    python src/main.py backfill --phases 6

    # 預期 Bronze ~3556 rows(1778 dates × 2 names);Silver 衍生欄 ~100% fill
    psql $DATABASE_URL -c "SELECT COUNT(*) FROM total_margin_purchase_short_sale_tw"
    python src/main.py silver phase 7a --full-rebuild --stocks 2330  # smoke test
    psql $DATABASE_URL -c "
        SELECT date, ratio, total_margin_purchase_balance, total_short_sale_balance
        FROM market_margin_maintenance_derived ORDER BY date DESC LIMIT 5"

------------------------------------------------------------------------------
依據
------------------------------------------------------------------------------

- 2026-05-08 user smoke test + FinMind /api/v4/data probe 結果
- spec §2.6.3 market_margin_maintenance_derived 衍生欄定義不變
- m2Spec/collector_rust_restructure_blueprint_v3_2.md §六 Bronze raw layer 原則
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


revision: str = "q6r7s8t9u0v1"
down_revision: Union[str, Sequence[str], None] = "p5q6r7s8t9u0"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


# =============================================================================
# 新 schema(raw FinMind 欄名,加 name 進 PK)
# =============================================================================

DDL_NEW_TOTAL_MARGIN = """
    CREATE TABLE total_margin_purchase_short_sale_tw (
        market         TEXT NOT NULL,
        date           DATE NOT NULL,
        name           TEXT NOT NULL,                -- 'MarginPurchase' | 'ShortSale'
        today_balance  BIGINT,
        yes_balance    BIGINT,
        buy            BIGINT,
        sell           BIGINT,
        return_amount  BIGINT,                        -- 'Return' SQL 保留字 → rename
        PRIMARY KEY (market, date, name)
    )
"""

# trigger CREATE(DROP TABLE 連帶 drop trigger,需重建)
TRG_NEW = """
    CREATE TRIGGER mark_market_margin_derived_from_total_dirty
    AFTER INSERT OR UPDATE ON total_margin_purchase_short_sale_tw
    FOR EACH ROW EXECUTE FUNCTION trg_mark_market_margin_dirty()
"""


# =============================================================================
# 舊 schema(p5q6r7s8t9u0 的版本,downgrade 用)
# =============================================================================

DDL_OLD_TOTAL_MARGIN = """
    CREATE TABLE total_margin_purchase_short_sale_tw (
        market                          TEXT NOT NULL,
        date                            DATE NOT NULL,
        total_margin_purchase_balance   BIGINT,
        total_short_sale_balance        BIGINT,
        PRIMARY KEY (market, date)
    )
"""


def upgrade() -> None:
    """DROP + 重建 + trigger 補回。資料安全:舊 1778 row 都是 NULL 衍生欄,丟掉無損失。"""

    # 1. DROP 表(連帶 drop trigger)
    op.execute("DROP TABLE IF EXISTS total_margin_purchase_short_sale_tw CASCADE")

    # 2. 重建表(新 schema)
    op.execute(DDL_NEW_TOTAL_MARGIN)

    # 3. 重建 trigger(reuse existing trg_mark_market_margin_dirty function)
    op.execute(TRG_NEW)


def downgrade() -> None:
    """回到 p5q6r7s8t9u0 schema(資料還是會丟,本 migration 沒做轉換)。"""

    op.execute("DROP TABLE IF EXISTS total_margin_purchase_short_sale_tw CASCADE")
    op.execute(DDL_OLD_TOTAL_MARGIN)
    op.execute(TRG_NEW)
