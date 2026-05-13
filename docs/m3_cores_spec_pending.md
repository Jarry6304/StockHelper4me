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

### 1.1 Stage 4 Validator 22 條規則(全 deferred,等 user 寫定書頁追溯)

| 規則 ID | 模組 | 範圍 |
|---|---|---|
| **R4 / R5 / R6 / R7** | `validator/core_rules.rs` | 4 條 core impulse rules |
| **F1 / F2** | `validator/flat_rules.rs` | 2 條 Flat 形態 |
| **Z1 / Z2 / Z3 / Z4** | `validator/zigzag_rules.rs` | 4 條 Zigzag 形態 |
| **T1 ~ T10** | `validator/triangle_rules.rs` | 10 條 Triangle 形態 |
| **W1 / W2** | `validator/wave_rules.rs` | 2 條 wave-level |

**user 需在 m3Spec/neely_core.md 寫定每條的**:
- 具體 magnitude / duration / fib ratio 門檻
- ±4% 容差(對齊 spec §4.4)是否所有規則一致
- Neely 書頁追溯(`neely_page` 欄目前是「P0 Gate 校準時補」)

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

### 1.3 留 PR-3c / PR-4b / PR-5b / PR-6b(spec-blocked)

- **PR-3c**:Stage 4 R4-R7 + F/Z/T/W 22 條完整實作(等 §1.1 寫定)
- **PR-4b**:Diagonal Leading vs Ending sub_kind 區分(spec §10.3)
- **PR-4b**:R3 Diagonal exception(目前 strict Impulse,Diagonal 允許 W4 重疊 W1)
- **PR-5b**:exhaustive compaction 真正窮舉合法 paths(目前 pass-through)
- **PR-6b**:Power Rating 完整 Neely 書頁查表 + Fibonacci 接 monowave price

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
| `rust_compute/cores/system/tw_cores/src/main.rs` | Monolithic dispatcher(`run-all` subcommand)|
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

**最後更新**:2026-05-13(§20 PR-4b classifier sub_kind + PR-6b-1/2/3 Power Rating / Fibonacci / Missing / Emulation / Triggers + PR-5b Round 3 Pause 落地)
**Rust workspace**:24 crate / **286 tests passed**(231 + 55 new)/ 22 cores production verified
**alembic head**:`x3y4z5a6b7c8`(不變,本 session 0 migration)
**Production state**(2026-05-12 全市場重跑後):
  - facts 重寫:institutional + foreign_holding + kd + macd + rsi (5 cores)
  - 總 facts 量級待 p2_calibration_data.sql 統計(修前 4.4M,修後 Divergence 降量預估 ↓15–40%)

---

## §16. m3Spec/fundamental_cores.md §4.5 valuation_core spec staleness(2026-05-13)

接 v1.33 spec alignment 校驗收尾後,2026-05-13 audit「除 neely 外 21 cores」揭露
**`valuation_core` code 領先 spec** — 一個 m3Spec/ 文件回寫項目。

### 偏離項

| 來源 | EventKind 數 | 狀態 |
|---|---|---|
| `m3Spec/fundamental_cores.md` §4.5(spec)| **8 個** stay-in-zone | PerExtremeHigh/Low / PbrExtremeHigh/Low / YieldExtremeHigh / YieldHighThreshold / PerNegative / PbrBelowBookValue |
| `rust_compute/cores/fundamental/valuation_core/src/lib.rs:90-108`(code)| **14 個** Round 4 transition | 12 個 Entered/Exited transition + PerNegative + PbrBelowBookValue |

### 偏離原因(v1.30/v1.31 Round 4 落地紀錄)

CLAUDE.md v1.30 / v1.31 「Round 4 transition pattern」commit 把 6 個 stay-in-zone
EventKind(連日重複觸發)改為 12 個 Entered/Exited transition pattern,對齊
fear_greed_core 範本。production 驗證 facts 從 9M → 4.4M(↓51%),valuation 部分
2.0M → 189K(↓91%)。

詳見 CLAUDE.md v1.30 / v1.31 Round 4 段落 + commit history `2b1cbc7` 系列。

### 處理方式

