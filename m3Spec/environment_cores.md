# Environment Cores 規格(環境)

> **版本**:v2.0 抽出版 r3
> **日期**:2026-05-07
> **配套文件**:`cores_overview.md`(共通規範)
> **包含 Core**:6 個
> **優先級**:全部 P2

---

## r3 修訂摘要(2026-05-07)

- **新增 §八 `business_indicator_core`**:對應 layered_schema `business_indicator_derived`,景氣指標(leading / coincident / lagging / monitoring / monitoring_color)的事件偵測。原 layered_schema §8.3 已備接點但 r2 版未對應 Core,本次補齊。Core 數量 5 → 6
- **stock_id 保留字粒度細化**(對齊 overview §6.2.1 r3 修訂):
  - `taiex_core` 拆出 `_index_taiex_`(TAIEX)與 `_index_tpex_`(TPEx)兩個保留字,反映兩者並列大盤的對等關係
  - `us_market_core` 從 `_global_` 改為專屬保留字 `_index_us_market_`,內部以 `metadata.subseries: "spy"|"vix"` 區分子序列(SPY 與 VIX 屬同源衍生關係)
  - `business_indicator_core` 使用新保留字 `_index_business_`
  - `_global_` 縮減為「真·全球變數」,僅 `exchange_rate_core` / `fear_greed_core` 使用
- **§3.7 / §4.7 Fact 範例補齊**:taiex_core 補 TPEx Fact 範例,展示拆細保留字的使用方式
- **章節編號順移**:原 §七「`market_margin_core`」順延至本版同章節,新增 §八「`business_indicator_core`」、§九「Environment 與個股 Core 的並排原則」(原 §八)
- **章節重排不影響其他文件 cross-reference**:其他文件對本子類的引用以 Core 名為主,未引用具體章節編號

---

## r2 修訂摘要(2026-05-07)

- **資料表對應改指 Silver derived**:依總綱 §4.4「Cores 一律從 Silver 層讀取,不直接讀 Bronze」,所有 Core 章首對照表更新為 `*_derived` 表名(對應 `layered_schema_post_refactor.md` §4.4)
- **`fear_greed_core` 例外註記**:Silver 目前無 `fear_greed_index_derived`,Core 直讀 Bronze `fear_greed_index` 為**已知架構例外**,須補 derived 後切換,詳見 §6.2 與 §6.7
- **Input Series 與載入器補齊**:§2.1 明列 5 種 `*Series` 與 `shared/environment_loader/`,對齊 chip / fundamental 子類格式
- **events 結構統一**:採 `{ date, kind, value, metadata }` 同構結構
- **計算策略表補齊**:§2.4 新增 batch 處理表,對齊 chip / fundamental 子類
- **warmup_periods 偏離理由補齊**:`market_margin_core` 的硬編 20 補理由
- **Silver PK 標註**:`exchange_rate_derived` 與 `market_margin_maintenance_derived` 為非標準 PK(不含 stock_id),於對應章節標明
- **跨 Core 整合禁止**:引用總綱 §11

---

## 目錄

