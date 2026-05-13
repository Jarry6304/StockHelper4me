# Fundamental Cores 規格(基本面)

> **版本**:v2.0 抽出版 r3
> **日期**:2026-05-07
> **配套文件**:`cores_overview.md`(共通規範)
> **包含 Core**:3 個
> **優先級**:全部 P2

---

## r3 修訂摘要(2026-05-07)

- **§5.2 Silver 內部依賴描述去耦**:原「Silver 端 builder 跑在 7b」改為「表名 + 欄位 + 引用 layered_schema §6.4」的穩定句型。理由:`layered_schema_post_refactor.md` 後續重構將去除 7a/7b 階段編號,子類文件不應寫死實作階段代號,改以「上游表名 + 具體欄位 + 語意目的 + 引用集中管理章節」確保表名與欄位作為穩定契約

---

## r2 修訂摘要(2026-05-07)

- **資料表對應改指 Silver derived**:依總綱 §4.4「Cores 一律從 Silver 層讀取,不直接讀 Bronze」,所有 Core 章首對照表更新為 `*_derived` 表名(對應 `layered_schema_post_refactor.md` §4.3)
- **Output Point 補 `fact_date` 欄位**:三個 Core 的 Point struct 直接欄位化「事件對應日期」,對齊 §6.2 Fact schema,避免 produce_facts 邏輯散落
- **events 結構統一**:採 `{ date, kind, value, metadata }` 同構結構(對齊 chip_cores / environment_cores)
- **Fact 邊界與 stock_id 保留字引用化**:不重述總綱清單,僅引用 §6.1.1、§6.2.1
- **Silver PK 標註**:`financial_statement_derived` 為 4 維 PK 含 `type`,於章節中標明
- **跨 Core 整合禁止**:引用總綱 §11

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

| Core | 名稱 | 上游 Silver 表 | Silver PK | 資料頻率 |
|---|---|---|---|---|
| `revenue_core` | 月營收 | `monthly_revenue_derived` | `(market, stock_id, date)` | 月頻(每月 10 日前發布) |
| `valuation_core` | PER / PBR / 殖利率 | `valuation_daily_derived` | `(market, stock_id, date)` | 日頻 |
| `financial_statement_core` | 財報三表 | `financial_statement_derived` | `(market, stock_id, date, type)` | 季頻 |

> **PK 注意**:`financial_statement_derived` 為 4 維 PK,`type` 區分 `income / balance / cashflow`(對應損益表 / 資產負債表 / 現金流量表)。Core 載入器需處理三類同日多列。

---

## 二、共通規範(本子類)

### 2.1 trait

全部走 `IndicatorCore` trait(見總綱 §3),`Input` 為各自對應的基本面資料序列(`MonthlyRevenueSeries` / `ValuationDailySeries` / `FinancialStatementSeries`),由 `shared/fundamental_loader/` 提供載入器(見總綱 §3.4)。各 Core 的 `warmup_periods()` 單位依輸入頻率(月份數 / 季別數 / 日數)決定,見總綱 §3.4 與 §7.3.1。

### 2.2 計算策略

| Core | 頻率 | batch 處理方式 |
|---|---|---|
| `revenue_core` | 月頻 | 每日 batch 掃描,新月份資料發布時計算 |
| `valuation_core` | 日頻 | 每日 batch 計算當日 |
| `financial_statement_core` | 季頻 | 每日 batch 掃描,新季資料發布時計算 |

寫入 `indicator_values` 時,**`date` 欄位記錄資料對應的時間點**(月底 / 季底),非 batch 執行日。

### 2.3 事件型 Fact 的時間標記

月營收與財報屬「事件型」資料,Fact 的 `fact_date` 為**事件發生日**(月份結束日 / 季別結束日),不是 batch 處理日。為使此映射在型別層即可見,本子類三個 Core 的 Point struct 直接欄位化:

- `period: String` — 人類可讀標籤(`"2026-03"` / `"2026Q1"`)
- `fact_date: NaiveDate` — 月底日 / 季末日,對齊 §6.2 Fact schema
- `report_date: NaiveDate` — 實際發布日

### 2.4 Output 統一結構

依總綱 §2「Output schema 同構」原則,所有 Fundamental Core 的 Output 採以下兩層結構:

