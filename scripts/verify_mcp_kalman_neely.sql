-- =============================================================================
-- verify_mcp_kalman_neely.sql — v3.31 verify pipeline:Kalman + Neely DB spot-check
--
-- 對齊 scripts/maintain_facts_stats.sql 4 phase 風格。直接看 Rust 寫進 DB 的
-- 真實內容是什麼,排除 MCP layer / Python helper 干擾(逐層 audit 起點)。
--
-- 用法:
--   psql $env:DATABASE_URL -f scripts/verify_mcp_kalman_neely.sql
--   psql $env:DATABASE_URL -v stock=3030 -f scripts/verify_mcp_kalman_neely.sql
--
-- 預設股票 2330(可用 :stock psql 變數 override)。
-- =============================================================================

\set ON_ERROR_STOP on
\if :{?stock}
\else
\set stock '2330'
\endif

\echo
\echo === Phase 1: Kalman indicator_values 真實內容 ===
\echo  • series_len 應 > 1500(2019-01-01 起每交易日 1 點)
\echo  • latest_kalman_state.smoothed_price 應 ≠ 0(若 = 0 → 跑 tw_cores 重算)
\echo  • latest_kalman_state.regime 應 ∈ {StableUp,Accelerating,Sideway,Decelerating,StableDown}
\echo

SELECT stock_id,
       value_date,
       jsonb_array_length(value->'series') AS series_len,
       value->'series'->-1                  AS latest_kalman_state
  FROM indicator_values
 WHERE stock_id = :'stock'
   AND source_core = 'kalman_filter_core'
 ORDER BY value_date DESC
 LIMIT 1;

\echo
\echo === Phase 2: Neely scenario_forest top scenario + W1 anchor ===
\echo  • scenario_count 通常 1-10(取決於 compaction)
\echo  • s0_label 形如 "5-wave from mw27 to mw31"
\echo  • w1_start_date / w1_end_date 揭露 model anchor 在何時:
\echo    - 若近期日期(過去 1 年)→ Fib zones 對齊近期 W1 合理
\echo    - 若多年前(2022 / 2023)→ Fib zones 對齊老 anchor(model behavior 非 bug)
\echo

SELECT stock_id,
       snapshot_date,
       jsonb_array_length(snapshot->'scenario_forest')                  AS scenario_count,
       snapshot->'scenario_forest'->0->>'structure_label'                AS s0_label,
       snapshot->'scenario_forest'->0->'power_rating'                    AS s0_power_rating,
       snapshot->'scenario_forest'->0->'wave_tree'->'children'->0->>'start' AS w1_start_date,
       snapshot->'scenario_forest'->0->'wave_tree'->'children'->0->>'end'   AS w1_end_date,
       snapshot->'scenario_forest'->0->'expected_fib_zones'->0           AS first_fib_zone,
       snapshot->'scenario_forest'->0->'expected_fib_zones'->-1          AS last_fib_zone
  FROM structural_snapshots
 WHERE stock_id = :'stock'
   AND core_name = 'neely_core'
 ORDER BY snapshot_date DESC
 LIMIT 1;

\echo
\echo === 解讀 ===
\echo Kalman:若 smoothed_price = 0 / regime stuck "Sideway" → 跑 tw_cores run-all --write
\echo Neely: 若 expected_fib_zones 偏離 current_price 過大 → 看 W1 anchor 日期判定 model
\echo        anchor 是否在老波段(屬模型行為,不是 Rust bug)
\echo
\echo (對 staleness 詳情走 scripts/maintain_facts_stats.sql + verify_event_kind_rate.sql)
