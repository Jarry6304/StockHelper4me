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


-- ============================================================================
-- §8. P0 Gate 六檔限定 Phase 14-17 metadata 驗證(2026-05-14 補完)
-- ============================================================================
--
-- 對齊 docs/benchmarks/neely_p0_gate_runbook.md §8.1-8.5。
-- 把 v5 全市場(7767 scenarios)的 §E/G query 收斂到 P0 Gate 六檔限定:
--   0050 / 1312 / 2330 / 3363 / 6547(+ 1 自選)
-- 對齊 runbook §10 (d) 判定條件:六檔 metadata 填充率對齊 v5 全市場 ±5%。

\echo '=== §8.1 Phase 13 max_retracement + Phase 14 PostBehavior(六檔)==='

SELECT
    COUNT(*) AS total_scenarios,
    COUNT(*) FILTER (WHERE s->'max_retracement' IS NOT NULL
                       AND jsonb_typeof(s->'max_retracement') = 'number') AS with_max_retr,
    array_agg(DISTINCT s->>'max_retracement') AS max_retr_values,
    array_agg(DISTINCT CASE
        WHEN jsonb_typeof(s->'post_pattern_behavior') = 'string'
            THEN s->>'post_pattern_behavior'
        WHEN jsonb_typeof(s->'post_pattern_behavior') = 'object'
            THEN (SELECT key FROM jsonb_each(s->'post_pattern_behavior') LIMIT 1)
        ELSE NULL
    END) AS behavior_kinds
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'scenario_forest') s
WHERE core_name = 'neely_core'
  AND stock_id IN ('0050', '1312', '2330', '3363', '6547');

\echo ''
\echo '=== §8.2 Phase 15 Scenario 群 2 fields(六檔)==='

SELECT
    COUNT(*) AS total_scenarios,
    COUNT(*) FILTER (WHERE s->>'round_state' IS NOT NULL) AS with_round_state,
    array_agg(DISTINCT s->>'round_state') AS round_states,
    COUNT(*) FILTER (WHERE jsonb_array_length(s->'monowave_structure_labels') > 0) AS with_mw_labels,
    COUNT(*) FILTER (WHERE jsonb_array_length(s->'pattern_isolation_anchors') > 0) AS with_anchors,
    COUNT(*) FILTER (WHERE (s->>'triplexity_detected')::bool = true) AS with_triplexity
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'scenario_forest') s
WHERE core_name = 'neely_core'
  AND stock_id IN ('0050', '1312', '2330', '3363', '6547');

\echo ''
\echo '=== §8.3 Phase 16 pattern_type 分布(六檔)==='

SELECT
    CASE jsonb_typeof(s->'pattern_type')
        WHEN 'string' THEN s->>'pattern_type'
        WHEN 'object' THEN (
            SELECT key || '(' || COALESCE(value->>'sub_kind',
                                          jsonb_path_query_first(value, '$.sub_kinds[0]')::text,
                                          '') || ')'
            FROM jsonb_each(s->'pattern_type') LIMIT 1
        )
        ELSE 'unknown'
    END AS pattern,
    COUNT(*) AS scenarios,
    array_agg(DISTINCT stock_id ORDER BY stock_id) AS stocks
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'scenario_forest') s
WHERE core_name = 'neely_core'
  AND stock_id IN ('0050', '1312', '2330', '3363', '6547')
GROUP BY 1 ORDER BY 2 DESC;

\echo ''
\echo '=== §8.4 Phase 17 StructuralFacts 7 sub-fields 填充率(六檔)==='

SELECT
    COUNT(*) AS total_scenarios,
    COUNT(*) FILTER (WHERE s->'structural_facts'->'fibonacci_alignment' IS NOT NULL
                       AND s->'structural_facts'->>'fibonacci_alignment' != 'null') AS fib,
    COUNT(*) FILTER (WHERE s->'structural_facts'->'alternation' IS NOT NULL
                       AND s->'structural_facts'->>'alternation' != 'null') AS alt,
    COUNT(*) FILTER (WHERE s->'structural_facts'->'channeling' IS NOT NULL
                       AND s->'structural_facts'->>'channeling' != 'null') AS chan,
    COUNT(*) FILTER (WHERE s->'structural_facts'->'time_relationship' IS NOT NULL
                       AND s->'structural_facts'->>'time_relationship' != 'null') AS tr,
    COUNT(*) FILTER (WHERE s->'structural_facts'->'volume_alignment' IS NOT NULL
                       AND s->'structural_facts'->>'volume_alignment' != 'null') AS vol,
    COUNT(*) FILTER (WHERE (s->'structural_facts'->>'gap_count')::int > 0) AS gaps_found,
    COUNT(*) FILTER (WHERE s->'structural_facts'->'overlap_pattern' IS NOT NULL
                       AND s->'structural_facts'->>'overlap_pattern' != 'null') AS overlap
FROM structural_snapshots,
     jsonb_array_elements(snapshot->'scenario_forest') s
WHERE core_name = 'neely_core'
  AND stock_id IN ('0050', '1312', '2330', '3363', '6547');

\echo ''
\echo '=== §8.5 v5 全市場 vs 六檔填充率對照(對齊判定 (d):±5%)==='
\echo '預期(v5 全市場 baseline):'
\echo '  fib  77.5% / alt 7.5% / chan 100% / tr 100% / vol 100% / gaps 100% / overlap 22.8%'
\echo '六檔 (d) 判定通過條件:各 sub-field 比例落入 v5 baseline ±5%'