```rust
pub struct XxxOutput {
    pub series: Vec<XxxPoint>,      // 時間序列數值(含 fact_date / report_date)
    pub events: Vec<XxxEvent>,      // 事件型 Fact 來源
}

pub struct XxxEvent {
    pub date: NaiveDate,            // 事件對應日(= fact_date)
    pub kind: XxxEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}
```

### 2.5 Fact 邊界提醒

Fact 邊界與禁用詞彙清單見總綱 §6.1.1,本子類涉及的「動能」、「優異」、「投資價值」、「基本面強勁」、「成長動能」等主觀詞彙已收錄於該節,此處不重述。

stock_id 保留字規範見總綱 §6.2.1。本子類所有 Core 處理個股,`stock_id` 一律使用真實股票代號,不使用保留字。

### 2.6 跨 Fundamental Core 整合的禁止

「營收創高 + PER 偏低 = 投資機會」這類綜合判斷由 Aggregation Layer 並排呈現,**不**在 Core 層整合。本原則見總綱 §11(跨指標訊號處理原則),適用於跨子類組合。

---

## 三、`revenue_core`

### 3.1 定位

月營收事實萃取,涵蓋 YoY、MoM、累計、創新高紀錄。

### 3.2 上游 Silver

- 表:`monthly_revenue_derived`
- PK:`(market, stock_id, date)`
- 關鍵欄位:`revenue / revenue_mom / revenue_yoy / detail`
- 載入器:`shared/fundamental_loader/`,提供 `MonthlyRevenueSeries`

### 3.3 Params

```rust
pub struct RevenueParams {
    pub yoy_high_threshold: f64,               // 預設 30.0(%)
    pub yoy_low_threshold: f64,                // 預設 -10.0(%)
    pub mom_significant_threshold: f64,        // 預設 20.0(%)
    pub streak_min_months: usize,              // 連續成長月數,預設 3
    pub historical_high_lookback_months: usize,// 創高回看,預設 60
}
```

### 3.4 warmup_periods

```rust
fn warmup_periods(&self, params: &RevenueParams) -> usize {
    params.historical_high_lookback_months + 12
}
```

**單位說明**:本 Core 輸入為月頻,單位為「月份數」(總綱 §3.4)。`+ 12` 為計算 YoY 與累計年增率所需的最小緩衝(12 個月)。

### 3.5 Output

```rust
pub struct RevenueOutput {
    pub series: Vec<RevenuePoint>,
    pub events: Vec<RevenueEvent>,
}

pub struct RevenuePoint {
    pub period: String,                // "2026-03"
    pub fact_date: NaiveDate,          // 月底日,= 2026-03-31
    pub report_date: NaiveDate,        // 實際發布日,例:2026-04-08
    pub revenue: i64,                  // 月營收(千元)
    pub yoy_pct: f64,                  // 年增率
    pub mom_pct: f64,                  // 月增率
    pub cumulative: i64,               // 年累計
    pub cumulative_yoy_pct: f64,       // 累計年增率
}

pub struct RevenueEvent {
    pub date: NaiveDate,               // = fact_date(月底日)
    pub kind: RevenueEventKind,
    pub value: f64,                    // YoY% 或 MoM% 或 累計值
    pub metadata: serde_json::Value,
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

### 3.6 Fact 範例

| Fact statement | metadata |
|---|---|
| `2026-03 revenue 12.5 億, YoY +35.2%` | `{ period: "2026-03", revenue: 1250000, yoy: 35.2, report_date: "2026-04-08" }` |
| `2026-03 revenue YoY positive for 6 consecutive months` | `{ period: "2026-03", months: 6 }` |
| `2026-03 revenue historical high in 60 months` | `{ period: "2026-03", lookback_months: 60 }` |
| `2026-03 cumulative YoY +28.5%` | `{ period: "2026-03", cumulative_yoy: 28.5 }` |

### 3.7 月營收的時間對齊

月營收於每月 10 日前發布,但 Fact 的 `fact_date` 對應**月份結束日**(例:2026-03 營收的 `fact_date = 2026-03-31`)。`report_date` 欄位記錄實際發布日(例:2026-04-08),供 Aggregation Layer 在「資訊何時可用」的時間軸上呈現。

---

## 四、`valuation_core`

### 4.1 定位

估值指標(PER、PBR、殖利率)的事實萃取,涵蓋歷史百分位、區間位置。

### 4.2 上游 Silver

- 表:`valuation_daily_derived`
- PK:`(market, stock_id, date)`
- 關鍵欄位:`per / pbr / dividend_yield / market_value_weight`
- 載入器:`shared/fundamental_loader/`,提供 `ValuationDailySeries`

### 4.3 Params

```rust
pub struct ValuationParams {
    pub timeframe: Timeframe,                  // 通常 Daily
    pub history_lookback_years: usize,         // 歷史百分位回看,預設 5
    pub percentile_high: f64,                  // 高百分位閾值,預設 80.0
    pub percentile_low: f64,                   // 低百分位閾值,預設 20.0
    pub yield_high_threshold: f64,             // 高殖利率閾值,預設 5.0
}
```

### 4.4 warmup_periods

```rust
fn warmup_periods(&self, params: &ValuationParams) -> usize {
    params.history_lookback_years * 252
}
```

**單位說明**:252 個交易日/年,5 年 = 1260 個交易日,供歷史百分位計算使用。

### 4.5 Output

```rust
pub struct ValuationOutput {
    pub series: Vec<ValuationPoint>,
    pub events: Vec<ValuationEvent>,
}

