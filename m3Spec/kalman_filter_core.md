# Kalman Filter Core 規格(卡爾曼濾波 — multi-horizon 1-D 趨勢平滑)

> **版本**:r2(v3.33 multi-horizon refactor,2026-05-18)
> **位置**:`rust_compute/cores/indicator/kalman_filter_core/`
> **上游 Silver**:`price_daily_fwd`(僅 close;OHLCV-based,對齊 atr/rsi 等)
> **優先級**:P3(對齊 P1 經典 indicator + P3 P3 statistical / state-space 子類)

## r2 修訂摘要(2026-05-18)

v3.33 — 從 single horizon(Q=1e-5,~12 年 halflife)改 **multi-horizon 4 個並行 recursion**。
對非平穩急漲股(如 3030 8 年漲 8 倍)single horizon smoothed 永遠落後 raw 數倍;
multi-horizon 各自獨立跑,LLM 看 4 horizon 一致性判讀訊號信心。

| Horizon | Q | Halflife | 對應期 | velocity_threshold | min_dur |
|---|---|---|---|---|---|
| **short** | 1e-1 | ~31 bars | 6 週 | 0.005 | 3 |
| **medium**(primary) | 1e-2 | ~99 bars | 5 月 | 0.002 | 5 |
| **long** | 1e-3 | ~310 bars | 1.2 年 | 0.0015 | 5 |
| **ultra_long** | 1e-5 | ~3100 bars | 12 年 | 0.001 | 5 |

facts(EventKind transition)**只從 primary horizon 產**,保留 ≤ 12/yr/stock production 行為。
其他 horizon 訊號走 `indicator_values.value.horizons[].series_last`,LLM 自己讀。

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

## 四、Params(v3.33 multi-horizon)

```rust
pub struct KalmanFilterHorizon {
    pub label: String,                     // "short" / "medium" / "long" / "ultra_long"
    pub process_noise_q: f64,
    pub velocity_threshold_pct: f64,
    pub min_regime_duration_days: usize,
}

pub struct KalmanFilterParams {
    pub timeframe: Timeframe,              // Daily
    pub measurement_noise_rel: f64,        // 預設 0.01(4 horizons 共用)
    pub warmup_days: usize,                // 預設 60(共用)
    pub horizons: Vec<KalmanFilterHorizon>,    // v3.33:預設 4 horizons
    pub primary_horizon: String,           // 預設 "medium"
}
```

### 4.1 Default 4 horizons(v3.33 拍版 2026-05-18)

| label | Q | halflife_bars | velocity_threshold | min_regime_dur |
|---|---|---|---|---|
| short      | 1e-1 | ~31  | 0.005 | 3 |
| medium     | 1e-2 | ~99  | 0.002 | 5 |
| long       | 1e-3 | ~310 | 0.0015 | 5 |
| ultra_long | 1e-5 | ~3100 | 0.001 | 5 |

**halflife 公式**:`halflife_bars ≈ 9.8 / sqrt(Q)`(對 R=(0.01·p)² 配方,production calibration)。

**velocity_threshold scaling rationale**:
- Q 越大 → smoothed 對 raw 跟得越緊 → daily velocity 自然越大(short horizon
  daily smoothed velocity ~ daily return ~ 1-5%)
- 原 0.001(0.1%)對 short horizon 等於每天都 fire → 改 0.005 過濾噪音
- 對 ultra_long(Q=1e-5)保留 0.001(對齊 v3.4 r2 production calibration)

### 4.2 Reference + 文獻

- **Kalman, R. E.** (1960). "A new approach to linear filtering and prediction
  problems." *Trans. ASME — Journal of Basic Engineering*, 82(1), 35–45.
  原始 paper,公式 2.4(Predict)+ 2.7(Update)1-D 退化版。
- **Roncalli, T.** (2013). *Lectures on Risk Management*. CRC Press, §11.2.
  個股 trend filter 應用,**Q ∈ [1e-5, 1e-3] 推薦範圍對應不同 horizon**
  (v3.33 將範圍擴成 1e-5 ~ 1e-1 4 horizons)。
- **Bork, L. & Petersen, A.M.** (2014). "Trends in stock prices?" Working
  Paper. relative R formulation。
- **R = (0.01 × mean_price)²**:相對 price 比例化,4 horizons 共用。日動 1%
  是台股日報酬率標準差的數量級。
- **warmup_days=60**:Kalman state 收斂在 30-60 bars 內,60 保守。對齊
  Roncalli 2013 推薦 30-60 day warmup,4 horizons 共用此 warmup。

### 4.3 Reference 經典文獻(歸併至 §4.2)

> v3.33 已合併文獻清單到 §4.2 多 horizon segment。本段保留歷史交叉鏈接。

## 五、warmup_periods

```rust
fn warmup_periods(&self, params: &KalmanFilterParams) -> usize {
    params.warmup_days
}
```

預設 60。Pass 3 transition events 在 warmup 結束後才產(對齊 Roncalli 2013
state convergence)。

## 六、Output(v3.33 multi-horizon)

```rust
pub struct KalmanFilterOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub primary_horizon: String,                  // 預設 "medium"
    pub series: Vec<KalmanPoint>,                 // primary horizon 完整 series(backward compat v3.30)
    pub events: Vec<KalmanEvent>,                 // primary horizon events(= facts source)
    pub horizons: Vec<KalmanHorizonOutput>,       // 4 horizons latest state + event_count
}

pub struct KalmanHorizonOutput {
    pub label: String,
    pub process_noise_q: f64,
    pub halflife_bars: f64,                       // 9.8 / sqrt(Q)
    pub velocity_threshold_pct: f64,
    pub min_regime_duration_days: usize,
    pub series_last: Option<KalmanPoint>,         // 每 horizon 自己的最末 state
    pub event_count: usize,                       // 該 horizon transition 次數(facts 不寫)
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

### 7.1 Kalman recursion(1-D)— v3.33 multi-horizon

對同一 OhlcvSeries,**對每個 horizon 各自跑一次** 1-D Kalman recursion(共用 R,Q
per-horizon)。

```rust
fn compute_kalman_recursion(bars: &[OhlcvBar], q: f64, r: f64) -> Vec<KalmanPoint> {
    let mut x = bars[0].close;       // x_0
    let mut p = r;                   // P_0(信任 measurement)
    let mut series = Vec::with_capacity(bars.len());
    let mut prev_x: Option<f64> = None;
    for bar in bars {
        // Predict
        let x_pred = x;              // constant-state model
        let p_pred = p + q;
        // Update
        let z = bar.close;
        let k = p_pred / (p_pred + r);
        x = x_pred + k * (z - x_pred);
        p = (1.0 - k) * p_pred;

        let velocity = prev_x.map(|prev| x - prev).unwrap_or(0.0);
        prev_x = Some(x);
        series.push(KalmanPoint { date: bar.date, raw_close: z,
            smoothed_price: x, uncertainty: p.sqrt(), velocity,
            regime: Regime::Sideway });   // placeholder
    }
    series
}
```

4 horizons 並行跑,result 寫進 `horizons: Vec<KalmanHorizonOutput>`:

```rust
for horizon in &params.horizons {
    let series = compute_kalman_recursion(bars, horizon.process_noise_q, r);
    let series = classify_series_regimes(series, horizon.velocity_threshold_pct);
    let events = detect_events_run_length(&series, warmup, horizon.min_regime_duration_days);
    // ... 寫進 horizon_outputs
}
```

**Top-level `series` / `events`** 對齊 primary horizon(預設 "medium"),保留 v3.30
series-last-entry path fix 的 backward compat。

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
