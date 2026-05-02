\encoding UTF8
-- =============================================================================
-- A-V3 spot-check: price_daily_fwd.volume 後復權行為驗證
-- =============================================================================
-- Windows PowerShell 中文亂碼:跑前先 chcp 65001 切到 UTF-8 console codepage
--   chcp 65001
--   psql $env:DATABASE_URL -f scripts\av3_spot_check.sql
-- =============================================================================
-- 目的:r2-1 動工項;驗證 collector / Rust 對 volume 後復權的實際行為,
--      解 P0-2 + P0-8(C1)+ blueprint §4.4 條件 ALTER 決策。
--
-- 重要發現(本腳本要揭露的矛盾):
--   * field_mapper.py:194-203 寫:純現金 dividend → volume_factor = 1.0(不動 volume)
--   * rust_compute/src/main.rs:447 寫:對所有事件 fwd_volume = raw_volume / multiplier
--   → 兩個邏輯互相不知道對方,實際 DB 行為由 Rust 決定(因為 Rust 是寫 fwd 表的)
--
-- 三派立場:
--   (a) 學術派(總報酬指數法): close × AF + volume / AF → dollar_vol 守恆 ← Rust 派
--   (b) 實務派: 現金 dividend 對 close ×AF 但 volume 不動 ← collector field_mapper 派
--   (c) 純粹派: close 乘 AF 但 volume 完全不動(反映實際流動性) ← OBV/VWAP 等指標常用
--
-- 用法:
--   psql $env:DATABASE_URL -f scripts\av3_spot_check.sql > av3_result.txt
--   或互動執行:psql $env:DATABASE_URL -f scripts\av3_spot_check.sql
--
-- =============================================================================

\echo ''
\echo '##############################################################'
\echo '# Test 1: 2330 已知 4 個黃金日(CLAUDE.md 後復權驗證資料對齊)'
\echo '##############################################################'
\echo '預期 close_ratio = fwd/raw,跟 CLAUDE.md 表對得上'
\echo '預期 vol_ratio = fwd/raw:'
\echo '  若 = 1.0 → volume 沒動(實務派)'
\echo '  若 = raw/fwd_close 的反比(即 1/close_ratio)→ Rust 派(volume / multiplier)'
\echo ''

SELECT
    r.date,
    r.close                                                       AS raw_close,
    f.close                                                       AS fwd_close,
    ROUND((f.close / NULLIF(r.close, 0))::numeric, 4)             AS close_ratio,
    r.volume                                                      AS raw_vol,
    f.volume                                                      AS fwd_vol,
    ROUND((f.volume::numeric / NULLIF(r.volume, 0)), 6)           AS vol_ratio,
    ROUND(((f.close * f.volume) / NULLIF(r.close * r.volume, 0))::numeric, 4)
                                                                  AS dollar_vol_preserved,
    CASE
        WHEN ABS((f.close / NULLIF(r.close, 0)) - 1.0) < 0.0005
         AND ABS((f.volume::numeric / NULLIF(r.volume, 0)) - 1.0) < 0.0005
            THEN 'no-adj-day(最新日或無事件日)'
        WHEN ABS((f.close * f.volume - r.close * r.volume)::numeric
                 / NULLIF(r.close * r.volume, 0)) < 0.005
            THEN '(a) Rust 派 — dollar_vol 守恆 → volume / AF'
        WHEN ABS((f.volume::numeric / NULLIF(r.volume, 0)) - 1.0) < 0.005
            THEN '(b)/(c) volume 不動 → 只動 OHLC'
        ELSE 'WEIRD — 需 deeper check'
    END                                                           AS verdict
FROM price_daily r
JOIN price_daily_fwd f USING (market, stock_id, date)
WHERE r.market = 'TW'
  AND r.stock_id = '2330'
  AND r.date IN ('2019-01-02', '2022-03-15', '2022-03-16', '2026-04-24')
ORDER BY r.date;


\echo ''
\echo '##############################################################'
\echo '# Test 2: 2330 全部除權息事件的當日 volume 行為'
\echo '##############################################################'
\echo '對純現金 dividend(stock_dividend = 0):'
\echo '  Rust 派:fwd_vol < raw_vol (vol_ratio = 1/AF)'
\echo '  實務派:fwd_vol = raw_vol (vol_ratio = 1.0)'
\echo ''

