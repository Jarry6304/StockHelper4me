# Indicator Cores:量能類

> **版本**:v2.0 抽出版 r1
> **日期**:2026-04-30
> **配套文件**:`cores_overview.md`(共通規範)
> **包含 Core**:3 個
> **優先級分布**:P1(1 個)/ P3(2 個)

---

## 目錄

1. [本文件範圍](#一本文件範圍)
2. [共通規範](#二共通規範本子類)
3. [`obv_core`](#三obv_corep1)
4. [`vwap_core`](#四vwap_corep3)
5. [`mfi_core`](#五mfi_corep3)
6. [Volume 不獨立成 Core 的說明](#六volume-不獨立成-core-的說明)

---

## 一、本文件範圍

| Core | 名稱 | 優先級 |
|---|---|---|
| `obv_core` | OBV(On-Balance Volume) | P1 |
| `vwap_core` | VWAP(Volume-Weighted Average Price) | P3 |
| `mfi_core` | MFI(Money Flow Index) | P3 |

**注意**:`volume`(成交量本身)**不**獨立成 Core,理由見第六章。

---

## 二、共通規範(本子類)

### 2.1 trait

全部走 `IndicatorCore` trait(見 `cores_overview.md` §3)。

### 2.2 計算策略

全部屬**滑動窗口型 / 累積型指標**,採增量計算策略。

### 2.3 量能類的特殊輸入

本子類 Core 同時消費:

- **價格**(close, high, low)— 來自 `price_*_fwd`
- **成交量**(volume)— 來自 `price_*_fwd.volume`

### 2.4 還原與成交量

**重要**:除權息事件會影響成交量(股票分割後成交股數會變)。本子類 Core **吃 TW-Market Core 處理過的 volume**(已隨除權息調整),不吃 raw volume。

---

## 三、`obv_core`(P1)

### 3.1 定位

OBV(On-Balance Volume),累積式量能指標,將每日成交量按收盤漲跌方向加減累計,反映「資金堆積方向」。

### 3.2 Params

```rust
pub struct ObvParams {
    pub timeframe: Timeframe,
    pub anchor_date: Option<NaiveDate>,    // 起算日,None 則從序列起始
    pub ma_period: Option<usize>,          // OBV 自身的均線週期(可選),預設 20
}
```

### 3.3 warmup_periods

```rust
fn warmup_periods(&self, params: &ObvParams) -> usize {
    // OBV 是累積值,理論上需要全歷史
    // 但實作上以 anchor_date 為起點,自該日起累積即可
    match params.ma_period {
        Some(p) => p + 10,
        None => 0,
    }
}
```

### 3.4 Output

```rust
pub struct ObvOutput {
    pub series: Vec<ObvPoint>,
    pub anchor_date: NaiveDate,
}

pub struct ObvPoint {
    pub date: NaiveDate,
    pub obv: f64,                  // 累積值
    pub obv_ma: Option<f64>,       // OBV 的均線(若設定 ma_period)
}
```

### 3.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `OBV bullish divergence: price LL 2026-04-10, OBV HL 2026-04-25` | `{ event: "bullish_divergence", price_date: "2026-04-10", obv_date: "2026-04-25" }` |
| `OBV bearish divergence: price HH 2026-03-20, OBV LH 2026-04-10` | `{ event: "bearish_divergence" }` |
| `OBV crossed above OBV_MA(20) at 2026-04-15` | `{ event: "obv_ma_bullish_cross", ma_period: 20 }` |
| `OBV new 6-month high at 2026-04-22` | `{ event: "obv_extreme_high", lookback: "6m" }` |

### 3.6 OBV 累積值的處理

OBV 是**累積式**指標,起點選擇影響絕對值。建議:

- 預設 `anchor_date = None`,從序列起始累積
- Workflow 可指定 anchor_date(例:某次大事件後重新累積)
- 寫入 `indicator_values` 時記錄 `anchor_date`,確保使用者知道「這條 OBV 從哪天起算」

---

## 四、`vwap_core`(P3)

### 4.1 定位

VWAP(Volume-Weighted Average Price),成交量加權平均價,常用於日內或單一週期內判斷買賣價位是否「合理」。

### 4.2 VWAP 的兩種計算方式

| 模式 | 說明 | 適用 |
|---|---|---|
| **Anchored VWAP** | 從某錨點(例:重要事件日)起算 | 結構性分析、中長期 |
| **Session VWAP** | 每個交易日重置 | 日內交易,但日線 K 棒已聚合,V2 不主推此模式 |

V2 主推 **Anchored VWAP**(日線 / 週線適用)。

### 4.3 Params

```rust
pub struct VwapParams {
    pub mode: VwapMode,
    pub anchor_date: NaiveDate,          // 必填(Anchored 模式)
    pub source: PriceSource,             // 預設 Hlc3((H+L+C)/3)
    pub timeframe: Timeframe,
}

pub enum VwapMode {
    Anchored,       // 從 anchor_date 起累積
    Session,        // 每日重置(P3 後考慮)
}
```

### 4.4 warmup_periods

```rust
fn warmup_periods(&self, params: &VwapParams) -> usize {
    0  // VWAP 從 anchor_date 起算,無需暖機
}
```

### 4.5 Output

```rust
pub struct VwapOutput {
    pub series: Vec<VwapPoint>,
    pub anchor_date: NaiveDate,
}

pub struct VwapPoint {
    pub date: NaiveDate,
    pub vwap: f64,
    pub upper_band_1sd: f64,    // VWAP + 1 倍標準差
    pub upper_band_2sd: f64,    // VWAP + 2 倍標準差
    pub lower_band_1sd: f64,
    pub lower_band_2sd: f64,
}
```

### 4.6 Fact 範例

| Fact statement | metadata |
|---|---|
| `Price crossed above VWAP(anchored 2026-01-01) at 2026-04-15` | `{ event: "vwap_bullish_cross", anchor: "2026-01-01" }` |
| `Price tested VWAP from above and bounced at 2026-04-22(VWAP=580, low=581)` | `{ event: "vwap_support_test" }` |
| `Price reached VWAP +2σ at 2026-04-25` | `{ event: "vwap_2sd_upper_touch" }` |

### 4.7 多錨點 VWAP

實務上常設多個錨點(例:年初、季初、重大事件日)同時觀察。Workflow toml 可宣告多次 entry:

```toml
[[indicator_cores]]
name = "vwap"
params = { mode = "anchored", anchor_date = "2026-01-01", timeframe = "daily" }

[[indicator_cores]]
name = "vwap"
params = { mode = "anchored", anchor_date = "2026-03-19", timeframe = "daily" }
```

各 entry 的 `params_hash` 不同,寫入 `indicator_values` 不會衝突。

---

## 五、`mfi_core`(P3)

### 5.1 定位

MFI(Money Flow Index),類似 RSI 但結合成交量,衡量「帶量的買賣壓力」。

### 5.2 Params

```rust
pub struct MfiParams {
    pub period: usize,             // 預設 14
    pub overbought: f64,           // 預設 80.0
    pub oversold: f64,             // 預設 20.0
    pub timeframe: Timeframe,
}
```

### 5.3 warmup_periods

```rust
fn warmup_periods(&self, params: &MfiParams) -> usize {
    params.period * 2 + 5
}
```

### 5.4 Output

```rust
pub struct MfiOutput {
    pub series: Vec<MfiPoint>,
}

pub struct MfiPoint {
    pub date: NaiveDate,
    pub value: f64,        // 0.0 ~ 100.0
}
```

### 5.5 Fact 範例

| Fact statement | metadata |
|---|---|
| `MFI(14) = 85 at 2026-04-25, > 80 for 5 consecutive days` | `{ event: "overbought_streak", days: 5 }` |
| `MFI(14) = 18 at 2026-04-28, < 20 for 3 consecutive days` | `{ event: "oversold_streak", days: 3 }` |
| `MFI(14) bearish divergence: price HH 2026-03-20, MFI LH 2026-04-10` | `{ event: "bearish_divergence" }` |

### 5.6 MFI vs RSI

兩者並存的意義:

- **RSI**:純價格動能
- **MFI**:價格動能 + 成交量

當兩者背離(例:RSI 超買但 MFI 未超買 / RSI 未背離但 MFI 背離),可能暗示量價配合度。**但這類綜合判讀屬使用者教學層,不在 Core 處理**。

---

## 六、Volume 不獨立成 Core 的說明

### 6.1 為何不獨立

成交量(`volume`)已存在於 raw 表(`price_daily_fwd.volume` 等),**無計算邏輯**。將其包成 `volume_core` 純屬冗餘。

### 6.2 P1 9 個指標的澄清

部分 Workflow toml 模板列出 P1 需要 9 個指標,其中包含 `volume`,但這並非新建 `volume_core`,而是「workflow 模板宣告需要 volume 資料」。實際 Aggregation Layer 直接從 raw 表撈即可。

實質 P1 需新建 Core 為 **8 個**:`macd / rsi / kd / adx / ma / bollinger / atr / obv`。

### 6.3 Volume 相關 Fact 的處理

部分量能事實雖看似屬「成交量本身」,但實際應由其他 Core 產出:

| 事實 | 由哪個 Core 處理 |
|---|---|
| 「單日量增 3 倍」 | 由 Aggregation Layer 加 simple SQL 比較,**不**為此立 Core |
| 「OBV 創新高」 | `obv_core` |
| 「量價背離」 | `obv_core` 或 `mfi_core` |
| 「VWAP 突破」 | `vwap_core` |
| 「籌碼面成交量分布」 | `day_trading_core`(籌碼類)或 `shareholder_core` |

### 6.4 例外情況

若未來出現「需要複雜計算的純量能事實」(例:成交量 anomaly detection、量能 Z-score 累積),可考慮獨立 Core,但 v2.0 P0~P3 階段**不考慮**。

---

## 附錄:量能類 Core 的時間框架建議

| Core | 日線 | 週線 | 月線 |
|---|---|---|---|
| `obv_core` | ✅ 主要 | ✅ | ✅ |
| `vwap_core` | ✅ 主要 | ⚠️ 意義較弱 | ❌ 不適用 |
| `mfi_core` | ✅ 主要 | ✅ | ⚠️ 意義較弱 |

**理由**:

- OBV 是累積式,各時間框架皆有意義
- VWAP 在日線錨點意義最強,週線以上錨點選擇困難
- MFI 為短中期動能指標,月線意義被長期均值平滑掉
