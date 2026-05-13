-- neely_p0_gate_production.sql
-- Neely Core production scale 校準資料蒐集 SQL(全市場 1263+ stocks)。
--
-- 對齊 m3Spec/neely_core_architecture.md §10.0 P0 Gate + §13.3 Degree Ceiling。
--
-- 與 `neely_p0_gate_check.sql`(5-檔版本)互補:本 SQL 對「全市場」做分布統計,
-- 用於校準 forest_max_size / compaction_timeout_ms 等需要看 p95/p99/max 的常數。
--
-- 執行:
--   psql $env:DATABASE_URL -f docs/benchmarks/neely_p0_gate_production.sql > p0_gate_production_<date>.txt   (PowerShell)
--   psql $DATABASE_URL     -f docs/benchmarks/neely_p0_gate_production.sql > p0_gate_production_<date>.txt   (bash)
--
-- 前提:user 已跑過 `tw_cores run-all --write`(全市場 1263+ stocks 寫入 structural_snapshots)。
--
-- 輸出 9 段:
--   §A  Forest size 全市場分布(校準 forest_max_size)
--   §B  Forest size 統計值(min / avg / max / p50 / p95 / p99)
--   §C  工程護欄觸發狀況(overflow / timeout / insufficient_data)
--   §D  Monowave / candidate 統計
--   §E  Validator pass_pct 分布
--   §F  全市場拒絕原因 Top 15
--   §G  P9-P12 新欄位觸發股票數
--   §H  Degree ceiling 分布(spec §13.3 驗證)
--   §I  22 cores facts 全市場產出量

\set ON_ERROR_STOP on

-- ============================================================
-- §A. Forest size 全市場分布
-- ============================================================
\echo '=== §A. Forest size 全市場分布(校準 forest_max_size 用)==='

SELECT
    (snapshot->'diagnostics'->>'forest_size')::int AS forest_size,
    COUNT(*) AS stock_count,
    ROUND(100.0 * COUNT(*) / SUM(COUNT(*)) OVER (), 2) AS pct
FROM structural_snapshots
WHERE core_name = 'neely_core'
GROUP BY forest_size
ORDER BY forest_size DESC
LIMIT 30;

-- ============================================================
-- §B. Forest size 統計值
-- ============================================================
\echo '=== §B. Forest size 統計(min/avg/max/p50/p95/p99)==='

SELECT
    COUNT(*) AS total_stocks,
    MIN((snapshot->'diagnostics'->>'forest_size')::int)         AS min_forest,
    ROUND(AVG((snapshot->'diagnostics'->>'forest_size')::int), 2) AS avg_forest,
    MAX((snapshot->'diagnostics'->>'forest_size')::int)         AS max_forest,
    PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY (snapshot->'diagnostics'->>'forest_size')::int) AS p50,
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY (snapshot->'diagnostics'->>'forest_size')::int) AS p95,
    PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY (snapshot->'diagnostics'->>'forest_size')::int) AS p99
FROM structural_snapshots WHERE core_name = 'neely_core';

-- ============================================================
-- §C. 工程護欄觸發狀況
-- ============================================================
\echo '=== §C. 工程護欄觸發狀況(overflow / timeout / insufficient_data)==='

SELECT
    (snapshot->'diagnostics'->>'overflow_triggered')::bool AS overflow_triggered,
    (snapshot->>'compaction_timeout')::bool                AS compaction_timeout,
    (snapshot->>'insufficient_data')::bool                 AS insufficient_data,
    COUNT(*) AS stock_count
FROM structural_snapshots WHERE core_name = 'neely_core'
GROUP BY 1, 2, 3
ORDER BY stock_count DESC;

-- ============================================================
-- §D. Monowave + candidate 統計
-- ============================================================
\echo '=== §D. Monowave + candidate 統計 ==='

SELECT
    'monowave_count' AS metric,
    MIN((snapshot->'diagnostics'->>'monowave_count')::int)           AS min,
    ROUND(AVG((snapshot->'diagnostics'->>'monowave_count')::int), 1) AS avg,
    PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY (snapshot->'diagnostics'->>'monowave_count')::int) AS p50,
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY (snapshot->'diagnostics'->>'monowave_count')::int) AS p95,
    MAX((snapshot->'diagnostics'->>'monowave_count')::int)           AS max
FROM structural_snapshots WHERE core_name = 'neely_core'
UNION ALL SELECT 'candidate_count',
    MIN((snapshot->'diagnostics'->>'candidate_count')::int),
    ROUND(AVG((snapshot->'diagnostics'->>'candidate_count')::int), 1),
    PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY (snapshot->'diagnostics'->>'candidate_count')::int),
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY (snapshot->'diagnostics'->>'candidate_count')::int),
    MAX((snapshot->'diagnostics'->>'candidate_count')::int)
