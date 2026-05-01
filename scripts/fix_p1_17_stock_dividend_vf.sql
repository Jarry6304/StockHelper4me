-- =============================================================================
-- P1-17 一次性修正:stock_dividend 事件的 volume_factor
-- =============================================================================
-- 全修 commit 6/8 配套腳本。
--
-- 背景:
--   field_mapper.py:194-198 對所有 dividend 事件統一寫 volume_factor=1.0,
--   只對純現金 dividend 對。stock_dividend > 0 的事件實際應根據配股率調整
--   (Taiwan 標準面額 10 元:vf = 1 / (1 + stock_div / 10))。
--
--   commit 6 加 post_process._recompute_stock_dividend_vf 修正 forward,
--   但既存歷史資料需要 one-shot SQL 補正(避免重跑全市場 Phase 2)。
--
-- 用法:
--   psql $env:DATABASE_URL -f scripts\fix_p1_17_stock_dividend_vf.sql
--
-- 跑完後:
--   python src\main.py backfill --phases 4
--   psql $env:DATABASE_URL -f scripts\av3_spot_check.sql > av3_after_p1_17.txt 2>&1
--
-- 預期 av3 結果:
--   Test 3 stock_dividend 事件 vol_ratio < 1.0(終於有調整)
--   3363 2026-01-20 stock_div=7.61: vol_ratio ≈ 1/1.761 ≈ 0.568
--   1312 2023-11-28 stock_div=0.42: vol_ratio ≈ 1/1.042 ≈ 0.960
--   3363 2023-10-17 stock_div=2.64: vol_ratio ≈ 1/1.264 ≈ 0.791
-- =============================================================================

\echo ''
\echo '=== Step 1: 修正前狀態 ==='
SELECT
    stock_id, date, event_type,
    cash_dividend, stock_dividend,
    adjustment_factor, volume_factor
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
    adjustment_factor, volume_factor,
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
