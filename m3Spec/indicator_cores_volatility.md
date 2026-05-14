# Indicator Cores:波動 / 通道類

> **版本**:v2.0 抽出版 r4
> **日期**:2026-05-14
> **配套文件**:`cores_overview.md`(共通規範)、`layered_schema_post_refactor.md`(Silver 層)、`adr/0001_tw_market_handling.md`
> **包含 Core**:4 個
> **優先級分布**:P1(2 個)/ P3(2 個)

---

## r4 修訂摘要(2026-05-14 — P2 production calibration 回寫)

- **`bollinger_core` EventKind 從 5 個擴 12 個**(對齊 v1.31 Round 4 production-driven
  修法,2026-05-10 commit `91804df`):
  - **既有 4 個** stay-in-zone / streak / extreme:`BandwidthExtremeLow`、
    `SqueezeStreak`、`WalkingUpperBand`、`WalkingLowerBand`(無需改動)
  - **新增 8 個 Entered/Exited transition**(取代 spec r2 原本的 stay-in-zone EventKind):
    `EnteredUpperBandTouch` / `ExitedUpperBandTouch`、
    `EnteredLowerBandTouch` / `ExitedLowerBandTouch`、
    `EnteredAboveUpperBand` / `ExitedAboveUpperBand`、
    `EnteredBelowLowerBand` / `ExitedBelowLowerBand`
- **修正動機**:r2 spec 列「`upper_band_touch` / `above_upper_band` 等 stay-in-zone」
  在 production 1263 stocks 全市場跑出每日重複觸發 → facts 量爆掉(對比 fear_greed_core
  的 zone transition 範本只在進出區間時觸發一次)。Round 4 fix(2026-05-10)
  改 Enter/Exit 邊緣偵測,facts 量從 466K → 457K(本質 bouncy,小幅降量)
- 詳見 `CLAUDE.md` v1.31 Round 4 §「Entered/Exited bouncy 防衛」

---

## r2 修訂摘要(2026-05-06)

- **跟進 overview r2**:廢除 `TW-Market Core`,所有資料前處理職責歸 Silver S1_adjustment Rust binary
- §4.5 `atr_core` Fact 範例第一條(每日 ATR 數值,不入 facts 表)從 Fact 表移除,改置於 §4.6
- §2 共通規範補「對應資料表」與「統一規範引用」段落(對齊 `neely_core.md` §17 / §15.5)

---

## 目錄

