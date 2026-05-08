# Chip Cores 規格(籌碼面)

> **版本**:v2.0 抽出版 r2
> **日期**:2026-05-07
> **配套文件**:`cores_overview.md`(共通規範)
> **包含 Core**:5 個
> **優先級**:全部 P2

---

## r2 修訂摘要(2026-05-07)

- **資料表對應改指 Silver derived**:依總綱 §4.4「Cores 一律從 Silver 層讀取,不直接讀 Bronze」,所有 Core 章首對照表更新為 `*_derived` 表名(對應 `layered_schema_post_refactor.md` §4.2)
- **Output 結構統一為 `series + events` 兩層**:`institutional_core` 原本的 `series + streaks + large_transactions` 三層收斂為標準兩層,事件結構化資料走 `metadata: serde_json::Value`(對齊 §6.2 Fact schema)
- **events 結構統一**:所有 Event 採 `{ date, kind, value, metadata }` 同構結構
- **Fact 邊界與 stock_id 保留字引用化**:不重述總綱清單,僅引用 §6.1.1、§6.2.1
- **warmup_periods 偏離理由補齊**:`margin_core` / `foreign_holding_core` / `day_trading_core` 的硬編 20 補理由
- **發布時間段補齊**:籌碼資料於收盤後 17:00–18:00 發布,batch 排程依據明示
- **Silver PK 標註**:每個 Core 補上上游 derived 表的 PK 結構

---

## 目錄

