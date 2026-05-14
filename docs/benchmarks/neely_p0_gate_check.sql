-- neely_p0_gate_check.sql
-- Neely Core P0 Gate 六檔股票實測校準資料蒐集 SQL。
--
-- 對齊 m3Spec/neely_core_architecture.md §10.0(P0 Gate / 校準目標)+
--      §13.3(Degree Ceiling)+ §8.5/§8.6(DegreeCeiling / CrossTimeframeHints)。
--
-- 執行:
--   psql $env:DATABASE_URL -f docs/benchmarks/neely_p0_gate_check.sql > p0_gate_results.txt   (PowerShell)
--   psql $DATABASE_URL     -f docs/benchmarks/neely_p0_gate_check.sql > p0_gate_results.txt   (bash)
--
-- 前提:user 已跑過
--   .\target\release\tw_cores.exe run --stock-id <X> --write  (對 6 檔每檔跑一次)
-- 後本 SQL 才有資料可查;若 structural_snapshots 表空 → §0 sanity 段會直接顯示空表訊息。
--
-- 輸出 7 段:
--   §0  Sanity check:每檔 snapshot 是否存在(找 0 row 提醒 user 該補跑哪檔)
--   §1  Forest 規模 / candidate / monowave 計數(每檔當日快照)
--   §2  Validator pass/reject + 工程護欄(overflow / compaction_timeout)觸發狀況
--   §3  RuleRejection 拒絕原因 breakdown(校準 forest_max_size / compaction_timeout 用)
--   §4  Stage_elapsed_ms 性能 breakdown(校準 elapsed_ms / peak_memory_mb 用)
--   §5  P9-P12 新欄位:missing_wave_suspects / emulation_suspects / reverse_logic /
--       round3_pause / degree_ceiling 觸發分布
--   §6  Facts 產出量(每檔每核心)— forest 規模合理性的另一視角

\set ON_ERROR_STOP on

-- 六檔目標股票(架構 §10.0):0050 / 2330 / 3363 / 6547 / 1312 + 1 檔自選
-- 若 user 用不同 6 檔,改下面 IN clause 即可。
\set P0_GATE_STOCKS '''0050'', ''2330'', ''3363'', ''6547'', ''1312'''

-- ============================================================
-- §0  Sanity check:六檔 snapshot 是否齊全
-- ============================================================
\echo '=== §0 Sanity check:Neely Core snapshot 覆蓋率 ==='

SELECT
    stock_id,
    COUNT(*)               AS snapshot_count,
    MIN(snapshot_date)     AS earliest_snapshot,
    MAX(snapshot_date)     AS latest_snapshot,
    MAX(source_version)    AS latest_version
FROM structural_snapshots
WHERE core_name = 'neely_core'
  AND stock_id IN (:P0_GATE_STOCKS)
GROUP BY stock_id
ORDER BY stock_id;

-- 若上面 query 回 < 6 row → user 缺跑該檔 tw_cores run --stock-id <X> --write
SELECT
    missing AS "缺資料 stock_id(請補跑 tw_cores run)"
FROM (VALUES ('0050'), ('2330'), ('3363'), ('6547'), ('1312')) AS expected(missing)
WHERE missing NOT IN (
    SELECT DISTINCT stock_id FROM structural_snapshots
    WHERE core_name = 'neely_core'
);

-- ============================================================
-- §1  Forest 規模 / candidate / monowave 計數(每檔最新快照)
-- ============================================================
\echo '=== §1 Forest 規模 + candidate + monowave 計數(每檔 latest snapshot)==='

SELECT
    stock_id,
    snapshot_date,
    timeframe,
    (snapshot->'diagnostics'->>'monowave_count')::int          AS monowave_count,
    (snapshot->'diagnostics'->>'candidate_count')::int         AS candidate_count,
    (snapshot->'diagnostics'->>'forest_size')::int             AS forest_size,
    (snapshot->'diagnostics'->>'compaction_paths')::int        AS compaction_paths,
    (snapshot->'diagnostics'->>'elapsed_ms')::int              AS total_elapsed_ms,
    (snapshot->'diagnostics'->>'peak_memory_mb')::int          AS peak_memory_mb
FROM structural_snapshots s
WHERE core_name = 'neely_core'
  AND stock_id IN (:P0_GATE_STOCKS)
  AND snapshot_date = (
        SELECT MAX(snapshot_date) FROM structural_snapshots
        WHERE core_name = 'neely_core' AND stock_id = s.stock_id
  )
ORDER BY stock_id;

