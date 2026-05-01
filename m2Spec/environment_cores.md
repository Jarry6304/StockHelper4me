# Environment Cores 規格(環境)

> **版本**:v2.0 抽出版 r1
> **日期**:2026-04-30
> **配套文件**:`cores_overview.md`(共通規範)
> **包含 Core**:5 個
> **優先級**:全部 P2

---

## 目錄

1. [本文件範圍](#一本文件範圍)
2. [共通規範](#二共通規範本子類)
3. [`taiex_core`](#三taiex_core)
4. [`us_market_core`](#四us_market_core)
5. [`exchange_rate_core`](#五exchange_rate_core)
6. [`fear_greed_core`](#六fear_greed_core)
7. [`market_margin_core`](#七market_margin_core)
8. [Environment 與個股 Core 的並排原則](#八environment-與個股-core-的並排原則)

---

## 一、本文件範圍

| Core | 名稱 | 對應資料表 |
|---|---|---|
| `taiex_core` | 加權指數 | `market_index_tw` |
| `us_market_core` | SPY / VIX / 美股 | `market_index_us` |
| `exchange_rate_core` | 匯率 | `exchange_rate` |
| `fear_greed_core` | 恐慌貪婪指數 | `fear_greed_index` |
| `market_margin_core` | 市場整體融資維持率 | `market_margin_maintenance` |

---

## 二、共通規範(本子類)

### 2.1 trait

全部走 `IndicatorCore` trait。

### 2.2 個股無關性

Environment Cores 的輸出**與個股無關**,反映整體市場 / 環境變數。寫入 `indicator_values` 時 `stock_id` 為 `_market_` 或 `_global_` 等保留值(具體規範見儲存層 spec)。

### 2.3 Environment Cores 的設計意義

Environment Cores 的事實**不會直接影響個股 Core 的計算**,而是在 Aggregation Layer 與個股事實**並排呈現**,讓使用者自己連結個股技術面與大環境的關聯。

### 2.4 與個股 Core 的關係

```
個股技術面 Fact(來自 Indicator Cores)
        +
個股籌碼面 Fact(來自 Chip Cores)
        +
個股基本面 Fact(來自 Fundamental Cores)
        +
大盤環境 Fact(來自 Environment Cores)
        ↓
   Aggregation Layer 並排呈現
        ↓
   使用者自己連線
```

### 2.5 Fact 邊界提醒

- ✅ `加權指數 MACD 黃金交叉`、`VIX 上升至 28`、`USD/TWD 突破 30.5`
- ❌ `市場情緒轉樂觀`、`美股拖累台股`、`匯率壓力減輕`

「拖累」、「壓力」、「轉樂觀」屬主觀因果判斷,**禁止**進入 Fact statement。

---

## 三、`taiex_core`

### 3.1 定位

加權指數的趨勢、技術指標、量能事實。

### 3.2 注意:此 Core 自身可呼叫其他 Indicator Core 嗎?

**不可以**。`taiex_core` **內嵌**自己所需的指標計算(MACD、RSI 等),**不**從外部 Indicator Core 取資料。

### 3.3 為何要重複實作

- 維持零耦合原則(Core 之間不互相 import)
- `taiex_core` 用的是大盤指數,個股 Indicator Core 用的是個股 OHLC,輸入不同
- 邏輯雷同部分可走 `shared/` 共用工具(若未來抽出)

### 3.4 Params

```rust
pub struct TaiexParams {
    pub timeframe: Timeframe,
    pub macd_fast: usize,                  // 預設 12
    pub macd_slow: usize,                  // 預設 26
    pub macd_signal: usize,                // 預設 9
    pub rsi_period: usize,                 // 預設 14
    pub volume_z_threshold: f64,           // 預設 2.0
    pub trend_lookback_bars: usize,        // 預設 60
}
```

### 3.5 warmup_periods

```rust
fn warmup_periods(&self, params: &TaiexParams) -> usize {
    (params.macd_slow * 4).max(params.trend_lookback_bars + 10)
}
```

### 3.6 Output

```rust
pub struct TaiexOutput {
    pub series: Vec<TaiexPoint>,
    pub events: Vec<TaiexEvent>,
}

pub struct TaiexPoint {
    pub date: NaiveDate,
    pub close: f64,
    pub volume: i64,
    pub change_pct: f64,
    pub macd_line: f64,
    pub macd_signal: f64,
    pub macd_histogram: f64,
    pub rsi: f64,
    pub volume_z: f64,
    pub trend_state: TrendState,           // BullishMa / BearishMa / Neutral
}

pub struct TaiexEvent {
    pub date: NaiveDate,
    pub kind: TaiexEventKind,
    pub value: f64,
}

pub enum TaiexEventKind {
    MacdGoldenCross,
    MacdDeathCross,
    RsiOverbought,
    RsiOversold,
    VolumeSurge,                  // 量能異常放大
    NewHigh20d,
    NewLow20d,
    BreakdownBelowMa60,
    BreakoutAboveMa60,
}
```

### 3.7 Fact 範例

| Fact statement | metadata |
|---|---|
| `TAIEX MACD golden cross at 2026-04-15` | `{ index: "taiex", event: "macd_golden_cross" }` |
| `TAIEX broke above MA60 at 2026-04-22(close=18,500, MA60=18,250)` | `{ index: "taiex", event: "ma60_breakout" }` |
| `TAIEX volume z-score 3.2 on 2026-04-25(volume surge)` | `{ index: "taiex", event: "volume_surge", z: 3.2 }` |
| `TAIEX RSI = 78 on 2026-04-25(overbought for 5 days)` | `{ index: "taiex", event: "rsi_overbought_streak", days: 5 }` |

---

## 四、`us_market_core`

### 4.1 定位

美股大盤(SPY)趨勢、VIX 區間、夜盤異動事實。

### 4.2 為何要看美股

台股與美股相關性高,美股事實作為台股 Core 的**外部環境**並排呈現。但 Core 本身**不**做「美股漲台股會漲」的因果推論。

### 4.3 Params

```rust
pub struct UsMarketParams {
    pub spy_macd_fast: usize,              // 預設 12
    pub spy_macd_slow: usize,              // 預設 26
    pub spy_macd_signal: usize,            // 預設 9
    pub vix_high_threshold: f64,           // 預設 25.0
    pub vix_low_threshold: f64,            // 預設 15.0
    pub overnight_change_threshold: f64,   // 夜盤大幅變動閾值,預設 1.5(%)
}
```

### 4.4 warmup_periods

```rust
fn warmup_periods(&self, params: &UsMarketParams) -> usize {
    params.spy_macd_slow * 4
}
```

### 4.5 Output

```rust
pub struct UsMarketOutput {
    pub series: Vec<UsMarketPoint>,
    pub events: Vec<UsMarketEvent>,
}

pub struct UsMarketPoint {
    pub date: NaiveDate,                   // 美國日(交易日)
    pub spy_close: f64,
    pub spy_change_pct: f64,
    pub spy_macd_histogram: f64,
    pub vix_close: f64,
    pub vix_change_pct: f64,
    pub vix_zone: VixZone,                 // Low / Normal / High / ExtremeHigh
}

pub struct UsMarketEvent {
    pub date: NaiveDate,
    pub kind: UsMarketEventKind,
    pub value: f64,
}

pub enum UsMarketEventKind {
    SpyMacdGoldenCross,
    SpyMacdDeathCross,
    VixSpike,                  // 單日 VIX 跳升
    VixHighZoneEntry,
    VixLowZoneEntry,
    SpyOvernightLargeMove,     // 美股大幅變動,影響台股次日跳空
}
```

### 4.6 Fact 範例

| Fact statement | metadata |
|---|---|
| `SPY MACD death cross at 2026-04-22` | `{ index: "spy", event: "macd_death_cross" }` |
| `VIX spike to 32.5 on 2026-04-25(+45% single day)` | `{ event: "vix_spike", vix: 32.5 }` |
| `VIX entered high zone(>25) on 2026-04-22` | `{ event: "vix_high_zone_entry" }` |
| `SPY -2.8% on 2026-04-22 evening(US time)` | `{ event: "spy_overnight_large_move", change: -2.8 }` |

### 4.7 時區注意

美股交易時間與台股不同步。Output 的 `date` 為美國日,Aggregation Layer 在對齊台股時間軸時需處理時差(美股當日收盤事實對應台股**次日**開盤的環境)。

---

## 五、`exchange_rate_core`

### 5.1 定位

匯率(USD/TWD 等)趨勢、突破事實。

### 5.2 為何要看匯率

匯率影響外資進出與出口股獲利。但 Core 本身**不**做「匯率升值對某個股利空」這類因果推論,僅輸出匯率事實。

### 5.3 Params

```rust
pub struct ExchangeRateParams {
    pub timeframe: Timeframe,
    pub currency_pairs: Vec<String>,           // ["USD/TWD"], 可多組
    pub ma_period: usize,                      // 預設 20
    pub key_levels: Vec<f64>,                  // 重要關鍵價位,預設 [30.0, 31.0, 32.0]
    pub significant_change_threshold: f64,     // 單日大幅變動,預設 0.5(%)
}
```

### 5.4 warmup_periods

```rust
fn warmup_periods(&self, params: &ExchangeRateParams) -> usize {
    params.ma_period + 10
}
```

### 5.5 Output

```rust
pub struct ExchangeRateOutput {
    pub series: Vec<ExchangeRatePoint>,
    pub events: Vec<ExchangeRateEvent>,
}

pub struct ExchangeRatePoint {
    pub date: NaiveDate,
    pub currency_pair: String,             // "USD/TWD"
    pub rate: f64,
    pub change_pct: f64,
    pub ma_value: f64,
    pub trend_state: TrendState,
}

pub struct ExchangeRateEvent {
    pub date: NaiveDate,
    pub currency_pair: String,
    pub kind: ExchangeRateEventKind,
    pub value: f64,
}

pub enum ExchangeRateEventKind {
    KeyLevelBreakout,          // 突破關鍵價位
    KeyLevelBreakdown,
    SignificantSingleDayMove,
    MaCross,
}
```

### 5.6 Fact 範例

| Fact statement | metadata |
|---|---|
| `USD/TWD broke above 30.5 on 2026-04-22(rate=30.55)` | `{ pair: "USD/TWD", event: "key_level_breakout", level: 30.5 }` |
| `USD/TWD -0.85% on 2026-04-25(largest move in 60 days)` | `{ pair: "USD/TWD", event: "significant_single_day_move", change: -0.85 }` |
| `USD/TWD crossed above MA(20) at 2026-04-15` | `{ pair: "USD/TWD", event: "ma_cross", direction: "above" }` |

---

## 六、`fear_greed_core`

### 6.1 定位

恐慌貪婪指數(可能為 CNN 美股指數 / 台股自製指標),區間判定與極端事件。

### 6.2 Params

```rust
pub struct FearGreedParams {
    pub timeframe: Timeframe,
    pub extreme_fear_threshold: f64,       // 預設 25.0
    pub fear_threshold: f64,               // 預設 45.0
    pub greed_threshold: f64,              // 預設 55.0
    pub extreme_greed_threshold: f64,      // 預設 75.0
    pub streak_min_days: usize,            // 連續天數,預設 5
}
```

### 6.3 warmup_periods

```rust
fn warmup_periods(&self, params: &FearGreedParams) -> usize {
    params.streak_min_days + 10
}
```

### 6.4 Output

```rust
pub struct FearGreedOutput {
    pub series: Vec<FearGreedPoint>,
    pub events: Vec<FearGreedEvent>,
}

pub struct FearGreedPoint {
    pub date: NaiveDate,
    pub value: f64,                        // 0.0 ~ 100.0
    pub zone: FearGreedZone,
}

pub enum FearGreedZone {
    ExtremeFear,
    Fear,
    Neutral,
    Greed,
    ExtremeGreed,
}

pub struct FearGreedEvent {
    pub date: NaiveDate,
    pub kind: FearGreedEventKind,
    pub value: f64,
}

pub enum FearGreedEventKind {
    EnteredExtremeFear,
    ExitedExtremeFear,
    EnteredExtremeGreed,
    ExitedExtremeGreed,
    StreakInZone,                  // 連續處於某區間
}
```

### 6.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `Fear & Greed Index entered extreme fear zone(22) on 2026-04-25` | `{ event: "entered_extreme_fear", value: 22 }` |
| `Fear & Greed Index in greed zone for 8 consecutive days` | `{ event: "streak_in_zone", zone: "greed", days: 8 }` |
| `Fear & Greed Index exited extreme greed at 73 on 2026-04-22` | `{ event: "exited_extreme_greed", value: 73 }` |

---

## 七、`market_margin_core`

### 7.1 定位

市場整體融資維持率,反映槓桿風險。

### 7.2 與 `margin_core` 的差異

| 項目 | `margin_core`(個股) | `market_margin_core`(整體) |
|---|---|---|
| 範圍 | 單一個股的融資融券 | 全市場融資維持率 |
| 分類 | Chip Cores | Environment Cores |
| `stock_id` | 個股代號 | `_market_` |
| 用途 | 個股籌碼分析 | 大環境風險評估 |

### 7.3 Params

```rust
pub struct MarketMarginParams {
    pub timeframe: Timeframe,
    pub maintenance_warning_threshold: f64,    // 預設 145.0(%)
    pub maintenance_danger_threshold: f64,     // 預設 130.0(%)
    pub significant_change_threshold: f64,     // 單日變化閾值,預設 5.0(%)
}
```

### 7.4 warmup_periods

```rust
fn warmup_periods(&self, params: &MarketMarginParams) -> usize {
    20
}
```

### 7.5 Output

```rust
pub struct MarketMarginOutput {
    pub series: Vec<MarketMarginPoint>,
    pub events: Vec<MarketMarginEvent>,
}

pub struct MarketMarginPoint {
    pub date: NaiveDate,
    pub maintenance_rate: f64,             // 整體融資維持率%
    pub change_pct: f64,
    pub zone: MarginZone,                  // Safe / Warning / Danger
}

pub enum MarginZone {
    Safe,        // > warning_threshold
    Warning,     // danger_threshold ~ warning_threshold
    Danger,      // < danger_threshold
}

pub struct MarketMarginEvent {
    pub date: NaiveDate,
    pub kind: MarketMarginEventKind,
    pub value: f64,
}

pub enum MarketMarginEventKind {
    EnteredWarningZone,
    EnteredDangerZone,
    ExitedDangerZone,
    SignificantSingleDayDrop,
}
```

### 7.6 Fact 範例

| Fact statement | metadata |
|---|---|
| `Market margin maintenance dropped to 142% on 2026-04-25(warning zone)` | `{ event: "entered_warning_zone", value: 142.0 }` |
| `Market margin maintenance reached 128% on 2026-04-28(danger zone)` | `{ event: "entered_danger_zone", value: 128.0 }` |
| `Market margin maintenance dropped 6.5% in single day on 2026-04-22` | `{ event: "significant_single_day_drop", change: -6.5 }` |

### 7.7 強制平倉風險警訊

當 `maintenance_rate < danger_threshold`(預設 130%),代表大量融資戶接近強制平倉,可能引發市場連鎖賣壓。但 Core 本身**不**做「強平風險即將引發崩盤」這類預測,僅輸出客觀數值與分區事件。

---

## 八、Environment 與個股 Core 的並排原則

### 8.1 不在 Core 層整合

「VIX 飆升 + 個股 RSI 超買 = 賣訊」這類綜合判斷涉及 Environment Core 與個股 Core,**不**在 Core 層整合。

### 8.2 並排呈現

由 Aggregation Layer 將兩類 Core 的 Fact 並排呈現,使用者自己連線。

### 8.3 為何不立「環境調整 Core」

過去版本曾考慮「VIX 高時調降個股技術指標權重」這類設計,但已棄用,理由:

- 違反「並排不整合」原則
- 「VIX 多高才算高」、「該調降多少權重」屬主觀判斷,寫進 Core 等於替使用者下決策
- v1.1 的 Combined confidence ×1.1/×0.7 已棄用,本子類不重蹈覆轍

### 8.4 使用者教學層的角色

如何結合大環境與個股屬投資哲學議題,由使用者教學文件提供識讀指引,不在架構層處理。

例:「VIX 高時優先看防禦股」屬投資策略,**不**寫進 Core 也**不**寫進 Aggregation Layer。