🟢 **code = source of truth**(production-verified)。Spec 文件待 user 同步更新:

- m3Spec/fundamental_cores.md §4.5 `ValuationEventKind` enum:
  - 砍 6 個 stay-in-zone variant(PerExtremeHigh/Low / PbrExtremeHigh/Low /
    YieldExtremeHigh / YieldHighThreshold)
  - 加 12 個 Entered/Exited variant
  - PerNegative / PbrBelowBookValue 保留(本來就是 transition)
- 加 Round 4 transition pattern 說明段(對齊 fear_greed 範本 + 連日重複觸發降量
  ~51% 的設計動機)

### 不阻塞項

本偏離是 **doc-only staleness**,production code 已驗證正確,無下游 consumer 出問題。
**不擋 P0 Gate / P3 後續工作**。等 user 寫 m3Spec/ 下一輪 spec update 時順手收掉。

### 同 session(2026-05-13)落地項

- `ma_core` lib.rs:加 Reference doc comments(Mulloy 1994 DEMA/TEMA + Hull 2005 HMA)+ 6 precision tests(SMA/EMA/WMA/DEMA/TEMA/HMA 數學恆等校準)
- `revenue_core` lib.rs:加 Reference doc comments for 5 threshold defaults
  (Lakonishok/Shleifer 1994 + Moskowitz 2012 + Brown & Warner 1985 + 業界慣例出處)
- 兩個 cores spec ↔ code **完全對齊**,無 staleness

---

## §17. neely_core PR-3c-pre 架構層 migration(2026-05-13 後續)

接 v1.34 落地後 user 拍版 Path A(完整對齊 spec r5 ~3 週 / 9 sub-PR)。
PR-3c-pre 是首個 sub-PR,純架構層 breaking change,為後續 PR-3c-1~3 + PR-4b/5b/6b
打底。

### 落地範圍(breaking change,~~178 → 179 tests)

| 變更項 | 對應 spec |
|---|---|
| RuleId enum chapter-based 重寫 | architecture.md §9.3(line 928-1034)|
| NeelyPatternType:Diagonal → TerminalImpulse + RunningCorrection | architecture.md §9.6 + r5 修正 |
| PowerRating:Bullish/Bearish → FavorContinuation/AgainstContinuation | architecture.md §9.2(line 883-891)方向中性 |
| PostBehavior:3 variant → 8 variant 結構化 enum | architecture.md §9.2 line 894-918 |
| StructuralFacts:7 → 8 子欄位(加 extension_subdivision_pair + Alternation5Axes) | architecture.md §9.5 line 1081-1112 |
| NeelyDiagnostics:加 atr_dual_mode_diff + peak_memory_mb 改 f64 + stage_elapsed_ms → stage_timings_ms | architecture.md §15.1 line 1413-1421 |
| ImpulseExtension / WaveNumber / EmulationKind / AlternationAxis / WaveAbc / TriangleWave / TriangleVariant 等 enum 新增 | architecture.md §9.3 line 1036-1044 |

### RuleId 遷移對照表(r2 → r5)

