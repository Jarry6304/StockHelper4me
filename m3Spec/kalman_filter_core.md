# Kalman Filter Core 規格(卡爾曼濾波 — 1-D 趨勢平滑)

> **版本**:r1(v3.4 上線時立稿,2026-05-15)
> **位置**:`rust_compute/cores/indicator/kalman_filter_core/`
> **上游 Silver**:`price_daily_fwd`(僅 close;OHLCV-based,對齊 atr/rsi 等)
> **優先級**:P3(對齊 P1 經典 indicator + P3 P3 statistical / state-space 子類)

## 一、本文件範圍

定義 `kalman_filter_core` 的 1-D Kalman recursion、5-class regime
classification、Params 預設值、Output 結構、EventKind 設計。

**不在本文件**:
- 2-D state-space / pairs trading 模型(v3.4 r1 範圍外,留 future)
- ML / particle filter / EKF(非線性 / 多 state 變體)
- MCP tool `kalman_trend` wrapper 細節(屬 `mcp_server/_kalman.py`)

## 二、定位

Kalman Filter(Kalman 1960)是經典 state-space 估計器。對個股應用,將
「true underlying price」當 unobservable state,close 當 observation,
recursion 出 smoothed series + uncertainty band + velocity。

**對 LLM 的價值**:
- `smoothed_price` 排除高頻 noise,看「真實趨勢」
- `velocity`(每日 smoothed 變化)給出 trend 方向 + 強度
- `uncertainty_band`(±1σ)給「目前訊號信心」
- `regime`(5 類分類)結論化趨勢狀態

### 2.1 1-D vs n-D(架構選擇)

v3.4 r1 採 1-D(state = price scalar)。理由:
- 對齊 cores_overview §四「Core per-stock 獨立」
- 對齊 toolkit v2 narrative 風格(個股級結論,不跨股)
- 公式單純(5 步 recursion),沙箱 cargo build / test 即可驗證
- pairs trading(2-D state-space β-hedge ratio)複雜度 ~3x,留 future PR

## 三、上游 Silver 表

- 表:`price_daily_fwd`
- PK:`(market, stock_id, date)`
- 關鍵欄位:`close`(其他 OHLCV 也存在,本 core 只用 close)
- 載入器:`shared/ohlcv_loader/load_daily`,提供 `OhlcvSeries`

## 四、Params

```rust
pub struct KalmanFilterParams {
    pub timeframe: Timeframe,              // Daily
    pub process_noise_q: f64,              // 預設 1e-5
    pub measurement_noise_rel: f64,        // 預設 0.01
    pub warmup_days: usize,                // 預設 60
    pub velocity_threshold_pct: f64,       // 預設 0.001(0.1%/day)
}
```

### 4.1 Reference + 預設值依據(user 拍版 2026-05-15)

- **Q=1e-5**:`process_noise`,越小 → smoothed_price 越平滑。對齊
  Roncalli, T. (2013). *Lectures on Risk Management*. CRC Press, §11.2
  個股 trend filter 推薦範圍 1e-5 ~ 1e-3。
- **R = (0.01 × mean_price)²**:`measurement_noise` 相對 price 比例化,
  對齊 Bork & Petersen (2014). "Trends in stock prices?" Working Paper
  推薦做法。日動 1% 是台股日報酬率標準差的數量級。
- **warmup_days=60**:Kalman state 收斂在 30-60 bars 內,60 保守。對齊
  Roncalli 2013 推薦 30-60 day warmup,避免前期 state noise 觸發 phantom events。
- **velocity_threshold_pct=0.001**:`|smoothed velocity / smoothed price| <
  0.1%` 判 Sideway(對齊台股日均報酬 ~0.05% 的 2x);此值需 production
  calibration 校準。

### 4.2 Reference 經典文獻

- **Kalman, R. E.** (1960). "A new approach to linear filtering and prediction
  problems." *Transactions of the ASME — Journal of Basic Engineering*,
  82(1), 35–45. 原始 paper,公式 2.4(Predict)+ 2.7(Update)1-D 退化版。