FROM structural_snapshots WHERE core_name = 'neely_core';

-- ============================================================
-- §E. Validator pass_pct 分布
-- ============================================================
\echo '=== §E. Pass percentage 分布 ==='

WITH pp AS (
    SELECT (100.0 *
        (snapshot->'diagnostics'->>'validator_pass_count')::int
        / NULLIF((snapshot->'diagnostics'->>'candidate_count')::int, 0)
    )::numeric AS pass_pct
    FROM structural_snapshots WHERE core_name = 'neely_core'
)
SELECT
    ROUND(MIN(pass_pct), 1) AS min_pct,
    ROUND(AVG(pass_pct), 1) AS avg_pct,
    ROUND(PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY pass_pct)::numeric, 1) AS p50,
    ROUND(PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY pass_pct)::numeric, 1) AS p95,
    ROUND(MAX(pass_pct), 1) AS max_pct,
    COUNT(*) FILTER (WHERE pass_pct = 0)    AS zero_pass_stocks,
    COUNT(*) FILTER (WHERE pass_pct IS NULL) AS no_candidate_stocks
FROM pp;

-- ============================================================
-- §F. 全市場拒絕原因 Top 15
-- ============================================================
\echo '=== §F. 全市場拒絕原因 Top 15(跨 1263+ stocks)==='

SELECT
    rej->>'rule_id'  AS rule_id,
    COUNT(*)         AS total_rejections,
    COUNT(DISTINCT s.stock_id) AS affected_stocks,
    ROUND(AVG((rej->>'gap')::numeric), 2) AS avg_gap_pct
FROM structural_snapshots s,
     jsonb_array_elements(s.snapshot->'diagnostics'->'rejections') AS rej
WHERE s.core_name = 'neely_core'
GROUP BY rej->>'rule_id'
ORDER BY total_rejections DESC
LIMIT 15;

-- ============================================================
-- §G. P9-P12 新欄位觸發股票數
-- ============================================================
\echo '=== §G. P9-P12 新欄位觸發股票數 ==='

SELECT
    COUNT(*)                                                                       AS total_stocks,
    COUNT(*) FILTER (WHERE jsonb_array_length(snapshot->'missing_wave_suspects') > 0) AS with_missing_wave,
    COUNT(*) FILTER (WHERE jsonb_array_length(snapshot->'emulation_suspects') > 0)    AS with_emulation,
    COUNT(*) FILTER (WHERE (snapshot->'reverse_logic_observation'->>'triggered')::bool = true) AS reverse_logic_t,
    COUNT(*) FILTER (WHERE snapshot->'round3_pause' != 'null'::jsonb)                 AS round3_pause,
    COUNT(*) FILTER (WHERE (snapshot->>'insufficient_data')::bool = true)             AS insufficient,
    COUNT(*) FILTER (WHERE (snapshot->>'compaction_timeout')::bool = true)            AS compact_timeout
FROM structural_snapshots WHERE core_name = 'neely_core';

-- ============================================================
-- §H. Degree ceiling 分布
-- ============================================================
\echo '=== §H. Degree ceiling 分布(spec §13.3 驗證)==='

SELECT
    snapshot->'degree_ceiling'->>'max_reachable_degree' AS degree,
    COUNT(*) AS stock_count,
    ROUND(100.0 * COUNT(*) / SUM(COUNT(*)) OVER (), 2) AS pct
FROM structural_snapshots WHERE core_name = 'neely_core'
GROUP BY degree
ORDER BY stock_count DESC;

-- ============================================================
-- §I. 22 cores facts 全市場產出量 + 觸發率
-- ============================================================
\echo '=== §I. 22 cores facts 全市場產出量 + 每股每年觸發率 ==='

SELECT
    source_core,
    COUNT(DISTINCT stock_id)                                                       AS stocks,
    COUNT(*)                                                                        AS facts,
    ROUND(1.0 * COUNT(*) / NULLIF(COUNT(DISTINCT stock_id), 0), 1)                  AS facts_per_stock,
    ROUND(1.0 * COUNT(*) / NULLIF(COUNT(DISTINCT stock_id), 0)
        / NULLIF(EXTRACT(YEAR FROM AGE(MAX(fact_date), MIN(fact_date)))::numeric + 1, 0), 2)
                                                                                    AS facts_per_stock_per_year
FROM facts
GROUP BY source_core
ORDER BY facts DESC;