-- 校準目標常數(neely_core/src/config.rs):
--   - forest_max_size       (預設 1000) — 若任一檔 forest_size 接近 1000 → 調高 / 觸發 BeamSearchFallback
--   - compaction_timeout_ms (預設 60000) — 若 elapsed_ms > 60000 → 校準
--   - beam_width            (預設 100;BEAM_CAP_MULTIPLIER × 10 = candidate_count 上限 1000)

-- ============================================================
-- §2  Validator pass/reject + 工程護欄觸發狀況
-- ============================================================
\echo '=== §2 Validator + 工程護欄 ==='

SELECT
    stock_id,
    (snapshot->'diagnostics'->>'validator_pass_count')::int    AS pass_count,
    (snapshot->'diagnostics'->>'validator_reject_count')::int  AS reject_count,
    ROUND(
        100.0 * (snapshot->'diagnostics'->>'validator_pass_count')::int
        / NULLIF((snapshot->'diagnostics'->>'candidate_count')::int, 0),
        1
    )                                                          AS pass_pct,
    (snapshot->'diagnostics'->>'overflow_triggered')::boolean  AS overflow_triggered,
    (snapshot->'diagnostics'->>'compaction_timeout')::boolean  AS compaction_timeout,
    (snapshot->>'insufficient_data')::boolean                  AS insufficient_data
FROM structural_snapshots s
WHERE core_name = 'neely_core'
  AND stock_id IN (:P0_GATE_STOCKS)
  AND snapshot_date = (
        SELECT MAX(snapshot_date) FROM structural_snapshots
        WHERE core_name = 'neely_core' AND stock_id = s.stock_id
  )
ORDER BY stock_id;

-- 預期(P0 Gate 校準目標):
--   - overflow_triggered = false       (預設 forest_max_size 1000 應該夠用)
--   - compaction_timeout = false       (60s timeout 應該夠快)
--   - pass_pct ∈ [10%, 60%]            (太低 → 校準容差 ±10%/±4%;太高 → 規則太鬆)
--   - insufficient_data 對 < warmup 期間應為 true,> warmup 應 false

-- ============================================================
-- §3  RuleRejection 拒絕原因 breakdown
-- ============================================================
\echo '=== §3 拒絕原因 Top 10 RuleId per stock ==='

WITH rejection_flat AS (
    SELECT
        s.stock_id,
        rej->>'rule_id'   AS rule_id_json,
        rej->>'expected'  AS expected,
        rej->>'actual'    AS actual,
        (rej->>'gap')::numeric AS gap
    FROM structural_snapshots s,
         jsonb_array_elements(s.snapshot->'diagnostics'->'rejections') AS rej
    WHERE s.core_name = 'neely_core'
      AND s.stock_id IN (:P0_GATE_STOCKS)
      AND s.snapshot_date = (
            SELECT MAX(snapshot_date) FROM structural_snapshots
            WHERE core_name = 'neely_core' AND stock_id = s.stock_id
      )
)
SELECT
    stock_id,
    rule_id_json,
    COUNT(*)                AS rejection_count,
    ROUND(AVG(gap)::numeric, 3)  AS avg_gap_pct
FROM rejection_flat
GROUP BY stock_id, rule_id_json
ORDER BY stock_id, rejection_count DESC
LIMIT 60;

-- 解讀:
--   - 若某 Ch5_Essential(N) 拒絕率 > 80% → 該規則太嚴(校準容差 ±4%/±10%)
--   - 若 Ch5_Overlap_Trending / Ch5_Overlap_Terminal 平均 fail 比例平衡(各 50%)→
--     Impulse vs Diagonal classifier 工作正常
--   - 若 Engineering_* 出現 → 工程護欄被觸發,需檢查 §2 overflow/timeout

-- ============================================================
-- §4  Stage_elapsed_ms 性能 breakdown
-- ============================================================
\echo '=== §4 各 Stage 性能 breakdown(2026-05-14 升 μs 精度)==='