- **Roncalli, T.** (2013). *Lectures on Risk Management*. CRC Press, §11.2.
  個股 trend filter 應用,Q / R 推薦範圍。
- **Bork, L. & Petersen, A.M.** (2014). "Trends in stock prices?" Working
  Paper. relative R formulation。

## 五、warmup_periods

```rust
fn warmup_periods(&self, params: &KalmanFilterParams) -> usize {
    params.warmup_days
}
```

預設 60。Pass 3 transition events 在 warmup 結束後才產(對齊 Roncalli 2013
state convergence)。

## 六、Output

```rust
pub struct KalmanFilterOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<KalmanPoint>,
    pub events: Vec<KalmanEvent>,
}

pub struct KalmanPoint {
    pub date: NaiveDate,
    pub raw_close: f64,
    pub smoothed_price: f64,                  // state estimate x_t|t
    pub uncertainty: f64,                     // sqrt(P_t|t),≈ 1-σ
    pub velocity: f64,                        // smoothed_t - smoothed_{t-1}
    pub regime: Regime,
}

pub enum Regime {
    StableUp,
    Accelerating,
    Sideway,
    Decelerating,
    StableDown,
}

pub struct KalmanEvent {
    pub date: NaiveDate,
    pub kind: KalmanEventKind,
    pub metadata: serde_json::Value,
}

pub enum KalmanEventKind {
    EnteredStableUp,
    EnteredAccelerating,
    EnteredSideway,
    EnteredDecelerating,
    EnteredStableDown,
}
```

### 6.1 5 類 regime 設計(user 拍版 2026-05-15)

對齊 stock_health 5-tier 風格(strong / leaning / neutral / weak / poor)+
fear_greed 5-zone(extreme_fear / fear / neutral / greed / extreme_greed)
的同款分類粒度。

| Regime | 條件 | 投資意涵 |
|---|---|---|
| `StableUp` | velocity > +threshold, accel ≈ 0 | 持續上漲動能 |
| `Accelerating` | velocity > +threshold, accel > 0 | 加速上漲(常見於突破 / 行情啟動) |
| `Sideway` | \|velocity\| < threshold | 整理 / 區間震盪 |
| `Decelerating` | (velocity > +threshold, accel < 0) ∪ (velocity < -threshold, accel > 0) | 動能消退 / reversal pending |
| `StableDown` | velocity < -threshold, accel ≤ 0 | 持續下跌動能 |

Decelerating 是個雙向類別:上漲動能在消退 / 下跌動能在減速(可能 reversal)。
對齊技術分析「動能 vs 方向」雙維邏輯。

**不收**(對齊 user 拍版 2026-05-15「5 類」):
- ❌ 3 類 simplification(uptrend / sideway / downtrend)— 缺加速 / 減速資訊
- ❌ 7 類 expansion(加 reversal_up_pending / reversal_down_pending)— 過度細分

## 七、計算策略

### 7.1 Kalman recursion(1-D)

```rust
// 初始化
let mut x = bars[0].close;       // x_0
let mut p = R;                   // P_0(信任 measurement)

for bar in bars {
    // Predict
    let x_pred = x;              // 趨勢 model:x_t = x_{t-1}(constant velocity 假設 in Pass 1 simplified)
    let p_pred = p + Q;
    // Update
    let z = bar.close;
    let k = p_pred / (p_pred + R);
    x = x_pred + k * (z - x_pred);
    p = (1.0 - k) * p_pred;
    // emit x as smoothed_price, p.sqrt() as uncertainty
}
```

### 7.2 Velocity / Acceleration

```rust
velocity_i = smoothed_i - smoothed_{i-1}
accel_i    = velocity_i - velocity_{i-REGIME_LOOKBACK_DAYS}    // 20-day window
```

`REGIME_LOOKBACK_DAYS=20`(寫死 const,對齊台股 ~1 month trading days)。