pub struct ValuationPoint {
    pub date: NaiveDate,
    pub fact_date: NaiveDate,              // = date(日頻 Core,兩者相同)
    pub per: Option<f64>,                  // 本益比(可能為負或 N/A)
    pub pbr: Option<f64>,                  // 股價淨值比
    pub dividend_yield: Option<f64>,       // 殖利率%
    pub per_percentile_5y: Option<f64>,    // PER 在 5 年中的百分位
    pub pbr_percentile_5y: Option<f64>,
    pub yield_percentile_5y: Option<f64>,
}

pub struct ValuationEvent {
    pub date: NaiveDate,                   // = fact_date
    pub kind: ValuationEventKind,
    pub value: f64,                        // 指標值 或 百分位值
    pub metadata: serde_json::Value,
}

pub enum ValuationEventKind {
    PerExtremeHigh,           // PER 高百分位
    PerExtremeLow,
    PbrExtremeHigh,
    PbrExtremeLow,
    YieldExtremeHigh,         // 殖利率歷史高位
    YieldHighThreshold,       // 殖利率超過絕對閾值
    PerNegative,              // PER 轉負(虧損)
    PbrBelowBookValue,        // PBR < 1
}
```

> **fact_date 與 date 為何並列**:本 Core 為日頻,兩者數值相同,但保留 `fact_date` 欄位是為了與 revenue / financial_statement 同構,便於下游通用處理。

### 4.6 Fact 範例

| Fact statement | metadata |
|---|---|
| `PER 12.3 at 2026-04-25, 5-year percentile 18%` | `{ per: 12.3, percentile_5y: 18.0 }` |
| `PER at 5-year low percentile(8.5) on 2026-04-22` | `{ percentile_5y: 8.5 }` |
| `Dividend yield 6.8% on 2026-04-25, exceeded 5% threshold` | `{ yield: 6.8, threshold: 5.0 }` |
| `PBR turned below 1.0 on 2026-04-22(0.95)` | `{ pbr: 0.95 }` |
| `PER turned negative on 2026-04-25(company in loss)` | `{ }` |

### 4.7 PER N/A 處理

虧損公司無有效 PER。Output 與 Fact 處理:

- `ValuationPoint.per = None`,寫入 JSONB 時為 null
- 產出 `PerNegative` 事件 Fact(若由獲利轉虧損)
- Aggregation Layer 對 null PER 不做百分位計算

---

## 五、`financial_statement_core`

### 5.1 定位

財報三表(損益表、資產負債表、現金流量表)的關鍵指標事實萃取。

### 5.2 上游 Silver

- 表:`financial_statement_derived`
- **PK:`(market, stock_id, date, type)` — 4 維 PK**
- `type` 取值:`income` / `balance` / `cashflow`
- 關鍵欄位:`detail`(JSONB,各會計科目)
- 載入器:`shared/fundamental_loader/`,提供 `FinancialStatementSeries`(載入器負責將同一季別的三類 row 組裝為單一 `FinancialPoint`)
- **Silver 內部依賴**:`financial_statement_derived` builder 需消費 `monthly_revenue_derived.fact_date` 進行財報季度日期映射對齊。具體 builder 排程順序見 `layered_schema_post_refactor.md §6.4`(Silver 內部依賴)。Core 層直接讀 derived,不需自行處理 Silver 內部依賴。

### 5.3 Params

```rust
pub struct FinancialStatementParams {
    pub gross_margin_change_threshold: f64,    // 毛利率變化閾值,預設 2.0(%)
    pub roe_high_threshold: f64,               // ROE 高閾值,預設 15.0(%)
    pub debt_ratio_high_threshold: f64,        // 負債比高閾值,預設 60.0(%)
    pub fcf_negative_streak_quarters: usize,   // 自由現金流連續為負季數,預設 4
}
```

### 5.4 warmup_periods

```rust
fn warmup_periods(&self, params: &FinancialStatementParams) -> usize {
    params.fcf_negative_streak_quarters * 90 + 60
}
```

**偏離 §7.3.1 慣例理由**:本 Core 為季頻資料,但 batch 為日頻執行。`* 90 + 60` 是將「N 季 + 緩衝」轉換為日頻 batch 可解讀的「天數」單位(每季約 90 天)。載入器解讀時依季別取資料,不直接以天數查詢。

### 5.5 Output

```rust
pub struct FinancialStatementOutput {
    pub series: Vec<FinancialPoint>,
    pub events: Vec<FinancialEvent>,
}