SELECT
    pae.date                                                      AS event_date,
    pae.event_type,
    COALESCE(pae.cash_dividend, 0)                                AS cash_div,
    COALESCE(pae.stock_dividend, 0)                               AS stock_div,
    f.adjustment_factor                                           AS af_in_fwd,
    pae.volume_factor                                             AS vf_in_pae,
    r.close                                                       AS raw_close,
    f.close                                                       AS fwd_close,
    ROUND((f.close / NULLIF(r.close, 0))::numeric, 4)             AS close_ratio,
    r.volume                                                      AS raw_vol,
    f.volume                                                      AS fwd_vol,
    ROUND((f.volume::numeric / NULLIF(r.volume, 0)), 6)           AS vol_ratio,
    CASE
        -- 除權息日當天 fwd = raw(Rust「先 push 再更新 multiplier」設計)
        WHEN ABS((f.close / NULLIF(r.close, 0)) - 1.0) < 0.0005
         AND ABS((f.volume::numeric / NULLIF(r.volume, 0)) - 1.0) < 0.0005
            THEN 'no-adj-day(除權息日當天/最新日)'
        -- dollar_vol 守恆 → Rust 派
        WHEN ABS(((f.close * f.volume - r.close * r.volume))::numeric
                  / NULLIF(r.close * r.volume, 0)) < 0.001
            THEN '✓ Rust 派(volume / AF, dollar_vol 守恆)'
        -- volume 不動 + stock_dividend > 0 → P1-17 殘餘
        WHEN ABS((f.volume::numeric / NULLIF(r.volume, 0)) - 1.0) < 0.001
         AND COALESCE(pae.stock_dividend, 0) > 0
            THEN '⚠ P1-17:stock_div 但 volume 沒調(field_mapper bug)'
        -- volume 不動 + 純現金 div → field_mapper 派
        WHEN ABS((f.volume::numeric / NULLIF(r.volume, 0)) - 1.0) < 0.001
         AND COALESCE(pae.cash_dividend, 0) > 0
            THEN '✓ field_mapper 派(cash div volume 不動)'
        -- 股本變動事件 volume 已調
        WHEN COALESCE(pae.stock_dividend, 0) > 0
         OR pae.event_type IN ('split', 'capital_reduction', 'par_value_change', 'capital_increase')
            THEN '股本變動事件 volume 已 / vf'
        ELSE '?'
    END                                                           AS faction_verdict
FROM price_adjustment_events pae
JOIN price_daily r       ON r.market = pae.market AND r.stock_id = pae.stock_id AND r.date = pae.date
JOIN price_daily_fwd f   ON f.market = r.market   AND f.stock_id = r.stock_id   AND f.date = r.date
WHERE pae.market = 'TW' AND pae.stock_id = '2330'
ORDER BY pae.date;


\echo ''
\echo '##############################################################'
\echo '# Test 3: 找全市場有 stock_dividend > 0 的事件(volume 調整最敏感)'
\echo '##############################################################'
\echo '對股票股利,vol_ratio 應 ≈ 1/(1+stock_dividend/10)'
\echo '若 vol_ratio = 1.0 → 連股本變動都沒動 volume(嚴重 bug)'
\echo ''

SELECT
    pae.market, pae.stock_id, pae.date AS event_date, pae.event_type,
    pae.cash_dividend, pae.stock_dividend,
    f.adjustment_factor                                           AS af_in_fwd,
    pae.volume_factor                                             AS vf_in_pae,
    r.volume                                                      AS raw_vol,
    f.volume                                                      AS fwd_vol,
    ROUND((f.volume::numeric / NULLIF(r.volume, 0)), 6)           AS vol_ratio,
    ROUND((f.close / NULLIF(r.close, 0))::numeric, 4)             AS close_ratio
