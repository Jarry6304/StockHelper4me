# M3 Cores 待定案總結（spec writing checklist）

> 目的：v1.29 PR-9a milestone 收尾後,22 cores production framework 全綠,但
> spec 仍多處走 best-guess threshold。本文是 user 寫定 `m3Spec/` 各份 spec
> 之前的「待定案項目清單」,逐項列出哪些 const / threshold / detail JSONB
> key 命名 / 規則細節需要 user 拍版。
>
> **使用方式**:user 動筆寫 m3Spec/X_cores.md 之前先掃對應段落,確認每項
> 的決定。code 端對應 const / TODO comment 已加註行號,寫定後 batch 同步
> Rust 常數即可。
>
> alembic head:`w2x3y4z5a6b7`(三表落地;本文 0 migration)
> Rust workspace:24 crate / 155 tests passed / 22 cores 全 inventory 註冊
>
> **2026-05-10 production state**:
>   - 1263 stocks(production scale 上限;另 340 stocks 為已退市 empty status)
>   - 4.4M facts production verified
>   - 9.2 分鐘 wall time(PR-9b/9c/9d 並行 + batch INSERT)
>   - 9 個阻塞點拍版決策見 §13(2026-05-10 user 拍板紀錄)

---

## 0. 已驗收的 framework(不需再動)

| 項目 | 範圍 |
|---|---|
| 22 cores dispatch | `tw_cores run-all` 全市場全核 production verified |
| 三表寫入路徑 | `indicator_values` / `structural_snapshots` / `facts` ON CONFLICT 各自正確 |
| 4 loaders schema 對齊 | OHLCV / chip / fundamental / environment NUMERIC → FLOAT8 cast |
| margin_core NULL skip | 28 false positive 清掉,regression test 落地 |
| 跨平台 clap subcommand | Windows + Linux 都認 `run-all` / `list-cores` |
| inventory CoreRegistration | 22 cores 全部 compile-time discover |
| `params_hash` blake3 | 對齊 cores_overview §7.4 |

---

## 1. Wave Core(neely_core)— P0 Gate

### 1.1 Stage 4 Validator 規則(✅ 已實作,v3.7 對齊 r5 spec 計數)

> **v3.7 update**(2026-05-16):本 §1.1 + §1.3 大幅清理。原文標的 22 條全 deferred 已過時 —
> R4-R7 / F1-F2 / Z1-Z2 / T1-T3 / W1-W2 全部已實作於 `validator/*.rs`,計數對齊 r5 spec
> (`m3Spec/neely_core_architecture.md §9.3`)而非 r4 舊版(r4 T1-T10 / Z1-Z4 在 r5 收斂為
> 3 Triangle + 2 Zigzag variants;對齊 RuleId enum 既有 dispatch 範圍 Ch5_*/Ch9_*/Engineering_*)。

| 規則 ID | 模組 | 狀態 |
|---|---|---|
| R4 / R5 / R6 / R7 | `validator/core_rules.rs:213-380` | ✅ 已實作 |
| F1 / F2(Flat_Min_BRatio / Flat_Min_CRatio)| `validator/flat_rules.rs:36-105` | ✅ 已實作(r5 收斂 2 條,對齊既有計數) |
| Z1 / Z2(Zigzag_Max_BRetracement / C_TriangleException)| `validator/zigzag_rules.rs:36-118` | ✅ 已實作(r5 收斂 2 條,原 Z1-Z4 計數 stale) |
| T1 / T2 / T3(Triangle_BRange / LegContraction / LegEquality_5Pct)| `validator/triangle_rules.rs:50-178` | ✅ 已實作(r5 收斂 3 條,原 T1-T10 計數 stale) |
| W1 / W2(Equality / Alternation)| `validator/wave_rules.rs:46-201` | ✅ 已實作 |

best-guess 閾值校準仍屬 P0 Gate 範圍(production data driven),非 spec 缺。

### 1.2 寫死常數(對齊 spec §4.4 / §6.6)— 等 user 拍版

| 常數 | 目前值 | 位置 | spec 段落 |
|---|---|---|---|
| `REVERSAL_ATR_MULTIPLIER` | 0.5 | `monowave/pure_close.rs` | §4.4 monowave 反轉門檻 |
| `STOCK_NEUTRAL_ATR_MULTIPLIER` | 1.0 | `monowave/neutrality.rs` | §10.4.1 個股 Neutral |
| `neutral_threshold_taiex` | 0.5% | `config.rs` | §10.4.1 加權指數 Neutral 例外 |
| `BEAM_CAP_MULTIPLIER` | 10 | `candidates/generator.rs` | Stage 3 候選上限 |
| Wilder ATR period | 14 | `monowave/pure_close.rs` | §4.4 |
| `forest_max_size` | 1000 | `config.rs` | Stage 8 Forest 上限 |
| `compaction_timeout_secs` | 60 | `config.rs` | Stage 8 timeout |
| `BeamSearchFallback.k` | 100 | `compaction/mod.rs` | Forest 過大時 fallback |

### 1.3 Code follow-up(spec 不缺;v3.7 reframe)

> **v3.7 update**(2026-05-16):原 PR-3c / PR-4b / PR-6b 全部已實作,本 §1.3 reframe 為
> 純 code follow-up。**spec 全部已齊** — `m3Spec/neely_core_architecture.md` + `m3Spec/neely_rules.md`
> 對映完整。

