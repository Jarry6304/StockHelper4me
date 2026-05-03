-- =============================================================================
-- tw-stock-collector — Postgres 17 Schema
-- Migrated from: src/db.py (SQLite, SCHEMA_VERSION=1.1)
-- Target: SCHEMA_VERSION=2.0 (Postgres baseline)
-- =============================================================================
--
-- 對齊原則:
--   1. 欄位名稱、PK、語意 100% 對應 SQLite 版,程式碼最少改動
--   2. 型別校準:TEXT 日期 -> DATE,TEXT JSON -> JSONB,適度用 NUMERIC
--   3. INSERT OR REPLACE -> INSERT ... ON CONFLICT DO UPDATE(由 db.py 處理)
--   4. PRAGMA table_info -> information_schema.columns(由 db.py 處理)
--   5. datetime('now') -> NOW()(時區用 UTC)
--   6. 不使用 partition,Collector 資料量小用不到;v2.0 的 indicator_values 才需要
--
-- 命名約定:
--   - schema 一律放 public(目前無多 schema 需求)
--   - 索引命名 idx_<table>_<purpose>
--   - 沒有 trigger 跟 stored procedure(維持 Collector 簡單性)
-- =============================================================================

-- 為了讓 schema 可重複執行(idempotent),全部用 IF NOT EXISTS

-- =============================================================================
-- Schema metadata 表(新增,Collector 原本沒有)
-- v2.0 新加的 schema version 對齊機制,Rust binary 啟動時會 assert
-- =============================================================================

CREATE TABLE IF NOT EXISTS schema_metadata (
    key             TEXT PRIMARY KEY,
    value           TEXT NOT NULL,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO schema_metadata (key, value) VALUES
    ('schema_version', '3.2'),                       -- m2 PR #2 bump:2.0 → 3.2(blueprint v3.2 r1 動工入口)
    ('migrated_from', 'sqlite_1.1'),
    ('migrated_at', NOW()::TEXT)
ON CONFLICT (key) DO NOTHING;


-- =============================================================================
-- Phase 1 — META(基礎資料)
-- =============================================================================

-- 股票基本資料(v3.2:stock_info → stock_info_ref;對應 blueprint §5.2)
-- 欄位 rename:market_type → type / industry → industry_category / delist_date → delisting_date
-- 🟡 listing_date / par_value / detail / source / updated_at 暫保留(stock_resolver.min_listing_days 等
-- collector 既有依賴),後續 PR 視 collector 重構決定砍除
CREATE TABLE IF NOT EXISTS stock_info_ref (
    market              TEXT NOT NULL,
    stock_id            TEXT NOT NULL,
    stock_name          TEXT,
    type                TEXT,                  -- twse | tpex | emerging(原 market_type)
    industry_category   TEXT,                  -- FinMind 主檔粗分類(原 industry)
    listing_date        DATE,                  -- 🟡 collector min_listing_days 依賴,暫保留
    delisting_date      DATE,                  -- NULL = 仍上市(原 delist_date)
    par_value           NUMERIC(10, 2),        -- 🟡 暫保留
    detail              JSONB,                 -- 🟡 v1.7 packs data_update_date,暫保留
    source              TEXT NOT NULL DEFAULT 'finmind',  -- 🟡 暫保留
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),  -- 🟡 ETL 內部,暫保留
    PRIMARY KEY (market, stock_id)
);

-- v3.2 §5.2 規定的 partial indexes
CREATE INDEX IF NOT EXISTS idx_sir_active
    ON stock_info_ref(market, stock_id)
    WHERE delisting_date IS NULL;
CREATE INDEX IF NOT EXISTS idx_sir_industry
    ON stock_info_ref(industry_category)
    WHERE industry_category IS NOT NULL;


-- 交易日曆(v3.2:trading_calendar → trading_date_ref;對應 blueprint §5.1)
-- 設計:row 存在 = 交易日,不存在 = 非交易日 → 砍 source 欄位(僅 'finmind' 無區別性)
CREATE TABLE IF NOT EXISTS trading_date_ref (
    market  TEXT NOT NULL,
    date    DATE NOT NULL,
    PRIMARY KEY (market, date)
);


-- 台股加權報酬指數
CREATE TABLE IF NOT EXISTS market_index_tw (
    market          TEXT NOT NULL,
    stock_id        TEXT NOT NULL,         -- TAIEX | TPEx
    date            DATE NOT NULL,
    price           NUMERIC(15, 4),
    source          TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, stock_id, date)
);

CREATE INDEX IF NOT EXISTS idx_market_index_tw_id_date
    ON market_index_tw (stock_id, date DESC);


-- 台股大盤 OHLCV(v3.2 B-1/B-2 — TAIEX / TPEx 日頻;與 market_index_tw 並存)
-- 來源:TaiwanStockTotalReturnIndex(close)+ TaiwanVariousIndicators5Seconds
-- (intraday 5-sec aggregate to daily OHLCV);Bronze layer raw,無衍生欄位
-- 注意:multi-source merge 邏輯留待 PR #17 重構 phase_executor 時實作
CREATE TABLE IF NOT EXISTS market_ohlcv_tw (
    market          TEXT NOT NULL,
    stock_id        TEXT NOT NULL,         -- TAIEX | TPEx
    date            DATE NOT NULL,
    open            NUMERIC(15, 4),
    high            NUMERIC(15, 4),
    low             NUMERIC(15, 4),
    close           NUMERIC(15, 4),
    volume          BIGINT,
    detail          JSONB,
    source          TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, stock_id, date)
);

