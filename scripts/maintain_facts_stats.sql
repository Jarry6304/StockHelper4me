-- ============================================================================
-- scripts/maintain_facts_stats.sql
-- ============================================================================
-- v3.19(2026-05-17):facts 表 stats 維護 + wall time regression diagnostic
--
-- 動機:v3.17 → v3.18 production verify 揭露 wall time +24%(561s → 695s),
-- 對應 per-core elapsed_s(cumulative across concurrent workers)暴增 5-15×。
-- 沙箱 investigation 揭露最可能 root cause(60% 信心):
--
--   facts 表大量 DELETE+INSERT 後 stats 過期 → query planner 對 facts
--   `uq_facts_dedup` unique index 選錯 plan → ON CONFLICT 路徑掃描變慢
--   → sqlx connection hold time 上升 → concurrent worker queue 形成 →
--   wall time 線性放大
--
-- 適用場景:
--   1. 每次 Round N calibration 後手動 DELETE + 重跑 tw_cores 前後跑
--   2. 每月一次 routine maintenance(autovacuum 未及時 trigger 時)
--   3. wall time 反常變慢時優先跑(便宜 ~ 50-200ms ANALYZE,可能省幾分鐘)
--
-- 跑法:
--   psql $env:DATABASE_URL -f scripts/maintain_facts_stats.sql
--
-- ============================================================================

\timing on

-- ────────────────────────────────────────────────────────────────────────────
-- Phase 1: pre-maintenance stats(留 baseline 對照)
-- ────────────────────────────────────────────────────────────────────────────

\echo
\echo === Phase 1: pre-maintenance stats ===

SELECT
    relname,
    n_live_tup,
    n_dead_tup,
    ROUND(100.0 * n_dead_tup / NULLIF(n_live_tup + n_dead_tup, 0), 2) AS dead_pct,
    last_vacuum,
    last_autovacuum,
    last_analyze,
    last_autoanalyze
FROM pg_stat_user_tables
WHERE relname IN ('facts', 'indicator_values', 'structural_snapshots')
ORDER BY n_live_tup DESC;

-- ────────────────────────────────────────────────────────────────────────────
-- Phase 2: ANALYZE 三張 M3 寫入表(便宜 ~50-200ms,planner stats refresh)
-- ────────────────────────────────────────────────────────────────────────────

\echo
\echo === Phase 2: ANALYZE M3 tables(stats refresh)===

ANALYZE facts;
ANALYZE indicator_values;
ANALYZE structural_snapshots;

-- ────────────────────────────────────────────────────────────────────────────
-- Phase 3: VACUUM dead tuples(若 dead_pct > 10% 才值得跑,看 Phase 1 輸出)
--
-- 註:VACUUM 不 reclaim disk space(那是 VACUUM FULL,會 lock 表,不適合
-- production 直接跑)。VACUUM 只回收 row visibility 給後續 INSERT 重用。
-- ────────────────────────────────────────────────────────────────────────────

\echo
\echo === Phase 3: VACUUM(reclaim dead tuples for reuse,不 lock 表)===

VACUUM (VERBOSE, ANALYZE) facts;
VACUUM (VERBOSE, ANALYZE) indicator_values;

-- ────────────────────────────────────────────────────────────────────────────
-- Phase 4: post-maintenance stats(對照 Phase 1)
-- ────────────────────────────────────────────────────────────────────────────

\echo
\echo === Phase 4: post-maintenance stats(對照 Phase 1)===

SELECT
    relname,
    n_live_tup,
    n_dead_tup,
    ROUND(100.0 * n_dead_tup / NULLIF(n_live_tup + n_dead_tup, 0), 2) AS dead_pct,
    last_vacuum,
    last_analyze
FROM pg_stat_user_tables
WHERE relname IN ('facts', 'indicator_values', 'structural_snapshots')
ORDER BY n_live_tup DESC;

-- ────────────────────────────────────────────────────────────────────────────
-- Phase 5: index 健康度(uq_facts_dedup 是 ON CONFLICT 主路徑,最關鍵)
-- ────────────────────────────────────────────────────────────────────────────

\echo
\echo === Phase 5: index health(unique dedup index 最關鍵)===

SELECT
    schemaname,
    relname AS table_name,
    indexrelname AS index_name,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch,
    pg_size_pretty(pg_relation_size(indexrelid)) AS index_size
FROM pg_stat_user_indexes
WHERE relname IN ('facts', 'indicator_values', 'structural_snapshots')
ORDER BY relname, idx_scan DESC;

\echo
\echo === maintenance done ===
\echo (若 dead_pct > 30%% 仍持續,考慮跑 VACUUM FULL 在 maintenance window)
\echo (若 wall time 仍反常,跑 scripts/diagnose_slow_tw_cores.sql 看 pg_stat_activity)
