# Cores Schema Map(22 cores 為主軸)

> **本檔定位**:**by-core 反查** — 給定 core 名 → 立刻看到讀哪張 Silver / 寫哪三表 / spec 章節。與 [`m3Spec/cores_overview.md`](../m3Spec/cores_overview.md)(by-spec 契約)、[`m2Spec/layered_schema_post_refactor.md`](../m2Spec/layered_schema_post_refactor.md)(by-table 規範)**反方向**。
> **盤點時間**:2026-05-14
> **總 cores**:22 / 已實作 22 ✅ / spec 有但 code 未實作 1(`business_indicator_core`,留 follow-up)

---

## 1. 文件定位

| 想做的事 | 文件 |
|---|---|
| 看 core 讀寫哪些表(本檔)| 本檔 §2 / §4 |
| 看 core 的 Params / EventKind / Output 詳細規範 | `m3Spec/{子類}_cores.md` 對應章節(本檔 §2 / §3 連結) |
| 看 Silver 表的欄位細節 | [`m2Spec/layered_schema_post_refactor.md`](../m2Spec/layered_schema_post_refactor.md) §3 / §4 |
| 看 M3 三表的 PK / 寫入規約 | [`m3Spec/cores_overview.md`](../m3Spec/cores_overview.md) §3 / §六 / §7 |
| 看單一表速查 | [`docs/schema_reference.md`](./schema_reference.md) |

---

## 2. Cores 全清單(22 cores)

> **欄位說明**:`寫入 M3` 用 `IV` = `indicator_values`、`SS` = `structural_snapshots`、`F` = `facts`。所有 cores 走 `IndicatorCore` 或 `WaveCore` trait,都會寫 `facts`。

| Core | 子類 | 優先級 | 主要 Silver 輸入 | 寫入 M3 | spec 錨點 |
|---|---|---|---|---|---|
| `neely_core` | Wave | P0 | `price_daily_fwd` | SS + F | [neely_core_architecture](../m3Spec/neely_core_architecture.md) + [neely_rules](../m3Spec/neely_rules.md) |
| `macd_core` | Indicator-Momentum | P1 | `price_daily_fwd` | IV + F | [momentum §三](../m3Spec/indicator_cores_momentum.md) |
| `rsi_core` | Indicator-Momentum | P1 | `price_daily_fwd` | IV + F | [momentum §四](../m3Spec/indicator_cores_momentum.md) |
| `kd_core` | Indicator-Momentum | P1 | `price_daily_fwd` | IV + F | [momentum §五](../m3Spec/indicator_cores_momentum.md) |
| `adx_core` | Indicator-Momentum | P1 | `price_daily_fwd` | IV + F | [momentum §六](../m3Spec/indicator_cores_momentum.md) |
| `ma_core` | Indicator-Momentum | P1 | `price_daily_fwd` | IV + F | [momentum §七](../m3Spec/indicator_cores_momentum.md) |
| `bollinger_core` | Indicator-Volatility | P1 | `price_daily_fwd` | IV + F | [volatility §三](../m3Spec/indicator_cores_volatility.md) |
| `atr_core` | Indicator-Volatility | P1 | `price_daily_fwd` | IV + F | [volatility §四](../m3Spec/indicator_cores_volatility.md) |
| `obv_core` | Indicator-Volume | P1 | `price_daily_fwd` | IV + F | [volume §三](../m3Spec/indicator_cores_volume.md) |
| `institutional_core` | Chip | P2 | `institutional_daily_derived` | IV + F | [chip §三](../m3Spec/chip_cores.md) |
| `margin_core` | Chip | P2 | `margin_daily_derived` | IV + F | [chip §四](../m3Spec/chip_cores.md) |
| `foreign_holding_core` | Chip | P2 | `foreign_holding_derived` | IV + F | [chip §五](../m3Spec/chip_cores.md) |
| `shareholder_core` | Chip | P2 | `holding_shares_per_derived` | IV + F | [chip §六](../m3Spec/chip_cores.md) |
| `day_trading_core` | Chip | P2 | `day_trading_derived` | IV + F | [chip §七](../m3Spec/chip_cores.md) |
| `revenue_core` | Fundamental | P2 | `monthly_revenue_derived` | IV + F | [fundamental §三](../m3Spec/fundamental_cores.md) |
| `valuation_core` | Fundamental | P2 | `valuation_daily_derived` | IV + F | [fundamental §四](../m3Spec/fundamental_cores.md) |
| `financial_statement_core` | Fundamental | P2 | `financial_statement_derived` | IV + F | [fundamental §五](../m3Spec/fundamental_cores.md) |
| `taiex_core` | Environment | P2 | `taiex_index_derived`(雙序列 TAIEX/TPEx) | IV + F | [environment §三](../m3Spec/environment_cores.md) |
| `us_market_core` | Environment | P2 | `us_market_index_derived`(SPY + VIX) | IV + F | [environment §四](../m3Spec/environment_cores.md) |
| `exchange_rate_core` | Environment | P2 | `exchange_rate_derived` | IV + F | [environment §五](../m3Spec/environment_cores.md) |
| `fear_greed_core` | Environment | P2 | `fear_greed_index`(直讀 Bronze,**架構例外**) | IV + F | [environment §六](../m3Spec/environment_cores.md) |
| `market_margin_core` | Environment | P2 | `market_margin_maintenance_derived` | IV + F | [environment §七](../m3Spec/environment_cores.md) |

