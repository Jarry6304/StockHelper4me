# Indicator Cores:型態 / 價位類

> **版本**:v2.0 抽出版 r2
> **日期**:2026-05-06
> **配套文件**:`cores_overview.md`(共通規範)、`layered_schema_post_refactor.md`(Silver 層)、`adr/0001_tw_market_handling.md`
> **包含 Core**:3 個
> **優先級分布**:P2(全部 3 個)

---

## r2 修訂摘要(2026-05-06)

- **跟進 overview r2**:廢除 `TW-Market Core`,所有資料前處理職責歸 Silver S1_adjustment Rust binary
- §4.4 `SrLevel` / `SrStrength` 移除 `touch_count` 欄位重複(SrStrength 不再持有 touch_count,改由外層 SrLevel 提供)
- §2 共通規範補「對應資料表」與「統一規範引用」段落(對齊 `neely_core.md` §17 / §15.5)
- 註:本子類為**結構性指標**,寫入目標為 `structural_snapshots` 而非 `indicator_values`(見 §2.4)

---

## 目錄

1. [本文件範圍](#一本文件範圍)
2. [共通規範](#二共通規範本子類)
3. [`candlestick_pattern_core`](#三candlestick_pattern_corep2)
4. [`support_resistance_core`](#四support_resistance_corep2)
5. [`trendline_core`](#五trendline_corep2)
6. [Fibonacci 不獨立的說明](#六fibonacci-不獨立的說明)

---

## 一、本文件範圍

| Core | 名稱 | 優先級 |
|---|---|---|
| `candlestick_pattern_core` | K 線型態 | P2 |
| `support_resistance_core` | 靜態撐壓位 | P2 |
| `trendline_core` | 趨勢線 | P2 |

---

## 二、共通規範(本子類)

本子類全走 `IndicatorCore` trait(見總綱 §3),但屬**結構性指標**,寫入策略與動量 / 波動 / 量能類不同。

### 2.1 結構性指標的特色

與動量 / 波動 / 量能類不同,本子類:

- 輸出**結構性快照**(SR 位、趨勢線、K 線型態),寫入 `structural_snapshots` 而非 `indicator_values`(見總綱 §7.1)
- 計算成本較高,通常**每日全量重算**而非滑動窗口增量
- 對歷史資料敏感(回看窗口較大)
- 輸入仍為 Silver `price_*_fwd`(經 S1_adjustment 處理過的 OHLC)

### 2.2 Stage 4 拆分對應

Batch Pipeline Stage 4 拆 4a / 4b:

- **Stage 4a**:獨立結構性 Core(Neely / Traditional / `support_resistance_core` / `candlestick_pattern_core`)平行執行
- **Stage 4b**:依賴 4a 的 Core(`trendline_core` / Fib 投影)在 4a 完成後執行

> **注**:overview r2 §12.4 列舉 Stage 4a 時僅以「SR」縮寫代表,本節為精確展開版本(含 candlestick_pattern_core)。後續 overview 修訂應同步補上。

### 2.3 統一規範引用

- Fact statement 詞彙限制遵循 `cores_overview.md` §6.1.1(禁用主觀詞彙)
- `stock_id` 編碼遵循 `cores_overview.md` §6.2.1(保留字規範);本子類 Core 處理個股,實務上使用真實股票代號
- Facts 表 unique constraint 與 `params_hash` 演算法遵循 `cores_overview.md` §6.3 / §7.4
- Output 結構不自帶 `source_version` / `params_hash`,由 Pipeline 在寫入 `structural_snapshots` / `facts` 時補入

### 2.4 對應資料表

本子類所有 Core 的資料來源與寫入目標(對齊 `neely_core.md` §17 範本):

| 用途 | 資料表 |
|---|---|
| 輸入 OHLC | Silver `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd`(由 S1_adjustment Rust binary 產出) |
| 寫入結構快照 | `structural_snapshots`(JSONB),`source_core` 為各 Core 名稱 |
| 寫入 Fact | `facts`(append-only),`source_core` 為各 Core 名稱 |
| 特例:`trendline_core` 額外消費 | Neely Core 的 `monowave_series` 輸出(`structural_snapshots.snapshot.monowave_series`,`core_name = 'neely_core'`),見 §5 |

> Silver `price_*_fwd` 表結構與處理邏輯由 `layered_schema_post_refactor.md` §4.1 定義。各 Core 章節不再重述本表,**統一引用本節**。

---

## 三、`candlestick_pattern_core`(P2)

### 3.1 定位

K 線型態識別,**僅嚴格規則式型態**,不含視覺判斷型「形似」。

### 3.2 嚴格規則式的判別準則

進入本 Core 的型態必須滿足:

1. **可數學定義**:用 OHLC 與比例條件可寫出 if/else
2. **無歧義**:任何時候執行都產出相同結果
3. **不依賴上下文情緒**:不看「市場氛圍」、「籌碼結構」

### 3.3 收錄型態清單(P2 第一版)

#### 單根 K 線型態

| 型態 | 條件示意 |
|---|---|
| `Doji` | `|close - open| / (high - low) < 0.1` |
| `Long-legged Doji` | Doji 且 `high - low > 1.5 * ATR(14)` |
| `Hammer` | 下影線 ≥ 2 倍實體 + 上影線 ≤ 0.5 倍實體 + 處於下跌趨勢中 |
| `Inverted Hammer` | 上影線 ≥ 2 倍實體 + 下影線 ≤ 0.5 倍實體 + 處於下跌趨勢中 |
| `Hanging Man` | 形狀同 Hammer 但出現於上漲趨勢中 |
| `Shooting Star` | 形狀同 Inverted Hammer 但出現於上漲趨勢中 |
| `Marubozu(Bullish)` | 紅 K + 上下影線 < 實體 5% |
| `Marubozu(Bearish)` | 黑 K + 上下影線 < 實體 5% |

#### 雙根 K 線型態

| 型態 | 條件示意 |
|---|---|
| `Bullish Engulfing` | 黑 K 後接紅 K,紅 K 實體完全吞噬黑 K 實體 |
| `Bearish Engulfing` | 紅 K 後接黑 K,黑 K 實體完全吞噬紅 K 實體 |
| `Tweezer Top` | 兩根 K 的高點在 ±0.5% 容差內,出現於上漲趨勢中 |
| `Tweezer Bottom` | 兩根 K 的低點在 ±0.5% 容差內,出現於下跌趨勢中 |

#### 三根 K 線型態

| 型態 | 條件示意 |
|---|---|
| `Morning Star` | 大黑 K + 小實體跳空 + 大紅 K 收復一半 |
| `Evening Star` | 大紅 K + 小實體跳空 + 大黑 K 跌破一半 |
| `Three White Soldiers` | 三根連續紅 K + 開盤在前根實體內 + 收高於前根 |
| `Three Black Crows` | 三根連續黑 K + 開盤在前根實體內 + 收低於前根 |

### 3.4 Params

```rust
pub struct CandlestickPatternParams {
    pub timeframe: Timeframe,
    pub trend_lookback: usize,         // 判斷「趨勢中」的回看窗口,預設 5
    pub doji_threshold: f64,           // 預設 0.1
    pub tweezer_tolerance: f64,        // 預設 0.005(0.5%)
    pub enabled_patterns: Vec<PatternKind>,  // 可選擇開啟的型態
}
```

### 3.5 warmup_periods

```rust
fn warmup_periods(&self, params: &CandlestickPatternParams) -> usize {
    // 結構性 Core,依總綱 §7.3.1 慣例:lookback + 緩衝
    params.trend_lookback + 5
}
```

### 3.6 Output

```rust
pub struct CandlestickPatternOutput {
    pub patterns: Vec<DetectedPattern>,
}

pub struct DetectedPattern {
    pub date: NaiveDate,
    pub pattern_kind: PatternKind,
    pub bar_count: usize,              // 1 / 2 / 3
    pub trend_context: TrendContext,   // Uptrend / Downtrend / Sideways
    pub strength_metric: f64,          // 型態符合程度的度量值(非主觀分數)
}

pub enum PatternKind {
    Doji,
    LongLeggedDoji,
    Hammer,
    InvertedHammer,
    HangingMan,
    ShootingStar,
    MarubozuBullish,
    MarubozuBearish,
    BullishEngulfing,
    BearishEngulfing,
    TweezerTop,
    TweezerBottom,
    MorningStar,
    EveningStar,
    ThreeWhiteSoldiers,
    ThreeBlackCrows,
}
```

### 3.7 Fact 範例

| Fact statement | metadata |
|---|---|
| `Doji at 2026-04-15(body/range=0.04)` | `{ pattern: "doji", body_ratio: 0.04 }` |
| `Bullish Engulfing at 2026-04-22 in downtrend` | `{ pattern: "bullish_engulfing", trend_context: "downtrend" }` |
| `Three White Soldiers from 2026-04-20 to 2026-04-22` | `{ pattern: "three_white_soldiers", start: "2026-04-20", end: "2026-04-22" }` |
| `Hammer at 2026-04-25 in downtrend(lower_shadow/body=3.5)` | `{ pattern: "hammer", shadow_body_ratio: 3.5 }` |

### 3.8 strength_metric 的意義

`strength_metric` **不是主觀分數**,而是「型態符合程度的度量值」,例:

- Hammer:`lower_shadow / body`(數值越大,影線越誇張)
- Doji:`|close - open| / range`(數值越小,十字形越標準)

**用途**:供使用者篩選「典型 Hammer」vs「勉強 Hammer」,但 Pipeline 不替使用者排序。

### 3.9 不收錄的型態

以下型態**明確不收錄**:

- `Head and Shoulders` / `Double Top` / `Triangle`(屬中長期型態,由 Neely Core 或 Traditional Core 處理)
- `Cup and Handle` / `Wedge`(視覺判讀過重,規則化困難)
- 「紅三兵看起來像反轉」等基於後續價格行為的型態

---

## 四、`support_resistance_core`(P2)

### 4.1 定位

靜態撐壓位偵測,基於歷史價位的觸碰次數識別重要水平位。

### 4.2 Params

```rust
pub struct SupportResistanceParams {
    pub timeframe: Timeframe,
    pub lookback_bars: usize,              // 預設 120(日線約半年)
    pub touch_count_min: usize,            // 至少觸碰次數,預設 3
    pub price_cluster_tolerance: f64,      // 價位聚類容差(相對),預設 0.01(1%)
    pub min_distance_between_levels: f64,  // 兩位之間的最小距離,預設 0.02(2%)
}
```

### 4.3 warmup_periods

```rust
fn warmup_periods(&self, params: &SupportResistanceParams) -> usize {
    params.lookback_bars + 10
}
```

### 4.4 Output

```rust
pub struct SupportResistanceOutput {
    pub support_levels: Vec<SrLevel>,
    pub resistance_levels: Vec<SrLevel>,
    pub generated_at: NaiveDate,
}

pub struct SrLevel {
    pub price: f64,
    pub level_kind: SrKind,            // Support / Resistance
    pub touch_count: usize,            // 觸碰次數(主屬性)
    pub touch_dates: Vec<NaiveDate>,
    pub first_seen: NaiveDate,
    pub last_seen: NaiveDate,
    pub strength_metric: SrStrength,   // 客觀度量,非主觀分數
}

pub struct SrStrength {
    pub recency_bars: usize,           // 最近一次觸碰距今 K 棒數
    pub time_span_bars: usize,         // 第一次到最後一次觸碰跨度
    pub avg_volume_at_touches: f64,    // 觸碰時的平均成交量
}
```

> **設計說明**:`touch_count` 屬 SrLevel 主屬性(任何撐壓位都需要它),`SrStrength` 僅持有「次級度量」。前端排序與篩選時,`touch_count` 為第一指標,`SrStrength.recency_bars` / `time_span_bars` 為第二指標。

### 4.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `Resistance at 580.0(touched 5 times from 2026-01-15 to 2026-04-20)` | `{ price: 580.0, kind: "resistance", touch_count: 5 }` |
| `Support at 456.1(touched 4 times from 2026-02-10 to 2026-04-25)` | `{ price: 456.1, kind: "support", touch_count: 4 }` |
| `Price broke below support at 456.1 on 2026-04-28` | `{ event: "support_break", broken_level: 456.1 }` |
| `Resistance flipped to support: 520.0 broken on 2026-03-15, retested as support 2026-04-10` | `{ event: "level_flip", price: 520.0 }` |

### 4.6 撐壓互換的判定

「Resistance flipped to Support」屬經典結構行為,本 Core **規則式判定**:

1. 某 resistance 被向上突破(收盤站上 1.02 倍)
2. 後續 N 棒內(預設 30)價格回測該位
3. 回測後價格反彈,未再向下跌破

三步全成立才產出 `level_flip` Fact。

### 4.7 與 Neely Core 的差異

`support_resistance_core` 是**靜態水平位**(price-based),不涉及波浪結構。Neely Core 的 W4 終點等屬**結構性位**(wave-based)。兩者並存,並排呈現,讓使用者自己看出「靜態撐壓位是否與波浪結構吻合」。

---

## 五、`trendline_core`(P2)

### 5.1 定位

趨勢線偵測,基於 swing point 連線。

### 5.2 **唯一耦合例外的 Core**

`trendline_core` 是**全 Core 系統中唯一允許消費另一個 Core 輸出的例外**,可讀取 Neely Core 的 monowave 輸出(僅 monowave,不讀 scenario forest)。

詳細耦合管控見 `cores_overview.md` §12。

### 5.3 Params

```rust
pub struct TrendlineParams {
    pub timeframe: Timeframe,
    pub swing_source: SwingSource,         // Neely monowave / 自實作 swing detector
    pub min_pivots: usize,                 // 至少需幾個 pivot,預設 3
    pub touch_tolerance: f64,              // 觸碰容差(相對),預設 0.005
    pub min_slope_bars: usize,             // 趨勢線最短跨度,預設 10
    pub max_lookback_bars: usize,          // 最大回看,預設 250
}

pub enum SwingSource {
    NeelyMonowave,         // 消費 neely_core 輸出(P2 第一版)
    SharedSwingDetector,   // 消費 shared/swing_detector(P3 後考慮)
}
```

### 5.4 warmup_periods

```rust
fn warmup_periods(&self, params: &TrendlineParams) -> usize {
    params.max_lookback_bars + 10
}
```

### 5.5 Output

```rust
pub struct TrendlineOutput {
    pub trendlines: Vec<Trendline>,
    pub generated_at: NaiveDate,
}

pub struct Trendline {
    pub id: String,
    pub direction: TrendDirection,         // Ascending / Descending
    pub kind: TrendlineKind,               // Support / Resistance
    pub anchor_pivots: Vec<PivotRef>,      // 至少兩個 pivot,連成線
    pub additional_touches: Vec<NaiveDate>,// 後續觸碰記錄
    pub start_date: NaiveDate,
    pub last_valid_date: NaiveDate,
    pub slope: f64,                        // 斜率(每根 K 棒的價格變動)
    pub status: TrendlineStatus,           // Active / Broken
    pub broken_at: Option<NaiveDate>,
    pub source_core: Option<String>,       // "neely_core" 標註資料來源
}

pub struct PivotRef {
    pub date: NaiveDate,
    pub price: f64,
    pub neely_monowave_id: Option<String>, // 追溯至 Neely Core 的 monowave
}

pub enum TrendlineStatus {
    Active,
    Broken,
    Reclaimed,    // 跌破後又站回(視情況保留)
}
```

### 5.6 Fact 範例

| Fact statement | metadata |
|---|---|
| `Ascending trendline from 2026-01-15 to 2026-03-10, broken at 2026-04-22` | `{ event: "trendline_break", direction: "ascending" }` |
| `Descending trendline from 2026-02-10 to 2026-03-25, reclaimed at 2026-04-15` | `{ event: "trendline_reclaim", direction: "descending" }` |
| `Trendline at 580.0 has 4 valid touches over 60 bars` | `{ touch_count: 4, span_bars: 60 }` |
| `Trendline retested at 2026-04-25 after first break` | `{ event: "trendline_retest" }` |

### 5.7 耦合管控規則

`trendline_core` 的 Cargo.toml 必須明確宣告對 `neely_core` 的依賴:

```toml
[dependencies]
neely_core = { path = "../../neely_core" }

[package.metadata.tw_stock]
depends_on = ["neely_core"]
coupling_kind = "documented_exception"
coupling_reason = "swing point detection reuse"
```

### 5.8 已知耦合清單

V2 spec 維護「已知耦合」清單,目前僅 `trendline_core → neely_core` 一項。**新增任何其他跨 Core 引用都需走架構審查流程**,不可隱式建立耦合。

### 5.9 替代方案(P3 後考慮)

把 swing point detection 抽出為 `shared/swing_detector/`,Neely Core 與 trendline_core 都消費 shared module。

**第一版傾向直接消費 Neely Core 輸出,第二版視情況重構**。

---

## 六、Fibonacci 不獨立的說明

### 6.1 Fibonacci 屬 Neely Core 子模組

Fibonacci 比率與容差屬 **Neely Core 內部子模組**,輸出在 `Scenario.expected_fib_zones` 欄位,**不獨立成 Core**。

### 6.2 為何不在型態 / 價位類

雖然 Fibonacci 看似屬「價位類」,但其產出**強依賴 Neely Core 的 wave 結構**(從 W4 終點到 W5 投影、ABC 修正的回撤位等),離開波浪結構單獨計算 Fibonacci 沒有意義。

### 6.3 fib_zones 快照的去向

若以 `core_name='fib_zones'` 寫入 `structural_snapshots`,該 row **必須附 `derived_from_core='neely_core'`** 與相應 `snapshot_date`,以保留追溯性。

投影邏輯放在 Aggregation Layer,屬「資料整理」非「計算」,不違反「並排不整合」原則。

### 6.4 與本子類 Core 的關係

| 本子類 Core | 與 Fibonacci 的關係 |
|---|---|
| `support_resistance_core` | 獨立計算,**不**消費 Fibonacci。但 Aggregation Layer 可並排呈現「靜態 SR 位」與「Fibonacci 投影位」 |
| `trendline_core` | 獨立計算,**不**消費 Fibonacci |
| `candlestick_pattern_core` | 不涉及 |

各 Core 各自輸出,使用者自己對照看「SR 位是否與 Fib 位重合」。
