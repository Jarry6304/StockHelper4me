"""v3.20: 5 個 sponsor-tier dataset 接入 Bronze

接 v3.19 probe `--max 0` 揭露的 5 個高價值 unused datasets,user 拍版動工:

1. `loan_collateral_balance_tw` — 詳細融券細項(35 columns,擴 margin coverage)
2. `block_trade_tw` — 大宗交易(配對交易等,institutional flow 補強)
3. `market_value_daily` — 個股市值(目前計算欄,直拉省 share 推算誤差)
4. `disposition_securities_period_tw` — 處置股風險警示(注意 / 分盤撮合)
5. `commodity_price_daily` — 商品(初版 gold;對齊 exchange_rate macro pattern)

User 拍版(2026-05-17):
- 不做日內 tick / 5-second 資料
- 不做權證 / 期貨 / 期權
- 不做可轉債
- GoldPrice 5 分鐘粒度資料,每日只存第一筆(aggregator `first_per_day` 處理)

設計選擇:
- block_trade_tw PK 加 `trade_type` 維度(對齊 v3.14 gov_bank `bank_name`
  pattern)。同 (stock_id, date, trade_type) 多筆視為同 logical row,
  Bronze 接受最後一筆(再次 backfill 冪等)。
- disposition_securities_period_tw `param_mode = all_market`(probe 揭露
  with_data_id=2330 → 0 row,no_data_id → 17 row,FinMind 對 all-market dataset
  典型行為)。
- commodity_price_daily PK (market, commodity, date) 開放未來擴 silver/oil/etc。
  GoldPrice 以 commodity='GOLD' 固定值寫入,first_per_day aggregator
  group by (commodity, date::date) 取 min(time) 那筆。

Revision ID: b7c8d9e0f1g2
Revises: a6b7c8d9e0f1
Create Date: 2026-05-17
"""

from alembic import op


revision = 'b7c8d9e0f1g2'
down_revision = 'a6b7c8d9e0f1'
branch_labels = None
depends_on = None