| r2 simple | r5 chapter-based | 用途 |
|---|---|---|
| `RuleId::Core(1-3)` | `RuleId::Ch5Essential(1-3)` | R1/R2/R3 完整實作 |
| `RuleId::Core(4-7)` | `RuleId::Ch3PreConstructive { rule: N, condition: 'a', ... }` | R4-R7 Deferred |
| `RuleId::Flat(1)` | `RuleId::Ch5FlatMinBRatio` | F1 b-wave 回測比 |
| `RuleId::Flat(2)` | `RuleId::Ch5FlatMinCRatio` | F2 c-wave 比例 |
| `RuleId::Zigzag(1)` | `RuleId::Ch5ZigzagMaxBRetracement` | Z1 b ≤ 61.8%×a |
| `RuleId::Zigzag(2)` | `RuleId::Ch11ZigzagWaveByWave { wave: C }` | Z2 c-wave 範圍 |
| `RuleId::Zigzag(3)` | `RuleId::Ch4ZigzagDetour` | Z3 DETOUR Test |
| `RuleId::Zigzag(4)` | `RuleId::Ch5ZigzagCTriangleException` | Z4 Triangle 例外 |
| `RuleId::Triangle(1-3)` | `Ch11TriangleVariantRules { variant: Horizontal/Irregular/RunningLimiting, wave: C }` | Contracting Limiting 3 種 |
| `RuleId::Triangle(4)` | `Ch5TriangleBRange` | Triangle b-wave 範圍 |
| `RuleId::Triangle(5)` | `Ch5TriangleLegContraction` | 每段更短 |
| `RuleId::Triangle(6)` | `Ch5TriangleLegEquality5Pct` | 等邊 5% 容差 |
| `RuleId::Triangle(7-9)` | `Ch11TriangleVariantRules { variant: HorizontalExpanding/.../RunningExpanding, wave: E }` | Expanding 3 種 |
| `RuleId::Triangle(10)` | `Ch6TriangleExpandingNonConfirmation` | Expanding Non-Confirmation |
| `RuleId::Wave(1)` | `Ch11ImpulseWaveByWave { ext: ThirdExt, wave: Three }` | Impulse Extension 6 情境 |
| `RuleId::Wave(2)` | `Ch12FibonacciInternal` | Essential + Fibonacci 內部比 |

### PowerRating 遷移對照表

| r2 簡稱 | r5 方向中性 | 數值對映 |
|---|---|---|
| StrongBullish | StronglyFavorContinuation | +3 |
| Bullish | ModeratelyFavorContinuation | +2 |
| SlightBullish | SlightlyFavorContinuation | +1 |
| Neutral | Neutral | 0 |
| SlightBearish | SlightlyAgainstContinuation | -1 |
| Bearish | ModeratelyAgainstContinuation | -2 |
| StrongBearish | StronglyAgainstContinuation | -3 |

### NeelyPatternType 遷移

| r2 | r5 |
|---|---|
| `Diagonal { sub_kind: Leading }` | `TerminalImpulse` |
| `Diagonal { sub_kind: Ending }` | `TerminalImpulse` |
| `Zigzag { sub_kind: ZigzagKind::Single }` | `Zigzag { sub_kind: ZigzagVariant::Normal }` |
| `Flat { sub_kind: FlatKind::Regular }` | `Flat { sub_kind: FlatVariant::Common }` |
| `Triangle { sub_kind: TriangleKind::Contracting }` | `Triangle { sub_kind: TriangleVariant::HorizontalLimiting }` |
| (新增) | `RunningCorrection`(獨立 top-level variant) |

### 改動檔(11 個)

| 檔 | 改動類型 |
|---|---|
| `output.rs` | RuleId / NeelyPatternType / PowerRating / PostBehavior / StructuralFacts / NeelyDiagnostics 全部重寫 |
| `validator/core_rules.rs` | RuleId::Core → Ch5Essential / Ch3PreConstructive |
| `validator/flat_rules.rs` | RuleId::Flat → Ch5FlatMin{B,C}Ratio |
| `validator/zigzag_rules.rs` | RuleId::Zigzag → 4 個 Ch4/Ch5/Ch11 variant |
| `validator/triangle_rules.rs` | RuleId::Triangle → 7 個 Ch5/Ch6/Ch11 variant |
| `validator/wave_rules.rs` | RuleId::Wave → Ch11ImpulseWaveByWave + Ch12FibonacciInternal |
| `classifier/mod.rs` | TerminalImpulse 取代 Diagonal + .copied() → .cloned() |
| `compaction/mod.rs` | PowerRating 方向中性語意 |
| `power_rating/{mod,table}.rs` | PowerRating + TerminalImpulse 命名 |
| `triggers/mod.rs` | RuleId 新名 + TerminalImpulse |
| `facts.rs` | produce_facts 序列化新 enum + diagnostics 欄位名 |
| `lib.rs` | stage_elapsed_ms → stage_timings_ms(2 處)|

### 留 PR-3c-1 ~ PR-6b-3 補

