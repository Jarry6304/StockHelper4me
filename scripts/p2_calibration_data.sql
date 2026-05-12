-- p2_calibration_data.sql
-- P2 阻塞 5(c) C 類常數校準：每股每年觸發率 + MA period 分析
-- 執行：psql $env:DATABASE_URL -f scripts/p2_calibration_data.sql
-- 或   psql $DATABASE_URL -f scripts/p2_calibration_data.sql
--
-- 輸出共 4 段：
--   §1  總覽：各 EventKind 的 event_count / affected_stocks / 時間跨度
--   §2  每股每年觸發率（C 類常數判斷矩陣）
--   §3  ma_core breakdown：按 MA kind + period 分類（校準 ABOVE_MA_STREAK_MIN scaling）
--   §4  foreign_holding_core：60d vs 252d milestone 比較

-- ============================================================
-- §1  總覽（原 p2_eventkinds.sql 強化版）
-- ============================================================
\echo '=== §1 EventKind 總覽 ==='

SELECT
    source_core,
    CASE
        WHEN source_core IN ('rsi_core', 'macd_core', 'bollinger_core', 'atr_core', 'adx_core',
                             'kd_core', 'obv_core', 'ma_core')
            THEN SPLIT_PART(statement, ' ', 2)
        WHEN source_core = 'financial_statement_core'
            THEN SPLIT_PART(statement, ' ', 1)
        WHEN source_core = 'foreign_holding_core' THEN
            CASE
                WHEN statement LIKE 'Foreign holding 60d high%'  THEN 'HoldingMilestoneHigh'
                WHEN statement LIKE 'Foreign holding 60d low%'   THEN 'HoldingMilestoneLow'
                WHEN statement LIKE 'Foreign holding 252d high%' THEN 'HoldingMilestoneHighAnnual'
                WHEN statement LIKE 'Foreign holding 252d low%'  THEN 'HoldingMilestoneLowAnnual'
                WHEN statement LIKE 'Foreign holding reached%'   THEN 'LimitNearAlert'
                ELSE 'SignificantSingleDayChange'
            END
        WHEN source_core = 'margin_core' THEN
            CASE
                WHEN statement LIKE 'Margin balance up%'                         THEN 'MarginSurge'
                WHEN statement LIKE 'Margin balance down%'                       THEN 'MarginCrash'
                WHEN statement LIKE 'Short balance down%'                        THEN 'ShortSqueeze'
                WHEN statement LIKE 'Short balance up%'                          THEN 'ShortBuildUp'
                WHEN statement LIKE 'Short-to-margin ratio entered ExtremeHigh%' THEN 'EnteredShortRatioExtremeHigh'
                WHEN statement LIKE 'Short-to-margin ratio exited ExtremeHigh%'  THEN 'ExitedShortRatioExtremeHigh'
                WHEN statement LIKE 'Short-to-margin ratio entered ExtremeLow%'  THEN 'EnteredShortRatioExtremeLow'
                WHEN statement LIKE 'Short-to-margin ratio exited ExtremeLow%'   THEN 'ExitedShortRatioExtremeLow'
                WHEN statement LIKE 'Margin maintenance entered%'                THEN 'EnteredMaintenanceLow'
                WHEN statement LIKE 'Margin maintenance exited%'                 THEN 'ExitedMaintenanceLow'
                ELSE SPLIT_PART(statement, ' ', 1)
            END
        WHEN source_core = 'shareholder_core' THEN
            CASE
                WHEN statement LIKE 'Small holders count decreased%'   THEN 'SmallHoldersDecreasing'
                WHEN statement LIKE 'Small holders count increased%'   THEN 'SmallHoldersIncreasing'
                WHEN statement LIKE 'Large holders accumulating%'      THEN 'LargeHoldersAccumulating'
                WHEN statement LIKE 'Large holders reducing%'          THEN 'LargeHoldersReducing'
                WHEN statement LIKE 'Super-large%accumulating%'        THEN 'SuperLargeHoldersAccumulating'
                WHEN statement LIKE 'Super-large%reducing%'            THEN 'SuperLargeHoldersReducing'
                WHEN statement LIKE 'Concentration index up%'          THEN 'ConcentrationRising'
                WHEN statement LIKE 'Concentration index down%'        THEN 'ConcentrationDecreasing'
                ELSE SPLIT_PART(statement, ' ', 1)
            END
        WHEN source_core = 'day_trading_core' THEN
            CASE
                WHEN statement LIKE 'Day trade ratio reached%'         THEN 'RatioExtremeHigh'
                WHEN statement LIKE 'Day trade ratio dropped%'         THEN 'RatioExtremeLow'
                WHEN statement LIKE 'Day trade ratio above%'           THEN 'RatioStreakHigh'
                WHEN statement LIKE 'Day trade ratio below%'           THEN 'RatioStreakLow'
                ELSE SPLIT_PART(statement, ' ', 1)
            END
        ELSE SPLIT_PART(statement, ' ', 1)
    END                              AS event_kind,
    COUNT(*)                         AS event_count,
    COUNT(DISTINCT stock_id)         AS affected_stocks,
    MIN(fact_date)                   AS earliest,
    MAX(fact_date)                   AS latest,
    ROUND(
        (DATE_PART('year', MAX(fact_date)) - DATE_PART('year', MIN(fact_date)) + 1)::numeric,
        0
    )                                AS years_span