### 7.3 Regime classification

```rust
fn classify_regime(vel_pct: f64, accel: f64, threshold: f64) -> Regime {
    if vel_pct.abs() < threshold { return Regime::Sideway; }
    if vel_pct > 0.0 {
        if accel > 0.0 { Regime::Accelerating }
        else if accel < 0.0 { Regime::Decelerating }
        else { Regime::StableUp }
    } else {
        if accel > 0.0 { Regime::Decelerating }
        else { Regime::StableDown }
    }
}
```

### 7.4 Transition events(warmup 後)

```rust
let warmup = params.warmup_days.min(n);
let mut prev_regime = None;
for (i, point) in series.iter().enumerate() {
    if i < warmup { prev_regime = Some(point.regime); continue; }
    if Some(point.regime) != prev_regime {
        events.push(EnteredXxx { ... });
        prev_regime = Some(point.regime);
    }
}
```

對齊 transition pattern(v1.32 P2 acceptance + valuation/fear_greed/bollinger
等 5 cores 同款設計)。

### 7.5 Metadata 結構

```json
{
  "smoothed_price": 1220.3,
  "raw_close":      1234.5,
  "velocity":       0.42,
  "uncertainty":    8.5,
  "from_regime":    "Sideway",
  "to_regime":      "StableUp"
}
```

## 八、Fact 範例

| Fact statement | metadata |
|---|---|
| `EnteredStableUp on 2026-05-10` | `{ smoothed_price: 1220.3, raw_close: 1234.5, velocity: 0.42, uncertainty: 8.5, from_regime: "Sideway", to_regime: "StableUp" }` |
| `EnteredDecelerating on 2026-04-22` | `{ smoothed_price: 1180.0, raw_close: 1175.0, velocity: 0.15, uncertainty: 9.2, from_regime: "Accelerating", to_regime: "Decelerating" }` |
| `EnteredSideway on 2026-03-15` | `{ ... from_regime: "Decelerating", to_regime: "Sideway" }` |

## 九、預估觸發頻率

對齊 v1.32 P2 acceptance ≤ 12/yr/stock:

| Regime transition | 預估頻率 |
|---|---|
| EnteredStableUp ↔ EnteredAccelerating | 5-8/yr(常見 trend 持續中加速 / 減速) |
| Trend ↔ Sideway | 4-6/yr(行情啟動 / 結束) |
| EnteredStableDown / EnteredDecelerating | 5-8/yr(同上對稱) |

合計 ~10-15/yr/stock(略高於 v1.32 上限,但 regime 變化是本 core 主要 deliverable)。

校準路線(v3.4 r2 後可能補):若 production 觸發率 > 15/yr,考慮加
MIN_REGIME_DURATION constraint(連續 N 天同 regime 才算 transition),
對齊 v1.32 P2 Round 5/6 calibration pattern。

## 十、Production 注意

- 對 1700 stocks × 252 daily bars = ~430K KalmanPoint 計算,Rust workload
  ~ms 量級(對齊 atr_core 計算複雜度)
- warmup 60 days:第一次 production run 對歷史 1y 序列只有 ~192 個有效 point
  能產 event;後續 incremental 加 1 row/day
- LLM 端走 MCP `kalman_trend(stock_id, date, lookback_days=180)`,payload
  ~1.5 KB / ~400 tokens

## 十一、不收錄(留 future PR / 獨立 core)

- ❌ Extended Kalman Filter(非線性 state model)
- ❌ Particle Filter(non-Gaussian state model)
- ❌ 2-D state-space pairs trading(留 future,user 拍版 v3.4 r1 範圍外)
- ❌ Velocity-augmented state(x_t = [price, velocity];Pass 1 simplified 用
  constant-velocity 假設;留 r2 升 2-D state)
- ❌ Cross-asset Kalman(market-neutral basket;另一系列 strategy)

對齊 cores_overview §四「不抽象」+ §十四「P3 後考慮」原則:這些變體
都不在 v3.4 範圍內。
