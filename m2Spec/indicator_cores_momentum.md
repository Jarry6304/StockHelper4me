# Indicator Cores:動量 / 趨勢 / 強度類

> **版本**:v2.0 抽出版 r1
> **日期**:2026-04-30
> **配套文件**:`cores_overview.md`(共通規範)
> **包含 Core**:9 個
> **優先級分布**:P1(4 個)/ P3(5 個)

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

---

## 一、本文件範圍

| Core | 名稱 | 優先級 |
|---|---|---|
| `macd_core` | MACD | P1 |
| `rsi_core` | RSI | P1 |
| `kd_core` | KD / Stochastic | P1 |
| `adx_core` | ADX / DMI | P1 |
| `ma_core` | SMA / EMA / WMA | P1 |
| `ichimoku_core` | 一目均衡表 | P3 |
| `williams_r_core` | Williams %R | P3 |
| `cci_core` | CCI | P3 |
| `coppock_core` | Coppock Curve | P3 |

---

## 二、共通規範(本子類)

### 2.1 trait

全部走 `IndicatorCore` trait(見 `cores_overview.md` §3)。

### 2.2 計算策略

全部屬**滑動窗口型指標**,採「最近 N 天 + 暖機區增量計算」,每日 batch 寫入當日值至 `indicator_values` JSONB 欄位。

### 2.3 輸入

統一從 `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd` 讀取**經 TW-Market Core 處理過的 OHLC**。

各 Core 預設使用 `LimitMergeStrategy::None`(每日 K 棒保留),除非 Workflow toml 明確指定。

### 2.4 Fact 邊界提醒

僅產出**機械式可重現的事實**:

- ✅ `golden cross`(MACD 線穿越訊號線)
- ✅ `divergence`(明確規則式背離,需有頂底定義)
- ❌ `動能轉強` / `趨勢確立` 等經驗判斷詞彙

### 2.5 寫入分流

| 資料 | 目的地 |
|---|---|
| 每日數值(MACD line / RSI value / ...) | `indicator_values`(JSONB) |
| 事件式事實(golden_cross / overbought_streak) | `facts`(append-only) |

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
    // EMA 收斂約需 3-4 倍週期
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
    params.period * 6  // ADX 收斂較慢,需更多暖機
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

統一處理 SMA / EMA / WMA / DEMA / TEMA 等同族均線指標,以 enum 參數區分子型號。

### 7.2 為何同族統一

- 演算法相近,差異僅在權重計算
- 一個 Core 約 200 行可完成,不需開五個 Core
- Workflow toml 可一次宣告多條均線(MA20 + MA60 + MA120)

### 7.3 Params

```rust
pub struct MaParams {
    pub kind: MaKind,
    pub period: usize,
    pub source: PriceSource,       // 預設 Close
    pub timeframe: Timeframe,
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
```

### 7.4 warmup_periods

```rust
fn warmup_periods(&self, params: &MaParams) -> usize {
    match params.kind {
        MaKind::Sma => params.period,
        MaKind::Ema => params.period * 4,
        MaKind::Wma => params.period,
        MaKind::Dema => params.period * 6,
        MaKind::Tema => params.period * 8,
        MaKind::Hma => params.period * 2,
    }
}
```

### 7.5 Output

```rust
pub struct MaOutput {
    pub series: Vec<MaPoint>,
}

pub struct MaPoint {
    pub date: NaiveDate,
    pub value: f64,
}
```

### 7.6 Fact 範例

| Fact statement | metadata |
|---|---|
| `Price crossed above SMA(20) at 2026-04-15` | `{ event: "ma_bullish_cross", ma_kind: "sma", period: 20 }` |
| `Price crossed below EMA(60) at 2026-04-22` | `{ event: "ma_bearish_cross", ma_kind: "ema", period: 60 }` |
| `SMA(20) crossed above SMA(60) at 2026-04-10(golden cross)` | `{ event: "ma_golden_cross", short_period: 20, long_period: 60 }` |
| `Price held above EMA(200) for 60 consecutive days` | `{ event: "above_ma_streak", ma_kind: "ema", period: 200, days: 60 }` |

### 7.7 多均線組合查詢

**注意**:單一 `ma_core` 實例只算一條均線。Workflow 若需 5 / 10 / 20 / 60 / 120 / 240 等多條,需在 toml 列多次 entry,各帶不同 params:

```toml
[[indicator_cores]]
name = "ma"
params = { kind = "sma", period = 5, timeframe = "daily" }

[[indicator_cores]]
name = "ma"
params = { kind = "sma", period = 20, timeframe = "daily" }

[[indicator_cores]]
name = "ma"
params = { kind = "ema", period = 60, timeframe = "daily" }
```

各 entry 的 `params_hash` 不同,寫入 `indicator_values` 不會衝突。

### 7.8 跨均線交叉 Fact

跨均線交叉(SMA20 cross SMA60)由 `ma_core` 內部偵測產出,**不**屬「跨指標訊號」(因為都是同一 Core 同族子型號)。

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

### 9.3 Output

```rust
pub struct WilliamsROutput {
    pub series: Vec<WilliamsRPoint>,
}

pub struct WilliamsRPoint {
    pub date: NaiveDate,
    pub value: f64,    // -100.0 ~ 0.0
}
```

### 9.4 Fact 範例

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

### 10.3 Output

```rust
pub struct CciOutput {
    pub series: Vec<CciPoint>,
}

pub struct CciPoint {
    pub date: NaiveDate,
    pub value: f64,
}
```

### 10.4 Fact 範例

| Fact statement | metadata |
|---|---|
| `CCI(20) = 215 at 2026-04-25, exceeded extreme zone(>200)` | `{ event: "extreme_high", value: 215.0 }` |
| `CCI(20) crossed above 100 at 2026-04-15` | `{ event: "overbought_entry" }` |
| `CCI(20) zero line cross(positive) at 2026-04-10` | `{ event: "zero_cross_positive" }` |

---

## 十一、`coppock_core`(P3)

### 11.1 定位

Coppock Curve,長期動能指標,主要用於月線判斷大型底部訊號。

### 11.2 Params

```rust
pub struct CoppockParams {
    pub roc1_period: usize,        // 預設 14(月)
    pub roc2_period: usize,        // 預設 11(月)
    pub wma_period: usize,         // 預設 10(月)
    pub timeframe: Timeframe,      // 預設 Monthly(此指標主要用於月線)
}
```

### 11.3 warmup_periods

```rust
fn warmup_periods(&self, params: &CoppockParams) -> usize {
    params.roc1_period + params.wma_period + 12  // 月線單位
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
| `Coppock(14,11,10) crossed above zero at 2026-04(monthly)` | `{ event: "zero_cross_positive", timeframe: "monthly" }` |
| `Coppock(14,11,10) bottomed at -45 in 2026-02, rising since` | `{ event: "trough", value: -45, date: "2026-02" }` |

### 11.6 注意事項

Coppock Curve 主要用於月線,日線數值意義不大。Workflow toml 中應限制 timeframe 為 monthly,Pipeline 在 daily / weekly 不執行此 Core(或標記為 not_applicable)。
