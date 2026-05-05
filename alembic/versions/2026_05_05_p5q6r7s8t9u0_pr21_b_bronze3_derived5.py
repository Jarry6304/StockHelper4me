"""pr21_b_bronze3_derived5

Revision ID: p5q6r7s8t9u0
Revises: o4p5q6r7s8t9
Create Date: 2026-05-05 00:00:00.000000

==============================================================================
m2 collector 重構 PR #21-B(per blueprint v3.2 r1 §六 + spec §2.6.1/2/3)。

PR #21-A 補了 2 個 builder-only 衍生欄(market_value_weight / day_trading_ratio)。
PR #21-B 補剩下 3 條需新 Bronze 表的衍生欄(共 5 欄):

  1. institutional.gov_bank_net                      ← government_bank_buy_sell_tw
  2. market_margin.total_margin_purchase_balance    ← total_margin_purchase_short_sale_tw
  3. market_margin.total_short_sale_balance         ← total_margin_purchase_short_sale_tw
  4. margin.sbl_short_sales_short_sales              ← short_sale_securities_lending_tw
  5. margin.sbl_short_sales_returns                  ← short_sale_securities_lending_tw
  6. margin.sbl_short_sales_current_day_balance      ← short_sale_securities_lending_tw

------------------------------------------------------------------------------
3 張新 Bronze raw 表
------------------------------------------------------------------------------

(a) government_bank_buy_sell_tw — 八大行庫買賣超(per stock per day)
    PK (market, stock_id, date)
    FinMind dataset:TaiwanStockGovernmentBankBuySell
    假設 FinMind 回 aggregate(8 家行庫合計);若實際回 per-bank 多 row,
    user 首次 backfill 會踩 PK 衝突,屆時補 bank_name 進 PK 改 schema。

(b) total_margin_purchase_short_sale_tw — 整體市場融資融券(market-level)
    PK (market, date)— 無 stock_id
    FinMind dataset:TaiwanStockTotalMarginPurchaseShortSale
    feed Silver market_margin_maintenance_derived 的 2 個衍生欄。

(c) short_sale_securities_lending_tw — 借券賣出明細(daily aggregate per stock)
    PK (market, stock_id, date)
    FinMind dataset:TaiwanStockShortSaleSecuritiesLending(候選名)
    與既有 securities_lending_tw(借券成交明細,trade-level)是 different
    datasets:後者是「借入交易」per-trade,前者是「借券賣出」daily aggregate
    含 short_sales / returns / current_day_balance。
    若 candidate dataset 名 404,user 首次 backfill 改名(同 PR #18.5 流程)。

------------------------------------------------------------------------------
3 個 Bronze→Silver dirty trigger(per PR #20 §5.5 contract)
------------------------------------------------------------------------------

(1) mark_institutional_derived_from_gov_bank_dirty
    government_bank_buy_sell_tw → institutional_daily_derived
    走 generic trg_mark_silver_dirty('institutional_daily_derived')
    (PK shape 對齊:Bronze (market, stock_id, date) → Silver (market, stock_id, date))

(2) mark_market_margin_derived_from_total_dirty
    total_margin_purchase_short_sale_tw → market_margin_maintenance_derived
    重用既有 trg_mark_market_margin_dirty()(2-col PK 函式 body 一致,
    僅讀 NEW.market / NEW.date,可服務多個 source Bronze)

(3) mark_margin_derived_from_short_sale_dirty
    short_sale_securities_lending_tw → margin_daily_derived
    走 generic trg_mark_silver_dirty('margin_daily_derived')
    (注意:既有 mark_margin_derived_from_sbl_dirty 是從 securities_lending_tw
    來的,與本 trigger 來源不同,trigger 名稱也不同,不衝突)

------------------------------------------------------------------------------
規模 + user 操作流程
------------------------------------------------------------------------------

⚠️ 首次 backfill ~30-40h calendar-time:
   3 個新 entries × 1700+ stocks(總業 + 市場層 1 entry)× 21 年 segments
   @ 1600 reqs/h ≈ 30-40h(對齊 v1.13 PR #18.5 流程)。

   - gov_bank_buy_sell_v3:per_stock,1700+ stocks × 21 年 segments
   - short_sale_securities_lending_v3:per_stock,1700+ stocks × 21 年 segments
   - total_margin_purchase_short_sale_v3:all_market,1 × 21 年 segments(快)

User 操作流程(沙箱無 FinMind 連線,以下都本機跑):

  1. git pull
  2. alembic upgrade head                                   # p5q6r7s8t9u0
  3. python src/main.py validate                            # 確認 collector.toml 通過
  4. # smoke test 單股(快速驗 dataset 名 + 欄位語意):
     python src/main.py backfill --phases 5,6 --stocks 2330
  5. # 全市場(預期 30-40h):
     python src/main.py backfill --phases 5,6
  6. # 跑 Silver builder 確認 5 欄已填:
     python src/main.py silver phase 7a --stocks 2330 --full-rebuild
     psql $DATABASE_URL -c "
       SELECT stock_id, date, gov_bank_net
       FROM institutional_daily_derived
       WHERE stock_id='2330' ORDER BY date DESC LIMIT 5
     "

------------------------------------------------------------------------------
依據
------------------------------------------------------------------------------

- m2Spec/collector_schema_consolidated_spec_v3_2.md §2.6.1 / 2.6.2 / 2.6.3
- m2Spec/collector_rust_restructure_blueprint_v3_2.md §六 + §八(dual-write
  pattern,T0+21 後砍 v2.0 entries)
- 模板對齊:
  - alembic l1m2n3o4p5q6 (PR #18.5 Bronze 3 張 refetch)
  - alembic n3o4p5q6r7s8 (PR #20 trigger 6 functions + 15 triggers)

Coexist 模式:本 PR 只「加」3 張新 Bronze;既有 v2.0 + v3.2 Bronze 表都不動。

Rollback:downgrade DROP 3 個 trigger + 3 張 Bronze。資料安全(本 PR 沒寫資料)。
==============================================================================
"""
from typing import Sequence, Union

