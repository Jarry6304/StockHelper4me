SELECT
    source_core,
    CASE
        WHEN source_core IN ('rsi_core', 'macd_core', 'bollinger_core', 'atr_core', 'adx_core',
                             'kd_core', 'obv_core', 'ma_core')
            THEN SPLIT_PART(statement, ' ', 2)
        WHEN source_core = 'financial_statement_core'
            THEN SPLIT_PART(statement, ' ', 1)
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
    END AS event_kind,
    COUNT(*)                    AS event_count,
    COUNT(DISTINCT stock_id)    AS affected_stocks,
    MIN(fact_date)              AS earliest,
    MAX(fact_date)              AS latest
FROM facts
GROUP BY source_core, event_kind
ORDER BY source_core, event_count DESC;