- ~~**PR-3c**:Stage 4 R4-R7 + F/Z/T/W 22 條完整實作~~ → ✅ 已實作(見 §1.1)
- ~~**PR-4b**:Diagonal Leading vs Ending sub_kind 區分~~ → ✅ 已實作(`classifier/mod.rs:243-269` + `output.rs:560-563`)
- ~~**PR-4b**:R3 Diagonal exception~~ → ✅ 已實作(Stage 6 Post-Validator)
- **PR-5b**:exhaustive compaction 真正窮舉合法 paths(目前 pass-through)— **spec 已齊**
  (`m3Spec/neely_rules.md §Three Rounds` line 1198-1256 詳述 Round 1-3 + 邊界波 retracement
  reevaluation),v3.7 Phase B 動工
- ~~**PR-6b**:Power Rating 完整 Neely 書頁查表 + Fibonacci 接 monowave price~~ → ✅ 已實作
  (`power_rating/{mod,table,max_retracement,post_behavior}.rs` 892 行 + `fibonacci/projection.rs`)

### 1.4 P0 Gate 五檔實測校準清單

跑 0050 / 2330 / 3363 / 6547 / 1312,visual review forest 後校準上述 §1.2 常數,
寫入 `docs/benchmarks/`(目前不存在,P0 Gate 時新建)。

---

## 2. Indicator Cores(8 個)

### 2.1 macd_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `DIVERGENCE_MIN_BARS` | 20 | spec §3.6 對齊?(寫死 const) |
| HistogramExpansion | 連 5+ expansion | spec 沒明列門檻 |
| 6 EventKind | GoldenCross / DeathCross / HistogramExpansion / HistogramZeroCross / Bullish / BearishDivergence | 對齊 spec §3.5 |

### 2.2 rsi_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| **FailureSwing** 四步邏輯 | ✅ **已實作**(commit 458a45a):Wilder 1978 §7 四步完整實作 | — |
| Bullish/BearishDivergence min bars | 20(對齊 macd) | spec 對齊? |

### 2.3 kd_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `k_smooth` / `d_smooth` | 3 / 3(Taiwan formula) | spec 對齊?台股 KD vs 美股 Stochastic 預設不同 |
| Taiwan formula | `(ks-1)/ks × prev_K + 1/ks × RSV` | spec 沒列,best-guess |

### 2.4 adx_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `strong_trend_threshold` | 25 | spec §5.3 對齊? |
| `very_strong_threshold` | 50 | 同上 |

### 2.5 ma_core(複雜,需仔細寫)
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `MaKind` | 6 種:Sma / Ema / Wma / Dema / Tema / Hma | Dema/Tema/Hma 公式精確性需 user 對 reference 校 |
| `PriceSource` | 7 種 enum | 哪些常用要進預設 spec |
| `CrossPairPolicy` | None / AllPairs / Pairs | spec 對齊? |
| 各 kind 暖機倍數 | SMA×1 / EMA×4 / DEMA×6 / TEMA×8 / HMA×2 | spec 對齊? |
| Output | `series_by_spec: Vec<MaSeriesEntry>`(非 single series) | 對齊 spec |

### 2.6 bollinger_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `source: PriceSource` | 預設 Close | spec 對齊 |
| `percent_b` 公式 | `(close-lower)/(upper-lower)` | spec 對齊 |
| 8 EventKind | Squeeze / Expansion / WalkingTheBands / Touch等 | spec 對齊 |

### 2.7 atr_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| VolatilityExtremeHigh/Low lookback | 1 年(252 bars) | spec 寫死還是可調? |
| Expansion 10d 50% 門檻 | 寫死 | spec 對齊? |
| `atr_pct` 公式 | `ATR/close×100` | spec 對齊 |

### 2.8 obv_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `anchor_date: Option<NaiveDate>` | 預設 None(自動取首日) | spec §6.4 anchor 起算邏輯 |
| `ma_period: Option<usize>` | 預設 None | spec 預設值? |
| 6 EventKind | Divergence / TrendChange / MaCross 等 | spec 對齊 |

---

## 3. Chip Cores(5 個)

### 3.1 day_trading_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `day_trade_volume` 公式 | best-guess `= day_trading_buy` | **spec §7.4 真正定義待 user 確認** |
| `total_volume` 公式 | best-guess `= day_trade_volume × 100 / ratio` | 同上 |
| `STREAK_MIN_DAYS` | 3 const | spec 對齊? |
| 「historical high」label | ✅ **已實作**(commit a372879):`RatioExtremeHigh` metadata 加 `historical_high: bool` | — |

### 3.2 margin_core(NULL skip 已修)
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `margin_change_pct_threshold` | 5.0 | spec §4.3 |
| `short_change_pct_threshold` | 10.0 | spec §4.3 |
| `short_to_margin_ratio_high` | 30.0 | spec §4.3 |
| `short_to_margin_ratio_low` | 5.0 | spec §4.3 |
| `MAINTENANCE_LOW_THRESHOLD` | 145.0 const | spec §4.5 列 EventKind 但 §4.3 未列 Param,目前寫死 |
| 「historical high」label | ✅ **已實作**(commit 3617d84):`EnteredShortRatioExtremeHigh` metadata 含 `historical_high: bool` | — |

### 3.3 shareholder_core(2026-05-10 Round 1 完成 — user 拍版)
| 項目 | 拍版值 | 來源 |
|---|---|---|
| ~~detail JSONB key 命名~~ | ✅ iterate 真 17 levels | 對齊 Silver real structure |
| ~~`STREAK_MIN_WEEKS`~~ | ✅ **8 週** | Moskowitz, Ooi, Pedersen (2012) "Time Series Momentum" JFE(⚠️ 跨領域援引,需 Phase 2 回測驗證) |
| ~~分類邊界~~ | ✅ **4-level**:small ≤ 50 張(8 levels)/ mid 50-400 張(3)/ large 400-1000 張(3)/ super_large > 1000 張(1) | Money 錢雜誌 50/400 + 凱基/集保 1000 張大戶 |
| ~~`concentration_index` 公式~~ | ✅ `(large.unit + super_large.unit) / total.unit` | 業務「籌碼集中度」標準定義,採 unit (股數) |
| EventKind | 加 `SuperLargeHoldersAccumulating` / `SuperLargeHoldersReducing` 2 個 | 對齊 4-level 完整 streak coverage |
| Skip rules | `差異數調整(說明4)` 異常 row 不算 | 確認 |

