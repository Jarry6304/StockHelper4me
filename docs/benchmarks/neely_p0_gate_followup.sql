-- neely_p0_gate_followup.sql
-- P0 Gate v2 follow-up:missing_wave 深入分析 + 22 cores EventKind 級觸發率
--
-- 對應 docs/benchmarks/neely_p0_gate_results_v2_2026-05-14.md §G missing_wave 警示 +
-- §I 22 cores facts 拆 EventKind 校準。
--
-- 執行:
--   psql $env:DATABASE_URL -f docs/benchmarks/neely_p0_gate_followup.sql > p0_gate_followup_<date>.txt
--
-- 輸出 5 段:
--   §J  missing_wave_count 每檔分布(min/avg/p50/p95/max + top 10 stocks)
--   §K  missing_wave_position 分類觸發率(M1Center / M0Center / M2Endpoint / Ambiguous)
--   §L  emulation_kind 拆解(RunningDoubleThreeAsImpulse / DiagonalAsImpulse / 等)
--   §M  reverse_logic suggested_filter_ids 分布(過濾 stock 比例)
--   §N  22 cores 拆 EventKind 觸發率(對齊 v1.32 P2 校準目標)

\set ON_ERROR_STOP on

-- ============================================================
-- §J. missing_wave_count 每檔分布
-- ============================================================
\echo '=== §J. missing_wave_count 每檔分布(校準 P2 MissingWaveBundle 閾值)==='

WITH mw AS (
    SELECT
        stock_id,
        jsonb_array_length(snapshot->'missing_wave_suspects') AS mw_count
    FROM structural_snapshots
    WHERE core_name = 'neely_core'
)
SELECT
    COUNT(*) AS total_stocks,
    MIN(mw_count) AS min,
    ROUND(AVG(mw_count)::numeric, 2) AS avg,
    PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY mw_count) AS p50,
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY mw_count) AS p95,
    PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY mw_count) AS p99,
    MAX(mw_count) AS max,
    COUNT(*) FILTER (WHERE mw_count = 0) AS zero_mw_stocks,
    COUNT(*) FILTER (WHERE mw_count > 20) AS over_20_stocks
FROM mw;

\echo '=== §J.1 Top 10 stocks 含最多 missing_wave_suspects ==='

SELECT
    stock_id,
    jsonb_array_length(snapshot->'missing_wave_suspects') AS mw_count,
    (snapshot->'diagnostics'->>'monowave_count')::int     AS monowave_count,
    (snapshot->'diagnostics'->>'forest_size')::int        AS forest_size
FROM structural_snapshots
WHERE core_name = 'neely_core'
ORDER BY mw_count DESC NULLS LAST
LIMIT 10;

-- ============================================================
-- §K. missing_wave_position 分類觸發率
-- ============================================================
\echo '=== §K. missing_wave position 分類分布(spec 1055-1057)==='

SELECT
    suspect->>'position' AS position,
    COUNT(*) AS total_occurrences,
    COUNT(DISTINCT s.stock_id) AS affected_stocks
FROM structural_snapshots s,
     jsonb_array_elements(s.snapshot->'missing_wave_suspects') AS suspect
WHERE s.core_name = 'neely_core'
GROUP BY suspect->>'position'
ORDER BY total_occurrences DESC;

-- ============================================================
-- §L. emulation_kind 拆解
-- ============================================================
\echo '=== §L. Emulation kind 拆解(spec §Ch12 4 種偽裝)==='

SELECT
    suspect->>'kind' AS kind,
    COUNT(*) AS total_occurrences,
    COUNT(DISTINCT s.stock_id) AS affected_stocks
FROM structural_snapshots s,
     jsonb_array_elements(s.snapshot->'emulation_suspects') AS suspect
WHERE s.core_name = 'neely_core'
GROUP BY suspect->>'kind'
ORDER BY total_occurrences DESC;

-- ============================================================
-- §M. Reverse Logic suggested_filter_ids 分布
-- ============================================================
\echo '=== §M. Reverse Logic suggested_filter_ids 過濾比例(校準 is_near_completion)==='

WITH rl AS (
    SELECT
        stock_id,
        (snapshot->'reverse_logic_observation'->>'scenario_count')::int AS scenario_count,
        jsonb_array_length(snapshot->'reverse_logic_observation'->'suggested_filter_ids') AS filter_count
    FROM structural_snapshots
    WHERE core_name = 'neely_core'
      AND (snapshot->'reverse_logic_observation'->>'triggered')::bool = true
)
SELECT
    COUNT(*) AS triggered_stocks,
    ROUND(AVG(scenario_count)::numeric, 2) AS avg_forest,
    ROUND(AVG(filter_count)::numeric, 2)   AS avg_filter,
    ROUND(AVG(100.0 * filter_count / NULLIF(scenario_count, 0))::numeric, 1) AS avg_filter_pct,
    COUNT(*) FILTER (WHERE filter_count = 0) AS no_filter_stocks,
    COUNT(*) FILTER (WHERE filter_count = scenario_count) AS all_filter_stocks
FROM rl;

-- ============================================================
-- §N. 22 cores EventKind 級觸發率(對齊 v1.32 P2 校準)
-- ============================================================
\echo '=== §N. 22 cores EventKind 級觸發率(對齊 v1.32 P2 目標)==='