def upgrade() -> None:
    # ──────────────────────────────────────────────────────────────
    # 1. loan_collateral_balance_tw — 借券抵押餘額 35 欄細項
    #    FinMind `TaiwanStockLoanCollateralBalance` 一日回 ~2k stocks,
    #    each stock 1 row 帶 5 大借券類別 × 7 欄(Previous/Buy/Sell/
    #    CashRedemption/Replacement/CurrentDayBalance/NextDayQuota)。
    # ──────────────────────────────────────────────────────────────
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS loan_collateral_balance_tw (
            market                              TEXT NOT NULL,
            stock_id                            TEXT NOT NULL,
            date                                DATE NOT NULL,
            -- Margin(融資)6 欄
            margin_previous_day_balance         BIGINT,
            margin_buy                          BIGINT,
            margin_sell                         BIGINT,
            margin_cash_redemption              BIGINT,
            margin_current_day_balance          BIGINT,
            margin_next_day_quota               BIGINT,
            -- Securities Firm Loan(券商自有借券)7 欄
            firm_loan_previous_day_balance      BIGINT,
            firm_loan_buy                       BIGINT,
            firm_loan_sell                      BIGINT,
            firm_loan_cash_redemption           BIGINT,
            firm_loan_replacement               BIGINT,
            firm_loan_current_day_balance       BIGINT,
            firm_loan_next_day_quota            BIGINT,
            -- Unrestricted Loan(無限制借券)7 欄
            unrestricted_loan_previous_day_balance  BIGINT,
            unrestricted_loan_buy                   BIGINT,
            unrestricted_loan_sell                  BIGINT,
            unrestricted_loan_cash_redemption       BIGINT,
            unrestricted_loan_replacement           BIGINT,
            unrestricted_loan_current_day_balance   BIGINT,
            unrestricted_loan_next_day_quota        BIGINT,
            -- Securities Finance Secured Loan(證金擔保借券)7 欄
            finance_loan_previous_day_balance       BIGINT,
            finance_loan_buy                        BIGINT,
            finance_loan_sell                       BIGINT,
            finance_loan_cash_redemption            BIGINT,
            finance_loan_replacement                BIGINT,
            finance_loan_current_day_balance        BIGINT,
            finance_loan_next_day_quota             BIGINT,
            -- Settlement Margin(交割保證金借券)7 欄
            settlement_margin_previous_day_balance  BIGINT,
            settlement_margin_buy                   BIGINT,
            settlement_margin_sell                  BIGINT,
            settlement_margin_cash_redemption       BIGINT,
            settlement_margin_replacement           BIGINT,
            settlement_margin_current_day_balance   BIGINT,
            settlement_margin_next_day_quota        BIGINT,
            detail                              JSONB,
            PRIMARY KEY (market, stock_id, date)
        )
        """
    )

    # ──────────────────────────────────────────────────────────────
    # 2. block_trade_tw — 大宗交易(配對 / 鉅額 / 自營)
    #    FinMind `TaiwanStockBlockTrade` 同 (stock_id, date) 可能多筆,
    #    PK 加 trade_type 維度(對齊 gov_bank bank_name pattern)。
    # ──────────────────────────────────────────────────────────────
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS block_trade_tw (
            market          TEXT NOT NULL,
            stock_id        TEXT NOT NULL,
            date            DATE NOT NULL,
            trade_type      TEXT NOT NULL,         -- 配對交易 / 鉅額 / 自營 ...
            price           NUMERIC(15, 4),
            volume          BIGINT,
            trading_money   BIGINT,
            detail          JSONB,
            PRIMARY KEY (market, stock_id, date, trade_type)
        )
        """
    )

    # ──────────────────────────────────────────────────────────────
    # 3. market_value_daily — 個股市值(單欄)
    #    FinMind `TaiwanStockMarketValue` 一日 1 row × stock,直接拉
    #    省 stock_info_ref.shares × close 推算誤差。
    # ──────────────────────────────────────────────────────────────
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS market_value_daily (
            market          TEXT NOT NULL,
            stock_id        TEXT NOT NULL,
            date            DATE NOT NULL,
            market_value    BIGINT,                 -- 市值(NTD)
            detail          JSONB,
            PRIMARY KEY (market, stock_id, date)
        )
        """
    )

    # ──────────────────────────────────────────────────────────────
    # 4. disposition_securities_period_tw — 處置股
    #    FinMind `TaiwanStockDispositionSecuritiesPeriod` all_market mode
    #    (probe 揭露 with_data_id=2330 → 0 row,no_data_id → 17 row)。
    #    date = 公告日,period_start/end = 處置期間。
    # ──────────────────────────────────────────────────────────────
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS disposition_securities_period_tw (
            market              TEXT NOT NULL,
            stock_id            TEXT NOT NULL,
            date                DATE NOT NULL,         -- 公告日
            disposition_cnt     INTEGER,                -- 第 N 次處置
            period_start        DATE,                   -- 處置開始
            period_end          DATE,                   -- 處置結束
            condition           TEXT,                   -- 觸發條件
            measure             TEXT,                   -- 處置措施描述(長文)
            detail              JSONB,
            PRIMARY KEY (market, stock_id, date, disposition_cnt)
        )
        """
    )

    # ──────────────────────────────────────────────────────────────
    # 5. commodity_price_daily — 商品(初版 gold)
    #    FinMind `GoldPrice` 5 分鐘粒度,first_per_day aggregator 取每日
    #    第一筆。PK (market, commodity, date) 開放未來擴 silver/oil/etc。
    # ──────────────────────────────────────────────────────────────
    op.execute(
        """
        CREATE TABLE IF NOT EXISTS commodity_price_daily (
            market          TEXT NOT NULL,
            commodity       TEXT NOT NULL,             -- GOLD | (future: SILVER / OIL ...)
            date            DATE NOT NULL,
            price           NUMERIC(15, 4),
            detail          JSONB,
            PRIMARY KEY (market, commodity, date)
        )
        """
    )


def downgrade() -> None:
    op.execute("DROP TABLE IF EXISTS commodity_price_daily")
    op.execute("DROP TABLE IF EXISTS disposition_securities_period_tw")
    op.execute("DROP TABLE IF EXISTS market_value_daily")
    op.execute("DROP TABLE IF EXISTS block_trade_tw")
    op.execute("DROP TABLE IF EXISTS loan_collateral_balance_tw")