### 3.4 foreign_holding_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `LimitNearAlert` 邏輯 | ✅ **已實作**(commit 458a45a):從 `detail` JSONB 取 `foreign_limit_pct`,NULL 時 skip | — |
| `foreign_holding_ratio` threshold | `MILESTONE_LOOKBACK=60`(寫死 const) | spec 校準? |

### 3.5 institutional_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| 5 法人 streak 邏輯 | best-guess | spec 對齊 |
| Foreign net buy/sell 連續天數門檻 | 3 天 | spec 對齊? |

### 3.6 gov_bank_core 🟡 **proposal(2026-05-17,等 user 拍版)**

`m3Spec/chip_cores.md §九` draft 落地。8 個 open question 等 user 拍版,動 Rust 前必看:

| # | 問題 | best-guess | 影響 |
|---|---|---|---|
| 9.10.1 | 保留 `GovBankSilenceBreak` EventKind | YES | 砍此 → silence_period_days 也砍 |
| 9.10.2 | `silence_period_days` 預設值 | 10(~2 週)| SilenceBreak 觸發頻率 |
| 9.10.3 | per-bank breakdown EventKind | NO | YES 需 Bronze→Silver pivot + 8 metadata 細項 |
| 9.10.4 | `GovBankFlowReversal`(20d 累積翻轉)| NO | 跨指標 reversal 屬 Aggregation Layer 識讀 |
| 9.10.5 | NULL gov_bank_net 處理 | 視同 0 | streak 連續性 |
| 9.10.6 | timeframe 支援 | Daily only | V3 加週/月聚合 |
| 9.10.7 | 2021-06-30 前 Bronze 無資料處理 | chip_loader 自動 cut | 確認 loader 不 panic |
| 9.10.8 | structural_snapshots 是否寫入 | NO(對齊 indicator-style)| facts only |

決策後再上 Rust(對齊「best-guess 不上 Rust」鐵律)。

---

## 4. Fundamental Cores(3 個)

### 4.1 revenue_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `yoy_high_threshold` / `yoy_low_threshold` | 預設值待 user 寫定 | spec |
| `mom_significant_threshold` | 預設值待 user 寫定 | spec |
| `streak_min_months` | 3 | spec 對齊? |
| `historical_high_lookback_months` | 預設值待 user 寫定 | spec(影響 warmup = lookback + 12) |

### 4.2 valuation_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `5y_percentile` lookback | 5 年(252×5 bars) | spec §4.5 對齊? |
| `PerNegative` 邏輯 | 簡化(per < 0 觸發) | spec §4.7 完整邏輯待寫 |
| `history_lookback_years` | 5(影響 warmup = years × 252) | spec |

### 4.3 financial_statement_core(2026-05-11 修 round 3)
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| detail JSONB key | ✅ **全形括號 `\u{FF08}` `\u{FF09}` + IFRS 中文 fallback chain** | 對齊 user 揭露 2330 真 detail key |
| **Silver builder `_per` suffix fix**(2026-05-11) | ✅ `financial_statement.py:60-62` 加 `_per` suffix:balance 元值→`detail["資產總額"]`,% →`detail["資產總額_per"]` | — |
| **ROE / ROA / balance 元值欄** | ✅ **已計算**(commit 本 session):`roe_pct = net_income / total_equity × 100`;`debt_ratio_pct = total_liabilities / total_assets × 100` | — |
| `RoeHigh` EventKind | ✅ **觸發中**(`roe_pct >= 15%` 觸發;2330 歷年 ROE ~20-30% 應大量觸發) | threshold 校準見 §13 阻塞 5(c) |
| Quarterly approximation | ✅ `Timeframe::Quarterly`(commit 458a45a 加 enum variant) | — |
| Threshold 預設 | `gross_margin_change_threshold=2.0` / `roe_high_threshold=15.0` / `debt_ratio_high_threshold=60.0` / `fcf_negative_streak_quarters=4` | spec 校準 |

---

## 5. Environment Cores(5 個)

### 5.1 taiex_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| 內嵌 ema/sma/wilder_rsi | 對齊 §3.3 零耦合(不從 indicator_kernel 取) | 設計確認 |
| 9 Point fields | close/volume/change_pct/macd_*/rsi/volume_z/trend_state | 對齊 spec §3.5 |
| 9 EventKind | spec 對齊 | spec |
| `stock_id` 保留字 | `_index_taiex_` | 對齊 cores_overview §6.2.1 |

### 5.2 us_market_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `VixZone` 切分 | Low / Normal / High / ExtremeHigh | spec §4.6 邊界值 |
| Input | `UsMarketCombinedSeries { spy, vix }` wrapper | 對齊 spec §4.6 同 Point 含兩者欄位 |
| 6 EventKind | spec 對齊 | spec |
| `stock_id` 保留字 | `_global_` | 對齊 cores_overview §6.2.1 |

### 5.3 exchange_rate_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `currency_pairs: Vec<String>` | 預設空 | **spec §5.4 預設值待 user 寫定**(USDTWD / JPYTWD / 等) |
| `key_levels: Vec<f64>` | 預設空 | **spec §5.4 預設值待 user 寫定**(支撐壓力位) |
| `ma_period` | 20 | spec 對齊? |
| `significant_change_threshold` | 預設值待 user 寫定 | spec |