FROM price_adjustment_events pae
JOIN price_daily r       ON r.market = pae.market AND r.stock_id = pae.stock_id AND r.date = pae.date
JOIN price_daily_fwd f   ON f.market = r.market   AND f.stock_id = r.stock_id   AND f.date = r.date
WHERE pae.event_type = 'dividend'
  AND COALESCE(pae.stock_dividend, 0) > 0
ORDER BY pae.date DESC
LIMIT 10;


\echo ''
\echo '##############################################################'
\echo '# Test 4: split / capital_reduction / capital_increase 事件'
\echo '##############################################################'
\echo '這些是「股本變動」事件,volume 一定要調(兩派都同意)'
\echo ''

SELECT
    pae.market, pae.stock_id, pae.date AS event_date, pae.event_type,
    f.adjustment_factor                                           AS af_in_fwd,
    pae.volume_factor                                             AS vf_in_pae,
    r.close                                                       AS raw_close,
    f.close                                                       AS fwd_close,
    ROUND((f.close / NULLIF(r.close, 0))::numeric, 4)             AS close_ratio,
    r.volume                                                      AS raw_vol,
    f.volume                                                      AS fwd_vol,
    ROUND((f.volume::numeric / NULLIF(r.volume, 0)), 6)           AS vol_ratio
FROM price_adjustment_events pae
JOIN price_daily r       ON r.market = pae.market AND r.stock_id = pae.stock_id AND r.date = pae.date
JOIN price_daily_fwd f   ON f.market = r.market   AND f.stock_id = r.stock_id   AND f.date = r.date
WHERE pae.event_type IN ('split', 'capital_reduction', 'capital_increase')
ORDER BY pae.event_type, pae.date DESC
LIMIT 20;


\echo ''
\echo '##############################################################'
\echo '# Test 5: PR #17 後 fwd 4 個新欄是否寫入有值(per event_type)'
\echo '##############################################################'
\echo 'fwd 表新欄:cumulative_adjustment_factor(累積 AF)/ cumulative_volume_factor'
\echo '         (累積 vf)/ is_adjusted(該日是否動過)/ adjustment_factor(單日 AF)'
\echo '預期:事件日當天 fwd.adjustment_factor != 1.0,且 pae.volume_factor 不等於它(P0-11 後拆兩 multiplier)'
\echo ''

SELECT
    pae.event_type,
    COUNT(*)                                                      AS event_count,
    SUM(CASE WHEN f.is_adjusted = TRUE THEN 1 ELSE 0 END)         AS days_marked_adjusted,
    SUM(CASE WHEN f.adjustment_factor IS NOT NULL THEN 1 ELSE 0 END)
                                                                  AS days_with_af_written,
    ROUND(AVG(f.adjustment_factor)::numeric, 4)                   AS avg_fwd_af,
    ROUND(AVG(pae.volume_factor)::numeric, 4)                     AS avg_pae_vf,
    SUM(CASE WHEN ABS(f.adjustment_factor - pae.volume_factor) >= 0.0001 THEN 1 ELSE 0 END)
                                                                  AS days_af_diff_vf
FROM price_adjustment_events pae
JOIN price_daily_fwd f
    ON f.market = pae.market AND f.stock_id = pae.stock_id AND f.date = pae.date
WHERE f.adjustment_factor IS NOT NULL AND f.adjustment_factor != 1.0
GROUP BY pae.event_type
ORDER BY pae.event_type;

\echo ''
\echo '# Test 5b: 4 個新欄全表 sanity(整體寫入率,不只事件日)'
\echo ''

SELECT
    COUNT(*)                                                      AS total_fwd_rows,
    SUM(CASE WHEN cumulative_adjustment_factor IS NOT NULL THEN 1 ELSE 0 END)
                                                                  AS rows_with_cum_af,
    SUM(CASE WHEN cumulative_volume_factor IS NOT NULL THEN 1 ELSE 0 END)
                                                                  AS rows_with_cum_vf,
    SUM(CASE WHEN is_adjusted = TRUE THEN 1 ELSE 0 END)           AS rows_marked_adjusted,
    SUM(CASE WHEN adjustment_factor IS NOT NULL THEN 1 ELSE 0 END)
                                                                  AS rows_with_single_af,
    -- sanity:cum_af 應該對 2330 是 1.0822(2019-01-02 起點)
    ROUND(MAX(cumulative_adjustment_factor)::numeric, 4)          AS max_cum_af,
    ROUND(MIN(CASE WHEN cumulative_adjustment_factor != 0 THEN cumulative_adjustment_factor END)::numeric, 4)
                                                                  AS min_cum_af