CREATE INDEX IF NOT EXISTS idx_market_ohlcv_tw_id_date
    ON market_ohlcv_tw (stock_id, date DESC);


-- =============================================================================
-- Phase 2 — EVENTS(除權息 / 減資 / 分割 / 面額變更 / 純現增)
-- =============================================================================

-- 價格調整事件(五種 event_type 共用一張表)
-- v3.2 PR #17 (B-3) 砍 3 欄:adjustment_factor / after_price / source
--   * adjustment_factor:Rust 內現算(用 before_price + reference_price 反推)
--   * after_price:      Beta 不抓
--   * source:           用 event_type 推導
-- volume_factor 保留:P0-11 後 Rust compute_forward_adjusted 必要輸入
CREATE TABLE IF NOT EXISTS price_adjustment_events (
    market              TEXT NOT NULL,
    stock_id            TEXT NOT NULL,
    date                DATE NOT NULL,
    event_type          TEXT NOT NULL,
    before_price        NUMERIC(15, 4),
    reference_price     NUMERIC(15, 4),
    volume_factor       NUMERIC(20, 10) NOT NULL DEFAULT 1.0,
    cash_dividend       NUMERIC(15, 6),
    stock_dividend      NUMERIC(15, 6),
    detail              JSONB,
    PRIMARY KEY (market, stock_id, date, event_type),
    CONSTRAINT chk_event_type CHECK (
        event_type IN ('dividend', 'capital_reduction', 'split',
                       'par_value_change', 'capital_increase')
    )
);

CREATE INDEX IF NOT EXISTS idx_price_adj_event_type_date
    ON price_adjustment_events (event_type, date DESC);


-- 股利政策暫存表(post_process 後不保留)
CREATE TABLE IF NOT EXISTS _dividend_policy_staging (
    market          TEXT NOT NULL,
    stock_id        TEXT NOT NULL,
    date            DATE NOT NULL,
    detail          JSONB,
    source          TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, stock_id, date)
);


-- 個股暫停交易事件(v3.2 B-4 新表;blueprint §四 + §附錄 B)
-- 用途:prev_trading_day(stock_id, date) 模組 + tw_market_core 個股級交易缺口識別
-- 取代 v3.1 提案的 tw_market_event_log(只保留個股暫停)
CREATE TABLE IF NOT EXISTS stock_suspension_events (
    market              TEXT NOT NULL,
    stock_id            TEXT NOT NULL,
    suspension_date     DATE NOT NULL,
    suspension_time     TEXT,                      -- 暫停時間
    resumption_date     DATE,                      -- 復牌日
    resumption_time     TEXT,
    reason              TEXT,                      -- 暫停原因
    detail              JSONB,                     -- 額外 metadata
    PRIMARY KEY (market, stock_id, suspension_date)
);

CREATE INDEX IF NOT EXISTS idx_sse_stock_date
    ON stock_suspension_events(market, stock_id, suspension_date DESC);


-- 借券成交明細(v3.2 B-5 新表;blueprint §附錄 B)
-- 用途:margin_core 借券關鍵欄位來源(SBL 6 欄餵 margin_daily_derived)
-- PK 5 欄:同股同日「議借」+「競價」各自獨立,且各 fee_rate 一筆
CREATE TABLE IF NOT EXISTS securities_lending_tw (
    market                  TEXT NOT NULL,
    stock_id                TEXT NOT NULL,
    date                    DATE NOT NULL,
    transaction_type        TEXT NOT NULL,        -- 議借 / 競價
    volume                  BIGINT,
    fee_rate                NUMERIC(8, 4),
    close                   NUMERIC(15, 4),
    original_return_date    DATE,
    original_lending_period INT,
    detail                  JSONB,
    PRIMARY KEY (market, stock_id, date, transaction_type, fee_rate)
);

CREATE INDEX IF NOT EXISTS idx_sbl_stock_date
    ON securities_lending_tw(market, stock_id, date DESC);


-- 景氣指標(v3.2 B-6 新表;blueprint §六 + §6.2)
-- 月頻指標,單市場 'tw';v3.2 反方審查 Q2 拍板降為 reference 級別(不算 Core)
-- 砍 v3.1 提案的 leading_notrend / coincident_notrend / lagging_notrend(總經學家用,Beta 不需)
-- 🔧 hotfix:leading / coincident / lagging 加 _indicator 後綴
-- 「leading」是 PG 保留字(TRIM(LEADING ...))不能直接當欄位名,所以全部統一加後綴
CREATE TABLE IF NOT EXISTS business_indicator_tw (
    market                  TEXT NOT NULL DEFAULT 'tw',
    date                    DATE NOT NULL,           -- 月初
    leading_indicator       NUMERIC(10, 4),
    coincident_indicator    NUMERIC(10, 4),
    lagging_indicator       NUMERIC(10, 4),
    monitoring              INT,                     -- 綜合分數
    monitoring_color        TEXT,                    -- R / YR / G / YB / B
    detail                  JSONB,
    PRIMARY KEY (market, date)
);


