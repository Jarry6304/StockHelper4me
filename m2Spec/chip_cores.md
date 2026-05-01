# Chip Cores 規格(籌碼面)

> **版本**:v2.0 抽出版 r1
> **日期**:2026-04-30
> **配套文件**:`cores_overview.md`(共通規範)
> **包含 Core**:5 個
> **優先級**:全部 P2

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

| Core | 名稱 | 對應資料表 |
|---|---|---|
| `institutional_core` | 法人買賣(外資 / 投信 / 自營) | `institutional_daily` |
| `margin_core` | 融資融券 | `margin_daily` |
| `foreign_holding_core` | 外資持股比率 | `foreign_holding` |
| `shareholder_core` | 持股級距(籌碼集中度) | `holding_shares_per` |
| `day_trading_core` | 當沖統計 | `day_trading` |

---

## 二、共通規範(本子類)

### 2.1 trait

全部走 `IndicatorCore` trait(見 `cores_overview.md` §3),但輸入不是 `OHLCVSeries`,而是各自對應的籌碼資料表。Pipeline 會以 adapter 將籌碼表包裝成 `IndicatorCore` 可消費的形式。

### 2.2 計算策略

| Core | 策略 |
|---|---|
| `institutional_core` | 增量(每日新增當日紀錄) |
| `margin_core` | 增量 |
| `foreign_holding_core` | 增量 |
| `shareholder_core` | 增量(週頻發布,但每日 batch 仍掃描) |
| `day_trading_core` | 增量 |

### 2.3 籌碼資料的特性

- **發布時間**:多數籌碼資料於收盤後 17:00~18:00 發布,batch 排程需配合
- **時間粒度**:多數為日頻;`shareholder_core` 為週頻(週末發布)
- **可能延遲**:外資持股偶有 1~2 日延遲補發,Core 需處理 backfill

### 2.4 Fact 邊界提醒

- ✅ `外資連續買超 5 天` / `融資餘額單日減少 8%` / `散戶人數較上週減 1200 人`
- ❌ `籌碼面轉強` / `主力進場` / `主力洗盤`

「主力」、「進場」、「洗盤」屬主觀判讀詞彙,**禁止**進入 Fact statement。

### 2.5 跨 Chip Core 事實的處理

「外資買 + 散戶賣」這類綜合判斷由 Aggregation Layer 並排呈現,**不**在 Core 層整合。詳見第八章。

---

## 三、`institutional_core`

### 3.1 定位

法人買賣超(外資 / 投信 / 自營商)資料的事實萃取。

### 3.2 Params

```rust
pub struct InstitutionalParams {
    pub timeframe: Timeframe,                  // 日 / 週 / 月聚合
    pub streak_min_days: usize,                // 連續買賣超的最小天數,預設 3
    pub large_transaction_z: f64,              // 大額異動的 Z-score 閾值,預設 2.0
    pub lookback_for_z: usize,                 // 計算 Z-score 的回看窗口,預設 60
}
```

### 3.3 warmup_periods

```rust
fn warmup_periods(&self, params: &InstitutionalParams) -> usize {
    params.lookback_for_z + 10
}
```

### 3.4 Output

```rust
pub struct InstitutionalOutput {
    pub series: Vec<InstitutionalPoint>,
    pub streaks: Vec<StreakEvent>,
    pub large_transactions: Vec<LargeTransactionEvent>,
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

pub struct StreakEvent {
    pub kind: InstitutionKind,       // Foreign / Trust / Dealer / Combined
    pub direction: NetDirection,     // Buy / Sell
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub days: usize,
    pub total_amount: i64,
}

pub struct LargeTransactionEvent {
    pub date: NaiveDate,
    pub kind: InstitutionKind,
    pub amount: i64,
    pub z_score: f64,
}
```