### 寫入 M3 表選擇規則

依 `cores_overview.md` §7.1「三類資料寫入分流」:

- **時序型 Output**(每日有值 / 多 row per stock × date) → `indicator_values`(IV)
- **結構快照**(每 stock × timeframe 一份完整結構) → `structural_snapshots`(SS)
- **事件邊界**(boolean 觸發 / 量化事實) → `facts`(F)— 所有 cores 都寫

目前 SS 寫入者只有 `neely_core`(Scenario Forest);其他 21 cores 寫 IV + F。

### Spec 有但 code 未實作

| Core | 預計 spec | 狀態 |
|---|---|---|
| `business_indicator_core` | [environment §八 r3](../m3Spec/environment_cores.md)(景氣指標 — 領先/同時/落後/燈號) | Silver `business_indicator_derived` 已建,Core 未實作 |

---

## 3. 子類分組 deep-link

每 sub-class 一節,標 spec 路徑 + 共用 Silver 輸入 + typical Params 維度。**不重述 spec 內容**。

### 3.1 Wave(P0,1 core)

- **Spec**:[`m3Spec/neely_core_architecture.md`](../m3Spec/neely_core_architecture.md)(1763 行)+ [`m3Spec/neely_rules.md`](../m3Spec/neely_rules.md)(2657 行)
- **共用 Silver 輸入**:`price_daily_fwd`
- **Trait**:`WaveCore`(`m3Spec/cores_overview.md` §3.3)
- **Params 維度**:`timeframe / atr_period=14 / forest_max_size=200(v2 調) / compaction_timeout_secs=60 / beam_width / use_fixed_reference_scale(P0 Gate §15.4 待實作)`
- **寫入**:`structural_snapshots`(Scenario Forest)+ `facts`(power_rating / pattern_classified 等事件)
- **Cores**:`neely_core`

### 3.2 Indicator — Momentum(P1,5 cores)

- **Spec**:[`m3Spec/indicator_cores_momentum.md`](../m3Spec/indicator_cores_momentum.md)
- **共用 Silver 輸入**:`price_daily_fwd`(close + OHLCV)
- **Trait**:`IndicatorCore`
- **Params 維度**(各 core 個別):
  - `macd_core`:`12 / 26 / 9` + `MIN_PIVOT_DIST=12`(v4)+ `MIN_*_CROSS_SPACING`
  - `rsi_core`:`period=14 / overbought=70 / oversold=30 / MIN_PIVOT_DIST=12`
  - `kd_core`:`period=9 / k_smooth=3 / d_smooth=3 / 80 / 20 / MIN_KD_CROSS_SPACING=15`
  - `adx_core`:`period=14 / strong=25 / very_strong=50`
  - `ma_core`:`Vec<MaSpec>` 多均線同算(SMA/EMA/WMA/Dema/Tema/Hma)+ `MIN_MA_CROSS_SPACING=15`
- **共同 EventKind 模式**:GoldenCross / DeathCross / Divergence(pivot-based,v1.31 P5 重寫)
- **Cores**:`macd_core` / `rsi_core` / `kd_core` / `adx_core` / `ma_core`

### 3.3 Indicator — Volatility(P1,2 cores)

- **Spec**:[`m3Spec/indicator_cores_volatility.md`](../m3Spec/indicator_cores_volatility.md)
- **共用 Silver 輸入**:`price_daily_fwd`
- **Params 維度**:
  - `bollinger_core`:`period=20 / std_multiplier=2 / SQUEEZE_STREAK_MIN=5`(r4 12 個 EventKind 含 Entered/Exited transition)
  - `atr_core`:`period=14 / EXPANSION_LOOKBACK=14`
- **Cores**:`bollinger_core` / `atr_core`

### 3.4 Indicator — Volume(P1,1 core;P3 有 vwap / mfi 未實作)