SELECT
    stock_id,
    -- Stage 1 / 2(monowave + classify)
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_1_monowave')::int          AS s1_monowave,
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_2_classify')::int          AS s2_classify,
    -- Stage 0(P2 Pre-Constructive ~200 branches — 最可能熱點)
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_0_preconstructive')::int   AS s0_preconstr,
    -- Stage 3 / 3.5 / 4
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_3_candidates')::int        AS s3_candidates,
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_3_5_pattern_isolation')::int AS s3_5_pi,
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_4_validator')::int         AS s4_validator,
    -- Stage 5 / 6 / 7
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_5_classifier')::int        AS s5_classifier,
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_6_post_validator')::int    AS s6_post,
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_7_complexity')::int        AS s7_complexity,
    -- Stage 7.5 / 8 / 8.5
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_7_5_advanced_rules')::int  AS s7_5_adv,
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_8_compaction')::int        AS s8_compact,
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_8_5_three_rounds')::int    AS s8_5_3r,
    -- Stage 9a / 9b / 10a / 10b / 10c / 10.5 / 11 / 12
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_9a_missing_wave')::int     AS s9a_mw,
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_9b_emulation')::int        AS s9b_emul,
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_10a_power_rating')::int    AS s10a_pr,
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_10b_fibonacci')::int       AS s10b_fib,
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_10c_triggers')::int        AS s10c_trg,
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_10_5_reverse_logic')::int  AS s10_5_rl,
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_11_degree_ceiling')::int   AS s11_dc,
    (snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_12_cross_timeframe')::int  AS s12_ct
FROM structural_snapshots s
WHERE core_name = 'neely_core'
  AND stock_id IN (:P0_GATE_STOCKS)
  AND snapshot_date = (
        SELECT MAX(snapshot_date) FROM structural_snapshots
        WHERE core_name = 'neely_core' AND stock_id = s.stock_id
  )
ORDER BY stock_id;

-- 預期熱點(若實測 > 10s,需 profile):
--   - stage_0_preconstructive(~200 branch if-else,monowave 多時最重)
--   - stage_4_validator(每 candidate × ~22 條規則)
--   - stage_8_compaction(若 forest 接近 forest_max_size)

-- ============================================================
-- §5  P9-P12 新欄位觸發分布
-- ============================================================
\echo '=== §5 P9-P12 新欄位觸發分布 ==='

SELECT
    stock_id,
    jsonb_array_length(snapshot->'missing_wave_suspects')      AS missing_wave_count,
    jsonb_array_length(snapshot->'emulation_suspects')         AS emulation_count,
    (snapshot->'reverse_logic_observation'->>'triggered')::boolean
                                                               AS reverse_logic_triggered,
    (snapshot->'reverse_logic_observation'->>'scenario_count')::int
                                                               AS rl_scenario_count,
    jsonb_array_length(snapshot->'reverse_logic_observation'->'suggested_filter_ids')
                                                               AS rl_suggested_filter,
    snapshot->'round3_pause'->>'reason'                        AS round3_pause_reason,
    (snapshot->'round3_pause'->>'affected_scenario_count')::int
                                                               AS round3_affected,
    snapshot->'degree_ceiling'->>'max_reachable_degree'        AS degree_max,
    snapshot->'degree_ceiling'->>'reason'                      AS degree_reason,
    jsonb_array_length(snapshot->'cross_timeframe_hints'->'monowave_summaries')
                                                               AS ct_summary_count
FROM structural_snapshots s
WHERE core_name = 'neely_core'
  AND stock_id IN (:P0_GATE_STOCKS)
  AND snapshot_date = (
        SELECT MAX(snapshot_date) FROM structural_snapshots
        WHERE core_name = 'neely_core' AND stock_id = s.stock_id
  )
ORDER BY stock_id;

-- 預期(P0 Gate 校準目標):
--   - missing_wave_count:稀疏(0-3),P2 MissingWaveBundle 嚴格觸發
--   - emulation_count:0-2(spec Ch12 4 種偽裝罕見)
--   - reverse_logic_triggered:scenario_count >= 2 才為 true;
--     若 forest_size > 5 → 應觸發,suggested_filter_ids 過多(> 50%)→ 校準 is_near_completion
--   - degree_max:0050 daily ~ Minor / 2330 daily ~ Minor(上市 ~30y → Cycle 級限月線)
--   - ct_summary_count == monowave_count(每 monowave 一條 summary)

-- ============================================================
-- §6  Facts 產出量(per stock × per core)
-- ============================================================
\echo '=== §6 Facts 產出量 ==='

SELECT
    f.stock_id,
    f.source_core,
    COUNT(*)                 AS fact_count,
    MIN(f.fact_date)         AS earliest_fact,
    MAX(f.fact_date)         AS latest_fact
FROM facts f
WHERE f.stock_id IN (:P0_GATE_STOCKS)
  AND f.source_core = 'neely_core'
GROUP BY f.stock_id, f.source_core
ORDER BY f.stock_id;

-- 預期:每檔每天約 (forest_size + 1) 條 fact;
-- 大量 facts(> 100 條 / day)→ forest_size 失控,校準 forest_max_size 下調