1. [本文件範圍](#一本文件範圍)
2. [共通規範](#二共通規範本子類)
3. [`taiex_core`](#三taiex_core)
4. [`us_market_core`](#四us_market_core)
5. [`exchange_rate_core`](#五exchange_rate_core)
6. [`fear_greed_core`](#六fear_greed_core)
7. [`market_margin_core`](#七market_margin_core)
8. [`business_indicator_core`](#八business_indicator_core)
9. [Environment 與個股 Core 的並排原則](#九environment-與個股-core-的並排原則)

---

## 一、本文件範圍

| Core | 名稱 | 上游 Silver 表 | Silver PK | 保留 stock_id |
|---|---|---|---|---|
| `taiex_core` | 加權指數(TAIEX + TPEx) | `taiex_index_derived` | `(market, stock_id, date)` | `_index_taiex_` / `_index_tpex_` |
| `us_market_core` | SPY + VIX(美股大盤) | `us_market_index_derived` | `(market, stock_id, date)` | `_index_us_market_` |
| `exchange_rate_core` | 匯率 | `exchange_rate_derived` | `(market, date, currency)` | `_global_` |
| `fear_greed_core` | 恐慌貪婪指數 | ⚠️ Bronze `fear_greed_index`(暫定) | `(market, date)` | `_global_` |
| `market_margin_core` | 市場整體融資維持率 | `market_margin_maintenance_derived` | `(market, date)` | `_market_` |
| `business_indicator_core` | 台灣景氣指標 | `business_indicator_derived` | `(market, stock_id, date)` | `_index_business_` |

> **PK 注意**:
> - `exchange_rate_derived` PK **不含 stock_id**,以 `currency` 區分(USD/TWD、JPY/TWD 等)
> - `market_margin_maintenance_derived` PK **不含 stock_id**,屬全市場單一序列
> - `fear_greed_index`(Bronze)PK 同樣不含 stock_id
> - `business_indicator_derived` PK **含 stock_id 但取 sentinel 值 `_market_`**(Silver 端 Bronze 2-col → Silver 3-col 升維,見 layered_schema §3.7);Cores 端 Fact 寫入時轉用保留字 `_index_business_`(語意:景氣指標而非市場整體)
>
> 上述各表的 stock_id 保留字依 §6.2.1(總綱)使用,載入器在組裝 `Series` 時填入保留字。

> **fear_greed_core 例外**:Silver 目前無 `fear_greed_index_derived`,本 Core 暫時直讀 Bronze。此為已知架構例外(違反總綱 §4.4 但已登記),待 Silver 端補 derived 後切換。詳見 §6.2 與 §6.7。

> **多序列保留字規則**:
> - `taiex_core` 處理 TAIEX 與 TPEx 兩條並列大盤序列,**各自使用獨立保留字**(`_index_taiex_` / `_index_tpex_`)。Fact 統計與查詢時可直接用 `WHERE stock_id = ?` 區分
> - `us_market_core` 處理 SPY 與 VIX 屬同源衍生關係(VIX 為 SPY 隱含波動率),**共用保留字 `_index_us_market_`**,Fact 以 `metadata.subseries: "spy"|"vix"` 區分
> - 設計意圖見總綱 §6.2.1「多序列區分規則」

---

## 二、共通規範(本子類)

### 2.1 trait

全部走 `IndicatorCore` trait(見總綱 §3),`Input` 為各自對應的環境資料序列(`MarketIndexTwSeries` / `MarketIndexUsSeries` / `ExchangeRateSeries` / `FearGreedIndexSeries` / `MarketMarginMaintenanceSeries` / `BusinessIndicatorSeries`),由 `shared/environment_loader/` 提供載入器(見總綱 §3.4)。各 Core 的 `warmup_periods()` 單位依輸入頻率決定,見總綱 §3.4 與 §7.3.1。

### 2.2 個股無關性

Environment Cores 的輸出**與個股無關**,反映整體市場 / 環境變數。`stock_id` 使用保留字(`_market_` / `_global_` / `_index_taiex_` / `_index_tpex_` / `_index_us_market_` / `_index_business_`),完整規範見總綱 §6.2.1,本節不重述。

### 2.3 Environment Cores 的設計意義

Environment Cores 的事實**不會直接影響個股 Core 的計算**,而是在 Aggregation Layer 與個股事實**並排呈現**,讓使用者自己連結個股技術面與大環境的關聯。

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

### 2.4 計算策略

| Core | 頻率 | batch 處理方式 |
|---|---|---|
| `taiex_core` | 日頻 | 增量,每日收盤後 |
| `us_market_core` | 日頻(美國日) | 增量,需處理時差(見 §4.7) |
| `exchange_rate_core` | 日頻 | 增量,多幣別並行 |
| `fear_greed_core` | 日頻 | 增量 |
| `market_margin_core` | 日頻 | 增量 |
| `business_indicator_core` | 月頻 | 每日 batch 掃描,新月份景氣指標發布時計算(每月 27 日左右,國發會發布) |

### 2.5 Output 統一結構

依總綱 §2「Output schema 同構」原則,所有 Environment Core 的 Output 採以下兩層結構:

```rust
pub struct XxxOutput {
    pub series: Vec<XxxPoint>,      // 時間序列數值
    pub events: Vec<XxxEvent>,      // 事件型 Fact 來源
}

pub struct XxxEvent {
    pub date: NaiveDate,
    pub kind: XxxEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}
```

### 2.6 Fact 邊界提醒

Fact 邊界與禁用詞彙清單見總綱 §6.1.1,本子類涉及的「市場情緒轉樂觀」、「美股拖累台股」、「匯率壓力減輕」等主觀詞彙已收錄於該節,此處不重述。

---

## 三、`taiex_core`

### 3.1 定位

加權指數的趨勢、技術指標、量能事實。

### 3.2 上游 Silver

- 表:`taiex_index_derived`
- PK:`(market, stock_id, date)`,`stock_id` 取值為 `TAIEX` / `TPEx`(Silver 端原始識別)
- 關鍵欄位:`open / high / low / close / volume`
- 載入器:`shared/environment_loader/`,提供 `MarketIndexTwSeries`,內部依 `stock_id` 拆為兩條獨立序列
- **保留 stock_id**(Cores 端 Fact 寫入):
  - `_index_taiex_` 對應 Silver `TAIEX` 序列
  - `_index_tpex_` 對應 Silver `TPEx` 序列
  - 兩者並列大盤,Fact 寫入時各自獨立保留字(設計依據:總綱 §6.2.1「並列獨立」規則)

### 3.3 此 Core 自身可呼叫其他 Indicator Core 嗎?

**不可以**。`taiex_core` **內嵌**自己所需的指標計算(MACD、RSI 等),**不**從外部 Indicator Core 取資料,理由:

- 維持零耦合原則(Core 之間不互相 import)
- `taiex_core` 用的是大盤指數,個股 Indicator Core 用的是個股 OHLC,輸入不同
- 邏輯雷同部分可走 `shared/` 共用工具(若未來抽出,屬 P3 後議題)

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

依 §7.3.1 慣例:MACD 系 ×4(EMA 收斂)與 lookback + 緩衝取大者,符合慣例不附偏離理由。

### 3.6 Output

```rust
pub struct TaiexOutput {
    pub series_by_index: Vec<TaiexSeriesEntry>,  // TAIEX 與 TPEx 各一條
    pub events: Vec<TaiexEvent>,
}

pub struct TaiexSeriesEntry {
    pub index_code: TaiexIndexCode,        // Taiex / Tpex
    pub series: Vec<TaiexPoint>,
}

pub enum TaiexIndexCode {
    Taiex,    // 對應保留 stock_id `_index_taiex_`
    Tpex,     // 對應保留 stock_id `_index_tpex_`
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
    pub index_code: TaiexIndexCode,        // 事件所屬指數
    pub kind: TaiexEventKind,
    pub value: f64,                        // 指標值或變動百分比
    pub metadata: serde_json::Value,
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

每個事件依其 `index_code` 寫入對應保留 stock_id:

| Fact statement | stock_id(寫入時) | metadata |
|---|---|---|
| `TAIEX MACD golden cross at 2026-04-15` | `_index_taiex_` | `{ index: "taiex" }` |
| `TAIEX broke above MA60 at 2026-04-22(close=18,500, MA60=18,250)` | `_index_taiex_` | `{ index: "taiex", close: 18500, ma60: 18250 }` |
| `TPEx MACD golden cross at 2026-04-18` | `_index_tpex_` | `{ index: "tpex" }` |
| `TPEx volume z-score 3.5 on 2026-04-25(volume surge)` | `_index_tpex_` | `{ index: "tpex", z: 3.5 }` |
| `TAIEX RSI = 78 on 2026-04-25(overbought for 5 days)` | `_index_taiex_` | `{ index: "taiex", rsi: 78, days: 5 }` |

> **設計提醒**:Fact statement 開頭明示 `TAIEX` 或 `TPEx`,即使 metadata 同樣帶 `index` 欄位,這是冗餘但故意保留 —— 確保 statement 在不查 metadata 時也具完整語意。

---

## 四、`us_market_core`

### 4.1 定位

美股大盤(SPY)趨勢、VIX 區間、夜盤異動事實。

### 4.2 上游 Silver

- 表:`us_market_index_derived`
- PK:`(market, stock_id, date)`,`stock_id` 取值為 `SPY` / `^VIX` 等代號(Silver 端原始識別)
- 關鍵欄位:`open / high / low / close / volume`
- 載入器:`shared/environment_loader/`,提供 `MarketIndexUsSeries`,內部依 `stock_id` 拆為 SPY / VIX 兩條序列
- **保留 stock_id**(Cores 端 Fact 寫入):`_index_us_market_`(SPY 與 VIX 共用,屬同源衍生)
- **多序列區分**:Fact metadata 帶 `subseries: "spy"|"vix"`(設計依據:總綱 §6.2.1「同源衍生」規則)

### 4.3 為何要看美股

台股與美股相關性高,美股事實作為台股 Core 的**外部環境**並排呈現。但 Core 本身**不**做「美股漲台股會漲」的因果推論。

### 4.4 Params

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

### 4.5 warmup_periods

```rust
fn warmup_periods(&self, params: &UsMarketParams) -> usize {
    params.spy_macd_slow * 4
}
```

依 §7.3.1 慣例:單層 EMA × 4 為收斂期,符合慣例不附偏離理由。

### 4.6 Output

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
    pub metadata: serde_json::Value,
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

### 4.7 Fact 範例

所有事件統一寫入保留 stock_id `_index_us_market_`,以 `metadata.subseries` 區分 SPY / VIX:

| Fact statement | stock_id | metadata |
|---|---|---|
| `SPY MACD death cross at 2026-04-22` | `_index_us_market_` | `{ subseries: "spy", event: "macd_death_cross" }` |
| `VIX spike to 32.5 on 2026-04-25(+45% single day)` | `_index_us_market_` | `{ subseries: "vix", value: 32.5, change_pct: 45.0 }` |
| `VIX entered high zone(>25) on 2026-04-22` | `_index_us_market_` | `{ subseries: "vix", value: 26.3, threshold: 25.0 }` |
| `SPY -2.8% on 2026-04-22 evening(US time)` | `_index_us_market_` | `{ subseries: "spy", change_pct: -2.8, us_date: "2026-04-22" }` |

### 4.8 時區注意

美股交易時間與台股不同步。Output 的 `date` 為美國日,Aggregation Layer 在對齊台股時間軸時需處理時差(美股當日收盤事實對應台股**次日**開盤的環境)。`metadata.us_date` 欄位供下游明確區分。

---

## 五、`exchange_rate_core`

### 5.1 定位

匯率(USD/TWD 等)趨勢、突破事實。

### 5.2 上游 Silver

- 表:`exchange_rate_derived`
- **PK:`(market, date, currency)` — 不含 stock_id**
- 關鍵欄位:`rate`
- 載入器:`shared/environment_loader/`,提供 `ExchangeRateSeries`(載入器將多幣別 row 組裝為單一 Series,以 `currency_pair` 區分)
- **保留 stock_id**:`_global_`(Fact 寫入時使用)

### 5.3 為何要看匯率

匯率影響外資進出與出口股獲利。但 Core 本身**不**做「匯率升值對某個股利空」這類因果推論,僅輸出匯率事實。

### 5.4 Params

```rust
pub struct ExchangeRateParams {
    pub timeframe: Timeframe,
    pub currency_pairs: Vec<String>,           // ["USD/TWD"], 可多組
    pub ma_period: usize,                      // 預設 20
    pub key_levels: Vec<f64>,                  // 重要關鍵價位,預設 [30.0, 31.0, 32.0]
    pub significant_change_threshold: f64,     // 單日大幅變動,預設 0.5(%)
}
```

### 5.5 warmup_periods

```rust
fn warmup_periods(&self, params: &ExchangeRateParams) -> usize {
    params.ma_period + 10
}
```

依 §7.3.1 慣例:SMA 視窗 + 緩衝,符合慣例不附偏離理由。

### 5.6 Output

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
    pub kind: ExchangeRateEventKind,
    pub value: f64,                        // 匯率值或變化%
    pub metadata: serde_json::Value,
}

pub enum ExchangeRateEventKind {
    KeyLevelBreakout,          // 突破關鍵價位
    KeyLevelBreakdown,
    SignificantSingleDayMove,
    MaCross,
}
```

### 5.7 Fact 範例

| Fact statement | metadata |
|---|---|
| `USD/TWD broke above 30.5 on 2026-04-22(rate=30.55)` | `{ pair: "USD/TWD", level: 30.5, rate: 30.55 }` |
| `USD/TWD -0.85% on 2026-04-25(largest move in 60 days)` | `{ pair: "USD/TWD", change: -0.85, lookback: "60d" }` |
| `USD/TWD crossed above MA(20) at 2026-04-15` | `{ pair: "USD/TWD", direction: "above", ma_period: 20 }` |

---

## 六、`fear_greed_core`

### 6.1 定位

恐慌貪婪指數(CNN 美股指數,經 FinMind API 取得),區間判定與極端事件。

### 6.2 上游資料(架構例外)

- ⚠️ **目前直讀 Bronze**:`fear_greed_index`,PK `(market, date)`
- 關鍵欄位:`score`(0–100)、`label`(Fear / Greed / Neutral / Extreme Fear / Extreme Greed)
- 載入器:`shared/environment_loader/`,提供 `FearGreedIndexSeries`
- **保留 stock_id**:`_global_`(Fact 寫入時使用)

**為何例外**:依總綱 §4.4「Cores 一律從 Silver 層讀取」,但 `fear_greed_index` 在 layered_schema r2 後**尚無 derived 表**(資料來源單純,目前無清洗需求)。本 Core 為**已登記的架構例外**。

**TODO(P3 前處理)**:
- 補建 `fear_greed_index_derived` 至 Silver 層 S6_derived_environment
- 本 Core 載入器切換為讀 derived
- 例外條目從本節移除

### 6.3 Params

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

### 6.4 warmup_periods

```rust
fn warmup_periods(&self, params: &FearGreedParams) -> usize {
    params.streak_min_days + 10
}
```

依 §7.3.1 慣例:連續事件偵測 = lookback + 緩衝,符合慣例不附偏離理由。

### 6.5 Output

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
    pub value: f64,                        // 指數值
    pub metadata: serde_json::Value,
}

pub enum FearGreedEventKind {
    EnteredExtremeFear,
    ExitedExtremeFear,
    EnteredExtremeGreed,
    ExitedExtremeGreed,
    StreakInZone,                  // 連續處於某區間
}
```

### 6.6 Fact 範例

| Fact statement | metadata |
|---|---|
| `Fear & Greed Index entered extreme fear zone(22) on 2026-04-25` | `{ value: 22, threshold: 25.0 }` |
| `Fear & Greed Index in greed zone for 8 consecutive days` | `{ zone: "greed", days: 8 }` |
| `Fear & Greed Index exited extreme greed at 73 on 2026-04-22` | `{ value: 73, threshold: 75.0 }` |

### 6.7 Bronze 直讀的影響範圍

本 Core 直讀 Bronze 期間,以下行為與其他 Environment Core 略有不同:

- 載入器讀 `fear_greed_index`(Bronze)而非 `*_derived`
- `is_dirty / dirty_at` 機制不可用,batch 排程改以「日期最大值」判斷新資料
- 切換至 derived 後,本 Core 僅需更新載入器,Core 邏輯不變

---

## 七、`market_margin_core`

> **命名注意**:本 Core 為**市場整體**融資維持率,與個股級 `margin_core`(Chip Cores)分立。`market_` 前綴依總綱 §13.2.1 命名規範強制使用。

### 7.1 定位

市場整體融資維持率,反映槓桿風險。

### 7.2 上游 Silver

- 表:`market_margin_maintenance_derived`
- **PK:`(market, date)` — 不含 stock_id**
- 關鍵欄位:`ratio` / `total_margin_purchase_balance` / `total_short_sale_balance`
- 載入器:`shared/environment_loader/`,提供 `MarketMarginMaintenanceSeries`
- **保留 stock_id**:`_market_`(Fact 寫入時使用)

### 7.3 與 `margin_core` 的差異

| 項目 | `margin_core`(個股) | `market_margin_core`(整體) |
|---|---|---|
| 範圍 | 單一個股的融資融券 | 全市場融資維持率 |
| 分類 | Chip Cores | Environment Cores |
| `stock_id` | 個股代號 | `_market_` |
| 上游 Silver | `margin_daily_derived` | `market_margin_maintenance_derived` |
| Silver PK | `(market, stock_id, date)` | `(market, date)` 不含 stock_id |
| 用途 | 個股籌碼分析 | 大環境風險評估 |

### 7.4 Params

```rust
pub struct MarketMarginParams {
    pub timeframe: Timeframe,
    pub maintenance_warning_threshold: f64,    // 預設 145.0(%)
    pub maintenance_danger_threshold: f64,     // 預設 130.0(%)
    pub significant_change_threshold: f64,     // 單日變化閾值,預設 5.0(%)
}
```

### 7.5 warmup_periods

```rust
fn warmup_periods(&self, params: &MarketMarginParams) -> usize {
    20
}
```

**偏離 §7.3.1 慣例理由**:本 Core 為閾值分區與短期波動偵測,無平滑收斂與結構性 lookback。固定 20 個交易日為單日異動「歷史最大」事件偵測所需的最小窗口。

### 7.6 Output

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
    pub value: f64,                        // 維持率值或變化%
    pub metadata: serde_json::Value,
}

pub enum MarketMarginEventKind {
    EnteredWarningZone,
    EnteredDangerZone,
    ExitedDangerZone,
    SignificantSingleDayDrop,
}
```

### 7.7 Fact 範例

| Fact statement | metadata |
|---|---|
| `Market margin maintenance dropped to 142% on 2026-04-25(warning zone)` | `{ value: 142.0, threshold: 145.0 }` |
| `Market margin maintenance reached 128% on 2026-04-28(danger zone)` | `{ value: 128.0, threshold: 130.0 }` |
| `Market margin maintenance dropped 6.5% in single day on 2026-04-22` | `{ change: -6.5, lookback: "20d" }` |

### 7.8 強制平倉風險警訊

當 `maintenance_rate < danger_threshold`(預設 130%),代表大量融資戶接近強制平倉,可能引發市場連鎖賣壓。但 Core 本身**不**做「強平風險即將引發崩盤」這類預測,僅輸出客觀數值與分區事件。

---

## 八、`business_indicator_core`

### 8.1 定位

台灣景氣指標(國發會發布,月頻)的事件偵測:領先指標轉折、景氣對策信號(藍/黃藍/綠/黃紅/紅)變化、連續燈號事件。

### 8.2 上游 Silver

- 表:`business_indicator_derived`
- PK:`(market, stock_id, date)`,`stock_id` 取 sentinel `_market_`(Silver 端 Bronze 2-col → 3-col 升維,見 layered_schema §3.7)
- 關鍵欄位:`leading_indicator`(領先指標)、`coincident_indicator`(同時指標)、`lagging_indicator`(落後指標)、`monitoring`(景氣對策信號燈分數,9–45)、`monitoring_color`(燈號:`blue`/`yellow_blue`/`green`/`yellow_red`/`red`)
- 載入器:`shared/environment_loader/`,提供 `BusinessIndicatorSeries`
- **保留 stock_id**(Cores 端 Fact 寫入):`_index_business_`(語意:景氣指標,而非市場整體統計)

> **保留字選擇理由**:Silver 端用 sentinel `_market_` 純粹為了 Bronze 2-col → Silver 3-col PK 升維(避免 NULL),語意與「市場整體統計」(`market_margin_core` 的 `_market_`)無關。Cores 端 Fact 改用 `_index_business_` 明確語意,避免下游查詢時將景氣事件誤歸為市場交易統計。Loader 負責轉換。

### 8.3 為何要看景氣指標

景氣循環是中長期經濟環境變數,影響個股基本面解讀(例:景氣藍燈期間電子股 EPS 通常承壓)。但 Core 本身**不**做「景氣轉好台股看多」這類因果推論,僅輸出客觀景氣事實。

### 8.4 Params

```rust
pub struct BusinessIndicatorParams {
    pub leading_streak_min_months: usize,         // 領先指標連續上升/下降月數,預設 3
    pub leading_turning_threshold: f64,           // 領先指標轉折變化率閾值(%),預設 0.5
    pub monitoring_streak_min_months: usize,      // 燈號連續月數,預設 3
}
```

### 8.5 warmup_periods

```rust
fn warmup_periods(&self, params: &BusinessIndicatorParams) -> usize {
    // 月頻資料,單位為月份數
    // 連續事件偵測 + 緩衝
    params.leading_streak_min_months
        .max(params.monitoring_streak_min_months) + 12
}
```

依 §7.3.1 慣例:連續事件偵測 = lookback + 緩衝。本 Core 為月頻,`+12` 緩衝為一年資料便於跨年比較,符合慣例不附偏離理由。

### 8.6 Output

```rust
pub struct BusinessIndicatorOutput {
    pub series: Vec<BusinessIndicatorPoint>,
    pub events: Vec<BusinessIndicatorEvent>,
}

pub struct BusinessIndicatorPoint {
    pub period: String,                           // "2026-03"(月份標籤)
    pub fact_date: NaiveDate,                     // 月底日(對齊 Fact schema)
    pub report_date: NaiveDate,                   // 國發會實際發布日(月底+27 日左右)
    pub leading_indicator: f64,
    pub coincident_indicator: f64,
    pub lagging_indicator: f64,
    pub monitoring: i32,                          // 9–45 分
    pub monitoring_color: MonitoringColor,
}

pub enum MonitoringColor {
    Blue,           // 9-16 分,景氣低迷
    YellowBlue,     // 17-22 分,景氣轉向
    Green,          // 23-31 分,景氣穩定
    YellowRed,      // 32-37 分,景氣轉熱
    Red,            // 38-45 分,景氣熱絡
}

pub struct BusinessIndicatorEvent {
    pub date: NaiveDate,                          // 事件月底日
    pub kind: BusinessIndicatorEventKind,
    pub value: f64,                               // 事件主要數值
    pub metadata: serde_json::Value,
}

pub enum BusinessIndicatorEventKind {
    LeadingTurningUp,                  // 領先指標連續下降後轉折向上
    LeadingTurningDown,                // 連續上升後轉折向下
    LeadingStreakUp,                   // 連續上升 N 月
    LeadingStreakDown,                 // 連續下降 N 月
    MonitoringColorChange,             // 燈號變化(metadata 帶 from / to)
    MonitoringStreakInColor,           // 連續同色 N 月
}
```

### 8.7 Fact 範例

統一寫入保留 stock_id `_index_business_`:

| Fact statement | stock_id | metadata |
|---|---|---|
| `Leading indicator turned up at 2026-03(after 5 months down)` | `_index_business_` | `{ event: "leading_turning_up", streak_before: 5, value: 99.8 }` |
| `Monitoring color changed from blue to yellow_blue at 2026-04(score=17)` | `_index_business_` | `{ event: "color_change", from: "blue", to: "yellow_blue", score: 17 }` |
| `Monitoring stayed in green for 6 consecutive months ending 2026-03` | `_index_business_` | `{ event: "color_streak", color: "green", months: 6 }` |
| `Leading indicator continuously rose for 4 months through 2026-04` | `_index_business_` | `{ event: "leading_streak_up", months: 4, current_value: 101.2 }` |

### 8.8 月頻資料的時間對齊

景氣指標屬月頻(每月 27 日左右國發會發布上月數據),Fact 的 `fact_date` 為**該月最後一個交易日**,`report_date` 為實際發布日。Aggregation Layer 在對齊個股日頻 Fact 時,景氣事件以「發布日後生效」處理(避免 look-ahead bias)。對齊規則同 `revenue_core` / `financial_statement_core`(見 fundamental_cores §2.3)。

---

## 九、Environment 與個股 Core 的並排原則

### 9.1 不在 Core 層整合

「VIX 飆升 + 個股 RSI 超買 = 賣訊」這類綜合判斷涉及 Environment Core 與個股 Core,**不**在 Core 層整合。本原則見總綱 §11(跨指標訊號處理原則),適用於跨子類組合。

### 9.2 並排呈現

由 Aggregation Layer 將兩類 Core 的 Fact 並排呈現,使用者自己連線。

### 9.3 為何不立「環境調整 Core」

過去版本曾考慮「VIX 高時調降個股技術指標權重」這類設計,但已棄用,理由:

- 違反「並排不整合」原則
- 「VIX 多高才算高」、「該調降多少權重」屬主觀判斷,寫進 Core 等於替使用者下決策
- v1.1 的 Combined confidence ×1.1/×0.7 已棄用,本子類不重蹈覆轍

### 9.4 使用者教學層的角色

如何結合大環境與個股屬投資哲學議題,由使用者教學文件提供識讀指引,不在架構層處理。

例:「VIX 高時優先看防禦股」屬投資策略,**不**寫進 Core 也**不**寫進 Aggregation Layer。