### 5.4 fear_greed_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `fear_threshold` | 45 | spec 對齊? |
| `greed_threshold` | 55 | spec 對齊? |
| `streak_min_days` | 5 | spec 對齊? |
| 5-state zone 邊界 | best-guess(<=25 ExtremeFear / <45 Fear / 45-55 Neutral / >55 Greed / >=75 ExtremeGreed) | spec 校準 |
| Schema | 直讀 Bronze `fear_greed_index.score`(無 derived 表)— §6.2 已登記架構例外 | 是否需建 `_derived` 表? |

### 5.5 market_margin_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `maintenance_warning_threshold` | 145.0 | spec §6.4 對齊? |
| `maintenance_danger_threshold` | 130.0 | spec §6.4 對齊? |
| `significant_change_threshold` | 5.0% | spec 對齊? |
| 4 EventKind | EnteredWarning / EnteredDanger / ExitedDanger / SignificantSingleDayDrop | spec 對齊 |
| `stock_id` 保留字 | `_market_` | 對齊 cores_overview §6.2.1 |

---

## 6. Silver schema 假設(影響多個 cores)

| 假設 | 影響 core | 處理方式 |
|---|---|---|
| `margin_daily_derived.margin_maintenance` 欄位是否存在 | margin_core | 不存在 → MaintenanceLow event 永遠不觸發(目前 NULL placeholder) |
| `foreign_holding_derived.foreign_limit_pct` stored col | foreign_holding_core | 目前 NULL placeholder,LimitNearAlert 不觸發 |
| `holding_shares_per_derived.detail` JSONB schema | shareholder_core | 目前 best-guess key,需對齊真實 schema |
| `market_margin_maintenance_derived` 完整欄位 | market_margin_core | 目前 ratio + total_*_balance 三欄;PR #21-B 已補 v1.19 |
| `fear_greed_index` 是否需 `_derived` 表 | fear_greed_core | 目前直讀 Bronze,§6.2 已登記架構例外 |
| `financial_statement_derived.detail` JSONB key | financial_statement_core | 英文 key 假設(EPS/Revenue 等),可能 Bronze 真欄是中文 origin_name |

**user 寫 m3Spec/ 時必須確認 Silver schema 是否需要補欄 / 改欄名**,
若需要則開新 alembic migration(本文 0 migration)。

---

## 7. 共用設計待 user 拍版

### 7.1 `Timeframe` enum
目前:`Daily / Weekly / Monthly`
缺:`Quarterly`(financial_statement_core 用 Monthly 近似)
**待決**:是否需要加 Quarterly variant?

### 7.2 Reserved stock_id
目前:`_market_` / `_global_` / `_index_taiex_`
**待決**:是否還有其他保留字?(對齊 cores_overview §6.2.1)

### 7.3 facts 表 statement 格式
目前:best-guess 英文 + 數值精度(`{:.1}` / `{:.2}` / `{:.4}` 各 core 不一)
**待決**:
- 是否需要中文(對齊 user 介面)?
- 數值精度全表統一還是各 core 自定?

### 7.4 ON CONFLICT DO NOTHING dedup 設計
目前:facts 表 UNIQUE 對 `(stock_id, fact_date, timeframe, source_core, params_hash, md5(statement))`
**已驗證**:重跑 stage 3 facts 寫入 0(全部 dedup),對齊設計
**待決**:當 statement 文字微改(精度 / 中英文切換),會不會踩 facts 累積膨脹?

---

## 8. 補實作 backlog(spec-blocked,等寫定後動)

| 範圍 | 估時 | 阻塞於 |
|---|---|---|
| RSI Failure Swing 四步邏輯 | 半天 | spec §4.6 寫定 |
| Diagonal Leading vs Ending sub_kind | 半天 | spec §10.3 寫定 |
| financial_statement detail JSONB key 對齊 | 1 天 + Bronze schema 確認 | spec §5.5 + Bronze origin_name 列 |
| shareholder detail JSONB key 對齊 | 半天 + Bronze schema 確認 | spec §6 + Bronze schema |
| foreign_holding_derived.foreign_limit_pct 補欄 | 半天 + alembic migration | spec §6.5 |
| margin_daily_derived.margin_maintenance 補欄 | 半天 + alembic migration | spec §4.5 |

---

## 8.5. Round 4 transition pattern fix(2026-05-10 落地)

對齊 fear_greed / market_margin 既有 EnteredX/ExitedX pattern,把 stay-in-zone
連日重複觸發改成 transition 只在進 zone / 出 zone 觸發一次。

| Core | EventKind 改動 | 預期 facts 量級降 |
|---|---|---|
| valuation_core | 6 改 12(加 PerExtremeHigh/Low / PbrExtremeHigh/Low / YieldExtremeHigh / YieldHighThreshold 各 Entered+Exited) | ~70%(2.0M → ~600K) |
| margin_core | 3 改 6(ShortRatioExtremeHigh/Low / MaintenanceLow Entered+Exited) | ~47%(1.3M → ~700K) |
| bollinger_core | 4 改 8(UpperBandTouch / LowerBandTouch / AboveUpperBand / BelowLowerBand Entered+Exited) | ~83%(466K → ~80K) |
| atr_core | **不改**(既有「new max/min」邏輯本身就是 transition,非 stay-in-zone) | n/a |

設計:不引入 Zone enum(對齊 user 拍版「去耦合 + 減少抽象 + 重工 OK」),用
獨立 bool `prev_*_in_zone` tracking。bouncy 不防衛(對齊 fear_greed 範本)。