FROM price_daily_fwd
WHERE market = 'TW' AND stock_id = '2330';


\echo ''
\echo '##############################################################'
\echo '# Test 6: 2330 最新日(應 ratio = 1.0,sanity check)'
\echo '##############################################################'

WITH latest AS (
    SELECT MAX(date) AS d FROM price_daily WHERE market='TW' AND stock_id='2330'
)
SELECT
    r.date,
    r.close                                                       AS raw_close,
    f.close                                                       AS fwd_close,
    ROUND((f.close / NULLIF(r.close, 0))::numeric, 6)             AS close_ratio,
    r.volume                                                      AS raw_vol,
    f.volume                                                      AS fwd_vol,
    ROUND((f.volume::numeric / NULLIF(r.volume, 0)), 6)           AS vol_ratio,
    CASE
        WHEN ABS((f.close / NULLIF(r.close, 0)) - 1.0) < 0.0005
         AND ABS((f.volume::numeric / NULLIF(r.volume, 0)) - 1.0) < 0.0005
            THEN '✓ sanity OK'
        ELSE '✗ FAIL — 最新日不該有差異'
    END                                                           AS sanity
FROM latest l
JOIN price_daily r     ON r.date = l.d AND r.market='TW' AND r.stock_id='2330'
JOIN price_daily_fwd f ON f.date = l.d AND f.market='TW' AND f.stock_id='2330';


\echo ''
\echo '=============================================================='
\echo '判讀指南(PR #17 後 r3.1 + P0-11 + P1-17 落地版)'
\echo '=============================================================='
\echo ''
\echo '【Test 1】2330 4 個關鍵日 close_ratio 對齊 CLAUDE.md 預期(1.0822/1.0822/1.0769/1.0000):'
\echo '  ✅ 通過 = 後復權主邏輯 + cum_af 倒推全對'
\echo ''
\echo '【Test 2】2330 17 個現金 dividend events 全 vol_ratio = 1.0:'
\echo '  ✅ 通過 = P0-11 後 convention 對(現金 dividend vf=1 → fwd_vol = raw_vol,'
\echo '         反映實際 share 流動性,不再強守 dollar_vol invariant)'
\echo ''
\echo '【Test 3】stock_dividend > 0 事件 vol_ratio < 1.0 ≈ 1/(1+stock_div/10):'
\echo '  ✅ 通過 = P1-17 修法生效(post_process._recompute_stock_dividend_vf)'
\echo '  ⚠ 若 vol_ratio = 1.0 → 該股 fwd 表還沒被 PR #17 後 Rust 重算過(stock 不在'
\echo '     stock_info_ref 或 phase 4 沒涵蓋)'
\echo ''
\echo '【Test 4】split / capital_reduction / capital_increase vol_ratio ≈ 1/vf:'
\echo '  ✅ 通過 = P0-11 修法生效(Rust 拆 price_multiplier / volume_multiplier)'
\echo '  ⚠ 若 vol_ratio = 1.0 → 同 Test 3 ⚠'
\echo ''
\echo '【Test 5】事件日當天 fwd.adjustment_factor != pae.volume_factor(per event_type):'
\echo '  ✅ days_af_diff_vf > 0 = P0-11 拆兩 multiplier 證據'
\echo ''
\echo '【Test 5b】2330 fwd 表 4 個新欄寫入率 + cum_af 範圍:'
\echo '  ✅ rows_with_cum_af = total_fwd_rows = PR #17 alembic + Rust upsert 全部生效'
\echo '  ✅ max_cum_af = 1.0822 對齊 CLAUDE.md 2019-01-02 預期值'
\echo ''
\echo '【Test 6】2330 最新日 ratio = 1.0:'
\echo '  ✅ sanity OK = Rust 從序列尾端倒推 multiplier 終點正確'