from alembic import op


revision: str = "p5q6r7s8t9u0"
down_revision: Union[str, Sequence[str], None] = "o4p5q6r7s8t9"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


# =============================================================================
# 3 張 Bronze raw 表 DDL
# =============================================================================

DDL_GOVERNMENT_BANK = """
    CREATE TABLE IF NOT EXISTS government_bank_buy_sell_tw (
        market    TEXT NOT NULL,
        stock_id  TEXT NOT NULL,
        date      DATE NOT NULL,
        buy       BIGINT,
        sell      BIGINT,
        PRIMARY KEY (market, stock_id, date)
    )
"""

DDL_TOTAL_MARGIN = """
    CREATE TABLE IF NOT EXISTS total_margin_purchase_short_sale_tw (
        market                          TEXT NOT NULL,
        date                            DATE NOT NULL,
        total_margin_purchase_balance   BIGINT,
        total_short_sale_balance        BIGINT,
        PRIMARY KEY (market, date)
    )
"""

DDL_SHORT_SALE_SBL = """
    CREATE TABLE IF NOT EXISTS short_sale_securities_lending_tw (
        market                TEXT NOT NULL,
        stock_id              TEXT NOT NULL,
        date                  DATE NOT NULL,
        short_sales           BIGINT,
        returns               BIGINT,
        current_day_balance   BIGINT,
        PRIMARY KEY (market, stock_id, date)
    )
"""

INDEXES = [
    ("idx_gov_bank_buy_sell_tw_stock_date_desc",         "government_bank_buy_sell_tw"),
    ("idx_short_sale_sbl_tw_stock_date_desc",            "short_sale_securities_lending_tw"),
    # total_margin 是 market-level,(market, date) PK 已是查詢主路徑,不另加索引
]


# =============================================================================
# 3 個新 trigger
# =============================================================================
# (trigger_name, bronze_table, kind, target)
#   kind="generic" → trg_mark_silver_dirty(target_silver_table)
#   kind="reuse"   → 重用既有 special function(target = function 名,不含括號)

NEW_TRIGGERS = [
    (
        "mark_institutional_derived_from_gov_bank_dirty",
        "government_bank_buy_sell_tw",
        "generic",
        "institutional_daily_derived",
    ),
    (
        "mark_market_margin_derived_from_total_dirty",
        "total_margin_purchase_short_sale_tw",
        "reuse",
        "trg_mark_market_margin_dirty",
    ),
    (
        "mark_margin_derived_from_short_sale_dirty",
        "short_sale_securities_lending_tw",
        "generic",
        "margin_daily_derived",
    ),
]


def _drop_trigger_if_exists(name: str, table: str) -> None:
    op.execute(f"DROP TRIGGER IF EXISTS {name} ON {table}")


def upgrade() -> None:
    """建 3 張 Bronze + 索引 + 3 個 dirty trigger。"""

    # A. 3 張 Bronze
    op.execute(DDL_GOVERNMENT_BANK)
    op.execute(DDL_TOTAL_MARGIN)
    op.execute(DDL_SHORT_SALE_SBL)

    # B. 索引
    for idx_name, table in INDEXES:
        op.execute(
            f"CREATE INDEX IF NOT EXISTS {idx_name} "
            f"ON {table} (stock_id, date DESC)"
        )

    # C. 3 個 trigger
    for trg_name, bronze_table, kind, target in NEW_TRIGGERS:
        _drop_trigger_if_exists(trg_name, bronze_table)
        if kind == "generic":
            op.execute(
                f"""
                CREATE TRIGGER {trg_name}
                AFTER INSERT OR UPDATE ON {bronze_table}
                FOR EACH ROW EXECUTE FUNCTION trg_mark_silver_dirty('{target}')
                """
            )
        else:  # reuse
            op.execute(
                f"""
                CREATE TRIGGER {trg_name}
                AFTER INSERT OR UPDATE ON {bronze_table}
                FOR EACH ROW EXECUTE FUNCTION {target}()
                """
            )


def downgrade() -> None:
    """DROP 3 trigger + 3 Bronze。資料安全(本 PR 沒寫資料,trigger 函式不動)。"""

    # 反向 C
    for trg_name, bronze_table, _kind, _target in NEW_TRIGGERS:
        _drop_trigger_if_exists(trg_name, bronze_table)

    # 反向 A(索引隨表 drop 自動消失)
    op.execute("DROP TABLE IF EXISTS short_sale_securities_lending_tw")
    op.execute("DROP TABLE IF EXISTS total_margin_purchase_short_sale_tw")
    op.execute("DROP TABLE IF EXISTS government_bank_buy_sell_tw")
