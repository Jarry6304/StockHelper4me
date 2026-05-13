# Indicator Cores:動量 / 趨勢 / 強度類

> **版本**:v2.0 抽出版 r2
> **日期**:2026-05-06
> **配套文件**:`cores_overview.md`(共通規範)、`layered_schema_post_refactor.md`(Silver 層)、`adr/0001_tw_market_handling.md`
> **包含 Core**:9 個
> **優先級分布**:P1(5 個)/ P3(4 個)

---

## r2 修訂摘要(2026-05-06)

- **跟進 overview r2**:廢除 `TW-Market Core`,所有資料前處理職責歸 Silver S1_adjustment Rust binary
- §2.1 輸入描述改為「經 Silver S1_adjustment 處理」,移除 `LimitMergeStrategy` enum 提及
- §7 `ma_core` Params 重構為 `Vec<MaSpec>`,單一實例計算多條均線,§7.5 Output / §7.6 Fact / §7.7 多均線組合 / §7.8 跨均線交叉同步重寫
- §九 `williams_r_core` / §十 `cci_core` 補 `warmup_periods` 子節
- 新增 §十一 `coppock_core`(P3)完整章節,原「動量類不收編說明」順移為 §十二
- 新增「對應資料表」段落於各 Core(對齊 `neely_core.md` §17 範本)
- §2 共通規範補「統一規範引用」收束(對齊 `neely_core.md` §15.5)

---

## 目錄