### 3.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `Foreign net buy 5 consecutive days from 2026-04-21 to 2026-04-25, total 12,500 lots` | `{ kind: "foreign", direction: "buy", days: 5, total: 12500 }` |
| `Foreign single-day large transaction: -8,200 lots on 2026-04-25(z=-2.8)` | `{ kind: "foreign", amount: -8200, z: -2.8 }` |
| `Trust net buy 3 consecutive days, total 1,800 lots` | `{ kind: "trust", days: 3, total: 1800 }` |
| `Dealer net sell on day of foreign net buy at 2026-04-22` | `{ event: "divergence_within_institution", date: "2026-04-22" }` |

---

## 四、`margin_core`

### 4.1 定位

融資融券餘額變化、融券回補、券資比異常等事實萃取。

### 4.2 Params

```rust
pub struct MarginParams {
    pub timeframe: Timeframe,
    pub margin_change_pct_threshold: f64,      // 預設 5.0(%)
    pub short_change_pct_threshold: f64,       // 預設 10.0(%)
    pub short_to_margin_ratio_high: f64,       // 券資比高閾值,預設 30.0
    pub short_to_margin_ratio_low: f64,        // 券資比低閾值,預設 5.0
}
```

### 4.3 warmup_periods

```rust
fn warmup_periods(&self, params: &MarginParams) -> usize {
    20  // 短期波動偵測,不需大量暖機
}
```

### 4.4 Output

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
    pub value: f64,
}

pub enum MarginEventKind {
    MarginSurge,            // 融資餘額暴增
    MarginCrash,            // 融資餘額暴減
    ShortSqueeze,           // 融券回補(餘額急減)
    ShortBuildUp,           // 融券暴增
    ShortRatioExtremeHigh,  // 券資比異常高
    ShortRatioExtremeLow,
}
```

### 4.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `Margin balance up 12% to 25,000 lots on 2026-04-22` | `{ event: "margin_surge", change_pct: 12.0, balance: 25000 }` |
| `Short balance down 35% to 3,200 lots on 2026-04-25(short squeeze)` | `{ event: "short_squeeze", change_pct: -35.0 }` |
| `Short-to-margin ratio reached 32% on 2026-04-20(historical high)` | `{ event: "short_ratio_extreme_high", ratio: 32.0 }` |
| `Margin maintenance dropped to 142% on 2026-04-28` | `{ event: "maintenance_low", maintenance: 142.0 }` |

---

## 五、`foreign_holding_core`

### 5.1 定位

外資持股比率變化、達到上限警訊。

### 5.2 Params

```rust
pub struct ForeignHoldingParams {
    pub timeframe: Timeframe,
    pub change_threshold_pct: f64,             // 預設 0.5(%)單日變化
    pub limit_alert_remaining: f64,            // 預設 5.0(剩餘可投資比率%)
}
```

### 5.3 warmup_periods

```rust
fn warmup_periods(&self, params: &ForeignHoldingParams) -> usize {
    20
}
```

### 5.4 Output

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
    pub value: f64,
}

pub enum ForeignHoldingEventKind {
    HoldingMilestoneHigh,       // 創新高
    HoldingMilestoneLow,        // 創新低(回看 N 期)
    LimitNearAlert,             // 接近上限警訊
    SignificantSingleDayChange, // 單日異動
}
```

### 5.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `Foreign holding reached 78.5% on 2026-04-25, near 80% limit` | `{ event: "limit_near_alert", holding: 78.5, limit: 80.0 }` |
| `Foreign holding 6-month high at 65.2% on 2026-04-22` | `{ event: "holding_milestone_high", lookback: "6m", value: 65.2 }` |
| `Foreign holding dropped 1.2% in single day on 2026-04-28(largest in 90 days)` | `{ event: "significant_single_day_change", change: -1.2 }` |

---

## 六、`shareholder_core`

### 6.1 定位

持股級距分布(散戶 / 中實 / 大戶)、籌碼集中度。資料來源為集保中心週頻發布。

### 6.2 Params

