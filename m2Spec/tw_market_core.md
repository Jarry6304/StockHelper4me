# TW-Market Core 規格

> **版本**:v2.0 抽出版 r1
> **日期**:2026-04-30
> **配套文件**:`cores_overview.md`(共通規範)
> **優先級**:**P0**
> **狀態**:核心必備,在所有其他 Core 之前執行

---

## 目錄

1. [定位](#一定位)
2. [執行順位](#二執行順位)
3. [輸入](#三輸入)
4. [Params](#四params)
5. [處理模組](#五處理模組)
6. [Output 結構](#六output-結構)
7. [Fact 產出規則](#七fact-產出規則)
8. [warmup_periods](#八warmup_periods)
9. [對應資料表](#九對應資料表)
10. [已棄用的設計](#十已棄用的設計)
11. [未來擴充考量](#十一未來擴充考量多市場支援)

---

## 一、定位

**TW-Market Core** 處理台股市場特性與**資料前處理**,是 Pipeline 中**第一個執行的 Core**。

### 1.1 設計意圖

- 將「台股市場特性」集中於單一 Core,讓 Neely Core / Traditional Core / Indicator Cores **完全不知道台股的存在**
- 所有後續 Core 吃的是經過台股處理的「乾淨 OHLC」
- 未來支援其他市場(美股、港股)只需替換此 Core

### 1.2 必要規範

- 屬 Market Core,**不**走 `IndicatorCore` trait
- 走 `MarketCore` trait(草案,P0 確定)
- 輸出處理過的 OHLCV + 處理事件 Fact

---

## 二、執行順位

```
Raw OHLC + 漲跌停資訊 + 還原資訊
        ↓
   TW-Market Core
   (所有市場特性處理集中於此)
        ↓
   處理過的 OHLCV
        ↓
   ├──→ Neely Core
   ├──→ Traditional Core
   ├──→ Indicator Cores (MACD / RSI / ...)
   ├──→ Chip Cores
   └──→ Fundamental Cores
```

### 2.1 為何選擇前置處理(選項 A)

v1.1 將「漲跌停合併」嵌在 Neely Engine 內(`[TW-MARKET]` 標記),v2.0 抽出獨立成 Core。原因:

1. **單一職責**:Neely Core 只做 Neely 規則,不知道台股
2. **多市場相容**:未來換市場只需替換 Market Core
3. **可被多個下游消費**:Indicator Cores 也吃處理過的 OHLC,不必各自重做漲跌停處理

---

## 三、輸入

| 輸入 | 來源 |
|---|---|
| `RawOHLCVSeries` | `price_daily` / `price_weekly` / `price_monthly`(未還原) |
| `PriceLimitInfo` | `price_limit` 表(每日漲跌停價) |
| `AdjustmentEvents` | `price_adjustment_events` 表(除權息事件) |
| `MarketIndex`(可選) | `market_index_tw`(加權指數,還原版本) |

---

## 四、Params

```rust
pub struct TwMarketCoreParams {
    pub timeframe: Timeframe,                          // Daily / Weekly / Monthly
    pub limit_merge_strategy: LimitMergeStrategy,      // 漲跌停合併策略
    pub adjustment_mode: AdjustmentMode,               // 還原模式
    pub neutral_threshold_taiex: f64,                  // 加權指數中性閾值
}

pub enum LimitMergeStrategy {
    /// 連續漲停 / 跌停日合併為單一 K 棒(Neely Core 推薦)
    MergeConsecutive,

    /// 不合併(Indicator Cores 預設,保留每日 K 棒)
    None,

    /// 僅合併「當日達到漲跌停價」的日子
    MergeAtLimitPrice,
}

pub enum AdjustmentMode {
    /// 後復權(Backward Adjusted),保留現價,歷史價往下調
    Backward,

    /// 前復權(Forward Adjusted),保留歷史價,現價往上調
    Forward,

    /// 不還原(僅供 raw 顯示用)
    None,
}
```

### 4.1 預設值

```rust
impl Default for TwMarketCoreParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            limit_merge_strategy: LimitMergeStrategy::MergeConsecutive,
            adjustment_mode: AdjustmentMode::Backward,
            neutral_threshold_taiex: 0.5, // %
        }
    }
}
```

### 4.2 不同下游的策略選擇

| 下游 Core | `limit_merge_strategy` 推薦 |
|---|---|
| Neely Core | `MergeConsecutive`(避免漲跌停日誤判為獨立波段) |
| Traditional Core | `MergeConsecutive` |
| MACD / RSI / KD / 多數技術指標 | `None`(每日 K 棒保留,不影響滑動計算) |
| Volume-based 指標(OBV / VWAP) | `None` |

**Workflow toml** 透過參數宣告各下游所需的版本,Pipeline 可能多次呼叫 TW-Market Core 產生不同輸出。

---

## 五、處理模組

### 5.1 連續漲跌停合併

**範圍**:當連續多日達到漲停或跌停,合併為單一 K 線。

**規則**:

- 連續 N 個交易日 close == limit_up_price → 合併為單一 K
- 合併 K 的 OHLC:open = 第一日 open,high = max(all highs),low = min(all lows),close = 最後一日 close
- volume = sum(all volumes)
- 在合併 K 上標記 `is_merged = true` 與 `merged_days_count = N`

**為何要合併**:漲跌停期間市場流動性異常,K 線形狀不反映真實供需,Neely Core 若不合併會將其視為獨立波段。

### 5.2 TAIEX Neutral 閾值

**範圍**:加權指數的中性區判定閾值較個股寬鬆。

**規則**:

- 個股:|單日漲跌幅| < 預設值 → Neutral
- 加權指數:|單日漲跌幅| < `neutral_threshold_taiex` → Neutral

**用途**:輸出供 Neely Core 的 Rule of Neutrality 使用,避免大盤被 Neely 誤判為趨勢日。

### 5.3 還原指數使用

**範圍**:處理除權息調整,提供 OHLC 的還原版本。

**規則**:

- 後復權(Backward):從最新交易日往回,逐除權息事件調整歷史價
- 前復權(Forward):從上市日往後,逐除權息事件調整當前價
- 還原比率來自 `price_adjustment_events` 表

**重要**:還原計算的精度由 Rust 端負責,**禁止前端聚合或還原**(避免精度問題)。

---

## 六、Output 結構

```rust
pub struct TwMarketCoreOutput {
    // 處理過的 OHLCV
    pub processed_ohlcv: OHLCVSeries,

    // 處理 metadata
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub adjustment_mode: AdjustmentMode,
    pub limit_merge_strategy: LimitMergeStrategy,

    // 處理事件清單
    pub limit_merge_events: Vec<LimitMergeEvent>,
    pub adjustment_events: Vec<AdjustmentEvent>,

    // 診斷
    pub diagnostics: TwMarketDiagnostics {
        original_bar_count: usize,
        processed_bar_count: usize,
        merged_groups: usize,
        adjusted_bar_count: usize,
        elapsed_ms: u64,
    },
}

pub struct LimitMergeEvent {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub direction: LimitDirection,  // LimitUp / LimitDown
    pub days_count: usize,
    pub merged_open: f64,
    pub merged_close: f64,
}

pub struct AdjustmentEvent {
    pub event_date: NaiveDate,
    pub event_type: AdjustmentEventType, // Dividend / StockSplit / RightsIssue
    pub adjustment_ratio: f64,
}
```

### 6.1 OHLCVSeries 標記

處理過的 K 棒額外帶以下 metadata 欄位:

| 欄位 | 意義 |
|---|---|
| `is_merged` | 是否為合併 K 棒 |
| `merged_days_count` | 合併日數(若 is_merged = true) |
| `is_adjusted` | 是否經過還原 |
| `adjustment_factor` | 累計還原因子 |

下游 Core 可選擇是否消費這些 metadata。

---

## 七、Fact 產出規則

### 7.1 漲跌停事件 Fact

| Fact 範例 | metadata |
|---|---|
| `Limit-up streak: 3 consecutive days, 2026-03-15 to 2026-03-17` | `{ direction: "up", days: 3, start: "2026-03-15", end: "2026-03-17" }` |
| `Limit-down streak: 5 consecutive days, 2026-04-25 to 2026-04-29` | `{ direction: "down", days: 5, start: "2026-04-25", end: "2026-04-29" }` |

### 7.2 除權息事件 Fact

| Fact 範例 | metadata |
|---|---|
| `Ex-dividend on 2026-07-15, ratio 0.985` | `{ event_type: "dividend", date: "2026-07-15", ratio: 0.985 }` |
| `Stock split 1:2 on 2026-09-01` | `{ event_type: "stock_split", date: "2026-09-01", ratio: 0.5 }` |

### 7.3 異常事件 Fact(可選)

| Fact 範例 | metadata |
|---|---|
| `Trading suspended on 2026-08-10` | `{ event_type: "suspension", date: "2026-08-10" }` |
| `Trading resumed on 2026-08-15` | `{ event_type: "resumption", date: "2026-08-15" }` |

---

## 八、warmup_periods

TW-Market Core 屬**前處理 Core**,理論上不需暖機(每筆資料各自處理)。但為了還原計算需取得足夠的歷史除權息事件:

```rust
fn warmup_periods(&self, params: &TwMarketCoreParams) -> usize {
    // 還原計算需取得整個歷史的除權息事件
    // 但 OHLCV 本身只需當前窗口
    match params.timeframe {
        Timeframe::Daily => 0,    // 增量處理,不需暖機
        Timeframe::Weekly => 0,
        Timeframe::Monthly => 0,
    }
}
```

**例外**:首次建表(initial backfill)時需取得全歷史,屬一次性處理,不走 `warmup_periods` 路徑。

---

## 九、對應資料表

| 用途 | 資料表 |
|---|---|
| 輸入 raw OHLC | `price_daily` / `price_weekly` / `price_monthly` |
| 輸入漲跌停資訊 | `price_limit` |
| 輸入除權息事件 | `price_adjustment_events` |
| 輸入加權指數(若用於 TAIEX neutral 判定) | `market_index_tw` |
| 輸出處理過的 OHLCV | `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd` |
| 寫入處理事件 Fact | `facts`,`source_core = 'tw_market_core'` |

### 9.1 `_fwd` 後綴命名

`price_*_fwd` 表存放 TW-Market Core 處理過的 OHLCV,**所有後續 Core 一律從 `_fwd` 表讀取**,不從 raw 表讀取。

---

## 十、已棄用的設計

| 棄用項 | 來源 | 棄用原因 |
|---|---|---|
| `[TW-MARKET]` Scorer 微調(`ext_type_prior_3rd`) | v1.1 Item 7.4 | 主觀加權,違反「忠於原作」 |
| `[TW-MARKET]` Scorer 微調(`alternation_tw_bonus`) | v1.1 Item 7.4 | 同上 |
| 漲跌停處理嵌在 Neely Engine | v1.1 Item 1.5 | 違反單一職責,Neely Core 不該知道台股 |
| 還原指數計算放在前端 | 早期討論 | 精度問題,前端不應做還原 |
| 在 Neely Core 內判斷加權指數 neutral 閾值 | v1.1 隱含 | 該判斷與台股市場特性綁定,應在 TW-Market Core |

---

## 十一、未來擴充考量(多市場支援)

### 11.1 設計伏筆

雖然 v2.0 僅支援台股,但 TW-Market Core 的命名暗示未來會有 `us_market_core` / `hk_market_core` 等對應其他市場的前處理 Core。

### 11.2 多市場架構草案(P3+)

```rust
pub trait MarketCore: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn process(&self, raw: &RawOHLCVSeries, ...) -> Result<MarketCoreOutput>;
}

// 各市場各自實作
pub struct TwMarketCore { ... }
pub struct UsMarketCore { ... }
pub struct HkMarketCore { ... }
```

Workflow 在 toml 宣告適用市場:

```toml
[market_core]
name = "tw_market"
```

Orchestrator 根據宣告自動載入對應 Core。

### 11.3 注意事項

- **不為「自動偵測市場」設計邏輯** — 由使用者透過 Workflow toml 明確指定
- **不在同一 Pipeline 混用多市場** — 一次 Pipeline 處理一個市場的一檔股票
- **跨市場比較**屬於 Aggregation Layer 的並排呈現範疇,不在 Core 層處理
