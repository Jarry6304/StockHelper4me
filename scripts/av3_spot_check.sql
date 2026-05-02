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
    pae.adjustment_factor                                         AS af_in_pae,
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
    pae.adjustment_factor                                         AS af_in_pae,
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
    pae.adjustment_factor                                         AS af_in_pae,
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
\echo '# Test 5: adjustment_factor vs volume_factor 是否一致'
\echo '##############################################################'
\echo 'pae 表同時有 adjustment_factor 跟 volume_factor 兩欄;'
\echo '若兩者不同,代表 collector 寫表時就分開了,但 Rust 只讀 AF → volume_factor 沒被消費'
\echo ''

SELECT
    event_type,
    COUNT(*)                                                      AS event_count,
    SUM(CASE WHEN ABS(adjustment_factor - volume_factor) < 0.0001 THEN 1 ELSE 0 END)
                                                                  AS af_eq_vf_count,
    SUM(CASE WHEN ABS(adjustment_factor - volume_factor) >= 0.0001 THEN 1 ELSE 0 END)
                                                                  AS af_diff_vf_count,
    ROUND(AVG(adjustment_factor)::numeric, 4)                     AS avg_af,
    ROUND(AVG(volume_factor)::numeric, 4)                         AS avg_vf
FROM price_adjustment_events
WHERE adjustment_factor != 1.0 OR volume_factor != 1.0
GROUP BY event_type
ORDER BY event_type;


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
\echo '判讀指南'
\echo '=============================================================='
\echo ''
\echo '若 Test 1 + Test 2 + Test 3 + Test 4 整體 verdict 一致:'
\echo '  → A-V3 verdict 確認哪一派,blueprint §4.4 ALTER 依此決策'
\echo ''
\echo '若 Test 2 顯示「現金 dividend 也走 Rust 派」:'
\echo '  → field_mapper.py:194-203 的 volume_factor 邏輯實際沒被用到(Rust 只讀 AF)'
\echo '  → 這是 v1.x 收尾不乾淨,可砍 volume_factor 欄位'
\echo '  → 或者改 Rust 用 volume_factor 而非 AF(學術派 vs 實務派決議)'
\echo ''
\echo '若 Test 5 顯示 af_diff_vf_count > 0:'
\echo '  → collector 確實有兩個不同 factor 但 Rust 忽略 volume_factor'
\echo '  → P0-8/C1 的 spec 修正需明示「volume 用哪個 factor」'
\echo ''
\echo '若 Test 6 sanity = FAIL:'
\echo '  → 最新日 fwd ≠ raw,Rust binary 出錯,先解這個再做其他事'
\echo ''
\echo '把整份輸出 paste 回來,我會根據結果產 r2-1 完工報告 + 連動修 blueprint §4.4 + spec'