FROM facts
GROUP BY source_core, event_kind
ORDER BY source_core, event_count DESC;

-- ============================================================
-- §2  每股每年觸發率（C 類常數判斷矩陣）
-- ============================================================
\echo ''
\echo '=== §2 每股每年觸發率（C 類校準判斷矩陣）==='
\echo '觸發率 < 0.1 = 失靈 | 0.5-3 = 季度訊號合理 | 3-12 = 偏頻繁 | > 20 = 噪音'
\echo ''

WITH classified AS (
    SELECT
        source_core,
        CASE
            WHEN source_core IN ('rsi_core', 'macd_core', 'bollinger_core', 'atr_core', 'adx_core',
                                 'kd_core', 'obv_core', 'ma_core')
                THEN SPLIT_PART(statement, ' ', 2)
            WHEN source_core = 'financial_statement_core'
                THEN SPLIT_PART(statement, ' ', 1)
            WHEN source_core = 'foreign_holding_core' THEN
                CASE
                    WHEN statement LIKE 'Foreign holding 60d high%'  THEN 'HoldingMilestoneHigh'
                    WHEN statement LIKE 'Foreign holding 60d low%'   THEN 'HoldingMilestoneLow'
                    WHEN statement LIKE 'Foreign holding 252d high%' THEN 'HoldingMilestoneHighAnnual'
                    WHEN statement LIKE 'Foreign holding 252d low%'  THEN 'HoldingMilestoneLowAnnual'
                    WHEN statement LIKE 'Foreign holding reached%'   THEN 'LimitNearAlert'
                    ELSE 'SignificantSingleDayChange'
                END
            WHEN source_core = 'margin_core' THEN
                CASE
                    WHEN statement LIKE 'Margin balance up%'    THEN 'MarginSurge'
                    WHEN statement LIKE 'Margin balance down%'  THEN 'MarginCrash'
                    WHEN statement LIKE 'Short balance down%'   THEN 'ShortSqueeze'
                    WHEN statement LIKE 'Short balance up%'     THEN 'ShortBuildUp'
                    ELSE SPLIT_PART(statement, ' ', 1)
                END
            WHEN source_core = 'shareholder_core' THEN
                CASE
                    WHEN statement LIKE 'Small holders count decreased%'   THEN 'SmallHoldersDecreasing'
                    WHEN statement LIKE 'Small holders count increased%'   THEN 'SmallHoldersIncreasing'
                    WHEN statement LIKE 'Large holders accumulating%'      THEN 'LargeHoldersAccumulating'
                    WHEN statement LIKE 'Large holders reducing%'          THEN 'LargeHoldersReducing'
                    WHEN statement LIKE 'Super-large%accumulating%'        THEN 'SuperLargeHoldersAccumulating'
                    WHEN statement LIKE 'Super-large%reducing%'            THEN 'SuperLargeHoldersReducing'
                    WHEN statement LIKE 'Concentration index up%'          THEN 'ConcentrationRising'
                    WHEN statement LIKE 'Concentration index down%'        THEN 'ConcentrationDecreasing'
                    ELSE SPLIT_PART(statement, ' ', 1)
                END
            WHEN source_core = 'day_trading_core' THEN
                CASE
                    WHEN statement LIKE 'Day trade ratio above%'  THEN 'RatioStreakHigh'
                    WHEN statement LIKE 'Day trade ratio below%'  THEN 'RatioStreakLow'
                    ELSE SPLIT_PART(statement, ' ', 1)
                END
            ELSE SPLIT_PART(statement, ' ', 1)
        END AS event_kind,
        stock_id,
        fact_date
    FROM facts
    -- 只含 C 類常數相關 cores
    WHERE source_core IN (
        'rsi_core', 'kd_core', 'ma_core', 'bollinger_core',
        'day_trading_core', 'shareholder_core', 'foreign_holding_core',
        'macd_core', 'institutional_core'
    )
),
agg AS (
    SELECT
        source_core,
        event_kind,
        COUNT(*)                    AS event_count,
        COUNT(DISTINCT stock_id)    AS affected_stocks,
        MIN(fact_date)              AS earliest,
        MAX(fact_date)              AS latest,
        (DATE_PART('year', MAX(fact_date)) - DATE_PART('year', MIN(fact_date)) + 1) AS years_span
    FROM classified
    GROUP BY source_core, event_kind
)
SELECT
    source_core,
    event_kind,
    event_count,
    affected_stocks,
    years_span::int                          AS years,
    ROUND(
        event_count::numeric
        / NULLIF(affected_stocks * years_span, 0),
        2
    )                                        AS avg_per_stock_per_year,
    CASE
        WHEN event_count::numeric / NULLIF(affected_stocks * years_span, 0) < 0.1  THEN '🔴 失靈'
        WHEN event_count::numeric / NULLIF(affected_stocks * years_span, 0) < 0.5  THEN '🟡 稀少'
        WHEN event_count::numeric / NULLIF(affected_stocks * years_span, 0) <= 6   THEN '🟢 合理'
        WHEN event_count::numeric / NULLIF(affected_stocks * years_span, 0) <= 20  THEN '🟠 偏頻'
        ELSE                                                                             '🔴 噪音'
    END                                      AS signal_quality