-- =============================================================================
-- Phase 3 — RAW PRICE(原始日 K + 漲跌停)
-- =============================================================================

-- 日 K 原始價格
CREATE TABLE IF NOT EXISTS price_daily (
    market          TEXT NOT NULL,
    stock_id        TEXT NOT NULL,
    date            DATE NOT NULL,
    open            NUMERIC(15, 4),
    high            NUMERIC(15, 4),
    low             NUMERIC(15, 4),
    close           NUMERIC(15, 4),
    volume          BIGINT,                -- SQLite INTEGER -> BIGINT(避免 32bit 溢位)
    turnover        NUMERIC(20, 2),
    detail          JSONB,
    source          TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, stock_id, date)
);


-- 漲跌停價格
CREATE TABLE IF NOT EXISTS price_limit (
    market          TEXT NOT NULL,
    stock_id        TEXT NOT NULL,
    date            DATE NOT NULL,
    limit_up        NUMERIC(15, 4),
    limit_down      NUMERIC(15, 4),
    detail          JSONB,
    source          TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, stock_id, date)
);


-- =============================================================================
-- Phase 4 — Rust 計算產出(後復權 K 線)
-- =============================================================================

-- 後復權日 K
-- v3.2 PR #17 (B-3) 加 4 欄:Rust 算完 multiplier 後落地此處,Wave Cores /
-- Aggregation Layer 反推 raw 用(blueprint §5.2 amend + §4.4 r3.1)
CREATE TABLE IF NOT EXISTS price_daily_fwd (
    market                       TEXT NOT NULL,
    stock_id                     TEXT NOT NULL,
    date                         DATE NOT NULL,
    open                         NUMERIC(15, 4),
    high                         NUMERIC(15, 4),
    low                          NUMERIC(15, 4),
    close                        NUMERIC(15, 4),
    volume                       BIGINT,
    cumulative_adjustment_factor NUMERIC(20, 10),  -- 反推 raw price
    cumulative_volume_factor     NUMERIC(20, 10),  -- 反推 raw volume(P0-11 split 必要)
    is_adjusted                  BOOLEAN NOT NULL DEFAULT FALSE,  -- 該日是否動過
    adjustment_factor            NUMERIC(20, 10),  -- 單日 AF,除錯用
    PRIMARY KEY (market, stock_id, date)
);

-- v2.0 Wave Cores 主要靠這個索引
CREATE INDEX IF NOT EXISTS idx_price_daily_fwd_id_date_desc
    ON price_daily_fwd (stock_id, date DESC);


-- 後復權週 K(ISO week)
CREATE TABLE IF NOT EXISTS price_weekly_fwd (
    market          TEXT NOT NULL,
    stock_id        TEXT NOT NULL,
    year            INTEGER NOT NULL,
    week            INTEGER NOT NULL CHECK (week BETWEEN 1 AND 53),
    open            NUMERIC(15, 4),
    high            NUMERIC(15, 4),
    low             NUMERIC(15, 4),
    close           NUMERIC(15, 4),
    volume          BIGINT,
    PRIMARY KEY (market, stock_id, year, week)
);


-- 後復權月 K
CREATE TABLE IF NOT EXISTS price_monthly_fwd (
    market          TEXT NOT NULL,
    stock_id        TEXT NOT NULL,
    year            INTEGER NOT NULL,
    month           INTEGER NOT NULL CHECK (month BETWEEN 1 AND 12),
    open            NUMERIC(15, 4),
    high            NUMERIC(15, 4),
    low             NUMERIC(15, 4),
    close           NUMERIC(15, 4),
    volume          BIGINT,
    PRIMARY KEY (market, stock_id, year, month)
);


-- =============================================================================
-- Phase 5 — CHIP / FUNDAMENTAL
-- =============================================================================

-- 三大法人買賣超(per stock,5 類)
CREATE TABLE IF NOT EXISTS institutional_daily (
    market                      TEXT NOT NULL,
    stock_id                    TEXT NOT NULL,
    date                        DATE NOT NULL,
    foreign_buy                 BIGINT,
    foreign_sell                BIGINT,
    foreign_dealer_self_buy     BIGINT,
    foreign_dealer_self_sell    BIGINT,
    investment_trust_buy        BIGINT,
    investment_trust_sell       BIGINT,
    dealer_buy                  BIGINT,
    dealer_sell                 BIGINT,
    dealer_hedging_buy          BIGINT,
    dealer_hedging_sell         BIGINT,
    source                      TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, stock_id, date)
);


-- 融資融券
CREATE TABLE IF NOT EXISTS margin_daily (
    market              TEXT NOT NULL,
    stock_id            TEXT NOT NULL,
    date                DATE NOT NULL,
    margin_purchase     BIGINT,
    margin_sell         BIGINT,
    margin_balance      BIGINT,
    short_sale          BIGINT,
    short_cover         BIGINT,
    short_balance       BIGINT,
    detail              JSONB,
    source              TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, stock_id, date)
);


