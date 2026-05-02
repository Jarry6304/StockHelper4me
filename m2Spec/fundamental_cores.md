# Fundamental Cores 規格(基本面)

> **版本**:v2.0 抽出版 r1
> **日期**:2026-04-30
> **配套文件**:`cores_overview.md`(共通規範)
> **包含 Core**:3 個
> **優先級**:全部 P2

---

## 目錄

1. [本文件範圍](#一本文件範圍)
2. [共通規範](#二共通規範本子類)
3. [`revenue_core`](#三revenue_core)
4. [`valuation_core`](#四valuation_core)
5. [`financial_statement_core`](#五financial_statement_core)
6. [基本面與技術面的並排原則](#六基本面與技術面的並排原則)

---

## 一、本文件範圍

| Core | 名稱 | 對應資料表 | 資料頻率 |
|---|---|---|---|
| `revenue_core` | 月營收 | `monthly_revenue` | 月頻(每月 10 日前發布) |
| `valuation_core` | PER / PBR / 殖利率 | `valuation_daily` | 日頻 |
| `financial_statement_core` | 財報三表 | `financial_statement` | 季頻 |

---

## 二、共通規範(本子類)

### 2.1 trait

全部走 `IndicatorCore` trait,但輸入為對應資料表(非 OHLCV)。

### 2.2 資料頻率異質性

本子類三個 Core 的資料頻率差異大:

| Core | 頻率 | batch 處理方式 |
|---|---|---|
| `revenue_core` | 月頻 | 每日 batch 掃描,新月份資料發布時計算 |
| `valuation_core` | 日頻 | 每日 batch 計算當日 |
| `financial_statement_core` | 季頻 | 每日 batch 掃描,新季資料發布時計算 |

寫入 `indicator_values` 時,**`date` 欄位記錄資料對應的時間點**(月底 / 季底),非 batch 執行日。

### 2.3 事件型 Fact 的時間標記

月營收與財報屬「事件型」資料,Fact 的 `fact_date` 應為**事件發生日**(月份 / 季別),不是 batch 處理日。Aggregation Layer 使用時可依需求轉換時間軸。

### 2.4 Fact 邊界提醒

- ✅ `2026 年 3 月營收 12.5 億,YoY +35%`
- ✅ `Q1 毛利率 42.3%,較上季 +1.8%`
- ❌ `營收動能轉強` / `公司基本面優異` / `具投資價值`

「動能」、「優異」、「投資價值」屬主觀判讀詞彙,**禁止**進入 Fact statement。

### 2.5 跨 Fundamental Core 整合的禁止

「營收創高 + PER 偏低 = 投資機會」這類綜合判斷由 Aggregation Layer 並排呈現,**不**在 Core 層整合。

---

## 三、`revenue_core`

### 3.1 定位

月營收事實萃取,涵蓋 YoY、MoM、累計、創新高紀錄。

### 3.2 Params

```rust
pub struct RevenueParams {
    pub yoy_high_threshold: f64,               // 預設 30.0(%)
    pub yoy_low_threshold: f64,                // 預設 -10.0(%)
    pub mom_significant_threshold: f64,        // 預設 20.0(%)
    pub streak_min_months: usize,              // 連續成長月數,預設 3
    pub historical_high_lookback_months: usize,// 創高回看,預設 60
}
```

### 3.3 warmup_periods

```rust
fn warmup_periods(&self, params: &RevenueParams) -> usize {
    // revenue_core 不吃 OHLCV,以「月份數」為單位
    // 但 IndicatorCore trait 統一以 K 棒數宣告,
    // Pipeline 透過 adapter 轉換
    params.historical_high_lookback_months + 12
}
```

### 3.4 Output

```rust
pub struct RevenueOutput {
    pub series: Vec<RevenuePoint>,
    pub events: Vec<RevenueEvent>,
}

pub struct RevenuePoint {
    pub year_month: String,            // "2026-03"
    pub report_date: NaiveDate,        // 實際發布日
    pub revenue: i64,                  // 月營收(千元)
    pub yoy_pct: f64,                  // 年增率
    pub mom_pct: f64,                  // 月增率
    pub cumulative: i64,               // 年累計
    pub cumulative_yoy_pct: f64,       // 累計年增率
}

pub struct RevenueEvent {
    pub year_month: String,
    pub report_date: NaiveDate,
    pub kind: RevenueEventKind,
    pub value: f64,
}

pub enum RevenueEventKind {
    YoyHigh,                  // YoY > threshold_high
    YoyLow,                   // YoY < threshold_low
    YoyStreakUp,              // 連續 YoY 為正
    YoyStreakDown,
    MomSignificantUp,
    MomSignificantDown,
    HistoricalHigh,           // 創 N 個月新高
    HistoricalLow,
}
```

### 3.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `2026-03 revenue 12.5 億, YoY +35.2%` | `{ year_month: "2026-03", revenue: 1250000, yoy: 35.2 }` |
| `2026-03 revenue YoY positive for 6 consecutive months` | `{ event: "yoy_streak_up", months: 6 }` |
| `2026-03 revenue historical high in 60 months` | `{ event: "historical_high", lookback_months: 60 }` |
| `2026-03 cumulative YoY +28.5%` | `{ cumulative_yoy: 28.5 }` |

### 3.6 月營收的時間對齊

月營收於每月 10 日前發布,但 Fact 的 `fact_date` 對應**月份本身**(例:2026-03 營收的 fact_date 應為 2026-03-31)。`report_date` 欄位記錄實際發布日,供 Aggregation Layer 在「資訊何時可用」的時間軸上呈現。

---

## 四、`valuation_core`

### 4.1 定位

估值指標(PER、PBR、殖利率)的事實萃取,涵蓋歷史百分位、區間位置。

### 4.2 Params

```rust
pub struct ValuationParams {
    pub timeframe: Timeframe,                  // 通常 Daily
    pub history_lookback_years: usize,         // 歷史百分位回看,預設 5
    pub percentile_high: f64,                  // 高百分位閾值,預設 80.0
    pub percentile_low: f64,                   // 低百分位閾值,預設 20.0
    pub yield_high_threshold: f64,             // 高殖利率閾值,預設 5.0
}
```

### 4.3 warmup_periods

```rust
fn warmup_periods(&self, params: &ValuationParams) -> usize {
    params.history_lookback_years * 252  // 252 個交易日/年
}
```

### 4.4 Output

```rust
pub struct ValuationOutput {
    pub series: Vec<ValuationPoint>,
    pub events: Vec<ValuationEvent>,
}

pub struct ValuationPoint {
    pub date: NaiveDate,
    pub per: Option<f64>,                  // 本益比(可能為負或 N/A)
    pub pbr: Option<f64>,                  // 股價淨值比
    pub dividend_yield: Option<f64>,       // 殖利率%
    pub per_percentile_5y: Option<f64>,    // PER 在 5 年中的百分位
    pub pbr_percentile_5y: Option<f64>,
    pub yield_percentile_5y: Option<f64>,
}

pub struct ValuationEvent {
    pub date: NaiveDate,
    pub kind: ValuationEventKind,
    pub value: f64,
}

pub enum ValuationEventKind {
    PerExtremeHigh,           // PER 高百分位
    PerExtremeLow,
    PbrExtremeHigh,
    PbrExtremeLow,
    YieldExtremeHigh,         // 殖利率歷史高位
    YieldHighThreshold,       // 殖利率超過絕對閾值
    PerNegative,              // PER 轉負(虧損)
}
```

### 4.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `PER 12.3 at 2026-04-25, 5-year percentile 18%` | `{ per: 12.3, percentile_5y: 18.0 }` |
| `PER at 5-year low percentile(8.5) on 2026-04-22` | `{ event: "per_extreme_low", value: 8.5 }` |
| `Dividend yield 6.8% on 2026-04-25, exceeded 5% threshold` | `{ event: "yield_high_threshold", value: 6.8 }` |
| `PBR turned below 1.0 on 2026-04-22(0.95)` | `{ event: "pbr_below_book_value", value: 0.95 }` |
| `PER turned negative on 2026-04-25(company in loss)` | `{ event: "per_negative" }` |

### 4.6 PER N/A 處理

虧損公司無有效 PER。Output 與 Fact 處理:

- `PerPoint.per = None`,寫入 JSONB 時為 null
- 產出 `PerNegative` 事件 Fact(若由獲利轉虧損)
- Aggregation Layer 對 null PER 不做百分位計算

---

## 五、`financial_statement_core`

### 5.1 定位

財報三表(損益表、資產負債表、現金流量表)的關鍵指標事實萃取。

### 5.2 Params

```rust
pub struct FinancialStatementParams {
    pub gross_margin_change_threshold: f64,    // 毛利率變化閾值,預設 2.0(%)
    pub roe_high_threshold: f64,               // ROE 高閾值,預設 15.0(%)
    pub debt_ratio_high_threshold: f64,        // 負債比高閾值,預設 60.0(%)
    pub fcf_negative_streak_quarters: usize,   // 自由現金流連續為負季數,預設 4
}
```

### 5.3 warmup_periods

```rust
fn warmup_periods(&self, params: &FinancialStatementParams) -> usize {
    params.fcf_negative_streak_quarters * 90 + 60  // 約 N 季 + 緩衝
}
```

### 5.4 Output

```rust
pub struct FinancialStatementOutput {
    pub series: Vec<FinancialPoint>,
    pub events: Vec<FinancialEvent>,
}

pub struct FinancialPoint {
    pub year_quarter: String,              // "2026Q1"
    pub report_date: NaiveDate,            // 實際發布日

    // 損益表(關鍵欄位)
    pub revenue: i64,
    pub gross_profit: i64,
    pub gross_margin_pct: f64,
    pub operating_profit: i64,
    pub operating_margin_pct: f64,
    pub net_income: i64,
    pub net_margin_pct: f64,
    pub eps: f64,

    // 資產負債表
    pub total_assets: i64,
    pub total_liabilities: i64,
    pub total_equity: i64,
    pub debt_ratio_pct: f64,

    // 現金流量表
    pub operating_cash_flow: i64,
    pub investing_cash_flow: i64,
    pub financing_cash_flow: i64,
    pub free_cash_flow: i64,

    // 比率指標
    pub roe_pct: f64,
    pub roa_pct: f64,
}

pub struct FinancialEvent {
    pub year_quarter: String,
    pub report_date: NaiveDate,
    pub kind: FinancialEventKind,
    pub value: f64,
}

pub enum FinancialEventKind {
    GrossMarginRising,        // 毛利率連續上升
    GrossMarginFalling,
    RoeHigh,                  // ROE 高於閾值
    DebtRatioRising,
    OperatingCashFlowNegative,// 營業現金流為負
    FreeCashFlowNegativeStreak,
    EpsTurnNegative,          // EPS 由正轉負
    EpsTurnPositive,
}
```

### 5.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `2026Q1 gross margin 42.3%, up 1.8% from last quarter` | `{ year_quarter: "2026Q1", gross_margin: 42.3, change: 1.8 }` |
| `2026Q1 ROE 18.5%, exceeded 15% threshold` | `{ event: "roe_high", value: 18.5 }` |
| `2026Q1 free cash flow negative for 4 consecutive quarters` | `{ event: "fcf_negative_streak", quarters: 4 }` |
| `2026Q1 EPS 3.25 NTD, YoY +28%` | `{ eps: 3.25, yoy: 28.0 }` |
| `2026Q1 debt ratio 62%, up from 58% last quarter` | `{ event: "debt_ratio_rising", value: 62.0 }` |

### 5.6 季報的時間對齊

季報發布時間(以台股為例):

- Q1 → 5 月 15 日前
- Q2 → 8 月 14 日前
- Q3 → 11 月 14 日前
- Q4 / 年報 → 隔年 3 月 31 日前

Fact 的 `fact_date` 對應**季別結束日**(如 2026-03-31 對應 2026Q1),`report_date` 欄位記錄實際發布日。

### 5.7 不收錄的指標

以下指標**不**收錄在第一版:

- 自定義「品質分數」、「成長分數」(屬主觀加權)
- ESG 相關指標(資料源不穩定)
- 同業比較指標(屬跨檔分析,非單檔 Core)

未來可考慮獨立 `peer_comparison_core`,但 v2.0 P0~P3 不規劃。

---

## 六、基本面與技術面的並排原則

### 6.1 不在 Core 層整合

「PER 偏低 + 技術面突破 = 多頭買點」這類綜合判斷涉及 Fundamental Core 與 Indicator Core 的整合,**不**在 Core 層處理。

### 6.2 並排呈現

由 Aggregation Layer 將兩類 Core 的 Fact 並排呈現,使用者自己連線。

### 6.3 為何不立「綜合 Core」

- 違反零耦合原則
- 「基本面 + 技術面」的權重因投資派別不同(價值投資 vs 動能投資)而異
- 寫進 Core 等於替使用者下投資哲學的定義

### 6.4 使用者教學層的角色

如何結合基本面與技術面屬投資哲學議題,由使用者教學文件提供識讀框架(例:Buffett-style / O'Neil-style)指引,不在架構層處理。
