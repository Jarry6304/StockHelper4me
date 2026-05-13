# Neely Core P0 Gate v2 Production 校準結果 — 2026-05-14

## 摘要

| 項目 | 值 |
|---|---|
| 執行日期 | 2026-05-14 |
| neely_core 版本 | 0.20.0(校準前) → **0.21.0**(校準後) |
| 環境 | 本機 PG 17 + production 級 Silver(`tw_cores run-all --write` 全市場 1263 stocks) |
| 跑的股票數 | 1263 stocks(+1 structural_snapshots = 1264 含 sentinel)|
| 資料範圍 | Daily 386-387 bars per stock(≈ 1.6 年,Bronze backfill 範圍上限) |
| 校準決策 | **A 路線:動 `forest_max_size` 1000 → 200**,其他 6 個常數一律不動 |
| 1 code commit | `config.rs` + `lib.rs` 版本 bump + test 對齊 |

## §A. Forest size 全市場分布

| forest_size | stock_count | pct |
|---|---|---|
| 37 | 1 | 0.08% |
| 31 | 1 | 0.08% |
| 29 | 1 | 0.08% |
| 22 | 1 | 0.08% |
| 21 | 2 | 0.16% |
| 20 | 1 | 0.08% |
| 19 | 1 | 0.08% |
| 17 | 4 | 0.32% |
| 16 | 2 | 0.16% |
| 15 | 2 | 0.16% |
| 14 | 4 | 0.32% |
| 13 | 5 | 0.40% |
| 12 | 5 | 0.40% |
| 11 | 13 | 1.03% |
| 10 | 22 | 1.74% |
| 9 | 35 | 2.77% |
| 8 | 56 | 4.43% |
| 7 | 92 | 7.28% |
| 6 | 121 | 9.57% |
| 5 | 193 | **15.27%** |
| 4 | 215 | **17.01%** |
| 3 | 199 | 15.74% |
| 2 | 144 | 11.39% |
| 1 | 86 | 6.80% |
| 0 | 58 | 4.59% |

**觀察**:99.92% stocks forest ≤ 31;最常見區間 forest 3-5(48.02%)。

## §B. Forest size 統計

| 統計 | 值 |
|---|---|
| total_stocks | 1264 |
| min_forest | 0 |
| avg_forest | 4.59 |
| **max_forest** | **37** |
| p50 | 4 |
| p95 | 10 |
| p99 | 16 |

→ **校準決策**:`forest_max_size` 1000 → **200**,留 5× p99 餘量 + 容受極端股票(37 × 5.4 = 200)。

## §C. 工程護欄全綠

```
overflow_triggered = false × 1264   ← 預設 forest_max_size=1000 全市場從未觸發
compaction_timeout = false × 1264   ← 預設 60s timeout 全市場從未觸發
insufficient_data  = true  × 1264   ← Bronze daily 1.6y < warmup 500(預期)
```

## §D. Monowave + Candidate 統計

| 指標 | min | avg | p50 | p95 | max |
|---|---|---|---|---|---|
| monowave_count | 0 | 73.2 | 75 | 93 | **147** |
| candidate_count | 0 | 50.4 | 48 | 86 | **288** |

→ candidate max=288 < beam_width × 10 = 1000,**beam_width 50 維持**(餘量 3.5×)。

## §E. Validator Pass Percentage

```
min=0% / avg=26.7% / p50=26.6% / p95=35.7% / max=54.5%
zero_pass_stocks    = 2     (極端結構錯亂)
no_candidate_stocks = 29    (空 forest 觸發,沒任何 candidate)
```

→ 規則嚴格度健康普適,平均 26.7% pass 對齊 dev v1 觀察的 23-29%。

## §F. 全市場拒絕原因 Top 15

| RuleId | total_rejections | avg_gap_pct |
|---|---|---|
| `Ch5_Essential(4)` (W3 須長於 W2) | 34421 | 35.91% |
| `Ch5_Essential(3)` (W2 不完全回測 W1) | 29508 | **188.01%** |
| `Ch5_Zigzag_Max_BRetracement` | 28922 | **148.74%** |
| `Ch5_Triangle_LegContraction` | 19280 | 0.00% |
| `Ch5_Equality` | 16911 | 69.31% |
| `Ch5_Overlap_Terminal` | 13052 | 0.00% |
| `Ch5_Essential(5)` | 11154 | 182.50% |
| `Ch5_Overlap_Trending` | 11124 | 149.24% |
| `Ch5_Zigzag_C_TriangleException` | 10469 | 0.00% |
| `Ch5_Flat_Min_BRatio` | 9320 | 21.80% |
| `Ch5_Essential(7)` | 7710 | 29.22% |
| `Ch5_Triangle_BRange` | 7528 | 174.46% |
| `Ch5_Alternation{Construction}` | 4704 | 0.00% |
| `Ch5_Flat_Min_CRatio` | 4007 | 12.76% |
| `Ch5_Essential(6)` | 2189 | 11.88% |

→ Top 3 跟 dev v1 完全一致,**拒絕結構性,規則嚴格度不需動**。

## §G. P9-P12 新欄位觸發狀況