- **Spec**:[`m3Spec/indicator_cores_volume.md`](../m3Spec/indicator_cores_volume.md)
- **共用 Silver 輸入**:`price_daily_fwd`(close + volume,volume 已 S1 後復權處理)
- **Params 維度**:`obv_core`:`anchor_date=None / ma_period=20 / MIN_PIVOT_DIST=12`(v4 pivot-based 重寫)
- **Cores**:`obv_core`(P3 留 `vwap_core` / `mfi_core` 待新增)

### 3.5 Chip(P2,5 cores)

- **Spec**:[`m3Spec/chip_cores.md`](../m3Spec/chip_cores.md)(r4)
- **Silver 輸入**:**各 core 對接不同 derived 表**(見 §2)
- **共同 EventKind 模式**(r4):多用 edge trigger / Entered/Exited transition 避免 stay-in-zone 每日重複觸發
- **Params 維度**:
  - `institutional_core`:`large_transaction_z=2.0 / lookback_for_z=60 / streak_min_days=3`
  - `margin_core`:`margin_change_pct=5.0 / short_change_pct=10.0 / 維持率 145 / 130`
  - `foreign_holding_core`:`change_z_threshold=2.0 / change_lookback=60 / 雙窗口 60d+252d`
  - `shareholder_core`:`small=5 / large=1000 張 / STREAK_MIN_WEEKS=8`(r4 4-level)
  - `day_trading_core`:`ratio_high=30 / ratio_low=5 / momentum_lookback=5`
- **Cores**:`institutional_core` / `margin_core` / `foreign_holding_core` / `shareholder_core` / `day_trading_core`

### 3.6 Fundamental(P2,3 cores)

- **Spec**:[`m3Spec/fundamental_cores.md`](../m3Spec/fundamental_cores.md)(r4)
- **Silver 輸入**:各 core 對接不同 derived 表(見 §2)
- **時間對齊**:revenue / financial_statement 屬月頻 / 季頻;fact_date 為「該頻率最後一個交易日」+ report_date 為實際發布日
- **Params 維度**:
  - `revenue_core`:`yoy_high=30 / yoy_low=-10 / mom=20 / streak=3 / lookback=60`(月頻)
  - `valuation_core`:`lookback_years=5 / pct_high=80 / pct_low=20 / yield_high=5.0`(r4 14 個 EventKind)
  - `financial_statement_core`:`gross_margin=2.0 / roe_high=15.0(Buffett 慣例)/ debt_ratio=60 / fcf_streak=4 / TTM 4-quarter sum`
- **Cores**:`revenue_core` / `valuation_core` / `financial_statement_core`

### 3.7 Environment(P2,5 cores;1 spec 有 code 未實作)

- **Spec**:[`m3Spec/environment_cores.md`](../m3Spec/environment_cores.md)(r3)
- **共同**:market-level,Output 與個股無關;`stock_id` 用**保留字**(`cores_overview.md` §6.2.1):
  - `taiex_core` → `_index_taiex_` / `_index_tpex_`(雙保留字,r4)
  - `us_market_core` → `_index_us_market_`(SPY / VIX 共用,以 `metadata.subseries` 區分)
  - `exchange_rate_core` → `_global_`
  - `fear_greed_core` → `_global_`
  - `market_margin_core` → `_market_`
  - `business_indicator_core`(未實作)→ `_index_business_`
- **Cores**:`taiex_core` / `us_market_core` / `exchange_rate_core` / `fear_greed_core` / `market_margin_core`

---

## 4. Silver → Cores 反向索引

給定 Silver 表名 → 哪些 cores 讀它(對接調試用)。

### 個股級 Silver

| Silver 表 | 讀取的 Cores |
|---|---|
| `price_daily_fwd`(OHLCV)| `neely_core` + **8 indicator cores**(`macd` / `rsi` / `kd` / `adx` / `ma` / `bollinger` / `atr` / `obv`)= **9 cores** |
| `price_weekly_fwd` / `price_monthly_fwd`(週/月 K)| 同上(透過 `Timeframe` Params 選)|
| `institutional_daily_derived` | `institutional_core` |
| `margin_daily_derived` | `margin_core` |
| `foreign_holding_derived` | `foreign_holding_core` |
| `day_trading_derived` | `day_trading_core` |
| `holding_shares_per_derived` | `shareholder_core` |
| `valuation_daily_derived` | `valuation_core` |
| `monthly_revenue_derived` | `revenue_core` |
| `financial_statement_derived` | `financial_statement_core` |

### 市場級 Silver

| Silver 表 | 讀取的 Cores |
|---|---|
| `taiex_index_derived`(TAIEX + TPEx 兩 row/date)| `taiex_core` |
| `us_market_index_derived`(SPY + VIX)| `us_market_core` |
| `exchange_rate_derived` | `exchange_rate_core` |
| `market_margin_maintenance_derived` | `market_margin_core` |
| `business_indicator_derived` | (`business_indicator_core` 未實作)|
| `fear_greed_index`(Bronze 直讀,**架構例外**)| `fear_greed_core` |