-- 外資持股
CREATE TABLE IF NOT EXISTS foreign_holding (
    market                      TEXT NOT NULL,
    stock_id                    TEXT NOT NULL,
    date                        DATE NOT NULL,
    foreign_holding_shares      BIGINT,
    foreign_holding_ratio       NUMERIC(8, 4),  -- 百分比,如 75.1234
    detail                      JSONB,
    source                      TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, stock_id, date)
);


-- 股權分散表
CREATE TABLE IF NOT EXISTS holding_shares_per (
    market          TEXT NOT NULL,
    stock_id        TEXT NOT NULL,
    date            DATE NOT NULL,
    detail          JSONB,                 -- 各級距持股人數與張數
    source          TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, stock_id, date)
);


-- 本益比 / 殖利率 / 淨值比
CREATE TABLE IF NOT EXISTS valuation_daily (
    market          TEXT NOT NULL,
    stock_id        TEXT NOT NULL,
    date            DATE NOT NULL,
    per             NUMERIC(10, 4),
    dividend_yield  NUMERIC(8, 4),
    pbr             NUMERIC(10, 4),
    source          TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, stock_id, date)
);


-- 當沖資訊
CREATE TABLE IF NOT EXISTS day_trading (
    market              TEXT NOT NULL,
    stock_id            TEXT NOT NULL,
    date                DATE NOT NULL,
    day_trading_buy     BIGINT,            -- 當沖買進金額(v1.6 修正後語意)
    day_trading_sell    BIGINT,            -- 當沖賣出金額
    detail              JSONB,
    source              TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, stock_id, date)
);


-- 指數成分權重
CREATE TABLE IF NOT EXISTS index_weight_daily (
    market          TEXT NOT NULL,
    stock_id        TEXT NOT NULL,
    date            DATE NOT NULL,
    weight          NUMERIC(8, 4),
    detail          JSONB,
    source          TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, stock_id, date)
);


-- 月營收
CREATE TABLE IF NOT EXISTS monthly_revenue (
    market          TEXT NOT NULL,
    stock_id        TEXT NOT NULL,
    date            DATE NOT NULL,         -- FinMind 用當月 1 號代表該月
    revenue         NUMERIC(20, 2),        -- 元為單位,十億級
    revenue_mom     NUMERIC(10, 4),        -- 月增百分比
    revenue_yoy     NUMERIC(10, 4),        -- 年增百分比
    detail          JSONB,
    source          TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, stock_id, date)
);


-- 財務報表(損益 / 資產負債 / 現金流共用)
CREATE TABLE IF NOT EXISTS financial_statement (
    market          TEXT NOT NULL,
    stock_id        TEXT NOT NULL,
    date            DATE NOT NULL,         -- 會計期間結束日
    type            TEXT NOT NULL,         -- income | balance | cashflow
    detail          JSONB,                 -- 各會計科目值,key 為 PascalCase 原文
    source          TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, stock_id, date, type),
    CONSTRAINT chk_fin_type CHECK (type IN ('income', 'balance', 'cashflow'))
);

-- 利於以 type 過濾
CREATE INDEX IF NOT EXISTS idx_financial_type_date
    ON financial_statement (type, date DESC);

-- 利於 detail JSON 查特定科目(GIN index for JSONB)
CREATE INDEX IF NOT EXISTS idx_financial_detail_gin
    ON financial_statement USING GIN (detail jsonb_path_ops);


-- =============================================================================
-- Phase 6 — MACRO(總體)
-- =============================================================================

-- 美股指數(SPY, VIX)
CREATE TABLE IF NOT EXISTS market_index_us (
    market          TEXT NOT NULL,
    stock_id        TEXT NOT NULL,         -- SPY | ^VIX
    date            DATE NOT NULL,
    open            NUMERIC(15, 4),
    high            NUMERIC(15, 4),
    low             NUMERIC(15, 4),
    close           NUMERIC(15, 4),
    volume          BIGINT,
    detail          JSONB,
    source          TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, stock_id, date)
);


-- 匯率(每日多幣別)
CREATE TABLE IF NOT EXISTS exchange_rate (
    market          TEXT NOT NULL,
    date            DATE NOT NULL,
    currency        TEXT NOT NULL,         -- USD | EUR | JPY ...
    rate            NUMERIC(15, 6),        -- spot_buy
    detail          JSONB,                 -- cash_buy / cash_sell / spot_sell
    source          TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, date, currency)
);


-- 全市場三大法人(無 stock_id)
CREATE TABLE IF NOT EXISTS institutional_market_daily (
    market                      TEXT NOT NULL,
    date                        DATE NOT NULL,
    foreign_buy                 BIGINT,
    foreign_sell                BIGINT,
    foreign_dealer_self_buy     BIGINT,
    foreign_dealer_self_sell    BIGINT,
    investment_trust_buy        BIGINT,
    investment_trust_sell       BIGINT,
    dealer_buy                  BIGINT,
    dealer_sell                 BIGINT,
    dealer_hedging_buy          BIGINT,
    dealer_hedging_sell         BIGINT,
    source                      TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, date)
);


