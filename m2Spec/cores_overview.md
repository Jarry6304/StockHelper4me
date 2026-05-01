# Cores 總綱:共同規範

> **版本**:v2.0 抽出版 r1
> **日期**:2026-04-30
> **基準**:`neo_pipeline_v2_architecture_decisions_r3.md`
> **適用範圍**:所有 Core(Wave Cores / Market Cores / Indicator Cores / Chip Cores / Fundamental Cores / Environment Cores / System Cores)
> **配套文件**:本總綱搭配以下九份分類規格文件使用
>
> 1. `traditional_core.md`
> 2. `tw_market_core.md`
> 3. `indicator_cores_momentum.md`
> 4. `indicator_cores_volatility.md`
> 5. `indicator_cores_volume.md`
> 6. `indicator_cores_pattern.md`
> 7. `chip_cores.md`
> 8. `fundamental_cores.md`
> 9. `environment_cores.md`
>
> Neely Core 規格另立 spec,不在本系列文件範圍。

---

## 目錄

1. [文件定位](#一文件定位)
2. [Core 切分原則](#二core-切分原則)
3. [統一 Trait 介面](#三統一-trait-介面)
4. [Core 之間的耦合規範](#四core-之間的耦合規範)
5. [部署模型](#五部署模型monolithic-binary)
6. [事實邊界規範](#六事實邊界規範)
7. [Output 與 Fact 寫入策略](#七output-與-fact-寫入策略)
8. [所有 Core 清單](#八所有-core-清單)
9. [開發優先級](#九開發優先級)
10. [不獨立成 Core 的清單](#十不獨立成-core-的清單)
    - 10.0 [Core 邊界判定三原則](#10-0-core-邊界判定三原則)
11. [跨指標訊號處理原則](#十一跨指標訊號處理原則)
12. [結構性指標的耦合例外](#十二結構性指標的耦合例外)
13. [命名規範](#十三命名規範)
14. [本系列文件未涵蓋的主題](#十四本系列文件未涵蓋的主題)

---

## 一、文件定位

本總綱集中描述**所有 Core 共同遵守的規範**,各分類文件僅描述該分類獨有的細節(參數、Fact、輸出 schema、資料表對應)。

**為何要有總綱**:原始 `neo_pipeline_v2_architecture_decisions_r3.md` 過於龐雜,Core 規格散布其中。本系列文件將 Core 定義切分,總綱統一管理共通決策,避免在每份分類文件重複說明。

**Neely Core 為何不在本系列**:Neely Core 規則複雜(monowave、Validator R1-R7、Compaction、Scenario Forest),且承載核心哲學(scenario 並排不整合、power_rating 截斷哲學論證),適合獨立成專門 spec。

---

## 二、Core 切分原則

> **核心多不是問題,問題是乾淨**

### 2.1 單一職責三條件

每個獨立 Core 必須滿足:

1. **單一職責**:一個 Core 只做一件事(算一個指標、做一類資料前處理、產出一類事實)
2. **可拔可插**:從 Workflow 抽掉某 Core,其他 Core 不受影響
3. **零語義耦合**:Core 之間不直接 import,不互相觸發

### 2.2 體積建議

每個 Core 是 **200-500 行的小程式**,各自單純,比一個 5000 行的大 Engine 遠遠更好維護。若某 Core 超過 500 行,代表職責可能不夠單一,需檢討拆分。

---

## 三、統一 Trait 介面

所有 Indicator / Chip / Fundamental / Environment Core 實作同一個 trait:

```rust
pub trait IndicatorCore: Send + Sync {
    type Params: Default + Clone + Serialize;
    type Output: Serialize;

    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn compute(&self, ohlcv: &OHLCVSeries, params: Self::Params) -> Result<Self::Output>;
    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact>;

    /// Core 自宣告所需的暖機 K 線數,供 Batch Pipeline 取資料窗口
    fn warmup_periods(&self, params: &Self::Params) -> usize;
}
```

### 3.1 trait 設計意義

統一介面後,Orchestrator 對待 30 個 Core 跟 3 個 Core 沒差別。新增 Core 只要實作 trait + 註冊,Orchestrator 不需改動。

### 3.2 Params 與 Output 約束

- `Params: Serialize` — 供 `params_hash` 計算用(canonical JSON keys ASC + blake3,取前 16 hex 字元)
- `Output: Serialize` — 供寫入 `indicator_values` 表的 JSONB 欄位
- `Params: Default` — 提供合理預設值,Workflow toml 不指定參數時使用

### 3.3 非 Indicator Core 的差異

Wave Cores(Neely / Traditional)與 Market Cores(TW-Market)輸出結構複雜,**不**強制走 `IndicatorCore` trait。它們有自己的 trait(例:`WaveCore` / `MarketCore`),但仍遵守相同的命名、註冊、版本控管規範。

---

## 四、Core 之間的耦合規範

### 4.1 ✅ 可共用(Shared Infrastructure,不是 Core)

```
shared/
├── ohlcv_loader/           # OHLCV 資料載入
├── timeframe_resampler/    # 日線→週線→月線聚合
├── fact_schema/            # Fact 統一資料結構
├── data_ref/               # 資料追溯機制
└── degree_taxonomy/        # 共用的 Degree 詞彙
```

這些是基礎建設,本身不做任何體系判斷,不算 Core。

### 4.2 ❌ 禁止跨 Core 引用

- **Core 之間不直接 import**:MACD Core 不能 `use rsi_core`
- **Core 不知道 Workflow 的存在**:每個 Core 是純函式黑盒
- **Core 不互相觸發**:沒有「MACD Core 看到背離通知 RSI Core」這種事

如果發現有需要跨 Core 的邏輯,那是 **Workflow / Orchestrator** 該處理的事,不是 Core 的事。

### 4.3 已知例外:trendline_core

**唯一**允許消費其他 Core 輸出的例外是 `trendline_core`,可讀取 Neely Core 的 monowave 輸出。詳見第十二章。

### 4.4 資料相依處理:TW-Market Core 前置

TW-Market Core 的「連續漲跌停合併」會改變 monowave 序列,但 Neely Core 也吃 monowave 序列。

**採用選項 A**:TW-Market Core 在 Neely Core 之前執行,做資料前處理。

```
Raw OHLC → TW-Market Core(合併漲跌停)→ 處理過的 OHLC → Neely Core / 其他 Cores
```

**設計意義**:Neely Core 完全不知道台股的存在,純淨執行 Neely 規則。

---

## 五、部署模型:Monolithic Binary

P0 / P1 / P2 階段一律採 **Monolithic Binary** 部署:所有 Core 編譯在同一 workspace 同一 binary,inventory 自動註冊,版本檢查由編譯期保證。

```rust
// 每個 Core 在自己的 lib 裡註冊
inventory::submit! {
    CoreRegistration::new("macd_core", "1.0.0", || Box::new(MacdCore::new()))
}

// Orchestrator 啟動時自動發現所有已編譯的 Core
let registry = CoreRegistry::discover();
```

新增 Core 不用改 Orchestrator 代碼,只要寫好 Core 並編譯進去,自動可用。

### 5.1 Monolithic 的代價(誠實標註)

- 改任一 Core 需重編全部(實測 ~5 分鐘可接受)
- 無法 hot-fix 單一 Core,但台股一日一交易、batch 模式下沒有這個需求
- Core 不能各自 versioning 對外發版,但 v2.0 沒有第三方 Core 生態,不需要

### 5.2 未來重議條件

當以下三條至少滿足兩條,才考慮升級到 Dynamic Loading 或 Subprocess + IPC:

1. Core 數量超過 50 且改動頻率高
2. 出現第三方 Core(社群開發者貢獻)
3. 出現必須 hot-fix 的線上場景

### 5.3 source_version 用途

`source_version` 欄位仍寫入 `indicator_values` / `structural_snapshots` / `facts` 三表,但用途限於**審計追溯**(回查某筆資料是哪個版本算出),**不做 runtime 相容性檢查**(Monolithic 編譯期已對齊)。

---

## 六、事實邊界規範

每個 Core 只做**嚴格規則式事實**,不做經驗判斷式:

| ✅ 進 Core(嚴格規則式) | ❌ 不進 Core(經驗判斷式) |
|---|---|
| `MACD(12,26,9) golden cross at 2026-04-15` | `MACD 顯示動能轉強` |
| `RSI(14) = 78, > 70 for 5 consecutive days` | `RSI 進入超買區,警示 W5 末端` |
| `ADX = 32, +DI > -DI for 20 days` | `ADX 確認趨勢中` |
| `Histogram expanded 8 consecutive bars` | `動能加速,看好突破` |
| `Bearish divergence: price made HH at A, indicator made LH at B, within N bars` | `視覺上的不對勁` |

### 6.1 判別準則

**問自己**:這個事實是否能被機械式重現?

- 給定相同 OHLC 與參數,任何時候執行都會產出**相同的 Fact 文字**?→ 是 → 進 Core
- 需要看圖 / 看上下文 / 經驗判斷?→ 否 → 不進 Core,屬使用者教學

### 6.2 Fact 統一 Schema

所有 Core 產出的 Fact 走 `shared/fact_schema/`,統一結構:

```rust
pub struct Fact {
    pub stock_id: String,
    pub fact_date: NaiveDate,
    pub timeframe: Timeframe,
    pub source_core: String,        // e.g., "macd_core"
    pub source_version: String,     // e.g., "1.0.0"
    pub params_hash: Option<String>,// canonical JSON keys ASC + blake3 前 16 字元
    pub statement: String,          // 機械式 Fact 文字
    pub metadata: serde_json::Value,// Core 特定的結構化資料
}
```

### 6.3 Facts 表 Unique Constraint

`(stock_id, fact_date, timeframe, source_core, COALESCE(params_hash, ''), md5(statement))` + INSERT ON CONFLICT DO NOTHING。確保同一事實不會重複寫入。

---

## 七、Output 與 Fact 寫入策略

### 7.1 三類資料寫入分流

| 資料種類 | 目的地 | 寫入頻率 |
|---|---|---|
| **時間序列值**(每日 MACD、RSI 數值) | `indicator_values`(JSONB 欄位) | 每日 batch,寫入當日值 |
| **結構性快照**(SR 位、趨勢線、Wave Forest) | `structural_snapshots` | 每日 batch,全量重算 |
| **事件式 Fact**(golden_cross、breakout、divergence) | `facts`(append-only) | 每日 batch,僅新增當日新事件 |

### 7.2 計算策略

| 指標類型 | 範例 | 策略 |
|---|---|---|
| **滑動窗口型** | MACD / RSI / KD / Bollinger / ATR / OBV | **最近 N 天 + 暖機區增量計算**(Core 透過 `warmup_periods()` 宣告 N) |
| **結構性** | Neely / SR / Trendline / Candlestick Pattern | **每日全量重算** |

### 7.3 warmup_periods 用途

Batch Pipeline 透過 `warmup_periods(params)` 取得 Core 所需的暖機 K 線數,確保:

- 增量計算的指標有足夠歷史可暖機(例:MACD(12,26,9) 至少需 ~50 根 K 線暖機)
- 結構性 Core 取得足夠歷史以建構結構

### 7.4 params_hash 演算法

```
params_hash = blake3(canonical_json(params, sort_keys=ASC))[..16] (hex)
```

- canonical JSON:鍵升序排序、無 trailing space
- 取 blake3 hex 前 16 字元(64 bits,collision 風險可忽略)
- 保證同一參數組產生同一 hash,供寫入去重與 on-demand 快取查詢

**進 hash 的 Params 欄位**(P1-15 / C5 補強):**所有** `Params` 結構 field 都進 hash,
包括但不限於:`enum 變體`(如 `BollingerParams.source: PriceSource`)、`anchor 日期`
(如 `VwapParams.anchor: NaiveDate`)、`mode`、`timeframe`、`thresholds`(如 RSI/MFI 的
`overbought` / `oversold`)。**不進 hash 的欄位需在 Core spec 個別宣告**(如某 Core 的
`debug_only` 欄位)。

### 7.5 Batch 觸發來源與 dirty 契約(P0-7 / r3.1 補強)

> 本節定義 dirty queue 的觸發契約 — 何時 Core 會被 batch dispatcher 排程重算。
> 短期實作見 `src/post_process.invalidate_fwd_cache`(commit `e051216`),長期完整
> 契約待 m3_compute 動工時落地(r3.1 暫以 collector 端 stock_sync_status 充當)。

#### 7.5.1 四個 dirty 觸發視角

| # | 觸發 | 來源 | 範例 |
|---|---|---|---|
| ① | **Bronze 變更** | Collector Phase 2 / 5 / 6 寫入新 raw 資料 | 新除權息事件 → `price_adjustment_events` |
| ② | **M3 內 Core 互引用** | Wave/Pattern Core 結果改變,影響 trendline_core 等下游 | neely_core scenario_forest 變動 |
| ③ | **tw_market_core 還原因子變更** | Bronze AF / vf 改動觸發 fwd 表全段重算 | P1-17 修正 stock_dividend vf |
| ④ | **Bronze 補單**(historical revision) | FinMind 修正過去資料 / Collector 重抓 | par_value_change 補資料 |

第 5 個視角(workflow toml 新增 params_hash)由 on-demand 路徑觸發,不走 batch dirty queue。

#### 7.5.2 寫入 dirty 的契約

- **Collector 端**:Phase 2/5/6 寫 Bronze 後 reset `stock_sync_status.fwd_adj_valid = 0`
  (現況短期補丁;長期 m3_compute 落地後改寫進 silver-level dirty queue)
- **Silver builder 端**(blueprint v3.2 規劃):每張 `*_derived` 表加 `is_dirty BOOLEAN` +
  `dirty_at TIMESTAMPTZ` 欄位;Bronze 變更 trigger 設此 flag
- **M3 端**:不寫 dirty(M3 是讀者,不是寫者),但 Core 可透過 `core_dependency_graph`
  通知 Orchestrator 「我的 output 變了」

#### 7.5.3 讀 dirty 的契約

- **Batch dispatcher** 讀 silver dirty 找出待算的 (market, stock_id, date_range),分發給對應 Core
- **Core 不感知 dirty queue**:Core 是純函式,給定 input + params 永遠產出同樣 output;
  dirty 純屬 dispatcher 排程資訊
- **m3_compute 不回寫 silver dirty**:讀完算完寫進 `indicator_values` / `structural_snapshots` /
  `facts`,不動 silver 的 dirty flag(避免循環依賴)

#### 7.5.4 收尾流程

每個 Core 計算完成後:
1. 寫進對應目的地表(§7.1 三類)
2. 更新自己的 `core_version` / `computed_at` 戳記
3. **不**清 silver dirty flag(由 silver builder 端負責;M3 不碰)
4. dispatcher 收 Core success → 清 dispatcher 內部隊列
5. 失敗則 retry(限定次數,失敗計數寫進 `batch_execution_log`)

#### 7.5.5 已知 dirty 漏觸發風險(待修)

- **跨 Core 互引用 (case ②)**:目前 `core_dependency_graph` 是靜態宣告,未做執行時通知
  → trendline_core 不會自動感知 neely_core 重算 → 需 dispatcher 端依 graph 連動排程
- **on-demand 路徑 (case 5)**:workflow toml 新增 entry 時尚未自動 backfill 歷史 → 由
  workflow_registry 的 `last_used_at` cron job 補(r3 §15.6)

---

## 八、所有 Core 清單

### 8.1 Wave Cores(波浪體系)

| Core | 文件 | 優先級 |
|---|---|---|
| `neely_core` | (另立 spec) | P0 |
| `traditional_core` | `traditional_core.md` | P3 |

### 8.2 Market Cores

| Core | 文件 | 優先級 |
|---|---|---|
| `tw_market_core` | `tw_market_core.md` | P0 |

### 8.3 Indicator Cores(17 個)

| 子類 | Core | 文件 |
|---|---|---|
| **動量 / 趨勢 / 強度類**(9) | `macd_core` / `rsi_core` / `kd_core` / `adx_core` / `ma_core` / `ichimoku_core` / `williams_r_core` / `cci_core` / `coppock_core` | `indicator_cores_momentum.md` |
| **波動 / 通道類**(4) | `bollinger_core` / `keltner_core` / `donchian_core` / `atr_core` | `indicator_cores_volatility.md` |
| **量能類**(3) | `obv_core` / `vwap_core` / `mfi_core` | `indicator_cores_volume.md` |
| **型態 / 價位類**(3) | `candlestick_pattern_core` / `support_resistance_core` / `trendline_core` | `indicator_cores_pattern.md` |

### 8.4 Chip Cores(5 個)

| Core | 文件 |
|---|---|
| `institutional_core` / `margin_core` / `foreign_holding_core` / `shareholder_core` / `day_trading_core` | `chip_cores.md` |

### 8.5 Fundamental Cores(3 個)

| Core | 文件 |
|---|---|
| `revenue_core` / `valuation_core` / `financial_statement_core` | `fundamental_cores.md` |

### 8.6 Environment Cores(5 個)

| Core | 文件 |
|---|---|
| `us_market_core` / `taiex_core` / `exchange_rate_core` / `fear_greed_core` / `market_margin_core` | `environment_cores.md` |

### 8.7 System Cores

| Core | 職責 |
|---|---|
| `aggregation_layer` | 並排呈現,不整合(即時請求路徑核心) |
| `orchestrator` | Workflow 編排,Core 註冊發現、依序呼叫、結果組裝 |

System Core 的詳細規格屬「即時路徑層」,不在本系列(Core 計算層)文件範圍。

---

## 九、開發優先級

| 優先級 | 範圍 | 包含 Core |
|---|---|---|
| **P0** | 基礎能跑 | `neely_core` / `tw_market_core` / `aggregation_layer` / `orchestrator` |
| **P1** | 技術指標 8 個 | `macd_core` / `rsi_core` / `kd_core` / `adx_core` / `ma_core` / `bollinger_core` / `atr_core` / `obv_core` |
| **P2** | 籌碼 + 基本面 + 環境 + 結構性指標 | Chip Cores(5)、Fundamental Cores(3)、Environment Cores(5)、`support_resistance_core` / `trendline_core` / `candlestick_pattern_core` |
| **P3** | 進階指標與離線模組 | `ichimoku_core` / `williams_r_core` / `cci_core` / `coppock_core` / `keltner_core` / `donchian_core` / `vwap_core` / `mfi_core` / `traditional_core` / Learner 離線模組 |

### 9.1 P0 完成後的 Gate

執行五檔股票實測(`0050 / 2330 / 3363 / 6547 / 1312`),校準 Neely Core 的 `forest_max_size` / `compaction_timeout_secs` / `BeamSearchFallback.k` 預設值,結果寫入 `docs/benchmarks/`。

P0 通過後才開始 P1。

---

## 十、不獨立成 Core 的清單

避免日後重複討論,r3 明確列出**不獨立**成 Core 的項目。

### 10.0 Core 邊界判定三原則(r3.1 抽出)

> 由 r2 §2.3 反向案例(R1 §3.6 institutional_market_daily 外擴 + R3 §A5 forest overflow 內縮)抽出的判定通則。下面表格中「實際處理方式」欄都應對齊此三原則。

1. **可重現原則**:給定相同輸入與 params,任何時候執行都產出相同 Output → 可立 Core
   - 反例:依靠 cache / 隨機種子 / 系統時間 的 Core 不能立
2. **無選擇原則**:Core 內部不做「擇一/排序/篩選」決策,所有候選並列輸出
   - 反例:Forest overflow 用 power_rating 排序丟棄(`neely_core §十二 12.2-12.3` — Track B user 拍板方案 1/2);Fibonacci ratio 多重命中 tiebreak (P1-16)
3. **無經驗原則**:不需經驗判讀的事實由 Core 機械產出;經驗類由 Aggregation Layer 判讀
   - 反例:`MACD 顯示動能轉強`(經驗詞)/ `bandwidth_extreme_low`(描述詞,P2 待修)

### 10.1 不獨立成 Core 的清單

| 項目 | 為何不獨立 | 實際處理方式 |
|---|---|---|
| **Volume(成交量)** | 已存在於 raw 表(`price_daily_fwd.volume`),無計算邏輯 | Aggregation Layer 直接從 raw 表查 |
| **全市場三大法人合計** | SUM 可算(SQL view) | r3 §2.5 `total_institutional_view` |
| **Fibonacci** | Neely Core 內部子模組,輸出在 `Scenario.expected_fib_zones` | 屬 Neely Core 範圍 |
| **TTM Squeeze 等跨指標訊號** | 違反零耦合原則 | 詳見第十一章 |
| **MA / SMA / EMA / WMA(同族)** | 演算法相近,差異僅在權重 | 統一為 `ma_core`,以 enum 參數區分子型號 |
| **Tag Core** | 即時路徑禁用 LLM | 改為離線 Learner 模組(P3) |

### 10.1 P1 9 個指標清單澄清

P1 之中的 `volume` 並非新建 `volume_core`,而是「workflow 模板宣告需要 volume 資料」,Aggregation Layer 從 raw 表撈即可。實質 P1 需新建 Core 為 **8 個**:`macd / rsi / kd / adx / ma / bollinger / atr / obv`。

---

## 十一、跨指標訊號處理原則

**不為「跨指標訊號」設立獨立 Core**(已決策)。

### 11.1 範例:TTM Squeeze

需要同時看布林通道與 Keltner Channel:

- ❌ 不寫成 `ttm_squeeze_core`,違反零耦合
- ✅ `bollinger_core` 輸出 `bandwidth` / `upper_band` / `lower_band`
- ✅ `keltner_core` 輸出 `upper_band` / `lower_band`
- ✅ Aggregation Layer 並排呈現,使用者自己看出「布林收進 Keltner 內」
- ✅ 教學文件提供「如何看出 Squeeze」使用者指引(屬 UI / 教學層)

### 11.2 通則

任何「同時看兩個以上 Core 輸出才能成立的訊號」,都屬使用者教學範疇,不進架構。

---

## 十二、結構性指標的耦合例外

`trendline_core` 是**唯一**已知的設計例外。

### 12.1 例外原因

- 趨勢線需要先做 swing point 偵測
- swing point 邏輯與 Neely Core 的 monowave detection 在演算法上重複
- 重複實作會帶來維護負擔與一致性風險

### 12.2 決策

- `trendline_core` **可消費 Neely Core 的 monowave 輸出**(僅讀 monowave,不讀 scenario forest)
- 此例外在 trendline_core 的 `Cargo.toml` 明確宣告 `depends_on = ["neely_core"]`
- 在 V2 spec 列入「已知耦合」清單

### 12.3 替代方案(若無法接受耦合)

把 swing point detection 抽出為 `shared/swing_detector/`,Neely Core 與 trendline_core 都消費 shared module。

**第一版傾向直接消費 Neely Core 輸出,第二版視情況重構**。

### 12.4 Stage 4 拆分

Batch Pipeline Stage 4 拆 4a / 4b 兩個子階段:

- **Stage 4a**:獨立結構性 Core(Neely / Traditional / SR)平行執行
- **Stage 4b**:依賴 4a 的 Core(Trendline / Fib 投影)在 4a 完成後執行

---

## 十三、命名規範

### 13.1 Workflow vs Orchestrator

```
從 user 視角 / 文件視角:Workflow
  例:「TW-Stock-Standard Workflow」、「Quick-Analysis Workflow」

從代碼視角 / 模組命名:Orchestrator
  例:Rust crate 名 = `orchestrator`,struct 名 = `WorkflowOrchestrator`
```

### 13.2 Core 命名

統一以 `_core` 結尾,Cargo.toml `name` 與 Rust crate 名一致。

```
macd_core / rsi_core / institutional_core / ...
```

### 13.3 目錄結構

```
cores/
├── neely_core/
├── tw_market_core/
├── traditional_core/
├── indicators/
│   ├── macd_core/
│   ├── rsi_core/
│   └── ...
├── chips/
│   ├── institutional_core/
│   └── ...
├── fundamentals/
│   ├── revenue_core/
│   └── ...
├── environment/
│   ├── us_market_core/
│   └── ...
└── system/
    ├── aggregation_layer/
    └── orchestrator/

shared/
├── ohlcv_loader/
├── timeframe_resampler/
├── fact_schema/
├── data_ref/
└── degree_taxonomy/

workflows/
├── tw_stock_standard.toml
├── tw_stock_deep_analysis.toml
└── quick_screening.toml
```

### 13.4 Core 內部目錄結構

```
some_core/
├── Cargo.toml
├── src/
│   ├── lib.rs        # trait impl + inventory::submit!
│   ├── compute.rs    # 計算邏輯
│   ├── facts.rs      # Fact 產生規則
│   └── params.rs     # Params 結構與預設值
└── tests/
    ├── unit_test.rs
    └── golden_test.rs  # 與外部標準比對的 golden test
```

---

## 十四、本系列文件未涵蓋的主題

以下主題屬 v2.0 架構但**不在本系列文件範圍**,請參考原始 `neo_pipeline_v2_architecture_decisions_r3.md`:

- **Neely Core 完整規格**(Monowave Detection / Validator / Compaction / Scenario Forest / Power Rating / Fibonacci 子模組)→ 另立 spec
- **儲存層四層架構**(Raw / Indicator Values / Structural Snapshots / Facts)→ 第十四章
- **Batch Pipeline 設計**(Stage 1-4 / on-demand 補算 / single-flight)→ 第十五章
- **Workflow 預設模板**(monthly / quarterly / half_yearly / yearly)→ 第十六章
- **前端職責邊界與錯誤態渲染**→ 第十七章
- **Aggregation Layer 詳細設計**→ 第十二章
- **PyO3 邊界規範**→ 13.3
- **Learner 離線模組界定**→ 附錄 C

---

## 附錄:已棄用的設計決策(摘錄)

| 棄用項 | 棄用原因 |
|---|---|
| Bayesian 後驗更新 | likelihood 主觀 |
| Softmax 動態溫度 τ | 主觀調參 |
| LLM 仲裁層(即時路徑) | 黑箱不可審計 |
| 容差 toml 外部化 | 誘導偏離原作 |
| Indicator-Wave Linker Core | 誘導耦合 |
| 按 Degree 分層輸出 Indicator | 偷偷耦合 |
| Engine_T + Engine_N 整合公式(`×1.1/×0.7`) | 主觀調參 |
| `[TW-MARKET]` Scorer 微調 | 主觀加權 |
| TTM Squeeze 等跨指標訊號獨立 Core | 違反零耦合 |
| Tag Core 即時模組 | 改為離線 Learner |
| 全量寫入 OLTP DB 給 Learner 用 | 已採 Batch + 同 DB,Learner 直讀 |
| 八層分類進架構作為 Core 分組 | 僅作 UI 與教學概念 |
| Core 之間 runtime 版本相容性檢查 | Monolithic 編譯期已對齊 |
| Fibonacci 獨立 Core | 確認為 Neely Core 子模組 |
| Volume 獨立 Core | Aggregation 直接讀 raw 表即可 |
| `Trigger.on_trigger.ReduceProbability` | 與不使用 probability 原則矛盾 |
