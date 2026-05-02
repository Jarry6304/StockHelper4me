-- =============================================================================
-- DEPRECATED — P1-17 任務已由 post_process 完成,此 one-shot SQL 不再需要
-- =============================================================================
-- 歷史背景(保留作為事件考古):
--   field_mapper.py:194-198 對所有 dividend 事件統一寫 volume_factor=1.0,
--   只對純現金 dividend 對。stock_dividend > 0 的事件實際應根據配股率調整
--   (Taiwan 標準面額 10 元:vf = 1 / (1 + stock_div / 10))。
--
-- 修正路徑(已落地):
--   1. post_process._recompute_stock_dividend_vf(commit `608d275`)
--      在 Phase 2 dividend_policy_merge 即時修正每筆寫入的 vf
--   2. PR #17 後 price_adjustment_events 重整(adjustment_factor 欄位移到
--      price_daily_fwd,pae 只留 volume_factor + before/after_price)
--   3. av3 全綠:av3_after_p1_17.txt 顯示 Test 3 stock_dividend 事件
--      vol_ratio 對齊 1/(1 + stock_div/10) 累積結果
--
-- 為何 deprecated:
--   * 此 SQL 對 PR #17 後的 schema 跑會 ERROR("adjustment_factor does not
--     exist"),因為 SELECT 列引用已被砍的 pae.adjustment_factor 欄位
--   * 即便修 SELECT 對齊新 schema,UPDATE 語句也會 0 row affected
--     (post_process 已在 Phase 2 path 修對全部既有資料)
--   * 留檔為了未來若再遇到類似 collector field_mapper bug 時可參考此
--     one-shot 補丁的寫法
--
-- 若你看到這份 SQL 並想跑它:停下來。先跑 av3_spot_check.sql 確認
-- Test 3 vol_ratio 是否已對。若已對 → 不需動。若沒對 → 你的 PG 環境跟
-- 主線分支不同步,先 git pull + alembic upgrade head + 全市場 Phase 2 重跑。
-- =============================================================================

\echo ''
\echo '=== Step 1: 修正前狀態 ==='
SELECT
    stock_id, date, event_type,
    cash_dividend, stock_dividend,
    volume_factor
FROM price_adjustment_events
WHERE event_type = 'dividend'
  AND COALESCE(stock_dividend, 0) > 0
  AND ABS(volume_factor - 1.0) < 0.0001
ORDER BY stock_id, date;


\echo ''
\echo '=== Step 2: 套用修正(vf = 1 / (1 + stock_div / 10)) ==='
UPDATE price_adjustment_events
   SET volume_factor = 1.0 / (1.0 + stock_dividend / 10.0)
 WHERE event_type = 'dividend'
   AND COALESCE(stock_dividend, 0) > 0
   AND ABS(volume_factor - 1.0) < 0.0001;


\echo ''
\echo '=== Step 3: 修正後狀態 ==='
SELECT
    stock_id, date, event_type,
    cash_dividend, stock_dividend,
    volume_factor,
    ROUND((volume_factor - (1.0 / (1.0 + stock_dividend / 10.0)))::numeric, 6) AS check_diff
FROM price_adjustment_events
WHERE event_type = 'dividend'
  AND COALESCE(stock_dividend, 0) > 0
ORDER BY stock_id, date;


\echo ''
\echo '=== Step 4: 觸發 Phase 4 重算受影響的股票 ==='
INSERT INTO stock_sync_status (market, stock_id, fwd_adj_valid)
SELECT DISTINCT market, stock_id, 0
  FROM price_adjustment_events
 WHERE event_type = 'dividend'
   AND COALESCE(stock_dividend, 0) > 0
ON CONFLICT (market, stock_id) DO UPDATE SET fwd_adj_valid = 0;


\echo ''
\echo '=== Step 5: 影響範圍統計 ==='
SELECT
    COUNT(DISTINCT stock_id)                                      AS affected_stocks,
    COUNT(*)                                                      AS affected_events,
    MIN(date)                                                     AS earliest_event,
    MAX(date)                                                     AS latest_event
FROM price_adjustment_events
WHERE event_type = 'dividend'
  AND COALESCE(stock_dividend, 0) > 0;


\echo ''
\echo '=============================================================='
\echo '修正完成。下一步:'
\echo '  cd C:\Users\jarry\source\repos\StockHelper4me'
\echo '  python src\main.py backfill --phases 4'
\echo '  psql $env:DATABASE_URL -f scripts\av3_spot_check.sql > av3_after_p1_17.txt 2>&1'
\echo '=============================================================='