- 22 條 Deferred 規則的具體實作邏輯(目前全部 stub return Deferred)
- Three Rounds Compaction 完整實作(目前 pass-through)
- Power Rating 7 級完整 Ch10 查表(目前 best-guess 4 級)
- Fibonacci 5 ratios + Internal/External per pattern(目前寫死 10 ratios)
- Missing Wave + Emulation + Triggers 完整實作
- StructuralFacts 8 子欄位 default → 真實計算
- atr_dual_mode_diff P0 Gate 校準時填入

### 沙箱驗證

```bash
cd rust_compute && cargo test --workspace --release --no-fail-fast
# 179 tests passed / 0 failed / 0 warnings
# (178 → 179:+1 terminal_impulse classifier test)
```

### 風險

🟡 RuleId 是 **breaking change**,但 production neely facts 量級極小
(~5 facts/stock × 1700 stocks ≈ 8500 rows),P0 Gate 後可全量重算,不需
alembic migration。

🟢 其他:
- 0 alembic / 0 collector.toml / 0 Python / 0 schema 改動(純 Rust)
- m2 收尾不阻塞
- production 既有 4.4M facts 不受影響

---

## §18. neely_core PR-3c-1 Wave-level 8 條規則落地(2026-05-13 後續)

接 PR-3c-pre RuleId chapter-based migration 後動工 PR-3c-1。8 條 wave-level
規則完整實作:F1-F2(Flat)+ Z1-Z3(Zigzag,Z4 仍 Deferred)+ W1-W2(通用 Impulse
+ Fibonacci)。

### 新增檔

- `validator/helpers.rs`(78 行)— 共用 helper:
  - `FIB_TOLERANCE_PCT = 4.0`(§4.2 ±4% Fibonacci 容差)
  - `NEELY_FIB_RATIOS_PCT = [38.2, 61.8, 100.0, 161.8, 261.8]`(spec 5 個標準比率)
  - `magnitude(c)` / `safe_pct(num, denom)` / `within_tolerance` / `matches_any_fib_ratio`

### 7 條落地規則對映 spec

| 規則 | RuleId | spec ref | 邏輯 |
|---|---|---|---|
| F1 | `Ch5FlatMinBRatio` | Ch5 p.5-34 | b/a ≥ 57.8%(= 61.8% - 4%)→ Pass(Flat-consistent);否則 NotApplicable |
| F2 | `Ch5FlatMinCRatio` | Ch5 p.5-34~36 | F1 Pass 且 c/b ≥ 34.2%(= 38.2% - 4%)→ Pass;c 過短 → Fail |
| Z1 | `Ch5ZigzagMaxBRetracement` | Ch5 p.5-41(v1.9 修正)| b/a ≤ 65.8%(= 61.8% + 4%)→ Pass(Zigzag-consistent);否則 NotApplicable |
| Z2 | `Ch11ZigzagWaveByWave{wave:C}` | Ch11 p.11-17 + Ch5 p.5-41~42 | Z1 Pass 且 c/a ≥ 34.2% → Pass(涵蓋 Truncated/Normal/Elongated 3 sub-type);c 過短 → Fail |
| Z3 | `Ch4ZigzagDetour` | Ch4 p.4-15~20 | DETOUR Test:c overshoot > 2.5×a + b/a < 38.2% → NotApplicable(Impulse-like);否則 Pass |
| Z4 | `Ch5ZigzagCTriangleException` | Ch5 line 968 | 仍 Deferred(需 Triangle context,PR-3c-2 完成後 classifier 可重新評估)|
| W1 | `Ch11ImpulseWaveByWave{ext,wave}` | Ch11 p.11-4~18 | 5-wave actionable wave magnitude > 0 → Pass(W1/W3/W5 哪條最長分類由 classifier 在 PR-4b 用同 helper)|
| W2 | `Ch12FibonacciInternal` | Ch12(精華版 Ch12)| 3-wave c/a 或 5-wave W3/W1, W5/W1 任一匹配 Fib 比率 ±4% → Pass;無匹配 → NotApplicable |

### 設計選擇

- 規則用 `NotApplicable` 表示「此 candidate 不是該規則的 pattern type」,**不阻塞**
  `overall_pass`(對齊 spec §10.3 deferred 暫時通過 + NotApplicable 不阻塞原則)