1. [本文件範圍](#一本文件範圍)
2. [共通規範](#二共通規範本子類)
3. [`macd_core`](#三macd_corep1)
4. [`rsi_core`](#四rsi_corep1)
5. [`kd_core`](#五kd_corep1)
6. [`adx_core`](#六adx_corep1)
7. [`ma_core`](#七ma_corep1)
8. [`ichimoku_core`](#八ichimoku_corep3)
9. [`williams_r_core`](#九williams_r_corep3)
10. [`cci_core`](#十cci_corep3)
11. [`coppock_core`](#十一coppock_corep3)
12. [動量類不收編說明](#十二動量類不收編說明)

---

## 一、本文件範圍

| Core | 名稱 | 優先級 |
|---|---|---|
| `macd_core` | MACD | P1 |
| `rsi_core` | RSI | P1 |
| `kd_core` | KD / Stochastic | P1 |
| `adx_core` | ADX / DMI | P1 |
| `ma_core` | SMA / EMA / WMA(同族統一) | P1 |
| `ichimoku_core` | 一目均衡表 | P3 |
| `williams_r_core` | Williams %R | P3 |
| `cci_core` | CCI | P3 |
| `coppock_core` | Coppock Curve | P3 |

> **注**:overview §9 P1 列出技術指標 8 個,本子類佔 5 個(`macd / rsi / kd / adx / ma`),其餘 P1 為 `bollinger / atr / obv`(波動 / 量能類)。

---

## 二、共通規範(本子類)

本子類全走 `IndicatorCore` trait(見總綱 §3)、滑動窗口型計算策略(總綱 §7.2)、輸出寫入 `indicator_values` 與 `facts`(總綱 §7.1)。

### 2.1 輸入

統一從 Silver 層 `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd` 讀取**經 Silver S1_adjustment Rust binary 處理過的 OHLC**(透過 `shared/ohlcv_loader/`,見總綱 §3.4)。

所有資料前處理(後復權、漲跌停合併)已由 Silver 層完成。Cores 層不再擁有 `LimitMergeStrategy` 等漲跌停處理 enum;若需查詢漲跌停事件,直接讀 Silver `price_limit_merge_events`(見 `layered_schema_post_refactor.md` §4.1)。

> **歷史備註**:v2.0 r1 曾規劃 Cores 層的 `TW-Market Core` 作為 OHLC 前置處理,r2 後此 Core 廢除,職責歸 Silver S1。詳見 `adr/0001_tw_market_handling.md`。

### 2.2 統一規範引用

- Fact statement 詞彙限制遵循 `cores_overview.md` §6.1.1(禁用主觀詞彙)
- `stock_id` 編碼遵循 `cores_overview.md` §6.2.1(保留字規範);本子類 Core 處理個股,實務上使用真實股票代號
- Facts 表 unique constraint 與 `params_hash` 演算法遵循 `cores_overview.md` §6.3 / §7.4
- Output 結構不自帶 `source_version` / `params_hash`,由 Pipeline 在寫入 `indicator_values` / `facts` 時補入

### 2.3 對應資料表

本子類所有 Core 的資料來源與寫入目標相同(對齊 `neely_core.md` §17 範本):

| 用途 | 資料表 |
|---|---|
| 輸入 OHLC | Silver `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd`(由 S1_adjustment Rust binary 產出) |
| 寫入時間序列值 | `indicator_values`(JSONB),`source_core` 為各 Core 名稱 |
| 寫入 Fact | `facts`(append-only),`source_core` 為各 Core 名稱 |

> Silver `price_*_fwd` 表結構與處理邏輯由 `layered_schema_post_refactor.md` §4.1 定義。各 Core 不關心 Silver 內部如何產出,只消費結果。各 Core 章節不再重述本表,**統一引用本節**。

---

## 三、`macd_core`(P1)

### 3.1 定位

MACD(Moving Average Convergence Divergence),動量指標,由快慢均線差值與訊號線組成。

### 3.2 Params

```rust
pub struct MacdParams {
    pub fast: usize,        // 預設 12
    pub slow: usize,        // 預設 26
    pub signal: usize,      // 預設 9
    pub timeframe: Timeframe,
}
```

### 3.3 warmup_periods

```rust
fn warmup_periods(&self, params: &MacdParams) -> usize {
    // EMA 慣例 ×4(總綱 §7.3.1)
    params.slow * 4  // 預設 26 * 4 = 104
}
```

### 3.4 Output

```rust
pub struct MacdOutput {
    pub series: Vec<MacdPoint>,
}

pub struct MacdPoint {
    pub date: NaiveDate,
    pub macd_line: f64,        // EMA(fast) - EMA(slow)
    pub signal_line: f64,      // EMA(macd_line, signal)
    pub histogram: f64,        // macd_line - signal_line
}
```

### 3.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `MACD(12,26,9) golden cross at 2026-04-15` | `{ event: "golden_cross", date: "2026-04-15", macd: 0.45, signal: 0.42 }` |
| `MACD(12,26,9) death cross at 2026-04-22` | `{ event: "death_cross", date: "2026-04-22" }` |
| `MACD(12,26,9) histogram expanded 8 consecutive bars` | `{ event: "histogram_expansion", bars: 8, end_date: "2026-04-25" }` |
| `MACD(12,26,9) bearish divergence: price HH 2026-03-20, MACD LH 2026-04-10` | `{ event: "bearish_divergence", price_date: "2026-03-20", indicator_date: "2026-04-10" }` |
| `MACD(12,26,9) histogram crossed zero at 2026-04-15(positive)` | `{ event: "histogram_zero_cross", direction: "positive" }` |

### 3.6 背離規則(機械式定義)

**僅產出嚴格規則式背離**,需符合:

- 兩個價格極值點(HH 或 LL)之間時間距離 ≥ N 根 K 棒(預設 N=20)
- 兩極值點之間 MACD 對應方向相反
- 該規則需明確寫死於 `compute.rs`,不接受「視覺背離」

---

## 四、`rsi_core`(P1)

### 4.1 定位

RSI(Relative Strength Index),動量振盪指標,衡量近期漲跌幅相對強度。

### 4.2 Params

```rust
pub struct RsiParams {
    pub period: usize,             // 預設 14
    pub overbought: f64,           // 預設 70.0
    pub oversold: f64,             // 預設 30.0
    pub timeframe: Timeframe,
}
```

### 4.3 warmup_periods

```rust
fn warmup_periods(&self, params: &RsiParams) -> usize {
    params.period * 4  // 預設 14 * 4 = 56
}
```

### 4.4 Output

```rust
pub struct RsiOutput {
    pub series: Vec<RsiPoint>,
}

pub struct RsiPoint {
    pub date: NaiveDate,
    pub value: f64,  // 0.0 ~ 100.0
}
```

### 4.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `RSI(14) = 78.3 at 2026-04-25, > 70 for 5 consecutive days` | `{ event: "overbought_streak", value: 78.3, days: 5 }` |
| `RSI(14) = 22.1 at 2026-04-28, < 30 for 3 consecutive days` | `{ event: "oversold_streak", value: 22.1, days: 3 }` |
| `RSI(14) crossed below 70 at 2026-04-26 (exiting overbought)` | `{ event: "overbought_exit", date: "2026-04-26" }` |
| `RSI(14) bearish divergence: price HH 2026-03-20, RSI LH 2026-04-10` | `{ event: "bearish_divergence" }` |
| `RSI(14) failure swing detected at 2026-04-22` | `{ event: "failure_swing", direction: "bearish" }` |

### 4.6 失敗擺動規則

Failure Swing 為 RSI 經典訊號,需符合:

1. RSI 進入超買 / 超賣區
2. RSI 退出超買 / 超賣區
3. RSI 折返但未再次進入超買 / 超賣區
4. RSI 跌破 / 突破前次低點 / 高點

四步全成立才產出 Fact。

---

## 五、`kd_core`(P1)

### 5.1 定位

KD(Stochastic Oscillator),動量振盪指標,衡量收盤價在 N 日區間的相對位置。

### 5.2 Params

```rust
pub struct KdParams {
    pub period: usize,             // 預設 9(台股慣例,美股常用 14)
    pub k_smooth: usize,           // 預設 3
    pub d_smooth: usize,           // 預設 3
    pub overbought: f64,           // 預設 80.0
    pub oversold: f64,             // 預設 20.0
    pub timeframe: Timeframe,
}
```

### 5.3 warmup_periods

```rust
fn warmup_periods(&self, params: &KdParams) -> usize {
    params.period + params.k_smooth + params.d_smooth + 10  // 緩衝
}
```

### 5.4 Output

```rust
pub struct KdOutput {
    pub series: Vec<KdPoint>,
}

pub struct KdPoint {
    pub date: NaiveDate,
    pub k: f64,    // 0.0 ~ 100.0
    pub d: f64,    // 0.0 ~ 100.0
}
```

### 5.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `KD(9,3,3) golden cross at 2026-04-15(K=23.1, D=21.5)` | `{ event: "golden_cross", k: 23.1, d: 21.5 }` |
| `KD(9,3,3) death cross at 2026-04-22(K=82.3, D=84.1)` | `{ event: "death_cross", k: 82.3, d: 84.1 }` |
| `KD(9,3,3) overbought streak: K > 80 for 7 consecutive days` | `{ event: "overbought_streak", days: 7 }` |
| `KD(9,3,3) bearish divergence at 2026-04-10` | `{ event: "bearish_divergence" }` |

### 5.6 台股慣例提醒

台股技術分析習慣使用 KD(9,3,3),不同於美股慣用的 (14,3,3)。**Params 預設值依台股慣例**,但 Workflow toml 可覆寫。

---

## 六、`adx_core`(P1)

### 6.1 定位

ADX(Average Directional Index)+ DMI(Directional Movement Index),衡量趨勢強度與方向。

### 6.2 Params

```rust
pub struct AdxParams {
    pub period: usize,             // 預設 14
    pub strong_trend_threshold: f64, // 預設 25.0
    pub very_strong_threshold: f64,  // 預設 50.0
    pub timeframe: Timeframe,
}
```

### 6.3 warmup_periods

```rust
fn warmup_periods(&self, params: &AdxParams) -> usize {
    // ADX 為雙層平滑(DI 平均後再平滑),×6 慣例(總綱 §7.3.1)
    params.period * 6
}
```

### 6.4 Output

```rust
pub struct AdxOutput {
    pub series: Vec<AdxPoint>,
}

pub struct AdxPoint {
    pub date: NaiveDate,
    pub adx: f64,           // 0.0 ~ 100.0
    pub plus_di: f64,       // +DI
    pub minus_di: f64,      // -DI
}
```

### 6.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `ADX(14) = 32.5 at 2026-04-25, +DI > -DI for 20 days` | `{ event: "uptrend_strength", adx: 32.5, days: 20 }` |
| `ADX(14) crossed above 25 at 2026-04-15(strong trend)` | `{ event: "strong_trend_start" }` |
| `ADX(14) DI cross: +DI crossed above -DI at 2026-04-10` | `{ event: "di_bullish_cross" }` |
| `ADX(14) ADX peak at 48.2 on 2026-04-20, declining since` | `{ event: "adx_peak", value: 48.2 }` |

### 6.6 ADX 解讀提醒

**注意**:ADX 本身不指示方向,僅指示強度。方向由 +DI / -DI 判斷。Fact 文字必須清楚區分,避免使用者誤解。

---

## 七、`ma_core`(P1)

### 7.1 定位

統一處理 SMA / EMA / WMA / DEMA / TEMA / HMA 等同族均線指標,以 `Vec<MaSpec>` 一次宣告多條均線,單一實例同時計算並偵測「Price cross MA」與「跨均線交叉」事件。

### 7.2 為何同族統一 + 多條同算

依 overview §2.3 同族合併三條件:

1. **Params 結構同構**:同樣的 `kind / period / source` 欄位集合,僅取值不同 ✅
2. **Output schema 同構**:同樣是 `(date, value)` 序列 ✅
3. **Fact 種類同構**:統一為 `ma_bullish_cross / ma_bearish_cross / ma_golden_cross / above_ma_streak` ✅

三條全成立 → 合併為單一 Core。

**為何單一實例算多條而非多次 entry**:
- 跨均線交叉(SMA20 vs SMA60)需同時看到兩條均線才能偵測
- 若拆成多個 entry,每個實例 `compute(input, params) -> output` 只見自己的 params,無法產出跨均線交叉 Fact
- 因此 ma_core 的 Params 持有 `Vec<MaSpec>`,單一實例輸出多條 series 並在內部偵測交叉
- 此設計**不**違反零耦合原則(同 Core 同族子型號內部協同,非跨 Core 引用)

### 7.3 Params

```rust
pub struct MaParams {
    pub specs: Vec<MaSpec>,       // 至少 1 條,常見 5 / 10 / 20 / 60 / 120 / 240
    pub timeframe: Timeframe,
    pub detect_cross_pairs: CrossPairPolicy,  // 跨均線交叉偵測策略
}

pub struct MaSpec {
    pub kind: MaKind,
    pub period: usize,
    pub source: PriceSource,       // 預設 Close
}

pub enum MaKind {
    Sma,
    Ema,
    Wma,
    Dema,
    Tema,
    Hma,    // Hull MA
}

pub enum PriceSource {
    Close,
    Open,
    High,
    Low,
    Hl2,    // (High + Low) / 2
    Hlc3,   // (High + Low + Close) / 3
    Ohlc4,
}

pub enum CrossPairPolicy {
    None,                  // 不偵測跨均線交叉
    AllPairs,              // 偵測所有兩兩組合
    Pairs(Vec<(usize, usize)>),  // 指定 (short_period, long_period) 對
}
```

`MaParams` derive `Default` 時,`specs = vec![MaSpec { kind: Sma, period: 20, source: Close }]`,`detect_cross_pairs = CrossPairPolicy::None`。

### 7.4 warmup_periods

各 MaKind 倍數依總綱 §7.3.1 慣例(SMA ×1 / EMA ×4 / DEMA ×6 / TEMA ×8 / HMA ×2),取所有 spec 中的最大值:

```rust
fn warmup_periods(&self, params: &MaParams) -> usize {
    params.specs.iter().map(|spec| {
        match spec.kind {
            MaKind::Sma => spec.period,
            MaKind::Ema => spec.period * 4,
            MaKind::Wma => spec.period,
            MaKind::Dema => spec.period * 6,
            MaKind::Tema => spec.period * 8,
            MaKind::Hma => spec.period * 2,
        }
    }).max().unwrap_or(0) + 5  // 緩衝
}
```

### 7.5 Output

```rust
pub struct MaOutput {
    pub series_by_spec: Vec<MaSeriesEntry>,
}

pub struct MaSeriesEntry {
    pub spec: MaSpec,             // 對應的 spec
    pub series: Vec<MaPoint>,
}

pub struct MaPoint {
    pub date: NaiveDate,
    pub value: f64,
}
```

每條 spec 對應一個 `MaSeriesEntry`,前端與 Aggregation Layer 依 `spec.kind / spec.period` 索引取用。

### 7.6 Fact 範例

| Fact statement | metadata |
|---|---|
| `Price crossed above SMA(20) at 2026-04-15` | `{ event: "ma_bullish_cross", ma_kind: "sma", period: 20 }` |
| `Price crossed below EMA(60) at 2026-04-22` | `{ event: "ma_bearish_cross", ma_kind: "ema", period: 60 }` |
| `SMA(20) crossed above SMA(60) at 2026-04-10(golden cross)` | `{ event: "ma_golden_cross", short: { kind: "sma", period: 20 }, long: { kind: "sma", period: 60 } }` |
| `EMA(50) crossed below EMA(200) at 2026-04-25(death cross)` | `{ event: "ma_death_cross", short: { kind: "ema", period: 50 }, long: { kind: "ema", period: 200 } }` |
| `Price held above EMA(200) for 60 consecutive days` | `{ event: "above_ma_streak", ma_kind: "ema", period: 200, days: 60 }` |

### 7.7 多均線組合宣告範例

Workflow toml 一次宣告多條均線 + 指定要偵測的交叉對:

```toml
[[indicator_cores]]
name = "ma"
params.timeframe = "daily"
params.detect_cross_pairs = { type = "pairs", pairs = [[20, 60], [50, 200]] }

[[params.specs]]
kind = "sma"
period = 5

[[params.specs]]
kind = "sma"
period = 20

[[params.specs]]
kind = "sma"
period = 60

[[params.specs]]
kind = "ema"
period = 50

[[params.specs]]
kind = "ema"
period = 200
```

整組宣告在單一 ma_core 實例內,`params_hash` 涵蓋整個 specs 清單。

### 7.8 跨均線交叉 Fact 為何不違反零耦合

跨均線交叉(SMA20 cross SMA60)由 `ma_core` **單一實例內部**同時產出兩條 series 並偵測,**不**屬「跨指標訊號」:

- 跨指標訊號(overview §11):需要消費**不同 Core** 的輸出(例:Bollinger + Keltner = TTM Squeeze)
- 跨均線交叉:**同一 Core 同一實例**的內部 series 比對,屬同族子型號協同

此設計與 overview §11.2「同時看兩個 Core 才能成立的訊號不進架構」並無衝突。

---

## 八、`ichimoku_core`(P3)

### 8.1 定位

一目均衡表(Ichimoku Kinkō Hyō),含 Tenkan / Kijun / Senkou Span A/B / Chikou 五條線,提供完整趨勢判讀。

### 8.2 Params

```rust
pub struct IchimokuParams {
    pub tenkan_period: usize,      // 預設 9
    pub kijun_period: usize,       // 預設 26
    pub senkou_b_period: usize,    // 預設 52
    pub displacement: usize,       // 預設 26(Senkou Span 前移、Chikou 後移)
    pub timeframe: Timeframe,
}
```

### 8.3 warmup_periods

```rust
fn warmup_periods(&self, params: &IchimokuParams) -> usize {
    params.senkou_b_period + params.displacement + 10
}
```

### 8.4 Output

```rust
pub struct IchimokuOutput {
    pub series: Vec<IchimokuPoint>,
}

pub struct IchimokuPoint {
    pub date: NaiveDate,
    pub tenkan: f64,
    pub kijun: f64,
    pub senkou_a: f64,
    pub senkou_b: f64,
    pub chikou: f64,
    pub cloud_color: CloudColor,    // Bullish(span_a > span_b) / Bearish
}
```

### 8.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `Ichimoku Tenkan/Kijun bullish cross at 2026-04-15` | `{ event: "tk_bullish_cross" }` |
| `Price entered Kumo cloud from above at 2026-04-22` | `{ event: "cloud_entry", direction: "from_above" }` |
| `Price broke above Kumo cloud at 2026-04-25` | `{ event: "cloud_breakout", direction: "above" }` |
| `Kumo twist(Senkou A/B cross) projected at 2026-05-10` | `{ event: "kumo_twist", projected_date: "2026-05-10" }` |
| `Chikou span broke above price at 2026-04-20` | `{ event: "chikou_bullish_break" }` |

---

## 九、`williams_r_core`(P3)

### 9.1 定位

Williams %R,動量振盪指標,類似 Stochastic 但反向(範圍 -100 ~ 0)。

### 9.2 Params

```rust
pub struct WilliamsRParams {
    pub period: usize,             // 預設 14
    pub overbought: f64,           // 預設 -20.0
    pub oversold: f64,             // 預設 -80.0
    pub timeframe: Timeframe,
}
```

### 9.3 warmup_periods

```rust
fn warmup_periods(&self, params: &WilliamsRParams) -> usize {
    // 與 Stochastic 同源,單層平滑取 ×4 慣例(總綱 §7.3.1)
    params.period * 4
}
```

### 9.4 Output

```rust
pub struct WilliamsROutput {
    pub series: Vec<WilliamsRPoint>,
}

pub struct WilliamsRPoint {
    pub date: NaiveDate,
    pub value: f64,    // -100.0 ~ 0.0
}
```

### 9.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `Williams %R(14) = -85 at 2026-04-25, < -80 for 4 days` | `{ event: "oversold_streak", days: 4 }` |
| `Williams %R(14) crossed above -80 at 2026-04-26(exiting oversold)` | `{ event: "oversold_exit" }` |

---

## 十、`cci_core`(P3)

### 10.1 定位

CCI(Commodity Channel Index),衡量價格相對 N 日均值的偏離程度。

### 10.2 Params

```rust
pub struct CciParams {
    pub period: usize,             // 預設 20
    pub overbought: f64,           // 預設 100.0
    pub oversold: f64,             // 預設 -100.0
    pub extreme_high: f64,         // 預設 200.0
    pub extreme_low: f64,          // 預設 -200.0
    pub timeframe: Timeframe,
}
```

### 10.3 warmup_periods

```rust
fn warmup_periods(&self, params: &CciParams) -> usize {
    // CCI 為視窗統計(均值與平均絕對偏差),依總綱 §7.3.1 慣例:period + 緩衝
    params.period + 5
}
```

### 10.4 Output

```rust
pub struct CciOutput {
    pub series: Vec<CciPoint>,
}

pub struct CciPoint {
    pub date: NaiveDate,
    pub value: f64,
}
```

### 10.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `CCI(20) = 215 at 2026-04-25, exceeded extreme zone(>200)` | `{ event: "extreme_high", value: 215.0 }` |
| `CCI(20) crossed above 100 at 2026-04-15` | `{ event: "overbought_entry" }` |
| `CCI(20) zero line cross(positive) at 2026-04-10` | `{ event: "zero_cross_positive" }` |

---

## 十一、`coppock_core`(P3)

### 11.1 定位

Coppock Curve(Coppock 曲線),長期動能指標,由 Edwin Coppock 於 1962 年提出,主要用於月線判斷主升段起點(由負轉正)。標準計算式:

```
Coppock = WMA(n_wma, ROC(n_long) + ROC(n_short))
```

預設參數 `WMA(10, ROC(14) + ROC(11))`,常用於月線。

### 11.2 Params

```rust
pub struct CoppockParams {
    pub roc_long: usize,           // 預設 14(長期 ROC 週期)
    pub roc_short: usize,          // 預設 11(短期 ROC 週期)
    pub wma_period: usize,         // 預設 10(WMA 平滑週期)
    pub timeframe: Timeframe,      // 主推 Monthly,日線 / 週線意義較弱
}
```

### 11.3 warmup_periods

```rust
fn warmup_periods(&self, params: &CoppockParams) -> usize {
    // ROC 為差分(視窗統計型),WMA 為加權平均(視窗統計型),兩層視窗串接
    // 依總綱 §7.3.1 慣例:max(roc_long, roc_short) + wma_period + 緩衝
    params.roc_long.max(params.roc_short) + params.wma_period + 5
}
```

### 11.4 Output

```rust
pub struct CoppockOutput {
    pub series: Vec<CoppockPoint>,
}

pub struct CoppockPoint {
    pub date: NaiveDate,
    pub value: f64,
}
```

### 11.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `Coppock(14,11,10) zero line cross(positive) at 2026-04-30` | `{ event: "zero_cross_positive", date: "2026-04-30" }` |
| `Coppock(14,11,10) zero line cross(negative) at 2026-03-31` | `{ event: "zero_cross_negative", date: "2026-03-31" }` |
| `Coppock(14,11,10) trough at -45.2 on 2026-02-28, rising since` | `{ event: "trough", value: -45.2, since_date: "2026-02-28" }` |
| `Coppock(14,11,10) bullish divergence: price LL 2026-01-31, Coppock HL 2026-04-30` | `{ event: "bullish_divergence", price_date: "2026-01-31", indicator_date: "2026-04-30" }` |

### 11.6 月線時間框架建議

Coppock Curve 的設計初衷是月線指標,日線 / 週線使用須注意:

| 時間框架 | 適用性 | 說明 |
|---|---|---|
| Monthly | ✅ 主要 | Coppock 原意設計,長期主升段判斷 |
| Weekly | ⚠️ 意義較弱 | 訊號頻率提高但失去長期動能本質 |
| Daily | ❌ 不建議 | 訊號雜訊過多,失去 Coppock 的篩選價值 |

Workflow toml 預設 `timeframe = "monthly"`。

### 11.7 背離規則

對齊 `macd_core §3.6` 規則:

- 兩個價格極值點(HH 或 LL)之間時間距離 ≥ N 月(預設 N=6,因月線粒度)
- 兩極值點之間 Coppock 對應方向相反
- 該規則需明確寫死於 `compute.rs`

---

## 十二、動量類不收編說明

為避免日後重複討論,明確列出**不獨立成 Core**的動量類概念。

### 12.1 不收編清單

| 項目 | 為何不獨立 | 實際處理方式 |
|---|---|---|
| **MACD-Histogram** | 已是 `macd_core` Output 子欄位 | 由 `macd_core` 一併輸出與產出 Fact |
| **Stochastic RSI(StochRSI)** | RSI 套 KD 平滑的衍生品,屬「跨指標衍生」,違反零耦合 | 不獨立;若使用者需要,屬使用者教學層 |
| **DMI(+DI / -DI)** | 已內建於 `adx_core`(ADX 由 +DI / -DI 計算而來) | 由 `adx_core` 一併輸出 |
| **TRIX / TSI / DPO** 等冷門動量 | 使用率低,不入 P1~P3 範圍 | 未列入,未來視需求加入 |
| **動量類綜合訊號**(如 RSI+ADX 雙指標確認) | 跨指標訊號,違反零耦合 | 援引總綱 §11,屬使用者教學層 |

### 12.2 動量類同族合併判準

依總綱 §2.3 三條件判定。動量類已執行的合併:

- **MA 族(SMA/EMA/WMA/DEMA/TEMA/HMA)合併為 `ma_core`**:三條件全滿足,且採 `Vec<MaSpec>` 設計支援多條同算與跨均線交叉偵測
- **MACD 系(主線 / signal / histogram)合併為 `macd_core`**:屬同一指標的多個輸出欄位,本就是單一 Core
- **RSI 與 StochRSI 不合併**:Output schema 異構(StochRSI 需多一層 KD 平滑),Fact 種類雖近但不同源

未來新增動量指標時,依總綱 §2.3 判準決定獨立或併入既有 Core。