舊 facts 處理:user 拍版 TRUNCATE facts 全清 + run-all 重跑(10 分鐘內完成)。

## 9. PR-9b 工程進階(spec 之外,可平行)

| 項目 | 估時 | 預期收益 |
|---|---|---|
| Workflow toml dispatch | 半天 | 動態決定跑哪些 cores(目前 hardcode 全 22) |
| ~~sqlx pool 並行 + per-stock task spawn~~ | ~~半天~~ | ✅ **2026-05-10 落地**:`max_connections` 對齊 `--concurrency`(default 16),`for_each_concurrent` 並行 Stage B per-stock |
| incremental dirty queue(只跑 `is_dirty=TRUE` stocks) | 半天 | 每日增量大幅省時 |
| dev DB scale up to 1700 stocks | 30~40h calendar | 真 production scale data 反饋 |

---

## 10. ⚠️ V2 階段禁止做(spec 已明文,不要動)

| 項目 | spec 來源 | 原因 |
|---|---|---|
| Indicator kernel 共用化(抽出 ema/sma/wilder_*) | cores_overview §十四「P3 後考慮,V2 不規劃」 | 2026-05-09 嘗試過,user 退板「禁止耦合,重複可接受」 |
| 跨指標訊號獨立 Core(TTM Squeeze / `chip_concentration_core` 等) | cores_overview §十一 / chip_cores §八「不在 Core 層整合」 | 跨指標訊號交給 Aggregation Layer |
| `financial_statement_core` 拆分(損益/資產負債/現金流獨立 Core) | cores_overview §十四「V3 議題,V2 不規劃」 | 18 欄目前同 Core 處理 |

---

## 11. 寫 m3Spec/ 建議順序(優先序)

1. **m3Spec/neely_core.md** — 最複雜,影響 Stage 4 22 條規則(`PR-3c` 全 deferred)。等寫定才能解 0050/2330 等股票的 `power_rating = Neutral, rules passed = 0, deferred = 22` 限制
2. **m3Spec/chip_cores.md** 完整版 — user 既有 chip_cores.md 部分寫過,需補 shareholder detail JSONB schema(0 events 阻塞)
3. **m3Spec/fundamental_cores.md** — financial_statement detail JSONB key + Bronze origin_name 對齊(0 events 阻塞)
4. **m3Spec/environment_cores.md** — exchange_rate currency_pairs / key_levels 預設值 + market_margin / fear_greed threshold
5. **m3Spec/indicator_cores_*.md** — ma_core 6 kind 公式校準 + macd/rsi divergence min bars + RSI FailureSwing 四步
6. **m3Spec/cores_overview.md** — 共用設計(Timeframe / Reserved stock_id / facts statement 格式)

寫定一份就 batch 同步 Rust 常數 + 補實作對應 deferred 項目。

---

## 12. 參考路徑

| 檔 | 內容 |
|---|---|
| `rust_compute/cores/wave/neely_core/src/` | Stage 1-10 完整 module + 22 deferred validator |
| `rust_compute/cores/{indicator,chip,fundamental,environment}/*/src/lib.rs` | 各 core 實作 + best-guess threshold 註解 |
| `rust_compute/cores_shared/{ohlcv,chip,fundamental,environment}_loader/` | Silver derived 讀取邏輯 |
| `rust_compute/cores/system/tw_cores/src/{main,cli,dispatcher,writers,run_environment,run_stock_cores,summary,helpers}.rs` | Monolithic dispatcher 拆 8 module(v3.5 R4 C8;`run-all` subcommand)|
| `m2Spec/oldm2Spec/cores_overview.md` | 共用設計參考 r2 |
| `m2Spec/oldm2Spec/{tw_market,traditional,neely,fundamental,chip,environment}_core.md` | 各 core spec r2(待 user 寫定 m3Spec/ 後 deprecate)|
| `m3Spec/chip_cores.md` | user 既有,部分寫定 |

---

## 13. 9 個阻塞點拍版決策紀錄(2026-05-10 user 拍板)

對齊 production 1263 stocks × 22 cores × 4.4M facts state 後,9 個阻塞點的
user 拍版決定 + Rust code reference 註解狀態:

### 阻塞 1:`financial_statement_core` Silver builder origin_name 元值/% 覆蓋 bug

**狀態**:✅ **2026-05-11 動工完成(commits a372879 / b5d8ab5 / 2f0cbf9 / a2a9df3 / 4484196)**

- **根因**:Silver builder `silver/builders/financial_statement.py:60-62` 用
  `row.get("origin_name")` 當 dict key,但 Bronze 同 origin_name(中文「應付帳款」)
  對映 2 個 type(`AccountsPayable` 元值 + `AccountsPayable_per` %),dict 後寫
  覆蓋前寫 → 元值消失 → balance 全是 %
- **影響**:ROE / ROA / RoeHigh / RoaHigh 4 個 EventKind 永久 0(Round 2 fix
  `roe_pct = 0.0` 後)

**實際修法**:
- Silver `_per` suffix(a372879):`item_key.endswith("_per")` 時加 `_per` 後綴
- Rust ROE/ROA 重啟(a372879):讀元值 keys 算 `roe_pct = net_income / total_equity × 100`
- ROE/ROA TTM 4-quarter sum(b5d8ab5):FinMind 給 quarterly net_income,Buffett 15%
  是 annual,改 `series[i-3..=i].iter().sum() / equity × 100`,前 3 季 fallback `× 4`
