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


-- =============================================================================
-- Phase 2 — EVENTS(除權息 / 減資 / 分割 / 面額變更 / 純現增)
-- =============================================================================

-- 價格調整事件(五種 event_type 共用一張表)
CREATE TABLE IF NOT EXISTS price_adjustment_events (
    market              TEXT NOT NULL,
    stock_id            TEXT NOT NULL,
    date                DATE NOT NULL,
    event_type          TEXT NOT NULL,
    before_price        NUMERIC(15, 4),
    after_price         NUMERIC(15, 4),
    reference_price     NUMERIC(15, 4),
    adjustment_factor   NUMERIC(20, 10) NOT NULL DEFAULT 1.0,  -- 後復權乘數
    volume_factor       NUMERIC(20, 10) NOT NULL DEFAULT 1.0,
    cash_dividend       NUMERIC(15, 6),
    stock_dividend      NUMERIC(15, 6),
    detail              JSONB,
    source              TEXT NOT NULL DEFAULT 'finmind',
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
CREATE TABLE IF NOT EXISTS price_daily_fwd (
    market          TEXT NOT NULL,
    stock_id        TEXT NOT NULL,
    date            DATE NOT NULL,
    open            NUMERIC(15, 4),
    high            NUMERIC(15, 4),
    low             NUMERIC(15, 4),
    close           NUMERIC(15, 4),
    volume          BIGINT,
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
-- 完成
-- =============================================================================

-- 給 db.py 的初始化檢查用
COMMENT ON TABLE schema_metadata IS 'Schema 版本與遷移歷史,Rust binary 啟動時 assert';