-- 整體市場融資維持率
CREATE TABLE IF NOT EXISTS market_margin_maintenance (
    market          TEXT NOT NULL,
    date            DATE NOT NULL,
    ratio           NUMERIC(8, 2),         -- 百分比,如 165.32
    source          TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, date)
);


-- CNN 恐懼貪婪指數
CREATE TABLE IF NOT EXISTS fear_greed_index (
    market          TEXT NOT NULL,
    date            DATE NOT NULL,
    score           NUMERIC(6, 2),         -- 0–100
    label           TEXT,                  -- Fear / Greed / Neutral / Extreme Fear / Extreme Greed
    detail          JSONB,
    source          TEXT NOT NULL DEFAULT 'finmind',
    PRIMARY KEY (market, date)
);


-- =============================================================================
-- 系統表(不對接外部 API)
-- =============================================================================

-- 同步狀態追蹤(per-stock)
CREATE TABLE IF NOT EXISTS stock_sync_status (
    market              TEXT NOT NULL,
    stock_id            TEXT NOT NULL,
    last_full_sync      DATE,
    last_incr_sync      DATE,
    fwd_adj_valid       SMALLINT NOT NULL DEFAULT 0,  -- 0=待算 / 1=已算
    PRIMARY KEY (market, stock_id),
    CONSTRAINT chk_fwd_adj_valid CHECK (fwd_adj_valid IN (0, 1))
);


-- API 層級斷點續傳進度
CREATE TABLE IF NOT EXISTS api_sync_progress (
    api_name        TEXT NOT NULL,
    stock_id        TEXT NOT NULL,
    segment_start   DATE NOT NULL,
    segment_end     DATE NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending',
    record_count    INTEGER NOT NULL DEFAULT 0,
    error_message   TEXT,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (api_name, stock_id, segment_start),
    CONSTRAINT chk_progress_status CHECK (
        status IN ('pending', 'completed', 'failed', 'empty', 'schema_mismatch')
    )
);

-- 利於查詢失敗的 segment 重試
CREATE INDEX IF NOT EXISTS idx_api_progress_status
    ON api_sync_progress (status, updated_at DESC)
    WHERE status != 'completed';


-- =============================================================================
-- v3.2 PR #18 Bronze reverse-pivot 表(blueprint §六 #11 / §十 PR #5)
-- =============================================================================
-- 5 張 Bronze raw 表,從 v2.0 pivot/pack 表反推。Coexist with legacy v2.0 表;
-- _legacy_v2 rename 留到 T0+21(blueprint §八.2)。
-- 與 alembic migration j9k0l1m2n3o4 保持同步;與 scripts/_reverse_pivot_lib.py
-- 的 SPECS 對齊欄名(SPECS 是 single source of truth)。

-- 三大法人(每法人 1 行,1 row 變 5 row)
CREATE TABLE IF NOT EXISTS institutional_investors_tw (
    market         TEXT NOT NULL,
    stock_id       TEXT NOT NULL,
    date           DATE NOT NULL,
    investor_type  TEXT NOT NULL,
    buy            BIGINT,
    sell           BIGINT,
    name           TEXT,
    PRIMARY KEY (market, stock_id, date, investor_type)
);
CREATE INDEX IF NOT EXISTS idx_institutional_investors_tw_stock_date_desc
    ON institutional_investors_tw (stock_id, date DESC);