```rust
pub struct ShareholderParams {
    pub timeframe: Timeframe,                          // 預設 Weekly
    pub small_holder_threshold: usize,                 // 小戶級距上限(張),預設 5
    pub large_holder_threshold: usize,                 // 大戶級距下限(張),預設 1000
    pub concentration_change_threshold: f64,           // 集中度變化閾值,預設 1.0(%)
    pub small_holder_count_change_threshold: usize,    // 散戶人數變化閾值,預設 500
}
```

### 6.3 warmup_periods

```rust
fn warmup_periods(&self, params: &ShareholderParams) -> usize {
    8  // 8 週(約 2 個月)
}
```

### 6.4 Output

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
    pub date: NaiveDate,
    pub kind: ShareholderEventKind,
    pub value: f64,
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

### 6.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `Small holders count down 1,250 to 38,500 on 2026-04-25(week)` | `{ event: "small_holders_decreasing", change: -1250 }` |
| `Large holders holding pct up 1.8% to 42.3% on 2026-04-25(week)` | `{ event: "large_holders_accumulating", change: 1.8 }` |
| `Concentration index up 2.1% over 4 consecutive weeks` | `{ event: "concentration_rising", weeks: 4 }` |

### 6.6 週頻資料的時間對齊

`shareholder_core` 為週頻資料,但 batch 每日執行。處理方式:

- 每日 batch 掃描 `holding_shares_per` 表,若有新週資料則計算
- 寫入 `indicator_values` 時,`date` 欄位記錄週末日(非執行日)
- Aggregation Layer 對使用者呈現時清楚標註資料頻率

---

## 七、`day_trading_core`

### 7.1 定位

當沖統計、當沖比率、當沖力道。

### 7.2 Params

```rust
pub struct DayTradingParams {
    pub timeframe: Timeframe,
    pub ratio_high_threshold: f64,             // 當沖比率高閾值,預設 30.0(%)
    pub ratio_low_threshold: f64,              // 當沖比率低閾值,預設 5.0(%)
    pub momentum_lookback: usize,              // 當沖力道回看,預設 5
}
```

### 7.3 warmup_periods

```rust
fn warmup_periods(&self, params: &DayTradingParams) -> usize {
    20
}
```

### 7.4 Output

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
    pub value: f64,
}

pub enum DayTradingEventKind {
    RatioExtremeHigh,
    RatioExtremeLow,
    RatioStreakHigh,    // 連續 N 日高當沖比
    RatioStreakLow,
}
```

### 7.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `Day trade ratio reached 38% on 2026-04-22(extreme high)` | `{ event: "ratio_extreme_high", ratio: 38.0 }` |
| `Day trade ratio above 30% for 5 consecutive days` | `{ event: "ratio_streak_high", days: 5 }` |
| `Day trade ratio dropped to 4.2% on 2026-04-28(extreme low)` | `{ event: "ratio_extreme_low", ratio: 4.2 }` |

---

## 八、跨 Chip Core 綜合事實的處理

### 8.1 不在 Core 層整合

「外資買 + 散戶賣 = 籌碼集中」這類綜合判斷涉及兩個以上 Core 輸出,屬「跨 Core 訊號」,**不**在 Core 層整合,理由與技術指標的 TTM Squeeze 同。

### 8.2 處理方式

- ✅ 各 Chip Core 各自輸出該 Core 對應資料表的事實
- ✅ Aggregation Layer 並排呈現
- ✅ 使用者教學文件提供「如何看出籌碼集中」識讀指引

### 8.3 範例:籌碼集中

使用者要看「籌碼集中」訊號,需同時觀察:

- `institutional_core`:外資是否連續買超
- `shareholder_core`:大戶持股是否上升、散戶人數是否減少
- `foreign_holding_core`:外資持股比率是否上升

三個 Core 各自輸出 Fact,使用者並排判讀。

### 8.4 為何不立 `chip_concentration_core`

- 違反零耦合原則
- 「籌碼集中」的定義因人而異(有人重外資、有人重大戶持股),寫進 Core 等於替使用者下定義
- 一個檔案說「集中」未必另一個視角也算集中,屬經驗判讀