- **A1 連動**:Bronze `financial_statement` PK 從 origin_name 改 type(2f0cbf9 + 2 hotfix)。
  舊 PK 同 origin_name 不同 type 衝突,`_per` 覆蓋元值;新 PK 兩者共存。詳見 §15。
- alembic head:`w2x3y4z5a6b7` → `x3y4z5a6b7c8`
- **3 檔受影響股票全修**:2330(6→16+ RoeHigh facts)/ 2357(0→7)/ 2836(0,金融業
  ROE < 15% 符合預期)
- 1074+ 其他股票元值原本就 survive(FinMind 多數情況元值寫在後),無需大規模重抓

### 阻塞 2:`shareholder_core` 4-level + STREAK_MIN_WEEKS + concentration_index

**狀態**:✅ **2026-05-10 動工完成(commit 458a45a)**

- **邊界**(user 拍版):small ≤ 50 張 / mid 50-400 張 / large 400-1000 張 / super_large > 1000 張
- **STREAK_MIN_WEEKS**:8 週(Moskowitz, Ooi, Pedersen 2012 JFE;⚠️ 跨領域援引)
- **concentration_index**:`(large.unit + super_large.unit) / total.unit`(採股數)
- **新加 EventKind**:`SuperLargeHoldersAccumulating` / `SuperLargeHoldersReducing`

### 阻塞 3:`neely_core` 22 條 R4-R7/F/Z/T/W deferred 規則

**狀態**:🟡 **跳過(user 既有拍板「先跳過」)**,留 PR-3c

- 22 條規則 deferred:`power_rating = Neutral, rules passed = 0, deferred = 22`
- 等 user 寫 m3Spec/neely_core.md 完整版(數天)
- 或 PR-3c 用 best-guess Frost-Prechter 通用規則 batch 補(我 ~1 天)

### 阻塞 4:Round 4 EnteredX/ExitedX bouncy 防衛

**狀態**:✅ **2026-05-10 拍板「不動」**

- user 拍版「不防衛」對齊 fear_greed 範本
- bollinger facts 從 466K → 457K(↓1.9%,bouncy 本質)
- 接受 swing trader 介面真實看到每次進退 zone

### 阻塞 5:100 個 threshold 校準路徑

**狀態**:✅ **全部完成(a+b+c+d 四類 2026-05-12 收尾)**

按分類 A/B/C/D 拍板:
- **(a) 分類 A 業界/學術標準(~7 個 indicator const)** = **不動,加 reference 註解** ✅
  - Wilder ATR/RSI/ADX (1978) / Appel MACD (1979) / Bollinger 20/2.0 (2002) / 5y percentile
- **(b) 分類 B 台灣特有(~15 個)** = **保留當前 best-guess,加 reference 註解** ✅
  - B-1 有 reference:證交所 145/130 維持率 / Buffett 15% ROE (1987) / Graham yield 5% (1949)
  - B-2 無 reference:KD 9(Asian convention)/ short_to_margin 30/5 / margin_change 5% /
    gross_margin_change 2% / debt_ratio 60% / day_trading streak 3
- **(c) 分類 C streak/lookback(~8 個)** = ✅ **2026-05-12 production data driven 校準完成**
  - C-1 `STREAK_MIN_DAYS=3` (RSI/KD/day_trading):保留不動(觸發率待 §2 SQL 驗)
  - C-2 `ABOVE_MA_STREAK_MIN` (ma_core):固定 30 → scaling fn `(period*3/2).min(30).max(5)` ✅
    (MA20 維持 ~30d 保持原行為;MA5/10 提高門檻避免噪音)
  - C-3 `EXPANSION_LOOKBACK=14` (atr_core):2026-05-11 對齊 Wilder period=14 ✅
  - C-4 `SQUEEZE_STREAK_MIN=5` (bollinger):保留(待 §2 SQL 確認)
  - C-5 `STREAK_MIN_WEEKS=8` (shareholder):保留(MOP 2012 跨領域援引,足夠)
  - C-6 `DIV_MIN_BARS=20` / Divergence 算法:→ **算法重寫(pivot-based) ✅ 2026-05-12**
    (Murphy 1999 p.248 要求比較 swing points,非固定間距;詳見 §16)
  - C-7 `MILESTONE_LOOKBACK=60` (foreign_holding):改雙時間窗口 60d(季)+252d(年) ✅
    (George & Hwang 2004 JF 59(5) 52 週高點;新增 HoldingMilestoneHighAnnual/Low)
  - C-8 `LOOKBACK_FOR_Z=60`/`LARGE_TRANSACTION_Z=2.0` (institutional):算法改 edge trigger ✅
    (Brown & Warner 1985 事件研究基準;詳見 §16)
- **(d) 分類 D environment(~7 個)** = **接受不動** ✅
  - Whaley VIX (2000) / CNN Fear & Greed / 央行匯率心理關卡

### 阻塞 6:`Timeframe::Quarterly` variant

**狀態**:✅ **2026-05-10 動工完成(commit 458a45a)**

- `fact_schema::Timeframe` enum 加 Quarterly variant
- `financial_statement_core` 從 Monthly approximation 改用 Quarterly
- `neely_core::warmup_periods` Quarterly = 60
- `ohlcv_loader::load_for_indicator` Quarterly 回 anyhow!error(財報專用)

### 阻塞 7:`foreign_holding_core` foreign_limit_pct stored col

**狀態**:✅ **2026-05-10 動工完成(commit 458a45a)** — user 拍板「(a) 嘗試看看」

- Bronze `foreign_investor_share_tw.upper_limit_ratio` 已存在(無需新 Bronze source)
- Silver builder 已 pack 進 detail JSONB
- chip_loader SQL 改 `(detail->>'upper_limit_ratio')::float8 AS foreign_limit_pct`
- 不需 alembic / 不需 Silver schema 改
- `LimitNearAlert` EventKind 解封(若 Bronze 真有料)

