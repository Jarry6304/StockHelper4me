-- =====================================================================
-- M2 NEO Pipeline v2.0 Storage Layer Schema
-- =====================================================================
-- 對應 m2Spec/m2_neo_pipeline_spec_r3.md 第十四章「儲存層架構」
--
-- 本檔案僅含 M2 Layer 2-4 + 系統表,與 collector 的 src/schema_pg.sql
-- 切分獨立,不依賴對方,各自管各自 ops / migration / retention。
--
-- Layer 1 (raw 資料) 由 collector 寫入,本檔不重複定義,
-- 但會在 Aggregation Layer / Batch Pipeline 端讀取,參考表:
--   price_daily_fwd / price_weekly_fwd / price_monthly_fwd
--   institutional_daily / margin_daily / foreign_holding / ...
--
-- 執行方式:在 collector schema 已建立的 PG 17 instance 上直接 psql 灌入。
--   psql $DATABASE_URL -f m2/schema_m2_pg.sql
--
-- =====================================================================

-- ---------------------------------------------------------------------
-- M2 schema 版本標記(獨立於 collector 的 schema_metadata)
-- ---------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS m2_schema_metadata (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO m2_schema_metadata (key, value)
VALUES ('m2_schema_version', '0.1.0-p0')
ON CONFLICT (key) DO NOTHING;


-- =====================================================================
-- Layer 2: indicator_values
--   每日 batch 寫入,滑動窗口型指標 + on-demand 補算共用
--   r3 14.2.2 / 14.5.1
-- =====================================================================
-- Partition 設計:
--   第一層 RANGE BY date(每年一個 partition)
--   第二層 HASH BY stock_id(modulus 8)
-- PK 必須包含 partition key (date, stock_id) → 已涵蓋。
-- =====================================================================

CREATE TABLE IF NOT EXISTS indicator_values (
    stock_id        VARCHAR(10) NOT NULL,
    date            DATE        NOT NULL,
    timeframe       VARCHAR(10) NOT NULL,   -- daily / weekly / monthly
    indicator_name  VARCHAR(50) NOT NULL,   -- macd / rsi / kd / ma / ...
    params_hash     VARCHAR(16) NOT NULL,   -- canonical JSON + blake3 取前 16 hex
    values          JSONB       NOT NULL,
    core_version    VARCHAR(20) NOT NULL,
    computed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (stock_id, date, timeframe, indicator_name, params_hash)
) PARTITION BY RANGE (date);

-- 主要查詢索引:某股某指標的時間序列(P0 上線即建)
-- 注意:PG 在 partitioned table 上建 index 會自動下推到所有 child partitions
CREATE INDEX IF NOT EXISTS idx_indicator_values_lookup
    ON indicator_values (stock_id, indicator_name, params_hash, date DESC);

-- JSONB GIN index 在 P0 不建,P2 視實際查詢 pattern 再評估
-- CREATE INDEX idx_indicator_values_jsonb
--     ON indicator_values USING GIN (values jsonb_path_ops);

-- ----- 年度 RANGE partitions(P0 起手:2021-2027,涵蓋熱資料 5 年 + 緩衝)-----
-- 依 14.5.2 retention:熱 5 年 SSD,2026 為當年。
-- 每年再切 8 個 hash sub-partition,讓 batch 16 worker 並行寫入不互鎖。

DO $$
DECLARE
    yr  INT;
    h   INT;
BEGIN
    FOR yr IN 2021..2027 LOOP
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS indicator_values_%s '
            'PARTITION OF indicator_values '
            'FOR VALUES FROM (%L) TO (%L) '
            'PARTITION BY HASH (stock_id)',
            yr, format('%s-01-01', yr), format('%s-01-01', yr + 1)
        );

        FOR h IN 0..7 LOOP
            EXECUTE format(
                'CREATE TABLE IF NOT EXISTS indicator_values_%s_p%s '
                'PARTITION OF indicator_values_%s '
                'FOR VALUES WITH (modulus 8, remainder %s)',
                yr, h, yr, h
            );
        END LOOP;
    END LOOP;
END $$;


-- =====================================================================
-- Layer 3: structural_snapshots
--   全量重算的結構性指標(Wave Forest / SR / Trendline 等)
--   r3 14.2.3
-- =====================================================================
-- P0 不 partition(資料量小,1800 檔 × 2500 天 × 2 core ≈ 900 萬 row,可承受)
-- P3 後視情況加。
-- =====================================================================

CREATE TABLE IF NOT EXISTS structural_snapshots (
    stock_id            VARCHAR(10) NOT NULL,
    snapshot_date       DATE        NOT NULL,
    timeframe           VARCHAR(10) NOT NULL,
    core_name           VARCHAR(50) NOT NULL,   -- neely_core / sr_levels / fib_zones
    snapshot_data       JSONB       NOT NULL,   -- Scenario forest / SR levels / ...
    core_version        VARCHAR(20) NOT NULL,
    derived_from_core   VARCHAR(50),            -- r3 5.2:fib_zones 投影視圖須附 neely_core
    computed_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (stock_id, snapshot_date, timeframe, core_name)
);

CREATE INDEX IF NOT EXISTS idx_structural_snapshots_latest
    ON structural_snapshots (stock_id, core_name, snapshot_date DESC);


