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
> Rust workspace:24 crate / 146 tests passed / 22 cores 全 inventory 註冊

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
| **FailureSwing** 四步邏輯 | **未實作 TODO**(framework `RsiEventKind::FailureSwing` 已存在) | spec §4.6 四步:RSI 進超買 → 退出 → 折返但未再進 → 跌破前低 |
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
| 「historical high」label | 未實作(spec 範例「reached 32% on 2026-04-20」needs lookback) | spec 範例語義待 user 寫定 |

### 3.2 margin_core(NULL skip 已修)
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `margin_change_pct_threshold` | 5.0 | spec §4.3 |
| `short_change_pct_threshold` | 10.0 | spec §4.3 |
| `short_to_margin_ratio_high` | 30.0 | spec §4.3 |
| `short_to_margin_ratio_low` | 5.0 | spec §4.3 |
| `MAINTENANCE_LOW_THRESHOLD` | 145.0 const | spec §4.5 列 EventKind 但 §4.3 未列 Param,目前寫死 |
| 「historical high」label | 未實作 | spec §4.6 範例 |

### 3.3 shareholder_core(2026-05-10 修)
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `STREAK_MIN_WEEKS` | 4 const | spec 對齊? |
| ~~detail JSONB key 命名~~ | ~~best-guess 英文~~ → ✅ **iterate 真 17 levels(2026-05-10 fix)** | 已對齊 Silver `holding_shares_per_derived.detail` 真結構 |
| **small/mid/large 邊界**(目前 best-guess) | small ≤ 5,000 股 / mid ≤ 50,000 股 / large > 50,000 股 | spec 拍版邊界張數 |
| **`concentration_index` 公式**(目前 best-guess) | `= large_holders_pct`(大戶集中度) | spec 是否要改 Top10 持股比 / Gini 等 |
| Skip rules | `差異數調整(說明4)` 異常 row 不算 | 確認 |

### 3.4 foreign_holding_core
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| `LimitNearAlert` 邏輯 | 未實作(`foreign_limit_pct` stored col 目前 NULL placeholder) | spec §6.5 + Silver schema 補欄 |
| `foreign_holding_ratio` threshold | 預設值待 user 寫定 | spec |

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

### 4.3 financial_statement_core(2026-05-10 修 round 2)
| 項目 | 目前值 | 待 user 確認 |
|---|---|---|
| detail JSONB key | ✅ **全形括號 `\u{FF08}` `\u{FF09}` + IFRS 中文 fallback chain** | 對齊 user 揭露 2330 真 detail key |
| **balance type 是 % common-size**(2026-05-10 揭露) | balance 全部是 % 對總資產比(`資產總額`=100,`權益總額`=68.84 等);income/cashflow 維持元值 | 是否要 user 寫 spec / 改 Silver builder pack 元值 + % 雙 dict?或維持 % only? |
| **ROE / ROA / TotalAssets / TotalLiabilities / TotalEquity 元值欄** | 全 = 0(skip cross-type 計算 income元 / balance%)| 等 user m3Spec/ 拍版 balance 元值 vs %;若拍 % only,RoeHigh / RoaHigh EventKind 永久不觸發,留作概念占位 |
| `debt_ratio_pct` | 直接讀「負債總額」% value(不再除 total_assets) | 對齊 balance 是 % 的事實 |
| Quarterly approximation | `Timeframe::Monthly`(enum 缺 Quarterly variant) | 是否需要加 Quarterly enum variant? |
| 8 EventKind | RoeHigh / DebtRatioRising 等待 balance 元值版 | spec 校準 |
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
| historical high label(margin / day_trading) | 1 天 | spec range definition |

---

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

**最後更新**:2026-05-09(v1.29 PR-9a milestone 收尾後)
**Rust workspace**:24 crate / 146 tests passed / 22 cores production verified
**alembic head**:`w2x3y4z5a6b7`