- 真正 `Fail` 只用於「結構違反(c 過短到無法描述為任何 pattern)」
- 規則之間用「先 type filter,後 sub-rule check」pattern(例:F2 先確認 F1 Pass-like
  條件,Z2 先確認 Z1 Pass-like 條件)
- 容差全部寫死 const(§4.5 / §6.6 不可外部化):FIB_TOLERANCE_PCT = 4.0 /
  FLAT_B_MIN_PCT = 57.8 / FLAT_C_MIN_PCT = 34.2 / ZIGZAG_B_MAX_PCT = 65.8 /
  ZIGZAG_C_MIN_PCT = 34.2 / DETOUR_OVERSHOOT_RATIO = 2.5 / W1_EXTENSION_RATIO = 1.1

### 留 PR-3c-2 / PR-4b 補

- **Z4 Triangle exception**:需要 Triangle context;PR-3c-2 Triangle 規則落地後
  classifier 可重新呼叫 Z4 with Triangle 上下文
- **W1 sub_kind 分類**:目前 W1 只 Pass/NotApplicable,具體哪個 ext(1st/3rd/5th/Non)
  由 classifier 在 PR-4b 用 helper magnitude 比較決定 NeelyPatternType sub_kind
- **F1/F2 sub_kind 分類**:同上,7 個 FlatVariant variant 在 PR-4b classifier 決定
- **Z2 sub_kind 分類**:Truncated/Normal/Elongated 3 個 ZigzagVariant variant 同上

### 沙箱驗證

```bash
cd rust_compute && cargo test --workspace --release --no-fail-fast
# 179 → 213 passed / 0 failed / 0 warnings
# 新增 34 個 unit test:
#   helpers.rs:6 個(safe_pct / within_tolerance / matches_any_fib_ratio 邊界)
#   flat_rules.rs:6 個(F1 in/out range / 5-wave N/A / F2 too short / F2 normal / F2 non-Flat)
#   zigzag_rules.rs:12 個(Z1 small/boundary/big / Z2 normal/truncated/elongated/too short /
#     Z3 normal/impulse-like / Z4 always deferred / 5-wave N/A)
#   wave_rules.rs:7 個(W1 5wave/3wave/zero-mag / W2 3wave 100%/61.8%/no-match /
#     5wave W3/W1=161.8% / no Fib + FIB_TOLERANCE_PCT)
```

### 風險

🟢 低:
- 0 alembic / 0 Python / 0 collector.toml / 0 schema 改動
- production 既有 facts 不受影響(neely scenario 數 ~ 1-2/stock,本變更只擴展
  validator output,classifier 行為仍對齊 PR-3c-pre)
- 既有 R1-R3 / W1-W2 行為對齊 spec 新閾值(W1 不會 Fail)
- Rollback:單 commit `git revert` 即可

---

## §19. neely_core PR-3c-2 + PR-3c-3 Triangle + R4-R7 落地(2026-05-13 後續)

接 PR-3c-1 完成後動工 PR-3c-2 + PR-3c-3(同 commit 收尾,平行 sub-PR 對應
plan §12.6 sequence)。

### PR-3c-2:T4-T6 Triangle 通用 Ch5 規則

對齊 m3Spec/neely_rules.md Ch5 line 1387 + 1547-1567。

| 規則 | RuleId | 邏輯 |
|---|---|---|
| T4 | `Ch5TriangleBRange` | 5-wave + b/a ∈ [34.2%, 265.8%](= 38.2%-4% ~ 261.8%+4%)→ Pass(Triangle-consistent);超出 → NotApplicable |
| T5 | `Ch5TriangleLegContraction` | 5-wave + T4 Pass-like + e<d<c → Pass(Contracting Triangle);e ≥ a → NotApplicable(Expanding);其他結構不一致 → Fail |
| T6 | `Ch5TriangleLegEquality5Pct` | 5-wave + T4 Pass-like + (|a-c|/a ≤ 5% OR |b-d|/b ≤ 5%)→ Pass(leg equality);無 → NotApplicable |