-- =====================================================================
-- Layer 4: facts
--   事件式紀錄,append-only,Learner 訓練主要資料源
--   r3 14.2.4 / 14.5.1
-- =====================================================================
-- Partition: RANGE BY fact_date(單層,facts 不 hash partition)
-- PK 必須包含 partition key (fact_date) → 改 (id, fact_date) 複合 PK
-- =====================================================================

CREATE TABLE IF NOT EXISTS facts (
    id              BIGINT      GENERATED BY DEFAULT AS IDENTITY,
    stock_id        VARCHAR(10) NOT NULL,
    fact_date       DATE        NOT NULL,
    timeframe       VARCHAR(10) NOT NULL,
    category        VARCHAR(50) NOT NULL,   -- momentum / trend / chips / structural / ...
    statement       TEXT        NOT NULL,   -- "MACD(12,26,9) golden cross at 2026-04-15"
    source_core     VARCHAR(50) NOT NULL,
    source_version  VARCHAR(20) NOT NULL,
    params_hash     VARCHAR(16),            -- 籌碼/結構類 fact 可為 NULL
    data_references JSONB,                  -- 可追溯到具體資料列
    computed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, fact_date)
) PARTITION BY RANGE (fact_date);

-- ----- 主要查詢索引(全 partitioned index)-----
CREATE INDEX IF NOT EXISTS idx_facts_stock
    ON facts (stock_id, fact_date DESC);

CREATE INDEX IF NOT EXISTS idx_facts_category
    ON facts (category, fact_date DESC);

CREATE INDEX IF NOT EXISTS idx_facts_core
    ON facts (source_core, params_hash, fact_date DESC);

-- ----- Idempotency unique constraint(r3 14.2.4)-----
-- statement 直接 unique 太大,改 md5(statement) 後 unique
-- params_hash 可為 NULL → 用 COALESCE 避免 NULL 比對問題
-- 注意:partition key (fact_date) 已包含,符合 PG 11+ partitioned unique 規範
CREATE UNIQUE INDEX IF NOT EXISTS idx_facts_unique
    ON facts (
        stock_id,
        fact_date,
        timeframe,
        source_core,
        COALESCE(params_hash, ''),
        md5(statement)
    );

-- ----- 年度 RANGE partitions -----
DO $$
DECLARE
    yr INT;
BEGIN
    FOR yr IN 2021..2027 LOOP
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS facts_%s '
            'PARTITION OF facts '
            'FOR VALUES FROM (%L) TO (%L)',
            yr, format('%s-01-01', yr), format('%s-01-01', yr + 1)
        );
    END LOOP;
END $$;


-- =====================================================================
-- workflow_registry:on-demand 累積參數組合,batch 自動納入
-- r3 15.6
-- =====================================================================

CREATE TABLE IF NOT EXISTS workflow_registry (
    indicator_name  VARCHAR(50) NOT NULL,
    params_hash     VARCHAR(16) NOT NULL,
    params_json     JSONB       NOT NULL,
    timeframe       VARCHAR(10) NOT NULL,
    first_seen_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    use_count       BIGINT      NOT NULL DEFAULT 0,
    PRIMARY KEY (indicator_name, params_hash, timeframe)
);

-- 30 天淘汰機制查詢索引(ops job 用)
CREATE INDEX IF NOT EXISTS idx_workflow_registry_stale
    ON workflow_registry (last_used_at);


-- =====================================================================
-- batch_execution_log:Batch Pipeline 觀察性
-- r3 15.5
-- =====================================================================

CREATE TABLE IF NOT EXISTS batch_execution_log (
    id              BIGINT      GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    batch_id        UUID        NOT NULL,
    core_name       VARCHAR(50) NOT NULL,
    core_version    VARCHAR(20) NOT NULL,
    stock_id        VARCHAR(10),                -- NULL 表示跨股的 stage 級紀錄
    stage           VARCHAR(20),                -- stage_2 / stage_3 / stage_4a / ...
    started_at      TIMESTAMPTZ NOT NULL,
    finished_at     TIMESTAMPTZ,
    status          VARCHAR(20) NOT NULL,       -- success / failed / skipped / running
    error_message   TEXT,
    rows_written    BIGINT      DEFAULT 0,
    CONSTRAINT chk_batch_log_status CHECK (
        status IN ('success', 'failed', 'skipped', 'running')
    )
);

CREATE INDEX IF NOT EXISTS idx_batch_log_batch_id
    ON batch_execution_log (batch_id, started_at);

CREATE INDEX IF NOT EXISTS idx_batch_log_core_status
    ON batch_execution_log (core_name, status, started_at DESC);


-- =====================================================================
-- Cold partition retention 預留(P0 不啟用,P1 後 ops job 啟用)
-- =====================================================================
-- 14.5.2 retention 策略:
--   indicator_values:熱 5 年 SSD,5-10 年 parquet 冷儲存,10 年以上不保留
--   facts:append-only,5 年以上轉冷儲存
--
-- 冷遷移流程(每年 1 月 ops job):
--   1. SELECT * FROM indicator_values_{old_year} → COPY TO parquet
--   2. ALTER TABLE indicator_values DETACH PARTITION indicator_values_{old_year}
--   3. DROP TABLE indicator_values_{old_year}_p0 ... _p7 CASCADE
--
-- 此處不寫成 stored procedure,留給 ops 層用 Python 腳本管理(可審計、可 dry-run)。
-- =====================================================================