### 阻塞 8:`rsi_core` FailureSwing 4-step 邏輯

**狀態**:✅ **2026-05-10 動工完成(commit 458a45a)**

- 對齊 Wilder J. Welles (1978). "New Concepts in Technical Trading Systems"
  §7 RSI Failure Swing
- 4-step state machine(進 OB → 退 OB → 反彈 fail → 跌破前低 = Bearish FS)
- Bullish FS 對稱(oversold zone)
- 2 個 regression test 落地

### 阻塞 9:`Diagonal` Leading vs Ending sub_kind

**狀態**:🟡 **跳過(user 拍板「等 NEELY」)**,留 PR-3c 同 neely 22 條一起做

---

## 下個 session 動工清單(2026-05-12 update)

| 優先 | 範圍 | 估時 |
|---|---|---|
| ~~**P1**~~ | ~~阻塞 1 Silver builder origin_name `_per` suffix fix + Rust ROE/ROA 改元值~~ | ✅ **2026-05-11 完成** |
| ~~**P2-5c**~~ | ~~阻塞 5(c) C 類常數校準~~ | ✅ **2026-05-12 完成** |
| ~~**P2-6**~~ | ~~4 個 🔴噪音 EventKind 根因修正(edge trigger + rolling z-score + pivot divergence)~~ | ✅ **2026-05-12 完成** |
| **P-verify** | 跑 `p2_calibration_data.sql` 驗算修後觸發率(§2 每股每年觸發次數)| user 端 ~5 分鐘 |
| **P3** | 阻塞 3 / 9 PR-3c neely 22 條 + Diagonal sub_kind(等 user m3Spec/neely_core.md 或用 best-guess) | ~1-2 天 |
| P4 | dev DB scale up(已確認 1263 是 production 上限,可不動)| 0(user 接受) |
| P5 | m3Spec/ 寫定各 core threshold spec(對齊 §13 拍版紀錄)| user 數天 |

---

## 15. A1 Bronze financial_statement PK fix(2026-05-11)

### 根因
舊 PK `(market, stock_id, date, event_type, origin_name)`。FinMind
`TaiwanStockBalanceSheet` 同 origin_name(如「資產總額」)回兩筆:
- `type='TotalAssets'`,元值 `2.6 兆`
- `type='TotalAssets_per'`,% common-size `67.85`

兩筆 PK 衝突,UPSERT 後者覆蓋前者。對 2330 等 3 檔股票,`_per` 被最後寫入 →
元值消失 → ROE/ROA 無法算 → 阻塞 1 Silver `_per` fix 雖正確但 Bronze 本身缺
元值,RoeHigh 在近期季報無法觸發。

### 修法
新 PK `(market, stock_id, date, event_type, type)`,以 FinMind 英文科目代碼
作 discriminator,`TotalAssets` 與 `TotalAssets_per` 不再衝突。

alembic `x3y4z5a6b7c8`,3 commit 收尾:

| commit | 範圍 |
|---|---|
| `2f0cbf9` | 初版 migration(誤用固定 constraint name)|
| `a2a9df3` | hotfix 1:動態查 PK constraint name(PR #R3 RENAME 不會 rename constraint,既有部署 PK 名仍是 `financial_statement_tw_pkey`)|
| `4484196` | hotfix 2:legacy_v2 表佔用 `financial_statement_pkey` 索引名,先 rename 釋出(PR #R2 RENAME 同樣不會 rename constraint)|

### 全市場受影響股票(僅 3 檔)
- **2330 TSMC**:RoeHigh 從 6 facts(2019-2020)→ 16+ facts(2019-2025,ROE 23-31%)
- **2357 華碩**:RoeHigh 0 → 7 facts(2021-2025)
- **2836 高雄銀**:0 facts(金融業 ROE 通常 < 15% Buffett threshold,符合預期)

剩餘 1074+ 檔股票元值原本就 survive(FinMind API 多數情況元值寫在後)。

### 重新套用流程(若 user 想全市場套用)
```sql
-- 找出仍只有 _per 沒有元值的股票
SELECT stock_id FROM financial_statement
WHERE event_type='balance' AND date='2025-09-30'
GROUP BY stock_id
HAVING COUNT(CASE WHEN type NOT LIKE '%_per' THEN 1 END) = 0;
```
對應 stock_ids 跑:
```bash
psql -c "DELETE FROM financial_statement WHERE stock_id IN (...);
         DELETE FROM api_sync_progress WHERE stock_id IN (...) AND api_name LIKE 'financial_%';"
python src/main.py backfill --stocks ... --phases 5
python src/main.py silver phase 7b --stocks ... --full-rebuild
cargo run --release -p tw_cores -- run-all --stocks ... --write
```

---

## 14. 22 cores const reference 註解狀態(2026-05-10 加)

10 個 cores 加 `Reference(2026-05-10 加)` doc 註解(對齊阻塞 5 a+b 拍板):

| Core | Reference doc 註解 | 主要 const + 出處 |
|---|---|---|
| `atr_core` | ✅ | period=14 Wilder (1978) Ch. 21 |
| `rsi_core` | ✅ | period=14 / overbought=70 Wilder (1978) + Murphy (1999);FailureSwing Wilder 1978 §7 |
| `macd_core` | ✅ | 12/26/9 Appel (1979) |
| `bollinger_core` | ✅ | 20/2.0 Bollinger (2002) |
| `adx_core` | ✅ | strong=25 / very_strong=50 Wilder (1978) |
| `kd_core` | ✅ | period=9 Asian convention(非國際標準,Lane 1957 原版 14)|
| `margin_core` | ✅ | MAINTENANCE 145 證交所 §39 / 其他經驗值 |
| `us_market_core` | ✅ | VIX zones Whaley (2000) Journal of Portfolio Management |
| `market_margin_core` | ✅ | maintenance 145/130 證交所 §39 |
| `valuation_core` | ✅ | yield 5% Graham (1949) / 5y percentile 業界共識 |
| `financial_statement_core` | ✅ | roe_high 15% Buffett (1987) / debt 60% 業界共識 |
| `shareholder_core` | ✅ | 4-level + STREAK 8 + concentration unit-based(user 拍版) |

---

## 16. P2 阻塞 6 + P5 Divergence 算法重寫(2026-05-12)

### 根因分析(production data 1263 stocks 揭露)

4 個 EventKind 觸發率遠超合理範圍(0.5–6 次/股/年):

| EventKind | 修前觸發率 | 根因 |
|---|---|---|
| RSI/KD/MACD BullishDivergence + BearishDivergence | 20–33/yr 🔴 | 固定 20-bar 間距比較,每天條件成立都 fire |
| institutional Foreign/Trust/Dealer LargeNetBuy/Sell | 91.83/yr 🔴 | Level trigger:每天 |z|≥2.0 都 fire,無狀態記憶 |
| foreign_holding LimitNearAlert | 50.06/yr 🔴 | Level trigger:持續在 near-limit zone 每天 fire |
| foreign_holding SignificantSingleDayChange | 34.87/yr 🔴 | 固定 0.5% 閾值不適應個股波動度 |

### 修法(全部 2026-05-12 完成,2 個 commit)

**Commit `2b1cbc7` — P2 阻塞 6 修法(institutional + foreign_holding)**

1. **institutional_core** LargeNetBuy/LargeNetSell — edge trigger:
   - 追蹤 `prev_z_abs`,僅在 `cur_z >= threshold && prev < threshold` 時 fire
   - Reference:Brown & Warner (1985) JFE 14:3-31 — 事件是狀態「轉變」不是狀態持續
   - 預期:91.83/yr → 6–12/yr

2. **foreign_holding LimitNearAlert** — edge trigger:
   - 加 `was_near_limit` boolean state,僅在進入 near-limit zone 當日 fire
   - Reference:Sheingold (1978) level trigger vs edge trigger 信號處理
   - 預期:50.06/yr → 2–6/yr

3. **foreign_holding SignificantSingleDayChange** — rolling z-score:
   - 將固定 0.5% (`change_threshold_pct`) 改為 `change_z_threshold=2.0` + `change_lookback=60d`
   - Reference:Fama, Fisher, Jensen & Roll (1969) IER 10(1) — 「顯著」= 個股歷史 2σ
   - 預期:34.87/yr → 10–15/yr

`ForeignHoldingParams` breaking change:刪 `change_threshold_pct`,加 `change_z_threshold` / `change_lookback`。

**Commit `8d3288a` — Pivot-based divergence 算法重寫(RSI/KD/MACD)**

核心問題:`bar[i]` vs `bar[i-20]` 每天跑一次 → 趨勢中連續 20–30 天都滿足 → 每天 fire。

Murphy (1999) p.248 / Wilder (1978) Ch.8 / Pring (1991) p.164 明確定義：
背離必須比較兩個連續的 swing HIGH/LOW 樞軸點（price 創新高但 indicator 未到新高）。

新 `detect_divergences()` 函式(三核心各自獨立 copy，對齊 §十四 零耦合原則):
```
PIVOT_N=3 (Lucas & LeBeau 1992 3-bar 確認)
MIN_PIVOT_DIST=10 (Murphy「20-60 intervals」實務下界)
is_swing_high: prices[pivot±k] 全嚴格 < prices[pivot] for k in 1..=PIVOT_N
每個 (prev_pivot, current_pivot) pair 最多觸發一次
confirm_date = pivot_idx + PIVOT_N (確認完成當天)
ind.abs() < 1e-12 skip warmup zeros (RSI/MACD 前幾 bar 為 0.0)
```

預期:20–33/yr 🔴 → 2–6/yr 🟢

### 驗證

- 168 tests passed / 0 failed / 0 warnings
- Production run: `tw_cores run-all --write` 1263 stocks, 539.8s, 0 errors
  - kd_core: 370,404 facts / macd_core: 322,703 / rsi_core: 109,129
  - institutional_core: 873,556 / foreign_holding_core: 433,695
- **待 user 驗**:跑 `p2_calibration_data.sql §2` 確認 5 組 EventKind 觸發率降到 🟢

### 已知未修項目(留 P5)

- `ma_core::AboveMaStreak` — C-2 scaling fn `(period*3/2).min(30).max(5)` 已落地但行為
  待 §3 SQL 確認(MA20 = 30d,應保持原 0.59/yr 水準)
- Divergence `MIN_PIVOT_DIST=10` — 若 §2 顯示仍 > 10/yr 可提高到 15-20

---

**最後更新**:2026-05-12(P2 阻塞 5c + 阻塞 6 + P5 divergence 算法重寫 全部收尾)
**Rust workspace**:24 crate / **168 tests passed** / 22 cores production verified
**alembic head**:`x3y4z5a6b7c8`(不變,本 session 0 migration)
**Production state(2026-05-12 全市場重跑後)**:
  - facts 重寫:institutional + foreign_holding + kd + macd + rsi (5 cores)
  - 總 facts 量級待 p2_calibration_data.sql 統計(修前 4.4M,修後 Divergence 降量預估 ↓15–40%)