T1-T3(Contracting Limiting 變體 wave-c)/ T7-T9(Expanding 變體 wave-e)/ T10
(Ch6 Expanding Non-Confirmation)維持 Deferred —這些是 sub-variant 特定規則,
PR-4b classifier 識別 TriangleVariant 後 dispatch。

容差寫死:`TRIANGLE_B_MIN_PCT=34.2 / TRIANGLE_B_MAX_PCT=265.8 / TRIANGLE_LEG_EQ_TOLERANCE_PCT=5.0`

### PR-3c-3:R4-R7 Ch3 Pre-Constructive m2/m1 ratio 範圍分類

對齊 m3Spec/neely_rules.md Ch3 p.3-48~60 line 422-493。

| 規則 | RuleId | m2/m1 範圍(含 ±4%) |
|---|---|---|
| R4 | `Ch3PreConstructive { rule: 4, condition: 'a', ... }` | 57.8% < ratio < 104% |
| R5 | `Ch3PreConstructive { rule: 5, condition: 'a', ... }` | 96% ≤ ratio < 165.8% |
| R6 | `Ch3PreConstructive { rule: 6, condition: 'a', ... }` | 157.8% ≤ ratio ≤ 265.8% |
| R7 | `Ch3PreConstructive { rule: 7, condition: 'a', ... }` | ratio > 257.8% |

落實到 candidate 時:
- 計算 m2/m1 比值(m1 = first monowave magnitude,m2 = second monowave)
- 哪個範圍 match → 該規則 Pass,其他 R4-R7 NotApplicable
- 邊界區重疊(±4% 容差導致 96-104% / 157.8-165.8% / 257.8-265.8% 兩規則並 Pass)

**簡化版**:只實作 ratio 範圍分類。具體 Condition × Category × sub_rule_index
完整 200+ 分支 Structure Label 決策樹(產出 `:F3 / :c3 / :sL3 / :s5 / :L5`
標籤)留 PR-4b classifier 的 structure_labeler 系統。

新 helper:`m2_over_m1_pct(candidate, classified) -> Option<f64>`
(取 monowave_indices[0/1] 的 magnitude 比值;m1 ≈ 0 → None)

### Stage 4 完整度更新

| 規則組 | 狀態 |
|---|---|
| R1-R3(Ch5 Essential)| ✅ 完整實作(PR-3b)|
| **R4-R7(Ch3 Pre-Constructive)** | **✅ ratio 範圍分類(PR-3c-3);Structure Label 完整邏輯留 PR-4b** |
| F1-F2(Ch5 Flat min)| ✅ 完整實作(PR-3c-1)|
| Z1-Z3(Ch5/Ch4/Ch11 Zigzag)| ✅ 完整實作(PR-3c-1)|
| Z4(Ch5 Zigzag Triangle exception)| 🟡 Deferred(需 Triangle context)|
| **T4-T6(Ch5 Triangle 通用)** | **✅ 完整實作(PR-3c-2)** |
| T1-T3 / T7-T10(Ch11/Ch6 Triangle sub-variant)| 🟡 Deferred(需 classifier sub_kind 識別)|
| W1-W2(Ch11/Ch12 通用)| ✅ 完整實作(PR-3c-1)|

**22 條 → 14 條完整實作 + 8 條 Deferred(sub-variant / context-dependent)**

### 沙箱驗證

```bash
cd rust_compute && cargo test --workspace --release --no-fail-fast
# 213 → 231 tests passed / 0 failed / 0 warnings
# 新增 18 個 unit test:
#   PR-3c-2 triangle_rules.rs:11 個(T4 in/out range / T5 contracting/non-contracting/expanding /
#     T6 leg ac equal / bd equal / no equality / constants / run returns 10)
#   PR-3c-3 core_rules.rs:7 個(r4 80% / r5 130% / r6 200% / r7 300% / short candidate /
#     zero m1 / mutually exclusive non-boundary + 100% boundary both pass)
```

### 風險

🟢 低:
- 0 alembic / 0 Python / 0 collector.toml / 0 schema 改動
- production 既有 facts 不受影響
- 留 PR-4b 補:T1-T3 / T7-T10 sub-variant 邏輯 / Z4 Triangle exception /
  R4-R7 Structure Label 200+ 分支