-- 復用 scripts/p2_calibration_data.sql §2 邏輯,各 core 拆 EventKind:
WITH event_kinds AS (
    SELECT
        source_core,
        CASE
            WHEN source_core IN ('rsi_core','macd_core','bollinger_core','atr_core','adx_core',
                                 'kd_core','obv_core','ma_core')
                THEN SPLIT_PART(statement, ' ', 2)
            WHEN source_core = 'financial_statement_core'
                THEN SPLIT_PART(statement, ' ', 1)
            WHEN source_core = 'foreign_holding_core' THEN
                CASE
                    WHEN statement LIKE 'Foreign holding 60d high%'  THEN 'HoldingMilestoneHigh'
                    WHEN statement LIKE 'Foreign holding 60d low%'   THEN 'HoldingMilestoneLow'
                    WHEN statement LIKE 'Foreign holding 252d high%' THEN 'HoldingMilestoneHighAnnual'
                    WHEN statement LIKE 'Foreign holding 252d low%'  THEN 'HoldingMilestoneLowAnnual'
                    WHEN statement LIKE 'Foreign holding reached%'   THEN 'LimitNearAlert'
                    ELSE 'SignificantSingleDayChange'
                END
            WHEN source_core = 'margin_core' THEN
                CASE
                    WHEN statement LIKE 'Margin balance up%'   THEN 'MarginSurge'
                    WHEN statement LIKE 'Margin balance down%' THEN 'MarginCrash'
                    WHEN statement LIKE 'Short balance down%'  THEN 'ShortSqueeze'
                    WHEN statement LIKE 'Short balance up%'    THEN 'ShortBuildUp'
                    WHEN statement LIKE 'Short-to-margin ratio entered ExtremeHigh%' THEN 'EnteredShortRatioExtremeHigh'
                    WHEN statement LIKE 'Short-to-margin ratio exited ExtremeHigh%'  THEN 'ExitedShortRatioExtremeHigh'
                    WHEN statement LIKE 'Short-to-margin ratio entered ExtremeLow%'  THEN 'EnteredShortRatioExtremeLow'
                    WHEN statement LIKE 'Short-to-margin ratio exited ExtremeLow%'   THEN 'ExitedShortRatioExtremeLow'
                    WHEN statement LIKE 'Maintenance%'         THEN 'MaintenanceLow'
                    ELSE 'Other'
                END
            WHEN source_core = 'day_trading_core' THEN
                -- Phase 19(2026-05-14):pattern 對齊 Rust event_to_fact format
                -- spec §7 4 EventKind:RatioExtremeHigh / RatioExtremeLow /
                --                      RatioStreakHigh / RatioStreakLow
                CASE
                    WHEN statement LIKE 'Day trade ratio reached%(extreme high)'
                        THEN 'RatioExtremeHigh'
                    WHEN statement LIKE 'Day trade ratio dropped to%(extreme low)'
                        THEN 'RatioExtremeLow'
                    WHEN statement LIKE 'Day trade ratio above threshold%'
                        THEN 'RatioStreakHigh'
                    WHEN statement LIKE 'Day trade ratio below threshold%'
                        THEN 'RatioStreakLow'
                    ELSE 'Other'
                END
            WHEN source_core = 'institutional_core' THEN
                -- Phase 19(2026-05-14):pattern 對齊 Rust event_to_fact format
                -- spec §3 4 EventKind:NetBuyStreak / NetSellStreak /
                --                      LargeTransaction / DivergenceWithinInstitution
                -- LargeTransaction 細分到 institution(Foreign / Trust / Dealer)by metadata
                CASE
                    WHEN statement LIKE 'Foreign net buy%consecutive days%' THEN 'NetBuyStreak'
                    WHEN statement LIKE 'Foreign net sell%consecutive days%' THEN 'NetSellStreak'
                    WHEN statement LIKE 'Foreign single-day large transaction%' THEN 'LargeTransactionForeign'
                    WHEN statement LIKE 'Trust single-day large transaction%' THEN 'LargeTransactionTrust'
                    WHEN statement LIKE 'Dealer single-day large transaction%' THEN 'LargeTransactionDealer'
                    WHEN statement LIKE '%single-day large transaction%' THEN 'LargeTransactionOther'
                    WHEN statement LIKE 'Foreign and dealer diverge%' THEN 'DivergenceWithinInstitution'
                    ELSE 'Other'
                END
            ELSE 'Aggregated'
        END AS event_kind,
        stock_id,
        fact_date
    FROM facts
)
SELECT
    source_core,
    event_kind,
    COUNT(*)                                                                       AS events,
    COUNT(DISTINCT stock_id)                                                       AS stocks,
    ROUND(1.0 * COUNT(*) / NULLIF(COUNT(DISTINCT stock_id), 0), 1)                  AS events_per_stock,
    ROUND(1.0 * COUNT(*) / NULLIF(COUNT(DISTINCT stock_id), 0)
        / NULLIF(EXTRACT(YEAR FROM AGE(MAX(fact_date), MIN(fact_date)))::numeric + 1, 0), 2)
                                                                                    AS events_per_stock_per_year
FROM event_kinds
GROUP BY source_core, event_kind
HAVING COUNT(*) > 0
ORDER BY source_core, events DESC;
