-- ============================================================================
-- scripts/diagnose_slow_tw_cores.sql
-- ============================================================================
-- v3.19(2026-05-17):tw_cores 慢時 diagnostic 取樣。
--
-- 在 tw_cores `run-all` 跑期間另開 psql 連線跑,可取得即時 worker / lock /
-- query plan 證據。對齊「per-core elapsed_s 暴增為 sqlx pool 阻塞」假設。
--
-- 跑法:tw_cores 開跑後 30 秒,另開終端:
--   psql $env:DATABASE_URL -f scripts/diagnose_slow_tw_cores.sql
-- ============================================================================

-- ────────────────────────────────────────────────────────────────────────────
-- Phase 1: pg_stat_activity 看 active session(sqlx 連線實況)
-- ────────────────────────────────────────────────────────────────────────────

\echo === Phase 1: active sessions(sqlx connection pool usage)===

SELECT
    pid,
    usename,
    state,
    wait_event_type,
    wait_event,
    EXTRACT(EPOCH FROM (NOW() - state_change))::int AS state_secs,
    EXTRACT(EPOCH FROM (NOW() - query_start))::int AS query_secs,
    LEFT(query, 100) AS query_head
FROM pg_stat_activity
WHERE datname = current_database()
  AND state IS NOT NULL
  AND state != 'idle'
ORDER BY query_start;

-- ────────────────────────────────────────────────────────────────────────────
-- Phase 2: lock contention(若 active 中很多 wait_event 屬 Lock,問題在此)
-- ────────────────────────────────────────────────────────────────────────────

\echo
\echo === Phase 2: lock waits ===

SELECT
    blocked.pid AS blocked_pid,
    blocked.query AS blocked_query,
    blocking.pid AS blocking_pid,
    blocking.query AS blocking_query,
    blocked.wait_event_type,
    blocked.wait_event
FROM pg_locks AS bl
JOIN pg_stat_activity AS blocked ON blocked.pid = bl.pid
JOIN pg_locks AS bg ON bg.transactionid = bl.transactionid AND bg.pid != bl.pid
JOIN pg_stat_activity AS blocking ON blocking.pid = bg.pid
WHERE NOT bl.granted;

-- ────────────────────────────────────────────────────────────────────────────
-- Phase 3: facts 表 ON CONFLICT 路徑 query plan(模擬 tw_cores upsert 行為)
--
-- 替換 stock_id / fact_date / source_core / params_hash 為你 production
-- 跑期間實際看到的值(從 Phase 1 取樣)。
-- ────────────────────────────────────────────────────────────────────────────

\echo
\echo === Phase 3: facts dedup query plan(替換實際值)===

EXPLAIN (ANALYZE, BUFFERS, TIMING)
SELECT 1
FROM facts
WHERE stock_id = '2330'
  AND fact_date = '2026-05-15'
  AND timeframe = 'daily'
  AND source_core = 'foreign_holding_core'
  AND params_hash = 'PLACEHOLDER_REPLACE_WITH_ACTUAL_HASH'
LIMIT 1;

-- ────────────────────────────────────────────────────────────────────────────
-- Phase 4: connection pool 滿載診斷(若 active session 數 == pool max,
-- 則 sqlx 在排隊等 connection,wall time 線性放大)
-- ────────────────────────────────────────────────────────────────────────────

\echo
\echo === Phase 4: connection pool saturation ===

SELECT
    COUNT(*) FILTER (WHERE state = 'active') AS active_sessions,
    COUNT(*) FILTER (WHERE state = 'idle in transaction') AS idle_in_tx,
    COUNT(*) FILTER (WHERE state = 'idle') AS idle_sessions,
    COUNT(*) AS total_sessions
FROM pg_stat_activity
WHERE datname = current_database();

-- ────────────────────────────────────────────────────────────────────────────
-- 解讀指南
--
-- 觀察組合 1:Phase 1 active session 多 + wait_event 多為 IO/Lock
--   → 假設 1:facts stats 過期或 dead tuples 多 → 跑 maintain_facts_stats.sql
--
-- 觀察組合 2:Phase 4 active_sessions == pool max(concurrency+4=36)
--   → 假設 2:sqlx pool 飽和 → 加大 pool size 或降 concurrency
--
-- 觀察組合 3:Phase 3 EXPLAIN 顯示 Seq Scan facts(應該 Index Scan)
--   → 假設 1 確認 → 跑 ANALYZE facts; 再驗
--
-- 觀察組合 4:Phase 2 blocking pid 集中在 1-2 個 query
--   → 假設 3:特定 query 鎖表(可能 DDL / VACUUM FULL),取 query_head 排查
-- ────────────────────────────────────────────────────────────────────────────