| 欄位 | 觸發 stocks | 比例 | 評估 |
|---|---|---|---|
| `missing_wave_count > 0` | 1218 / 1264 | **96.4%** | ⚠️ 偏高 — 留 follow-up 深入分析 |
| `emulation_count > 0` | 797 / 1264 | 63.1% | 中段(spec Ch12 預期分布合理) |
| `reverse_logic_triggered` | 1120 / 1264 | 88.6% | 對齊「forest ≥ 2 都觸發」設計 |
| `round3_pause` | 467 / 1264 | 36.9% | 對齊「forest 全 corrective 觸發」 |
| `insufficient_data` | 1264 / 1264 | 100% | Bronze 1.6y < warmup 500(預期) |

**missing_wave 96.4% 警示**:Phase 9 設計預期罕見偵測,但 production 幾乎所有 stock 至少 1 條 suspect。要看每檔 missing_wave_count 分布判斷是「合理長尾」還是「閾值太鬆」。

→ 跑 `docs/benchmarks/missing_wave_distribution.sql` 補資料(C+D follow-up SQL)。

## §H. Degree Ceiling 對齊 spec §13.3

```
Minute      : 1225 (96.9%)  ← 1-3y daily → Minute(spec §13.3 表)
SubMicro    :   27           ← no data
SubMinuette :   12           ← < 1y daily
```

→ Bronze backfill 1.6y 全市場 → Degree Ceiling 全部 Minute,**spec 預期對齊**。

## §I. 22 Cores Facts 全市場產出量

| Core | stocks | facts | facts/stock | facts/stock/year |
|---|---|---|---|---|
| obv_core | 1251 | 1694541 | 1354.5 | **225.76** |
| institutional_core | 1239 | 873771 | 705.2 | **117.54** |
| margin_core | 1156 | 606337 | 524.5 | 87.42 |
| day_trading_core | 1154 | 549302 | 476.0 | **79.33** |
| adx_core | 1250 | 549133 | 439.3 | 73.22 |
| bollinger_core | 1251 | 471340 | 376.8 | 62.80 |
| foreign_holding_core | 1225 | 433696 | 354.0 | **59.01** |
| kd_core | 1251 | 374920 | 299.7 | 49.95 |
| macd_core | 1251 | 344991 | 275.8 | 45.96 |
| atr_core | 1245 | 257452 | 206.8 | 34.46 |
| ma_core | 1251 | 207679 | 166.0 | 27.67 |
| valuation_core | 1087 | 207599 | 191.0 | 31.83 |
| rsi_core | 1251 | 117108 | 93.6 | 15.60 |
| shareholder_core | 1239 | 115784 | 93.4 | 15.57 |
| revenue_core | 1087 | 87259 | 80.3 | 10.03 |
| financial_statement_core | 1077 | 58227 | 54.1 | 7.72 |
| **neely_core** | **1236** | **38221** | **30.9** | **15.46** |
| taiex_core | 1 | 864 | 864.0 | 172.80 |
| us_market_core | 1 | 411 | 411.0 | 82.20 |
| exchange_rate_core | 1 | 234 | 234.0 | 46.80 |
| fear_greed_core | 1 | 201 | 201.0 | 40.20 |
| market_margin_core | 1 | 21 | 21.0 | 4.20 |

**重要**:`facts_per_stock_per_year` 是「全 EventKind 加總」,**單一 EventKind 觸發率需跑 P2 calibration SQL 才能判斷是否對齊 v1.32 校準目標**。

→ 跑 `scripts/p2_calibration_data.sql` 對拆 EventKind 層級觸發率(C+D follow-up)。

---

## 校準決策

| 常數 | 預設 | Production 觀察 | 決策 | 寫進 code |
|---|---|---|---|---|
| `forest_max_size` | 1000 | max=37, p99=16 | **降至 200** | ✅ commit |
| `compaction_timeout_secs` | 60 | false × 1264 | 不動 | ❌ |
| `beam_width` | 50 | candidate max=288 < 500 | 不動 | ❌ |
| `REVERSAL_ATR_MULTIPLIER` | 0.5 | monowave avg=73.2 / 386 bars | 不動 | ❌ |
| `STOCK_NEUTRAL_ATR_MULTIPLIER` | 1.0 | pass_pct avg=26.7% | 不動 | ❌ |
| `REVERSE_LOGIC_THRESHOLD` | 2 | 88.6% triggered | 不動 | ❌ |
| Daily `warmup_periods` | 500 | 100% insufficient | 不動(等 Bronze 5+y) | ❌ |

## 留待 v3 校準(P0 Gate 更深入)

1. **`missing_wave_count` 分布深入分析** → `docs/benchmarks/missing_wave_distribution.sql`
2. **22 cores facts 拆 EventKind 觸發率** → 跑 `scripts/p2_calibration_data.sql` 對齊 v1.32 校準目標
3. **Bronze 全市場 backfill 至 5+ 年**(production maintenance window) → 校準 daily warmup_periods 是否真的需要 500
4. **`taiex_core` / `us_market_core` / `exchange_rate_core` 跑全市場**(目前只 1 row,可能是 sentinel `_market_` stock_id) → 確認 market-level 5 cores 行為正確

## 結論

**neely_core v0.20.0 → v0.21.0**:`forest_max_size` 1000 → 200 落地,其他 6 個常數一律不動等更深入資料。

**P0 Gate v2 production 校準 #1 完成**。