---

## §20. neely_core 9 sub-PR sequence 完整收尾(2026-05-13 後續)

接 PR-3c-3 R4-R7 落地後同日連續推進 PR-4b → PR-6b-3 → PR-5b,9 個 sub-PR 全部
完成。從 178 tests(PR-3c-pre 起點)→ 286 tests(+108 across 9 sub-PR)。

### PR-4b:Classifier sub_kind 完整識別

`classifier/mod.rs` 完整重寫(665 行 with tests):

- 5-wave 分類:Impulse / TerminalImpulse / Triangle(三選)
  - R3 fail → TerminalImpulse(取代 r2 Diagonal)
  - Triangle 規則(T4 + T5/T6)Pass + leg contracting/equality → Triangle
- ImpulseExtension 分類:1st/3rd/5th/Non/FifthFailure(by longest wave magnitude)
- TriangleVariant 分類:9 variants(Horizontal/Irregular/Running × Limiting/NonLimiting/Expanding)
- 3-wave 分類:Flat 7 變體 / Zigzag 3 變體 / RunningCorrection top-level variant
  - b/a > 142.2% → RunningCorrection
  - b/a ≥ 57.8% → Flat(Common/BFailure/CFailure/Irregular/IrregularFailure/Elongated/DoubleFailure)
  - b/a < 57.8% → Zigzag(Normal/Truncated/Elongated by c/a)

### PR-6b-1:Power Rating 7 級完整 Ch10 查表

`power_rating/{mod,table}.rs` 完整重寫:

- Ch10 line 2006-2014 完整 7 級表(±3/±2/±1/0)
- direction-aware:Up 順趨勢 + Down 翻轉(line 2006「向下則反號」)
- Triangle / TerminalImpulse 內部 override → Neutral(line 2021)
- max_retracement 對映:0/±1/±2/±3 → 1.0/0.90/0.80/0.65(line 2017-2020)
- `apply_to_forest(forest)` 同時寫 power_rating + max_retracement

### PR-6b-2:Fibonacci 5 ratios + per-pattern alignment

`fibonacci/{ratios,projection,mod}.rs` 重寫:

- 從 r2 10 ratios 砍至 spec r5 5 standard(38.2/61.8/100/161.8/261.8)
- 加 NEELY_FIB_RATIOS_PCT 百分比版本 + match_fib_ratio helper
- `compute_internal_alignment(scenario, classified)`:per-pattern 比例匹配
  - Impulse:W3/W1 + W5/W1
  - Zigzag/Flat:c/a + c/b
  - Triangle:c/a + d/b
- 寫入 `Scenario.structural_facts.fibonacci_alignment`(§9.5 8 子欄位之一)

### PR-6b-3:Missing Wave + Emulation + Triggers 完整實作

#### Missing Wave(missing_wave/mod.rs)

Ch12 line 2580-2597 Min data points lookup:
- Impulse/TerminalImpulse min=8 / Zigzag/Flat min=5 / Triangle min=13 / Combination 13~18
- 標到 `structural_facts.overlap_pattern.label`(暫用,避免 schema 改動)

#### Emulation(emulation/mod.rs)

5 種 EmulationKind 啟發式偵測(§9.3 line 1026):
- DoubleFailureAsTriangle(Flat DoubleFailure 自然收斂)
- DoubleFlatAsImpulse(Combination DoubleThree)
- FirstExtAsZigzag(c/a < 0.5)
- FifthExtAsZigzag(c/a > 2.0)
- MultiZigzagAsImpulse(預留,Combination TripleThree)

#### Triggers(triggers/mod.rs)完整重寫

接 classified slice 拿真實 price endpoints,7 種 pattern_type 各自的 invalidation triggers:
- Impulse:R1(InvalidateScenario)+ R3(WeakenScenario → TerminalImpulse)
- TerminalImpulse:R1 only(R3 deferred)
- Zigzag:Ch5 Z1 b 跨 a 起點 → invalidate
- Flat:F1 b > 142.2% → Running Correction WeakenScenario
- Triangle:Ch12 B-D trendline 穿破 + Ch5 wave-e thrust 異常
- Combination:Ch8 結構違反
- RunningCorrection:Ch12 後續延伸 Impulse < 161.8% → 失效

