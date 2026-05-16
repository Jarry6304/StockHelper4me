-- ============================================================================
-- scripts/verify_event_kind_rate.sql
-- ============================================================================
-- v3.14(2026-05-17):per-EventKind 觸發率 verify SQL,Round N calibration 用。
-- v3.15(2026-05-16):Round 8 calibration 動工 — 3 EventKinds tighten
--   institutional/LargeTransaction:z 2.0 → 2.5(預期 23.49 → ~6.4/yr ✅)
--   foreign_holding/HoldingMilestoneLow:加 MIN_MILESTONE_SPACING_DAYS=10(預期 15.46 → ~8-10/yr ✅)
--   foreign_holding/SignificantSingleDayChange:z 2.0 → 2.1(預期 12.88 → ~10/yr ✅)
--
-- 對齊 v1.32 acceptance 標準:per-stock cores 每 EventKind ≤ 12/yr/stock。
-- 修 B.extra SQL bug:對 distinct_stocks ≤ 5 的 market-level cores(taiex /
-- us_market / exchange_rate / fear_greed / market_margin / business_indicator)
-- 不該用 per-stock-year metric(分母太小,自然超標)— 改用 events/year 評估。
--
-- 跑法:
--   psql $env:DATABASE_URL -f scripts/verify_event_kind_rate.sql
-- ============================================================================

\echo
\echo ============================================================================
\echo Section 1: Per-Stock Cores(distinct_stocks > 5)— events/stock/year ≤ 12
\echo ============================================================================
\echo (對齊 v1.32 acceptance:這是 default noise control 標準)

WITH per_kind AS (
    SELECT source_core,
           metadata->>'event_kind' AS event_kind,
           COUNT(*) AS total_events,
           COUNT(DISTINCT stock_id) AS distinct_stocks,
           (DATE_PART('year', MAX(fact_date)) - DATE_PART('year', MIN(fact_date)) + 1)::numeric AS years_span
    FROM facts
    WHERE metadata ? 'event_kind'
    GROUP BY source_core, metadata->>'event_kind'
)
SELECT source_core,
       event_kind,
       total_events,
       distinct_stocks,
       years_span::int AS years,
       ROUND(total_events::numeric / NULLIF(distinct_stocks * years_span, 0), 2) AS per_stock_year_rate,
       CASE
           WHEN total_events::numeric / NULLIF(distinct_stocks * years_span, 0) > 12.0 THEN '** OVER 12'
           ELSE 'OK'
       END AS status
FROM per_kind
WHERE distinct_stocks > 5
ORDER BY per_stock_year_rate DESC
LIMIT 30;

\echo
\echo ============================================================================
\echo Section 2: Market-Level Cores(distinct_stocks ≤ 5)— events/year 評估
\echo ============================================================================
\echo (taiex / us_market / exchange_rate / fear_greed 等單系列 cores,per-stock metric 不適用)

WITH per_kind AS (
    SELECT source_core,
           metadata->>'event_kind' AS event_kind,
           COUNT(*) AS total_events,
           COUNT(DISTINCT stock_id) AS distinct_stocks,
           (DATE_PART('year', MAX(fact_date)) - DATE_PART('year', MIN(fact_date)) + 1)::numeric AS years_span
    FROM facts
    WHERE metadata ? 'event_kind'
    GROUP BY source_core, metadata->>'event_kind'
)
SELECT source_core,
       event_kind,
       distinct_stocks,
       total_events,
       years_span::int AS years,
       ROUND(total_events::numeric / NULLIF(years_span, 0), 1) AS events_per_year
FROM per_kind
WHERE distinct_stocks <= 5
ORDER BY events_per_year DESC;

\echo
\echo ============================================================================
\echo Section 3: Round-Specific Verify(預設 Round 7 — 5 cores 全部 ≤ 12/yr 達標)
\echo ============================================================================

WITH per_kind AS (
    SELECT source_core,
           metadata->>'event_kind' AS event_kind,
           COUNT(*) AS total_events,
           COUNT(DISTINCT stock_id) AS distinct_stocks,
           (DATE_PART('year', MAX(fact_date)) - DATE_PART('year', MIN(fact_date)) + 1)::numeric AS years_span
    FROM facts
    WHERE source_core IN ('adx_core','atr_core','day_trading_core','margin_core','trendline_core')
      AND metadata ? 'event_kind'
    GROUP BY source_core, metadata->>'event_kind'
)
SELECT source_core,
       event_kind,
       total_events,
       distinct_stocks,
       ROUND(total_events::numeric / NULLIF(distinct_stocks * years_span, 0), 2) AS per_stock_year_rate
FROM per_kind
WHERE distinct_stocks > 0
  AND total_events::numeric / NULLIF(distinct_stocks * years_span, 0) > 12.0
ORDER BY per_stock_year_rate DESC;

\echo (上表 0 row = Round 7 達標)
\echo