-- 融資融券(14 raw fields,detail 攤平)
CREATE TABLE IF NOT EXISTS margin_purchase_short_sale_tw (
    market               TEXT NOT NULL,
    stock_id             TEXT NOT NULL,
    date                 DATE NOT NULL,
    margin_purchase      BIGINT,
    margin_sell          BIGINT,
    margin_balance       BIGINT,
    short_sale           BIGINT,
    short_cover          BIGINT,
    short_balance        BIGINT,
    margin_cash_repay    BIGINT,
    margin_prev_balance  BIGINT,
    margin_limit         BIGINT,
    short_cash_repay     BIGINT,
    short_prev_balance   BIGINT,
    short_limit          BIGINT,
    offset_loan_short    BIGINT,
    note                 TEXT,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_margin_purchase_short_sale_tw_stock_date_desc
    ON margin_purchase_short_sale_tw (stock_id, date DESC);


-- 外資持股(11 raw fields,detail 攤平)
CREATE TABLE IF NOT EXISTS foreign_investor_share_tw (
    market                  TEXT NOT NULL,
    stock_id                TEXT NOT NULL,
    date                    DATE NOT NULL,
    foreign_holding_shares  BIGINT,
    foreign_holding_ratio   NUMERIC(8, 4),
    remaining_shares        BIGINT,
    remain_ratio            NUMERIC(8, 4),
    upper_limit_ratio       NUMERIC(8, 4),
    cn_upper_limit          NUMERIC(8, 4),
    total_issued            BIGINT,
    declare_date            DATE,
    intl_code               TEXT,
    stock_name              TEXT,
    note                    TEXT,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_foreign_investor_share_tw_stock_date_desc
    ON foreign_investor_share_tw (stock_id, date DESC);


-- 當沖(4 raw fields,detail 攤平)
CREATE TABLE IF NOT EXISTS day_trading_tw (
    market             TEXT NOT NULL,
    stock_id           TEXT NOT NULL,
    date               DATE NOT NULL,
    day_trading_buy    BIGINT,
    day_trading_sell   BIGINT,
    day_trading_flag   TEXT,
    volume             BIGINT,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_day_trading_tw_stock_date_desc
    ON day_trading_tw (stock_id, date DESC);


-- 估值 PER / PBR / 殖利率(3 raw fields,無 detail)
CREATE TABLE IF NOT EXISTS valuation_per_tw (
    market          TEXT NOT NULL,
    stock_id        TEXT NOT NULL,
    date            DATE NOT NULL,
    per             NUMERIC(10, 4),
    pbr             NUMERIC(10, 4),
    dividend_yield  NUMERIC(8, 4),
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_valuation_per_tw_stock_date_desc
    ON valuation_per_tw (stock_id, date DESC);


-- =============================================================================
-- v3.2 PR #19a Silver `*_derived` 14 張 + dirty 欄位 + fwd ALTER
-- =============================================================================
-- per spec §2.3 canonical 清單。每張共通結構:source 表欄 + dirty 欄位 + 部分索引
-- (ON dirty_at WHERE is_dirty = TRUE,給 orchestrator pull queue 用)。
-- 與 alembic k0l1m2n3o4p5 對齊;Bronze→Silver trigger DDL 留 PR #20 enable。

-- 1. price_limit_merge_events(Rust 計算,schema TBD per PR #20)
CREATE TABLE IF NOT EXISTS price_limit_merge_events (
    market         TEXT NOT NULL,
    stock_id       TEXT NOT NULL,
    date           DATE NOT NULL,
    merge_type     TEXT,
    detail         JSONB,
    is_dirty       BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at       TIMESTAMPTZ,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_plme_dirty
    ON price_limit_merge_events (dirty_at) WHERE is_dirty = TRUE;

-- 2. monthly_revenue_derived
CREATE TABLE IF NOT EXISTS monthly_revenue_derived (
    market         TEXT NOT NULL,
    stock_id       TEXT NOT NULL,
    date           DATE NOT NULL,
    revenue        NUMERIC(20, 2),
    revenue_mom    NUMERIC(10, 4),
    revenue_yoy    NUMERIC(10, 4),
    detail         JSONB,
    is_dirty       BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at       TIMESTAMPTZ,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_mr_dirty
    ON monthly_revenue_derived (dirty_at) WHERE is_dirty = TRUE;

-- 3. valuation_daily_derived(+market_value_weight per §2.6.4)
CREATE TABLE IF NOT EXISTS valuation_daily_derived (
    market               TEXT NOT NULL,
    stock_id             TEXT NOT NULL,
    date                 DATE NOT NULL,
    per                  NUMERIC(10, 4),
    pbr                  NUMERIC(10, 4),
    dividend_yield       NUMERIC(8, 4),
    market_value_weight  NUMERIC(10, 6),
    is_dirty             BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at             TIMESTAMPTZ,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_vd_dirty
    ON valuation_daily_derived (dirty_at) WHERE is_dirty = TRUE;

-- 4. financial_statement_derived
CREATE TABLE IF NOT EXISTS financial_statement_derived (
    market         TEXT NOT NULL,
    stock_id       TEXT NOT NULL,
    date           DATE NOT NULL,
    type           TEXT NOT NULL,
    detail         JSONB,
    is_dirty       BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at       TIMESTAMPTZ,
    PRIMARY KEY (market, stock_id, date, type),
    CONSTRAINT chk_fin_derived_type CHECK (type IN ('income', 'balance', 'cashflow'))
);
CREATE INDEX IF NOT EXISTS idx_fs_dirty
    ON financial_statement_derived (dirty_at) WHERE is_dirty = TRUE;

-- 5. institutional_daily_derived(+gov_bank_net per §2.6.2)
CREATE TABLE IF NOT EXISTS institutional_daily_derived (
    market                      TEXT NOT NULL,
    stock_id                    TEXT NOT NULL,
    date                        DATE NOT NULL,
    foreign_buy                 BIGINT,
    foreign_sell                BIGINT,
    foreign_dealer_self_buy     BIGINT,
    foreign_dealer_self_sell    BIGINT,
    investment_trust_buy        BIGINT,
    investment_trust_sell       BIGINT,
    dealer_buy                  BIGINT,
    dealer_sell                 BIGINT,
    dealer_hedging_buy          BIGINT,
    dealer_hedging_sell         BIGINT,
    gov_bank_net                BIGINT,
    is_dirty                    BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at                    TIMESTAMPTZ,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_id_dirty
    ON institutional_daily_derived (dirty_at) WHERE is_dirty = TRUE;

-- 6. margin_daily_derived(+SBL 6 欄 per §2.6.1)
CREATE TABLE IF NOT EXISTS margin_daily_derived (
    market                                  TEXT NOT NULL,
    stock_id                                TEXT NOT NULL,
    date                                    DATE NOT NULL,
    margin_purchase                         BIGINT,
    margin_sell                             BIGINT,
    margin_balance                          BIGINT,
    short_sale                              BIGINT,
    short_cover                             BIGINT,
    short_balance                           BIGINT,
    detail                                  JSONB,
    margin_short_sales_short_sales          BIGINT,
    margin_short_sales_short_covering       BIGINT,
    margin_short_sales_current_day_balance  BIGINT,
    sbl_short_sales_short_sales             BIGINT,
    sbl_short_sales_returns                 BIGINT,
    sbl_short_sales_current_day_balance     BIGINT,
    is_dirty                                BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at                                TIMESTAMPTZ,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_md_dirty
    ON margin_daily_derived (dirty_at) WHERE is_dirty = TRUE;

-- 7. foreign_holding_derived
CREATE TABLE IF NOT EXISTS foreign_holding_derived (
    market                  TEXT NOT NULL,
    stock_id                TEXT NOT NULL,
    date                    DATE NOT NULL,
    foreign_holding_shares  BIGINT,
    foreign_holding_ratio   NUMERIC(8, 4),
    detail                  JSONB,
    is_dirty                BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at                TIMESTAMPTZ,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_fh_dirty
    ON foreign_holding_derived (dirty_at) WHERE is_dirty = TRUE;

-- 8. holding_shares_per_derived
CREATE TABLE IF NOT EXISTS holding_shares_per_derived (
    market         TEXT NOT NULL,
    stock_id       TEXT NOT NULL,
    date           DATE NOT NULL,
    detail         JSONB,
    is_dirty       BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at       TIMESTAMPTZ,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_hsp_dirty
    ON holding_shares_per_derived (dirty_at) WHERE is_dirty = TRUE;

-- 9. day_trading_derived
CREATE TABLE IF NOT EXISTS day_trading_derived (
    market             TEXT NOT NULL,
    stock_id           TEXT NOT NULL,
    date               DATE NOT NULL,
    day_trading_buy    BIGINT,
    day_trading_sell   BIGINT,
    detail             JSONB,
    is_dirty           BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at           TIMESTAMPTZ,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_dt_dirty
    ON day_trading_derived (dirty_at) WHERE is_dirty = TRUE;

-- 10. taiex_index_derived(對應 market_ohlcv_tw)
CREATE TABLE IF NOT EXISTS taiex_index_derived (
    market         TEXT NOT NULL,
    stock_id       TEXT NOT NULL,
    date           DATE NOT NULL,
    open           NUMERIC(15, 4),
    high           NUMERIC(15, 4),
    low            NUMERIC(15, 4),
    close          NUMERIC(15, 4),
    volume         BIGINT,
    detail         JSONB,
    is_dirty       BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at       TIMESTAMPTZ,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_tid_dirty
    ON taiex_index_derived (dirty_at) WHERE is_dirty = TRUE;

-- 11. us_market_index_derived(對應 market_index_us)
CREATE TABLE IF NOT EXISTS us_market_index_derived (
    market         TEXT NOT NULL,
    stock_id       TEXT NOT NULL,
    date           DATE NOT NULL,
    open           NUMERIC(15, 4),
    high           NUMERIC(15, 4),
    low            NUMERIC(15, 4),
    close          NUMERIC(15, 4),
    volume         BIGINT,
    detail         JSONB,
    is_dirty       BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at       TIMESTAMPTZ,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_usmid_dirty
    ON us_market_index_derived (dirty_at) WHERE is_dirty = TRUE;

-- 12. exchange_rate_derived(PK 含 currency,不是 stock_id)
CREATE TABLE IF NOT EXISTS exchange_rate_derived (
    market         TEXT NOT NULL,
    date           DATE NOT NULL,
    currency       TEXT NOT NULL,
    rate           NUMERIC(15, 6),
    detail         JSONB,
    is_dirty       BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at       TIMESTAMPTZ,
    PRIMARY KEY (market, date, currency)
);
CREATE INDEX IF NOT EXISTS idx_erd_dirty
    ON exchange_rate_derived (dirty_at) WHERE is_dirty = TRUE;

-- 13. market_margin_maintenance_derived(+2 欄 per §2.6.3)
CREATE TABLE IF NOT EXISTS market_margin_maintenance_derived (
    market                          TEXT NOT NULL,
    date                            DATE NOT NULL,
    ratio                           NUMERIC(8, 2),
    total_margin_purchase_balance   BIGINT,
    total_short_sale_balance        BIGINT,
    is_dirty                        BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at                        TIMESTAMPTZ,
    PRIMARY KEY (market, date)
);
CREATE INDEX IF NOT EXISTS idx_mmmd_dirty
    ON market_margin_maintenance_derived (dirty_at) WHERE is_dirty = TRUE;

-- 14. business_indicator_derived(NEW per §6.3)
-- 注意:spec §6.3 DDL 寫 bare leading / coincident / lagging,但 PG 保留字
-- (TRIM(LEADING ...))不能裸用。Bronze business_indicator_tw 早已加 `_indicator`
-- 後綴(line 204-205 hotfix),Silver 對齊 Bronze 1:1 不用 rename。
CREATE TABLE IF NOT EXISTS business_indicator_derived (
    market                  TEXT NOT NULL DEFAULT 'tw',
    stock_id                TEXT NOT NULL DEFAULT '_market_',
    date                    DATE NOT NULL,
    leading_indicator       NUMERIC(10, 4),
    coincident_indicator    NUMERIC(10, 4),
    lagging_indicator       NUMERIC(10, 4),
    monitoring              INT,
    monitoring_color        TEXT,
    is_dirty                BOOLEAN NOT NULL DEFAULT FALSE,
    dirty_at                TIMESTAMPTZ,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_bid_dirty
    ON business_indicator_derived (dirty_at) WHERE is_dirty = TRUE;

-- ─── 3 張 fwd 表加 dirty 欄位 + index(PR #17 已建表,本次只 ALTER)─────────
ALTER TABLE price_daily_fwd
    ADD COLUMN IF NOT EXISTS is_dirty BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS dirty_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS idx_price_daily_fwd_dirty
    ON price_daily_fwd (dirty_at) WHERE is_dirty = TRUE;

ALTER TABLE price_weekly_fwd
    ADD COLUMN IF NOT EXISTS is_dirty BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS dirty_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS idx_price_weekly_fwd_dirty
    ON price_weekly_fwd (dirty_at) WHERE is_dirty = TRUE;

ALTER TABLE price_monthly_fwd
    ADD COLUMN IF NOT EXISTS is_dirty BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS dirty_at TIMESTAMPTZ;
CREATE INDEX IF NOT EXISTS idx_price_monthly_fwd_dirty
    ON price_monthly_fwd (dirty_at) WHERE is_dirty = TRUE;


-- =============================================================================
-- v3.2 PR #18.5 Bronze refetch 3 張(blueprint §八.1 Option A)
-- =============================================================================
-- 因 detail JSONB unpack 不可逆(level taxonomy 未知 / 中→英 origin_name 對應丟失 /
-- FinMind 月營收 1 row/股/月),3 張表走 Option A 從 FinMind 全量重抓 raw bytes
-- (~30-40h calendar-time)。Coexist with v2.0 表(holding_shares_per /
-- financial_statement / monthly_revenue);_legacy_v2 rename + DROP 留 T0+21 / T0+60。
-- 對應 alembic l1m2n3o4p5q6 + collector.toml dual-write entries。

-- 股權分散表(每 level 1 row;FinMind raw)
CREATE TABLE IF NOT EXISTS holding_shares_per_tw (
    market               TEXT NOT NULL,
    stock_id             TEXT NOT NULL,
    date                 DATE NOT NULL,
    holding_shares_level TEXT NOT NULL,
    people               BIGINT,
    percent              NUMERIC(8, 4),
    unit                 BIGINT,
    PRIMARY KEY (market, stock_id, date, holding_shares_level)
);
CREATE INDEX IF NOT EXISTS idx_holding_shares_per_tw_stock_date_desc
    ON holding_shares_per_tw (stock_id, date DESC);


-- 財報三表合一(event_type ∈ income/balance/cashflow,reuse pae convention)
CREATE TABLE IF NOT EXISTS financial_statement_tw (
    market      TEXT NOT NULL,
    stock_id    TEXT NOT NULL,
    date        DATE NOT NULL,
    event_type  TEXT NOT NULL,
    type        TEXT,
    origin_name TEXT NOT NULL,
    value       NUMERIC(20, 4),
    PRIMARY KEY (market, stock_id, date, event_type, origin_name),
    CONSTRAINT chk_fs_tw_event_type CHECK (event_type IN ('income', 'balance', 'cashflow'))
);
CREATE INDEX IF NOT EXISTS idx_financial_statement_tw_stock_date_desc
    ON financial_statement_tw (stock_id, date DESC);


-- 月營收(raw FinMind 欄名;Silver builder 才 rename revenue_year → revenue_yoy 等)
-- create_time 用 TEXT(per PR #18.5 hotfix m2n3o4p5q6r7):FinMind 對某些 row 回 ""
-- 不是 NULL,Bronze raw 保留原始字串,Silver builder cast 用 NULLIF(...)::TIMESTAMPTZ
CREATE TABLE IF NOT EXISTS monthly_revenue_tw (
    market         TEXT NOT NULL,
    stock_id       TEXT NOT NULL,
    date           DATE NOT NULL,
    revenue        NUMERIC(20, 2),
    revenue_year   NUMERIC(10, 4),
    revenue_month  NUMERIC(10, 4),
    country        TEXT,
    create_time    TEXT,
    PRIMARY KEY (market, stock_id, date)
);
CREATE INDEX IF NOT EXISTS idx_monthly_revenue_tw_stock_date_desc
    ON monthly_revenue_tw (stock_id, date DESC);


-- =============================================================================
-- 完成
-- =============================================================================

-- 給 db.py 的初始化檢查用
COMMENT ON TABLE schema_metadata IS 'Schema 版本與遷移歷史,Rust binary 啟動時 assert';