1. [本文件範圍](#一本文件範圍)
2. [共通規範](#二共通規範本子類)
3. [`institutional_core`](#三institutional_core)
4. [`margin_core`](#四margin_core)
5. [`foreign_holding_core`](#五foreign_holding_core)
6. [`shareholder_core`](#六shareholder_core)
7. [`day_trading_core`](#七day_trading_core)
8. [跨 Chip Core 綜合事實的處理](#八跨-chip-core-綜合事實的處理)

---

## 一、本文件範圍

| Core | 名稱 | 上游 Silver 表 | Silver PK |
|---|---|---|---|
| `institutional_core` | 法人買賣(外資 / 投信 / 自營) | `institutional_daily_derived` | `(market, stock_id, date)` |
| `margin_core` | 融資融券 | `margin_daily_derived` | `(market, stock_id, date)` |
| `foreign_holding_core` | 外資持股比率 | `foreign_holding_derived` | `(market, stock_id, date)` |
| `shareholder_core` | 持股級距(籌碼集中度) | `holding_shares_per_derived` | `(market, stock_id, date)` |
| `day_trading_core` | 當沖統計 | `day_trading_derived` | `(market, stock_id, date)` |

---

## 二、共通規範(本子類)

### 2.1 trait

全部走 `IndicatorCore` trait(見總綱 §3),`Input` 為各自對應的籌碼資料序列(`InstitutionalDailySeries` / `MarginDailySeries` / `ForeignHoldingSeries` / `HoldingSharesPerSeries` / `DayTradingSeries`),由 `shared/chip_loader/` 提供載入器(見總綱 §3.4)。各 Core 的 `warmup_periods()` 單位依輸入頻率(交易日數 / 週數)決定,見總綱 §3.4 與 §7.3.1。

### 2.2 計算策略

| Core | 頻率 | batch 處理方式 |
|---|---|---|
| `institutional_core` | 日頻 | 增量(每日新增當日紀錄) |
| `margin_core` | 日頻 | 增量 |
| `foreign_holding_core` | 日頻 | 增量(偶有 1~2 日延遲補發,需處理 backfill) |
| `shareholder_core` | 週頻 | 增量(週末發布,但每日 batch 仍掃描) |
| `day_trading_core` | 日頻 | 增量 |

### 2.3 籌碼資料的發布時間

多數籌碼資料於收盤後 **17:00~18:00** 由交易所發布,batch 排程須在 17:30 後啟動。若使用者於收盤至發布之間發起即時請求,Aggregation Layer 應呈現「當日資料未到」並回退至前一交易日。**Fact 的 `fact_date` 為交易日,非 batch 執行日**。

`shareholder_core` 為週頻例外,資料於週末發布(集保中心),處理細節見 §6.6。

### 2.4 Output 統一結構

依總綱 §2「Output schema 同構」原則,所有 Chip Core 的 Output 採以下兩層結構:

```rust
pub struct XxxOutput {
    pub series: Vec<XxxPoint>,      // 時間序列數值
    pub events: Vec<XxxEvent>,      // 事件型 Fact 來源
}

pub struct XxxEvent {
    pub date: NaiveDate,            // 事件日期(區間事件取結束日)
    pub kind: XxxEventKind,         // 事件類型 enum
    pub value: f64,                 // 主要數值(供快速 sort/filter)
    pub metadata: serde_json::Value,// 結構化補充資料
}
```

`metadata` 用於攜帶事件特定的結構化資料(例:streak 的 `start_date / end_date / days`),序列化模式與 §6.2 Fact schema 一致,`produce_facts()` 可直接透傳。

### 2.5 Fact 邊界提醒

Fact 邊界與禁用詞彙清單見總綱 §6.1.1,本子類涉及的「主力」、「進場」、「洗盤」、「籌碼面轉強」等主觀詞彙已收錄於該節,此處不重述。

stock_id 保留字規範見總綱 §6.2.1。本子類所有 Core 處理個股,`stock_id` 一律使用真實股票代號,不使用保留字。

### 2.6 跨 Chip Core 事實的處理

「外資買 + 散戶賣」這類跨 Core 綜合判斷由 Aggregation Layer 並排呈現,**不**在 Core 層整合。原則同總綱 §11(跨指標訊號處理),詳見第八章。

---

## 三、`institutional_core`

### 3.1 定位

法人買賣超(外資 / 投信 / 自營商)資料的事實萃取。

### 3.2 上游 Silver

- 表:`institutional_daily_derived`
- PK:`(market, stock_id, date)`
- 關鍵欄位:`foreign_buy / foreign_sell / investment_trust_buy / investment_trust_sell / dealer_buy / dealer_sell / dealer_hedging_buy / dealer_hedging_sell / gov_bank_net`
- 載入器:`shared/chip_loader/`,提供 `InstitutionalDailySeries`

### 3.3 Params

```rust
pub struct InstitutionalParams {
    pub timeframe: Timeframe,                  // 日 / 週 / 月聚合
    pub streak_min_days: usize,                // 連續買賣超的最小天數,預設 3
    pub large_transaction_z: f64,              // 大額異動的 Z-score 閾值,預設 2.0
    pub lookback_for_z: usize,                 // 計算 Z-score 的回看窗口,預設 60
}
```

### 3.4 warmup_periods

```rust
fn warmup_periods(&self, params: &InstitutionalParams) -> usize {
    params.lookback_for_z + 10
}
```

### 3.5 Output

```rust
pub struct InstitutionalOutput {
    pub series: Vec<InstitutionalPoint>,
    pub events: Vec<InstitutionalEvent>,
}

pub struct InstitutionalPoint {
    pub date: NaiveDate,
    pub foreign_net: i64,            // 外資買賣超(股數,正為買超)
    pub trust_net: i64,              // 投信買賣超
    pub dealer_net: i64,             // 自營商買賣超
    pub total_net: i64,              // 三大法人合計
    pub foreign_cumulative_5d: i64,  // 外資 5 日累積
    pub foreign_cumulative_20d: i64, // 外資 20 日累積
}

pub struct InstitutionalEvent {
    pub date: NaiveDate,             // 區間事件取結束日
    pub kind: InstitutionalEventKind,
    pub value: f64,                  // streak 的累積金額 / large_tx 的金額
    pub metadata: serde_json::Value,
}

pub enum InstitutionalEventKind {
    NetBuyStreak,                    // 連續淨買超
    NetSellStreak,                   // 連續淨賣超
    LargeTransaction,                // 單日大額異動(z-score 超閾值)
    DivergenceWithinInstitution,     // 法人內部分歧
}
```

### 3.6 metadata 結構範例

| EventKind | metadata 欄位 |
|---|---|
| `NetBuyStreak` / `NetSellStreak` | `{ institution: "foreign"\|"trust"\|"dealer", start_date, end_date, days }` |
| `LargeTransaction` | `{ institution: "foreign"\|"trust"\|"dealer", z_score }` |
| `DivergenceWithinInstitution` | `{ foreign_direction: "buy"\|"sell", dealer_direction: "buy"\|"sell" }` |

### 3.7 Fact 範例

| Fact statement | metadata |
|---|---|
| `Foreign net buy 5 consecutive days from 2026-04-21 to 2026-04-25, total 12,500 lots` | `{ institution: "foreign", start_date: "2026-04-21", end_date: "2026-04-25", days: 5 }` |
| `Foreign single-day large transaction: -8,200 lots on 2026-04-25(z=-2.8)` | `{ institution: "foreign", z_score: -2.8 }` |
| `Trust net buy 3 consecutive days, total 1,800 lots` | `{ institution: "trust", days: 3 }` |
| `Dealer net sell on day of foreign net buy at 2026-04-22` | `{ foreign_direction: "buy", dealer_direction: "sell" }` |

---

## 四、`margin_core`

> **命名注意**:本 Core 為**個股級**融資融券事實。市場整體融資維持率為獨立 Core `market_margin_core`(屬 Environment Cores)。命名前綴規則見總綱 §13.2.1。

### 4.1 定位

融資融券餘額變化、融券回補、券資比異常等事實萃取(個股範圍)。

### 4.2 上游 Silver

- 表:`margin_daily_derived`(雙來源:`margin_purchase_short_sale_tw` + `securities_lending_tw`)
- PK:`(market, stock_id, date)`
- 關鍵欄位:`margin_purchase / margin_sell / margin_balance / short_sale / short_cover / short_balance / sbl_short_sales_*`
- 載入器:`shared/chip_loader/`,提供 `MarginDailySeries`

### 4.3 Params

```rust
pub struct MarginParams {
    pub timeframe: Timeframe,
    pub margin_change_pct_threshold: f64,      // 預設 5.0(%)
    pub short_change_pct_threshold: f64,       // 預設 10.0(%)
    pub short_to_margin_ratio_high: f64,       // 券資比高閾值,預設 30.0
    pub short_to_margin_ratio_low: f64,        // 券資比低閾值,預設 5.0
}
```

### 4.4 warmup_periods

```rust
fn warmup_periods(&self, params: &MarginParams) -> usize {
    20
}
```

**偏離 §7.3.1 慣例理由**:本 Core 為短期波動偵測,無平滑收斂問題,亦無結構性 lookback 窗口。固定 20 個交易日為經驗緩衝(約一個月交易日),供 `margin_change_pct` 等比較欄位有足夠歷史可參照。

### 4.5 Output

```rust
pub struct MarginOutput {
    pub series: Vec<MarginPoint>,
    pub events: Vec<MarginEvent>,
}

pub struct MarginPoint {
    pub date: NaiveDate,
    pub margin_balance: i64,         // 融資餘額(張)
    pub short_balance: i64,          // 融券餘額(張)
    pub margin_change_pct: f64,      // 較前日變化%
    pub short_change_pct: f64,
    pub short_to_margin_ratio: f64,  // 券資比%
    pub margin_maintenance: f64,     // 維持率%(若有)
}

pub struct MarginEvent {
    pub date: NaiveDate,
    pub kind: MarginEventKind,
    pub value: f64,                  // 變化% 或 比率值
    pub metadata: serde_json::Value,
}

pub enum MarginEventKind {
    MarginSurge,            // 融資餘額暴增
    MarginCrash,            // 融資餘額暴減
    ShortSqueeze,           // 融券回補(餘額急減)
    ShortBuildUp,           // 融券暴增
    ShortRatioExtremeHigh,  // 券資比異常高
    ShortRatioExtremeLow,
    MaintenanceLow,         // 維持率偏低
}
```

### 4.6 Fact 範例

| Fact statement | metadata |
|---|---|
| `Margin balance up 12% to 25,000 lots on 2026-04-22` | `{ change_pct: 12.0, balance: 25000 }` |
| `Short balance down 35% to 3,200 lots on 2026-04-25(short squeeze)` | `{ change_pct: -35.0, balance: 3200 }` |
| `Short-to-margin ratio reached 32% on 2026-04-20(historical high)` | `{ ratio: 32.0, lookback: "60d" }` |
| `Margin maintenance dropped to 142% on 2026-04-28` | `{ maintenance: 142.0 }` |

---

## 五、`foreign_holding_core`

### 5.1 定位

外資持股比率變化、達到上限警訊。

### 5.2 上游 Silver

- 表:`foreign_holding_derived`
- PK:`(market, stock_id, date)`
- 關鍵欄位:`foreign_holding_shares / foreign_holding_ratio`
- 載入器:`shared/chip_loader/`,提供 `ForeignHoldingSeries`

### 5.3 Params

```rust
pub struct ForeignHoldingParams {
    pub timeframe: Timeframe,
    pub change_threshold_pct: f64,             // 預設 0.5(%)單日變化
    pub limit_alert_remaining: f64,            // 預設 5.0(剩餘可投資比率%)
}
```

### 5.4 warmup_periods

```rust
fn warmup_periods(&self, params: &ForeignHoldingParams) -> usize {
    20
}
```

**偏離 §7.3.1 慣例理由**:本 Core 為比率異動偵測,無平滑收斂與結構性 lookback。固定 20 個交易日緩衝供「N 個月新高/新低」事件參照,計算成本低。

### 5.5 Output

```rust
pub struct ForeignHoldingOutput {
    pub series: Vec<ForeignHoldingPoint>,
    pub events: Vec<ForeignHoldingEvent>,
}

pub struct ForeignHoldingPoint {
    pub date: NaiveDate,
    pub foreign_holding_pct: f64,        // 外資持股比率%
    pub foreign_limit_pct: f64,          // 外資投資上限%
    pub remaining_pct: f64,              // 剩餘可投資比率%
    pub change_pct: f64,                 // 較前日變化%
}

pub struct ForeignHoldingEvent {
    pub date: NaiveDate,
    pub kind: ForeignHoldingEventKind,
    pub value: f64,                      // 持股比率 或 變化%
    pub metadata: serde_json::Value,
}

pub enum ForeignHoldingEventKind {
    HoldingMilestoneHigh,       // 創新高
    HoldingMilestoneLow,        // 創新低(回看 N 期)
    LimitNearAlert,             // 接近上限警訊
    SignificantSingleDayChange, // 單日異動
}
```

### 5.6 Fact 範例

| Fact statement | metadata |
|---|---|
| `Foreign holding reached 78.5% on 2026-04-25, near 80% limit` | `{ holding: 78.5, limit: 80.0, remaining: 1.5 }` |
| `Foreign holding 6-month high at 65.2% on 2026-04-22` | `{ lookback: "6m", value: 65.2 }` |
| `Foreign holding dropped 1.2% in single day on 2026-04-28(largest in 90 days)` | `{ change: -1.2, lookback: "90d" }` |

---

## 六、`shareholder_core`

### 6.1 定位

持股級距分布(散戶 / 中實 / 大戶)、籌碼集中度。資料來源為集保中心週頻發布。

### 6.2 上游 Silver

- 表:`holding_shares_per_derived`
- PK:`(market, stock_id, date)`
- 關鍵欄位:`detail`(JSONB,含 level taxonomy)
- 載入器:`shared/chip_loader/`,提供 `HoldingSharesPerSeries`

### 6.3 Params

```rust
pub struct ShareholderParams {
    pub timeframe: Timeframe,                          // 預設 Weekly
    pub small_holder_threshold: usize,                 // 小戶級距上限(張),預設 5
    pub large_holder_threshold: usize,                 // 大戶級距下限(張),預設 1000
    pub concentration_change_threshold: f64,           // 集中度變化閾值,預設 1.0(%)
    pub small_holder_count_change_threshold: usize,    // 散戶人數變化閾值,預設 500
}
```

### 6.4 warmup_periods

```rust
fn warmup_periods(&self, params: &ShareholderParams) -> usize {
    8
}
```

**偏離 §7.3.1 慣例理由**:本 Core 輸入為週頻資料,單位為「週數」。8 週(約 2 個月)為連續事件偵測(連續 N 週減少 / 累積 N 週上升)所需的最小窗口。

### 6.5 Output

```rust
pub struct ShareholderOutput {
    pub series: Vec<ShareholderPoint>,
    pub events: Vec<ShareholderEvent>,
}

pub struct ShareholderPoint {
    pub date: NaiveDate,                       // 週末日
    pub small_holders_count: usize,            // 小戶人數(<=5 張)
    pub small_holders_pct: f64,                // 小戶持股%
    pub mid_holders_count: usize,
    pub mid_holders_pct: f64,
    pub large_holders_count: usize,            // 大戶人數(>=1000 張)
    pub large_holders_pct: f64,                // 大戶持股%
    pub total_holders: usize,
    pub concentration_index: f64,              // 集中度指標(自定義或 Gini)
}

pub struct ShareholderEvent {
    pub date: NaiveDate,                       // 週末日
    pub kind: ShareholderEventKind,
    pub value: f64,                            // 變化幅度
    pub metadata: serde_json::Value,
}

pub enum ShareholderEventKind {
    SmallHoldersDecreasing,       // 散戶人數連續減少
    SmallHoldersIncreasing,
    LargeHoldersAccumulating,     // 大戶持股連續增加
    LargeHoldersReducing,
    ConcentrationRising,
    ConcentrationDecreasing,
}
```

### 6.6 Fact 範例

| Fact statement | metadata |
|---|---|
| `Small holders count down 1,250 to 38,500 on 2026-04-25(week)` | `{ change: -1250, count: 38500, frequency: "weekly" }` |
| `Large holders holding pct up 1.8% to 42.3% on 2026-04-25(week)` | `{ change: 1.8, pct: 42.3, frequency: "weekly" }` |
| `Concentration index up 2.1% over 4 consecutive weeks` | `{ change: 2.1, weeks: 4 }` |

### 6.7 週頻資料的時間對齊

本 Core 為週頻資料,但 batch 每日執行。處理方式:

- 每日 batch 掃描 `holding_shares_per_derived` 表,若有新週資料則計算
- 寫入 `indicator_values` 時,`date` 欄位記錄週末日(非執行日)
- Aggregation Layer 對使用者呈現時清楚標註資料頻率為 `weekly`

---

## 七、`day_trading_core`

### 7.1 定位

當沖統計、當沖比率、當沖力道。

### 7.2 上游 Silver

- 表:`day_trading_derived`
- PK:`(market, stock_id, date)`
- 關鍵欄位:`day_trading_buy / day_trading_sell / day_trading_ratio`
- 載入器:`shared/chip_loader/`,提供 `DayTradingSeries`
- **依賴關係**:Silver builder 需先取得 `price_daily_fwd.volume` 計算 `day_trading_ratio`,故 day_trading_derived 須等 S1 完成(見 layered_schema §5)。Core 層直接讀 derived,不需自行 join。

### 7.3 Params

```rust
pub struct DayTradingParams {
    pub timeframe: Timeframe,
    pub ratio_high_threshold: f64,             // 當沖比率高閾值,預設 30.0(%)
    pub ratio_low_threshold: f64,              // 當沖比率低閾值,預設 5.0(%)
    pub momentum_lookback: usize,              // 當沖力道回看,預設 5
}
```

### 7.4 warmup_periods

```rust
fn warmup_periods(&self, params: &DayTradingParams) -> usize {
    20
}
```

**偏離 §7.3.1 慣例理由**:本 Core 為比率異常偵測,無平滑收斂。固定 20 個交易日為連續事件(連續 N 日高當沖比)偵測所需的最小窗口。

### 7.5 Output

```rust
pub struct DayTradingOutput {
    pub series: Vec<DayTradingPoint>,
    pub events: Vec<DayTradingEvent>,
}

pub struct DayTradingPoint {
    pub date: NaiveDate,
    pub day_trade_volume: i64,           // 當沖股數
    pub total_volume: i64,
    pub day_trade_ratio: f64,            // 當沖比率%
    pub day_trade_buy: i64,              // 當沖買進(如資料源提供)
    pub day_trade_sell: i64,
    pub momentum: f64,                   // 當沖力道(可自定義)
}

pub struct DayTradingEvent {
    pub date: NaiveDate,
    pub kind: DayTradingEventKind,
    pub value: f64,                      // 比率值 或 連續天數
    pub metadata: serde_json::Value,
}

pub enum DayTradingEventKind {
    RatioExtremeHigh,
    RatioExtremeLow,
    RatioStreakHigh,    // 連續 N 日高當沖比
    RatioStreakLow,
}
```

### 7.6 Fact 範例

| Fact statement | metadata |
|---|---|
| `Day trade ratio reached 38% on 2026-04-22(extreme high)` | `{ ratio: 38.0 }` |
| `Day trade ratio above 30% for 5 consecutive days` | `{ days: 5, threshold: 30.0 }` |
| `Day trade ratio dropped to 4.2% on 2026-04-28(extreme low)` | `{ ratio: 4.2 }` |

---

## 八、跨 Chip Core 綜合事實的處理

### 8.1 不在 Core 層整合

「外資買 + 散戶賣 = 籌碼集中」這類綜合判斷涉及兩個以上 Core 輸出,屬「跨 Core 訊號」,**不**在 Core 層整合。本原則見總綱 §11(跨指標訊號處理原則),適用於跨子類組合。

### 8.2 處理方式

- ✅ 各 Chip Core 各自輸出該 Core 對應 Silver derived 表的事實
- ✅ Aggregation Layer 並排呈現
- ✅ 使用者教學文件提供「如何看出籌碼集中」識讀指引

### 8.3 範例:籌碼集中

使用者要看「籌碼集中」訊號,需同時觀察:

- `institutional_core`:外資是否連續買超
- `shareholder_core`:大戶持股是否上升、散戶人數是否減少
- `foreign_holding_core`:外資持股比率是否上升

三個 Core 各自輸出 Fact,使用者並排判讀。

### 8.4 為何不立 `chip_concentration_core`

- 違反零耦合原則(總綱 §2.1)
- 「籌碼集中」的定義因人而異(有人重外資、有人重大戶持股),寫進 Core 等於替使用者下定義
- 一個檔案說「集中」未必另一個視角也算集中,屬經驗判讀
