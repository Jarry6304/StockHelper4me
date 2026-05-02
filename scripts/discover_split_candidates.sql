\encoding UTF8
-- =============================================================================
-- discover_split_candidates.sql
-- =============================================================================
-- 目的:盤點 av3 Test 4(split / capital_reduction / par_value_change / capital_increase)
--      在你本機 DB 的可用資料,挑出值得 backfill 的股票清單。
--
-- 背景:
--   * split_price / par_value_change API 是 all_market_no_id,只要 Phase 2 跑過,
--     price_adjustment_events 表就有「全市場」歷史事件
--   * 但 av3 Test 4 還要 JOIN price_daily + price_daily_fwd
--   * 若該股票不在現行 stock_list(已下市 / 沒被 backfill),join 回不到 → Test 4 空集合
--
-- 用法:
--   psql $env:DATABASE_URL -f scripts\discover_split_candidates.sql > discover.txt 2>&1
--
-- 看 Step 4 結果決定要把哪些 stock_id 加進 stock_list 重跑 Phase 1+2+3+4
-- =============================================================================

\echo ''
\echo '##############################################################'
\echo '# Step 1: pae 表中 split / par_value / capital_reduction 事件統計'
\echo '##############################################################'

SELECT
    event_type,
    COUNT(*)                          AS event_count,
    COUNT(DISTINCT stock_id)          AS distinct_stocks,
    MIN(date)                         AS earliest,
    MAX(date)                         AS latest
FROM price_adjustment_events
WHERE event_type IN ('split', 'capital_reduction', 'par_value_change', 'capital_increase')
GROUP BY event_type
ORDER BY event_type;


\echo ''
\echo '##############################################################'
\echo '# Step 2: 各事件的代表性 stock_id(每 event_type 取近 10 檔)'
\echo '##############################################################'

WITH ranked AS (
    SELECT
        event_type, stock_id, date,
        ROW_NUMBER() OVER (PARTITION BY event_type ORDER BY date DESC) AS rn
    FROM price_adjustment_events
    WHERE event_type IN ('split', 'capital_reduction', 'par_value_change', 'capital_increase')
)
SELECT event_type, stock_id, date
FROM ranked
WHERE rn <= 10
ORDER BY event_type, date DESC;


\echo ''
\echo '##############################################################'
\echo '# Step 3: 哪些事件對應的 stock_id 在 price_daily 已有資料 (av3 Test 4 join 可成立)'
\echo '##############################################################'

SELECT
    pae.event_type,
    COUNT(*)                                                               AS total_events,
    SUM(CASE WHEN r.stock_id IS NOT NULL THEN 1 ELSE 0 END)               AS price_daily_hit,
    SUM(CASE WHEN f.stock_id IS NOT NULL THEN 1 ELSE 0 END)               AS price_daily_fwd_hit,
    SUM(CASE WHEN r.stock_id IS NOT NULL AND f.stock_id IS NOT NULL
             THEN 1 ELSE 0 END)                                            AS av3_test4_joinable
FROM price_adjustment_events pae
LEFT JOIN price_daily     r ON r.market = pae.market AND r.stock_id = pae.stock_id AND r.date = pae.date
LEFT JOIN price_daily_fwd f ON f.market = pae.market AND f.stock_id = pae.stock_id AND f.date = pae.date
WHERE pae.event_type IN ('split', 'capital_reduction', 'par_value_change', 'capital_increase')
GROUP BY pae.event_type
ORDER BY pae.event_type;


\echo ''
\echo '##############################################################'
\echo '# Step 4: 缺 price_daily 的事件清單(候選 backfill 名單)'
\echo '##############################################################'
\echo '把這份結果的 stock_id 加進 stock_list,跑 Phase 1+2+3+4 後 av3 Test 4 就有資料'
\echo ''

SELECT
    pae.stock_id,
    pae.event_type,
    pae.date,
    pae.volume_factor,
    CASE WHEN r.stock_id IS NULL THEN 'price_daily 缺' ELSE 'OK' END        AS price_daily_status,
    CASE WHEN f.stock_id IS NULL THEN 'price_daily_fwd 缺' ELSE 'OK' END    AS fwd_status
FROM price_adjustment_events pae
LEFT JOIN price_daily     r ON r.market = pae.market AND r.stock_id = pae.stock_id AND r.date = pae.date
LEFT JOIN price_daily_fwd f ON f.market = pae.market AND f.stock_id = pae.stock_id AND f.date = pae.date
WHERE pae.event_type IN ('split', 'capital_reduction', 'par_value_change', 'capital_increase')
  AND (r.stock_id IS NULL OR f.stock_id IS NULL)
ORDER BY pae.event_type, pae.date DESC
LIMIT 50;


\echo ''
\echo '##############################################################'
\echo '# Step 5: 你提到的 6505(台塑化)是否在 pae 表'
\echo '##############################################################'

SELECT
    stock_id, date, event_type,
    cash_dividend, stock_dividend,
    volume_factor
FROM price_adjustment_events
WHERE stock_id = '6505'
ORDER BY date DESC
LIMIT 20;


\echo ''
\echo '##############################################################'
\echo '# Step 6: 6505 是否在 stock_list / 有 price_daily 資料'
\echo '##############################################################'

SELECT
    'stock_info_ref'    AS table_name,
    COUNT(*)            AS rows
FROM stock_info_ref WHERE stock_id = '6505'
UNION ALL
SELECT 'price_daily', COUNT(*) FROM price_daily WHERE stock_id = '6505'
UNION ALL
SELECT 'price_daily_fwd', COUNT(*) FROM price_daily_fwd WHERE stock_id = '6505';


\echo ''
\echo '=============================================================='
\echo '判讀'
\echo '=============================================================='
\echo ''
\echo 'Step 3 av3_test4_joinable < total_events:'
\echo '  → 該 event_type 有事件但對應股票沒被 backfill,Test 4 看不到完整圖'
\echo ''
\echo 'Step 4 列出的 stock_id 就是要加進 stock_list 的候選'
\echo '  把它們的 stock_id 用逗號串起來,跑:'
\echo '    python src\\main.py backfill --stocks <ids> --phases 1,2,3,4'
\echo ''
\echo 'Step 5/6 6505 若 0 筆:可能 FinMind 沒給該股票資料,改挑 Step 4 的其他候選'
