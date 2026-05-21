-- scripts/diag_business_indicator.sql
-- business_indicator_core empty-series 診斷(v4.12 follow-up)
-- 用法:psql $env:DATABASE_URL -f scripts/diag_business_indicator.sql
--
-- 對齊 m3Spec/business_indicator_core_fix_plan.md §三。一次涵蓋全部候選根因:
--   (2)(3) → loader 撈不到 row(stock_id 不符 / date 太舊)
--   (1)(6) → 撈到了但某欄全 NULL(compute filter_map 一個 None 丟整點)
--   (4)(7) → 欄位實際字串值

\echo '=== (1) Silver business_indicator_derived 全貌 ==='
SELECT COUNT(*) AS rows, MIN(date) AS min_d, MAX(date) AS max_d,
       COUNT(leading_indicator)    AS n_leading,
       COUNT(coincident_indicator) AS n_coincident,
       COUNT(lagging_indicator)    AS n_lagging,
       COUNT(monitoring)           AS n_monitoring,
       COUNT(monitoring_color)     AS n_color
FROM business_indicator_derived;

\echo ''
\echo '=== (2) Silver distinct stock_id (loader 寫死 WHERE stock_id = _market_) ==='
SELECT DISTINCT stock_id FROM business_indicator_derived;

\echo ''
\echo '=== (3) 通過 loader filter 的 row 數 (stock_id=_market_ + 近 1825 天) ==='
SELECT COUNT(*) AS rows_passing_filter
FROM business_indicator_derived
WHERE stock_id = '_market_' AND date >= CURRENT_DATE - 1825;

\echo ''
\echo '=== (4) monitoring_color 實際值分佈 ==='
SELECT monitoring_color, COUNT(*) AS n
FROM business_indicator_derived
GROUP BY 1 ORDER BY 2 DESC;

\echo ''
\echo '=== (5) Silver 最新 3 筆完整 row ==='
SELECT * FROM business_indicator_derived ORDER BY date DESC LIMIT 3;

\echo ''
\echo '=== (6) Bronze business_indicator_tw 對照 ==='
SELECT COUNT(*) AS rows, MIN(date) AS min_d, MAX(date) AS max_d,
       COUNT(leading_indicator)    AS n_leading,
       COUNT(coincident_indicator) AS n_coincident,
       COUNT(lagging_indicator)    AS n_lagging,
       COUNT(monitoring)           AS n_monitoring,
       COUNT(monitoring_color)     AS n_color
FROM business_indicator_tw;

\echo ''
\echo '=== (7) Bronze 最新 3 筆 + detail JSONB (揭露 FinMind 原始欄名) ==='
SELECT date, leading_indicator, coincident_indicator, lagging_indicator,
       monitoring, monitoring_color, detail
FROM business_indicator_tw ORDER BY date DESC LIMIT 3;