### 未被任何 Core 讀的 Silver

- `price_limit_merge_events`(S1 Rust 計算,目前 Cores 端無 consumer)

### 未被任何 Core 讀的 Bronze(對 Cores 來說透明)

> Bronze 一律不被 Cores 直接讀(`cores_overview.md` §4.4)— 例外只有 `fear_greed_index`(spec §6.2 已登記)。

---

## 5. M3 三表寫入規約速查

引用 [`m3Spec/cores_overview.md`](../m3Spec/cores_overview.md) §7,**不重寫**。

| 表 | 何時寫 | 範圍 | 範例 |
|---|---|---|---|
| `indicator_values`(IV)| 時序型輸出 | 每 core × stock × `value_date` × `timeframe` × `params_hash` 一 row;`value_json` JSONB 包整個 Output | macd_line + signal + histogram 序列 / rsi 0-100 序列 / ma 多條同算 |
| `structural_snapshots`(SS)| 結構快照 | 每 core × stock × `snapshot_date` × `timeframe` × `params_hash` 一 row;`snapshot_json` JSONB 包整個 Forest / 結構 | neely Scenario Forest(P0)|
| `facts`(F)| 事件邊界 | per event,UPSERT ON CONFLICT DO NOTHING(unique 含 `md5(statement)`)| GoldenCross / Divergence / EnteredX / ExitedX / Pattern detected |

**寫入路徑**:`rust_compute/cores/system/tw_cores/src/main.rs:dispatch_indicator()` + `dispatch_neely()` 包 `compute() → produce_facts() → write_indicator_value() / write_facts()`(PR-9c batch INSERT,UNNEST array)。

---

## 6. params_hash 規約速覽

引用 [`m3Spec/cores_overview.md`](../m3Spec/cores_overview.md) §7.4,**不重寫**。

- **演算法**:`fact_schema::params_hash(&params)` = blake3(canonical JSON of params) → 16-char hex
- **三表用途**:Unique constraint 的一部分,確保「同 core 不同 Params」(例如不同 lookback / threshold)各自寫一份不衝突
- **意義**:同 core 跑 default Params + 跑 custom Params 兩條時序並存

---

## 7. 新增 Core 時 checklist

```
[1] 確認上游 Silver 表是否已存在
    - 若無 → 先補 Silver 規範至 m2Spec/layered_schema_post_refactor.md §4
    - 加 Silver builder src/silver/builders/<name>.py(若 SQL builder)
    - 或加 Rust Silver computation(若像 S1)
    - 加 alembic migration + schema_pg.sql 同步

[2] 寫 Core spec
    - 位置:m3Spec/{子類}_cores.md 對應子類
    - 含 Params / EventKind / Output / Fact 範例 / 與其他 Core 接點

[3] 實作 Rust Core
    - 位置:rust_compute/cores/{子類}/{name}_core/
    - 必要 trait:IndicatorCore 或 WaveCore(m3Spec/cores_overview.md §3)
    - inventory::submit! 註冊 CoreRegistration
    - 寫 unit tests(對齊既有 cores 的 test 風格)

[4] 接 tw_cores binary
    - rust_compute/cores/system/tw_cores/src/main.rs 加 dispatch match arm
    - 串列:loader → core.compute(input, params) → produce_facts → write_*
    - cores_shared/{ohlcv,chip,fundamental,environment}_loader 加新 loader fn 若需要

[5] 更新本檔
    - §2 大表加新列(主要 Silver 輸入 / 寫入 M3 / spec 錨點)
    - §3 對應子類 deep-link 加 Params 維度
    - §4 反向索引「Silver 表 → 讀取的 Cores」加新 entry

[6] 更新 docs/schema_reference.md
    - 若新增 Silver 表 → §2 全表清單 + §4 Silver 速查加新項

[7] cargo build --workspace + cargo test --workspace 全綠
```

---

## 8. Cross-references

- 表速查:[`docs/schema_reference.md`](./schema_reference.md)
- 表規範:[`m2Spec/layered_schema_post_refactor.md`](../m2Spec/layered_schema_post_refactor.md)
- 核規範:[`m3Spec/cores_overview.md`](../m3Spec/cores_overview.md)
- 核 deep-dive:`m3Spec/{indicator,chip,fundamental,environment,neely}_cores*.md`
- 總索引:[`docs/schema_master.md`](./schema_master.md)
- collector 索引:[`docs/api_pipeline_reference.md`](./api_pipeline_reference.md)
