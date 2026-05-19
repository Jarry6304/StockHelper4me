-- _schema_health.sql — Phase 2 內部:M3 表 row counts + cross_cores tables 存在驗證
-- 由 test_pipeline.ps1 / test_pipeline.sh 透過 psql -f 載入(避免 PS1 here-string 解析問題)

\echo '== M3 表 row counts =='
SELECT
  (SELECT COUNT(*) FROM facts)                AS facts_count,
  (SELECT COUNT(*) FROM indicator_values)     AS indicator_values_count,
  (SELECT COUNT(*) FROM structural_snapshots) AS structural_snapshots_count;

\echo ''
\echo '== 11 個 cross_cores tables 存在 =='
SELECT COUNT(*) AS cross_cores_tables_count
FROM pg_tables
WHERE schemaname = 'public'
  AND (tablename LIKE '%\_ranked\_derived' ESCAPE '\'
       OR tablename = 'monthly_trigger_signals_derived');