direction-aware:Down 趨勢 → PriceBreakAbove(取代 Below)

### PR-5b:Three Rounds Compaction 簡化版

完整 exhaustive nested compaction(需 sub-wave 嵌套結構)留未來 PR;
本 PR-5b 聚焦於 spec §8.4 Round3PauseInfo 整體狀態追蹤:

新增 output.rs:
- `Round3PauseInfo` struct(scenarios_affected / last_l_label_date / strategy_implication)
- `NeelyCoreOutput.round3_pause: Option<Round3PauseInfo>` 頂層欄位
- `Scenario.awaiting_l_label: bool`(雙標設計,§8.4 line 777)

lib.rs compute() 加 Round 3 Pause detection:
- 全部 scenario awaiting_l_label = true → Some(Round3PauseInfo)
- forest 空但 candidates 非空 → Some(Round3PauseInfo,「無 scenario 通過 validator」)
- 否則 → None

### 沙箱驗證

```bash
cd rust_compute && cargo test --workspace --release --no-fail-fast
# 231 → 286 tests passed / 0 failed / 0 warnings
# 新增 55 個 unit test:
#   classifier:15(5-wave Impulse/TerminalImpulse/Triangle + Extension + Flat 7 + Zigzag 3 + RunningCorrection)
#   power_rating:11(7 級 + direction flip + Triangle override + max_retracement + apply_to_forest)
#   fibonacci:9(5 ratios + project + per-pattern alignment + no-match)
#   missing_wave:9(lookup 各 pattern + apply_to_forest)
#   emulation:7(5 kinds + apply_to_forest)
#   triggers:8(7 pattern_type + direction flip + apply_to_forest)
#   compaction:4(Round 3 pause 觸發條件)
```

### 完整 9 sub-PR sequence 狀態

| PR | 狀態 | 範圍 |
|---|---|---|
| PR-3c-pre | ✅ | RuleId chapter-based + TerminalImpulse + Output/Diagnostics 對齊 spec |
| PR-3c-1 | ✅ | F1-F2 + Z1-Z3 + W1-W2 wave-level 規則 |
| PR-3c-2 | ✅ | T4-T6 Triangle 通用 Ch5 規則 |
| PR-3c-3 | ✅ | R4-R7 Ch3 Pre-Constructive m2/m1 ratio 範圍分類 |
| PR-4b | ✅ | classifier sub_kind 完整識別(本 batch) |
| PR-5b | ✅ | Round 3 Pause + awaiting_l_label(本 batch,簡化版) |
| PR-6b-1 | ✅ | Power Rating Ch10 7 級完整查表(本 batch) |
| PR-6b-2 | ✅ | Fibonacci 5 ratios + per-pattern alignment(本 batch) |
| PR-6b-3 | ✅ | Missing Wave + Emulation + Triggers 完整實作(本 batch) |

### 留未來 PR

- 完整 exhaustive nested compaction(需 candidate generator 支援 sub-wave 嵌套,改造 Stage 3)
- Z4 Triangle exception 在 classifier 中重新評估(目前 validator Deferred)
- T1-T3 / T7-T10 sub-variant 規則完整 wave-c/wave-e 具體門檻(目前 Deferred,
  classifier sub_kind 識別已落,規則細節 P0 Gate 五檔校準後補)
- R4-R7 Structure Label 200+ 分支(`:F3/:c3/:sL3/:s5/:L5` 完整決策樹)
- DegreeCeiling / CrossTimeframeHints(§8.5/8.6)
- P0 Gate 五檔(0050/2330/3363/6547/1312)production 校準

### 風險

🟢 低:
- 0 alembic / 0 Python / 0 collector.toml / 0 schema 改動
- 0 cargo warnings
- production 既有 facts 不受影響(Scenario 加 awaiting_l_label 默認 false,
  NeelyCoreOutput 加 round3_pause 默認 None — 都不影響既有 serialization)
- 22 條規則完整度:14 完整 + 8 Deferred(sub-variant specific,classifier 已能識別)