FROM agg
ORDER BY source_core, avg_per_stock_per_year DESC NULLS LAST;

-- ============================================================
-- §3  ma_core AboveMaStreak — 按 MA period 分析
-- ============================================================
\echo ''
\echo '=== §3 ma_core AboveMaStreak：按 MA period + kind 分析 ==='
\echo '用於判斷 above_ma_streak_min(period) = max(3, period/8) 是否合適'
\echo ''

SELECT
    (metadata->>'period')::int       AS ma_period,
    metadata->>'ma_kind'             AS ma_kind,
    COUNT(*)                         AS event_count,
    COUNT(DISTINCT stock_id)         AS affected_stocks,
    ROUND(AVG((metadata->>'days')::numeric), 1)   AS avg_streak_days,
    MIN((metadata->>'days')::int)                 AS min_streak_days,
    MAX((metadata->>'days')::int)                 AS max_streak_days,
    -- scaling fn 建議值
    GREATEST(3, (metadata->>'period')::int / 8)   AS scaling_fn_min
FROM facts
WHERE source_core = 'ma_core'
  AND statement LIKE '%AboveMaStreak%'
GROUP BY ma_period, ma_kind, scaling_fn_min
ORDER BY ma_period, ma_kind;

-- ============================================================
-- §4  foreign_holding_core：60d vs 252d milestone 觸發率比較
-- ============================================================
\echo ''
\echo '=== §4 foreign_holding_core：季新高(60d) vs 年新高(252d) 觸發率比較 ==='
\echo '年新高觸發率極低 → 252d 訊號更稀有更有價值'
\echo ''

SELECT
    CASE
        WHEN statement LIKE 'Foreign holding 60d high%'  THEN 'HoldingMilestoneHigh_60d'
        WHEN statement LIKE 'Foreign holding 60d low%'   THEN 'HoldingMilestoneLow_60d'
        WHEN statement LIKE 'Foreign holding 252d high%' THEN 'HoldingMilestoneHighAnnual_252d'
        WHEN statement LIKE 'Foreign holding 252d low%'  THEN 'HoldingMilestoneLowAnnual_252d'
        ELSE 'Other'
    END                              AS event_kind,
    COUNT(*)                         AS event_count,
    COUNT(DISTINCT stock_id)         AS affected_stocks,
    MIN(fact_date)                   AS earliest,
    MAX(fact_date)                   AS latest
FROM facts
WHERE source_core = 'foreign_holding_core'
  AND (
      statement LIKE 'Foreign holding 60d%'
      OR statement LIKE 'Foreign holding 252d%'
  )
GROUP BY event_kind
ORDER BY event_kind;

-- ============================================================
-- §5  C 類常數快速參考卡（供 session 回顧用）
-- ============================================================
\echo ''
\echo '=== §5 C 類常數變更快速參考 ==='
\echo 'C-1  STREAK_MIN_DAYS = 3       rsi/kd/day_trading   → 保留（無更好學術根據）'
\echo 'C-2  ABOVE_MA_STREAK_MIN       ma_core              → 已改 scaling fn max(3, period/8)'
\echo '     MA5/10/20→3d MA60→8d MA120→15d MA240→30d'
\echo 'C-3  SQUEEZE_STREAK_MIN = 5    bollinger_core        → 保留（Bollinger level-based）'
\echo 'C-4  STREAK_MIN_WEEKS = 8      shareholder_core      → 保留（待 §2 觸發率驗證）'
\echo 'C-5  DIV_MIN_BARS = 20         macd/rsi/kd           → 保留（Murphy 20-60 下界）'
\echo 'C-6  MILESTONE_LOOKBACK        foreign_holding_core  → 已改 60d(季) + 252d(年) 雙軌'
\echo 'C-7  LOOKBACK_FOR_Z = 60       institutional_core    → 保留（3 個月 baseline）'
\echo 'C-8  LARGE_TRANSACTION_Z = 2.0 institutional_core   → 保留（統計 2σ 通用標準）'