pub struct FinancialPoint {
    pub period: String,                    // "2026Q1"
    pub fact_date: NaiveDate,              // 季末日,= 2026-03-31
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
    pub date: NaiveDate,                   // = fact_date(季末日)
    pub kind: FinancialEventKind,
    pub value: f64,                        // 主要指標值
    pub metadata: serde_json::Value,
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

### 5.6 Fact 範例

| Fact statement | metadata |
|---|---|
| `2026Q1 gross margin 42.3%, up 1.8% from last quarter` | `{ period: "2026Q1", gross_margin: 42.3, change: 1.8 }` |
| `2026Q1 ROE 18.5%, exceeded 15% threshold` | `{ period: "2026Q1", roe: 18.5, threshold: 15.0 }` |
| `2026Q1 free cash flow negative for 4 consecutive quarters` | `{ period: "2026Q1", quarters: 4 }` |
| `2026Q1 EPS 3.25 NTD, YoY +28%` | `{ period: "2026Q1", eps: 3.25, yoy: 28.0 }` |
| `2026Q1 debt ratio 62%, up from 58% last quarter` | `{ period: "2026Q1", current: 62.0, previous: 58.0 }` |

### 5.7 季報的時間對齊

季報發布時間(以台股為例):

- Q1 → 5 月 15 日前
- Q2 → 8 月 14 日前
- Q3 → 11 月 14 日前
- Q4 / 年報 → 隔年 3 月 31 日前

`fact_date` 對應**季別結束日**(例:`2026-03-31` 對應 `2026Q1`),`report_date` 欄位記錄實際發布日。

### 5.8 不收錄的指標

以下指標**不**收錄在第一版:

- 自定義「品質分數」、「成長分數」(屬主觀加權)
- ESG 相關指標(資料源不穩定)
- 同業比較指標(屬跨檔分析,非單檔 Core)

未來可考慮獨立 `peer_comparison_core`,但 v2.0 P0~P3 不規劃。

---

## 六、基本面與技術面的並排原則

### 6.1 不在 Core 層整合

「PER 偏低 + 技術面突破 = 多頭買點」這類綜合判斷涉及 Fundamental Core 與 Indicator Core 的整合,**不**在 Core 層處理。本原則見總綱 §11(跨指標訊號處理原則),適用於跨子類組合。

### 6.2 並排呈現

由 Aggregation Layer 將兩類 Core 的 Fact 並排呈現,使用者自己連線。

### 6.3 為何不立「綜合 Core」

- 違反零耦合原則(總綱 §2.1)
- 「基本面 + 技術面」的權重因投資派別不同(價值投資 vs 動能投資)而異
- 寫進 Core 等於替使用者下投資哲學的定義

### 6.4 使用者教學層的角色

如何結合基本面與技術面屬投資哲學議題,由使用者教學文件提供識讀框架(例:Buffett-style / O'Neil-style)指引,不在架構層處理。