1. [本文件範圍](#一本文件範圍)
2. [共通規範](#二共通規範本子類)
3. [`bollinger_core`](#三bollinger_corep1)
4. [`atr_core`](#四atr_corep1)
5. [`keltner_core`](#五keltner_corep3)
6. [`donchian_core`](#六donchian_corep3)
7. [TTM Squeeze 等跨指標訊號的處理](#七ttm-squeeze-等跨指標訊號的處理)

---

## 一、本文件範圍

| Core | 名稱 | 優先級 |
|---|---|---|
| `bollinger_core` | 布林通道 | P1 |
| `atr_core` | ATR(Average True Range) | P1 |
| `keltner_core` | Keltner Channel | P3 |
| `donchian_core` | Donchian Channel | P3 |

---

## 二、共通規範(本子類)

本子類全走 `IndicatorCore` trait(見總綱 §3)、滑動窗口型計算策略(總綱 §7.2)、輸出寫入 `indicator_values` 與 `facts`(總綱 §7.1)。

### 2.1 「通道」概念統一

本子類中三個 Core(Bollinger / Keltner / Donchian)都是「以中線 ± 通道寬」形式,統一輸出 `upper_band` / `middle_band` / `lower_band` 三線結構,便於前端與 Aggregation Layer 一致處理。

### 2.2 ATR 的雙重身份提醒

**ATR 同時是**:

1. **獨立 Indicator Core**(`atr_core`)— 對外輸出 ATR 值與相關 Fact
2. **Neely Core 的工程參數依賴** — Neely Core 內部自帶 ATR 計算邏輯(`NeelyEngineConfig.atr_period = 14`)

兩者**互不引用**:Neely Core 不依賴 `atr_core`,因為其計算邏輯內嵌且為固定常數;`atr_core` 為對外服務,兩者數值相同但實作獨立。

### 2.3 統一規範引用

- Fact statement 詞彙限制遵循 `cores_overview.md` §6.1.1(禁用主觀詞彙)
- `stock_id` 編碼遵循 `cores_overview.md` §6.2.1(保留字規範);本子類 Core 處理個股,實務上使用真實股票代號
- Facts 表 unique constraint 與 `params_hash` 演算法遵循 `cores_overview.md` §6.3 / §7.4
- Output 結構不自帶 `source_version` / `params_hash`,由 Pipeline 在寫入 `indicator_values` / `facts` 時補入

### 2.4 對應資料表

本子類所有 Core 的資料來源與寫入目標相同(對齊 `neely_core.md` §17 範本):

| 用途 | 資料表 |
|---|---|
| 輸入 OHLC | Silver `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd`(由 S1_adjustment Rust binary 產出) |
| 寫入時間序列值 | `indicator_values`(JSONB),`source_core` 為各 Core 名稱 |
| 寫入 Fact | `facts`(append-only),`source_core` 為各 Core 名稱 |

> Silver `price_*_fwd` 表結構與處理邏輯由 `layered_schema_post_refactor.md` §4.1 定義。各 Core 不關心 Silver 內部如何產出,只消費結果。各 Core 章節不再重述本表,**統一引用本節**。

---

## 三、`bollinger_core`(P1)

### 3.1 定位

布林通道(Bollinger Bands),以 N 日 SMA 為中線、±k 倍標準差為上下軌。

### 3.2 Params

```rust
pub struct BollingerParams {
    pub period: usize,             // 預設 20
    pub std_multiplier: f64,       // 預設 2.0
    pub source: PriceSource,       // 預設 Close
    pub timeframe: Timeframe,
}
```

### 3.3 warmup_periods

```rust
fn warmup_periods(&self, params: &BollingerParams) -> usize {
    params.period + 5  // SMA 不需大量暖機
}
```

### 3.4 Output

```rust
pub struct BollingerOutput {
    pub series: Vec<BollingerPoint>,
}

pub struct BollingerPoint {
    pub date: NaiveDate,
    pub upper_band: f64,
    pub middle_band: f64,    // = SMA(period)
    pub lower_band: f64,
    pub bandwidth: f64,      // (upper - lower) / middle
    pub percent_b: f64,      // (close - lower) / (upper - lower)
}
```

### 3.5 EventKind 與 Fact 範例

```rust
pub enum BollingerEventKind {
    // 既有 streak / extreme(無需改動)
    BandwidthExtremeLow,
    SqueezeStreak,
    WalkingUpperBand,
    WalkingLowerBand,
    // Round 4 transition pattern(2026-05-10):取代 r2 spec 原本的 stay-in-zone EventKind
    EnteredUpperBandTouch,
    ExitedUpperBandTouch,
    EnteredLowerBandTouch,
    ExitedLowerBandTouch,
    EnteredAboveUpperBand,
    ExitedAboveUpperBand,
    EnteredBelowLowerBand,
    ExitedBelowLowerBand,
}
```

**Fact 範例**:

| Fact statement | metadata |
|---|---|
| `Bollinger(20,2) bandwidth at 5-year low(0.062) on 2026-04-25` | `{ event: "bandwidth_extreme_low", value: 0.062, lookback: "5y" }` |
| `Bollinger(20,2) squeeze: bandwidth < 0.10 for 8 consecutive days` | `{ event: "squeeze_streak", days: 8 }` |
| `Bollinger(20,2) walking the band: 5 consecutive closes near upper band` | `{ event: "walking_upper_band", days: 5 }` |
| `Price entered upper band touch zone at 2026-04-15(close=580, upper=578)` | `{ event: "entered_upper_band_touch", close: 580, upper: 578 }` |
| `Price exited upper band touch zone at 2026-04-18` | `{ event: "exited_upper_band_touch" }` |
| `Bollinger(20,2) %B entered above upper band at 2026-04-22(%B=1.05)` | `{ event: "entered_above_upper_band", percent_b: 1.05 }` |
| `Bollinger(20,2) %B exited above upper band at 2026-04-25(%B=0.95)` | `{ event: "exited_above_upper_band", percent_b: 0.95 }` |

> **設計提醒(r4)**:`EnteredX` / `ExitedX` 為**邊緣觸發**(edge trigger),
> 只在狀態轉變當日產出一次 Fact;非「每日落在 zone 內」每日觸發。對齊
> `fear_greed_core` r3 同款 transition pattern,避免 bouncy zone 帶來
> facts 表爆量。

### 3.6 Bollinger Squeeze 的處理

「Bollinger Squeeze」屬於本 Core 內部事實(僅看 bandwidth),可獨立產出 Fact。

但「**TTM Squeeze**」需同時看 Bollinger 與 Keltner,**不**在本 Core 處理 — 屬跨指標訊號,由使用者並排判讀。詳見第七章。

### 3.7 %B 的意義

`percent_b` 表示收盤價在通道內的相對位置:

- `percent_b = 0` → 收盤價剛好在下軌
- `percent_b = 1` → 收盤價剛好在上軌
- `percent_b > 1` → 收盤價突破上軌
- `percent_b < 0` → 收盤價跌破下軌

提供前端做視覺化定位。

---

## 四、`atr_core`(P1)

### 4.1 定位

ATR(Average True Range),平均真實波幅,衡量價格波動度。為許多停損與通道計算的基礎。

### 4.2 Params

```rust
pub struct AtrParams {
    pub period: usize,             // 預設 14
    pub timeframe: Timeframe,
}
```

### 4.3 warmup_periods

```rust
fn warmup_periods(&self, params: &AtrParams) -> usize {
    params.period * 4  // 單層 EMA 平滑,依總綱 §7.3.1 慣例
}
```

### 4.4 Output

```rust
pub struct AtrOutput {
    pub series: Vec<AtrPoint>,
}

pub struct AtrPoint {
    pub date: NaiveDate,
    pub atr: f64,
    pub atr_pct: f64,    // ATR / close * 100,百分比版本
}
```

### 4.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `ATR(14) % at 1-year high(5.8%) on 2026-04-22` | `{ event: "volatility_extreme_high", lookback: "1y", value_pct: 5.8 }` |
| `ATR(14) % at 1-year low(1.2%) on 2026-04-15` | `{ event: "volatility_extreme_low", lookback: "1y", value_pct: 1.2 }` |
| `ATR(14) expanded 50% over 10 days(2.0%→3.0%)` | `{ event: "volatility_expansion", from: 2.0, to: 3.0, days: 10 }` |

### 4.6 入 facts 與不入 facts 的區分

- **每日 ATR 數值**(例:`ATR(14) = 18.5, ATR% = 3.2%`)→ 寫 `indicator_values` JSONB,**不**寫 facts(避免 facts 表爆量)
- **極值事件 / 擴張收縮事件** → 寫 `facts`(即 §4.5 表中所列)

### 4.7 atr_pct 的意義

`atr_pct = ATR / close * 100`,提供**跨股票可比較的波動度**。例如比較 0050 與 2330 的波動度,直接比 ATR 沒意義(價格基準不同),比 atr_pct 才有意義。

---

## 五、`keltner_core`(P3)

### 5.1 定位

Keltner Channel,以 EMA 為中線、±k 倍 ATR 為上下軌。比 Bollinger 對價格 spike 敏感度較低。

### 5.2 Params

```rust
pub struct KeltnerParams {
    pub ema_period: usize,         // 預設 20
    pub atr_period: usize,         // 預設 10
    pub atr_multiplier: f64,       // 預設 2.0
    pub timeframe: Timeframe,
}
```

### 5.3 warmup_periods

```rust
fn warmup_periods(&self, params: &KeltnerParams) -> usize {
    // EMA 與 ATR 各取 ×4 慣例(總綱 §7.3.1),取大者再加緩衝
    (params.ema_period * 4).max(params.atr_period * 4) + 5
}
```

### 5.4 Output

```rust
pub struct KeltnerOutput {
    pub series: Vec<KeltnerPoint>,
}

pub struct KeltnerPoint {
    pub date: NaiveDate,
    pub upper_band: f64,
    pub middle_band: f64,    // = EMA(ema_period)
    pub lower_band: f64,
}
```

### 5.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `Price broke above Keltner(20,10,2) upper band at 2026-04-15` | `{ event: "keltner_upper_breakout" }` |
| `Price broke below Keltner(20,10,2) lower band at 2026-04-22` | `{ event: "keltner_lower_breakout" }` |
| `Price held above Keltner middle line for 30 consecutive days` | `{ event: "above_middle_streak", days: 30 }` |

### 5.6 Keltner 與 Bollinger 並存的設計意圖

兩者都是「中線 ± 寬度」形式但**寬度計算來源不同**:

- Bollinger 用標準差 → 對價格 spike 敏感
- Keltner 用 ATR → 對價格 spike 較不敏感,反映平均真實波動

兩者並存 → 使用者可看出「Bollinger 收進 Keltner 內」(TTM Squeeze)等綜合訊號,但**綜合判讀屬使用者教學層**。

---

## 六、`donchian_core`(P3)

### 6.1 定位

Donchian Channel,以 N 日最高價 / 最低價為上下軌,中線為兩者中點。經典海龜交易系統使用。

### 6.2 Params

```rust
pub struct DonchianParams {
    pub period: usize,             // 預設 20(海龜系統用 20 與 55)
    pub timeframe: Timeframe,
}
```

### 6.3 warmup_periods

```rust
fn warmup_periods(&self, params: &DonchianParams) -> usize {
    params.period + 5
}
```

### 6.4 Output

```rust
pub struct DonchianOutput {
    pub series: Vec<DonchianPoint>,
}

pub struct DonchianPoint {
    pub date: NaiveDate,
    pub upper_band: f64,    // N 日最高
    pub middle_band: f64,
    pub lower_band: f64,    // N 日最低
}
```

### 6.5 EventKind 與 Fact 範例

```rust
pub enum DonchianEventKind {
    BreakoutUp,        // 突破 N 日新高(close > prev_period high)
    Breakdown,         // 跌破 N 日新低
}

const MIN_BREAKOUT_SPACING: usize = 10;   // v1.34 Round 5 加:同方向突破至少間隔 10 bar
```

| Fact statement | metadata |
|---|---|
| `Donchian(20) breakout above 20-day high at 2026-04-15(close=580, high20=578)` | `{ event: "donchian_breakout_up", period: 20 }` |
| `Donchian(20) breakdown below 20-day low at 2026-04-22` | `{ event: "donchian_breakdown", period: 20 }` |
| `Donchian(55) new 55-day high at 2026-04-25` | `{ event: "donchian_breakout_up", period: 55 }` |

> **v1.34 Round 5 production calibration(2026-05-14)**:r3 spec 沒對突破加
> spacing,1264 stocks 跑出某型態(如 Doji)68.9/yr 🔴。加 `MIN_BREAKOUT_SPACING=10`
> + `last_breakout_idx` 狀態追蹤,降至 per-EventKind ≤ 12/yr/stock ✅。

### 6.6 海龜系統慣例

- **Donchian(20)** → 進場訊號(突破 20 日新高 / 新低)
- **Donchian(55)** → 較嚴格的長期突破訊號

Workflow toml 可同時宣告多個 period:

```toml
[[indicator_cores]]
name = "donchian"
params = { period = 20, timeframe = "daily" }

[[indicator_cores]]
name = "donchian"
params = { period = 55, timeframe = "daily" }
```

---

## 七、TTM Squeeze 等跨指標訊號的處理

### 7.1 不立獨立 Core

**TTM Squeeze** 需要同時看 Bollinger 與 Keltner,但**不寫成 `ttm_squeeze_core`**,理由是違反零耦合原則。

### 7.2 處理方式

- ✅ `bollinger_core` 輸出 `bandwidth` / `upper_band` / `lower_band`
- ✅ `keltner_core` 輸出 `upper_band` / `lower_band`
- ✅ Aggregation Layer 並排呈現
- ✅ 教學文件說明「布林收進 Keltner 內 = Squeeze」屬使用者識讀範疇

### 7.3 通則

任何「同時看兩個 Core 才能成立的訊號」都屬使用者教學範疇,不進架構。

### 7.4 例外情況

若某「跨指標訊號」未來成為使用者**極高頻使用**且**規則固化**的事實,可考慮:

- 抽出共用基礎(例:`shared/squeeze_detector/`)
- 在 Aggregation Layer 增加「複合指標」檢視(屬呈現層,不屬 Core 層)

但 v2.0 P0~P3 階段**不考慮**此類擴充,維持架構乾淨。
