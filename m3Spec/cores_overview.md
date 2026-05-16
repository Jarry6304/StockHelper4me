# Cores 總綱:共同規範

> **版本**:v2.0 抽出版 r5(v3.5 5 層架構重構附帶)
> **日期**:2026-05-16
> **基準**:`neo_pipeline_v2_architecture_decisions_r3.md`
> **適用範圍**:所有 Core(Wave Cores / Indicator Cores / Chip Cores / Fundamental Cores / Environment Cores / System Cores)
> **架構原則**:本文件遵循 README「架構原則:計算 / 規則分層」—— Cores 層只做規則 / 算式套用,複雜計算歸 Silver 層。
> **配套文件**:本總綱搭配以下八份分類規格文件使用
>
> 1. `traditional_core.md`
> 2. `indicator_cores_momentum.md`
> 3. `indicator_cores_volatility.md`
> 4. `indicator_cores_volume.md`
> 5. `indicator_cores_pattern.md`
> 6. `chip_cores.md`
> 7. `fundamental_cores.md`
> 8. `environment_cores.md`
>
> Neely Core 規格另立 spec(`neely_core.md`),不在本系列文件範圍。

---

## r5 修訂摘要(2026-05-16,v3.5 5 層架構重構附帶)

- **新增 Layer 2.5「Cross-Stock Cores」**(對齊 v3.5 R3):Cores 層**仍是 per-stock
  compute**(輸入 stock_id → 寫 facts / indicator_values / structural_snapshots);
  跨股 cross-rank / 分群 / 相關性歸新層 `src/cross_cores/`(Layer 2.5,在 Silver
  per-stock 與 M3 Cores 之間)。Magic Formula 因屬 cross-rank(輸入 date 取
  全市場 universe → 寫 `*_ranked_derived`),已從 `silver/builders/magic_formula_ranked.py`
  搬到 `src/cross_cores/magic_formula.py`。對應 Rust binary `magic_formula_core`
  trait 不變,仍 consume Silver-like cross-stock derived 表(spec 細節見
  `m3Spec/magic_formula_core.md` r2)。
- **tw_cores monolith 拆 8 module**(v3.5 R4 C8):`rust_compute/cores/system/tw_cores/`
  從 1693 行 monolith 拆 main.rs / cli.rs / dispatcher.rs / writers.rs /
  run_environment.rs / run_stock_cores.rs / summary.rs / helpers.rs。對 §五
  Monolithic Binary 部署模型零影響(仍單一 binary,inventory 自動註冊)。
- **kalman_filter_core `MIN_REGIME_DURATION_DAYS` const → params field**(v3.5 R4 C11):
  對齊 §3.2 Params 約束(常數值應透過 Params 暴露給 caller override)。

## r4 修訂摘要(2026-05-12)

- **新增 §7.5「子類內 Output 結構同構規範」**:子類內(Indicator / Chip / Fundamental / Environment)各 Core 的 Output 結構**強制**統一為 `series + events` 兩層,Event 採 `{ date, kind, value, metadata }` 四欄共通約定。此前 `chip_cores.md` r2 已採用此 pattern 但總綱無上位法源,r4 將此原則升至總綱以授權子類遵循並避免將來 fundamental / environment 子類各自重新發明
- **§2.3 末段加交叉引用**:「子類內 Output 結構」進一步同構規範指向 §7.5,避免讀者誤把 §2.3 同族合併判準套用到 Output schema 設計

---

## r3 修訂摘要(2026-05-07)

- **§8.2 Indicator Cores 數量修正**:副標題「17 個」改為「19 個」(實際加總 9+4+3+3=19,r2 副標題誤植)
- **§12.4 Stage 4a 名單展開**:原縮寫「SR」展開為 `support_resistance_core` + `candlestick_pattern_core`,「Trendline」改為 `trendline_core`,與 `indicator_cores_pattern.md §2.2` 對齊
- **§8.5 / §9 新增 `business_indicator_core`**(P2,Environment 子類):對應 layered_schema 已備但 r2 版未對應 Core 的 `business_indicator_derived`。Environment Cores 數量 5 → 6,P2 數量 16 → 17,總 Core 數 36 → 37
- **§6.2.1 stock_id 保留字規範重整**:從 3 個保留字擴充為 6 個並重新分配
  - 新增 `_index_tpex_`(taiex_core 處理 TPEx 子序列)、`_index_us_market_`(us_market_core 專用)、`_index_business_`(business_indicator_core 專用)
  - `_global_` 縮減為「真·全球變數」,僅 `exchange_rate_core` / `fear_greed_core` 使用,語意更乾淨
  - 新增「多序列區分規則」段落:並列獨立 → 拆細保留字;同源衍生 → 共用保留字 + `metadata.subseries`
  - 新增「Silver 層 vs Cores 層」說明:Silver PK 用真實代號(如 `TAIEX`/`TPEx`/`SPY`),Cores Fact 用保留字,Loader 負責轉換
- **本次未修訂的相關項**:子類文件對 Silver 內部依賴的描述方式(fundamental §5.2 / chip §4.6 引用 7a/7b 等階段編號)需跟進 `layered_schema_post_refactor.md` 重構後同步修訂,本次 r3 僅修訂上述項目與 overview 相關項目

---

## r2 修訂摘要(2026-05-06)

- **廢除 `TW-Market Core`**:依 README 架構原則,所有職責屬「複雜計算」,歸 Silver 層 S1_adjustment Rust binary。詳見 `adr/0001_tw_market_handling.md`
- 移除 `MarketCore` trait 提及(§3.3)
- 移除 §4.4「資料相依處理:TW-Market Core 前置」
- 移除 §8.2「Market Cores」清單
- §9 P0 移除 `tw_market_core`
- §10 新增「TW-Market 處理 → 歸 Silver 層 S1」
- 既有規範補充:子類 Core 對 §6.1.1 / §6.2.1 的引用統一收攏

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
11. [跨指標訊號處理原則](#十一跨指標訊號處理原則)
12. [結構性指標的耦合例外](#十二結構性指標的耦合例外)
13. [命名規範](#十三命名規範)
14. [本系列文件未涵蓋的主題](#十四本系列文件未涵蓋的主題)

---

## 一、文件定位

本總綱集中描述**所有 Core 共同遵守的規範**,各分類文件僅描述該分類獨有的細節(參數、Fact、輸出 schema、資料表對應)。

**為何要有總綱**:原始 `neo_pipeline_v2_architecture_decisions_r3.md` 過於龐雜,Core 規格散布其中。本系列文件將 Core 定義切分,總綱統一管理共通決策,避免在每份分類文件重複說明。

**Neely Core 為何不在本系列**:Neely Core 規則複雜(monowave、Validator R1-R7、Compaction、Scenario Forest),且承載核心哲學(scenario 並排不整合、power_rating 截斷哲學論證),適合獨立成專門 spec。

### 1.1 與 README 架構原則的關係

本總綱及所有子類 Cores 文件遵循 README「架構原則:計算 / 規則分層」:

- **Silver 層**:複雜計算(後復權、漲跌停合併、跨表 join、跨日狀態追溯)
- **Cores 層**:取資料 + 套規則 / 算式

任何 Core 文件不得違反此邊界。違反者(例:在 Cores 層做後復權、在 Silver 層判斷 Wave 結構)視為設計錯誤。

---

## 二、Core 切分原則

> **核心多不是問題,問題是乾淨**

### 2.1 單一職責三條件

每個獨立 Core 必須滿足:

1. **單一職責**:一個 Core 只做一件事(算一個指標、產出一類事實)
2. **可拔可插**:從 Workflow 抽掉某 Core,其他 Core 不受影響
3. **零語義耦合**:Core 之間不直接 import,不互相觸發

### 2.2 體積建議

每個 Core 是 **200-500 行的小程式**,各自單純,比一個 5000 行的大 Engine 遠遠更好維護。若某 Core 超過 500 行,代表職責可能不夠單一,需檢討拆分。

### 2.3 同族合併 vs 分立判準

「同族指標」（如 SMA/EMA/WMA 同屬 MA、Bollinger/Keltner/Donchian 同屬 Channel）何時合併為單一 Core、何時分立成多個 Core,依下列三條件判定:

1. **Params 結構同構**:同樣的欄位集合,僅取值不同
2. **Output schema 同構**:輸出資料結構完全一致
3. **Fact 種類同構**:產出的事件類型語意相近

**全部三條成立 → 合併為單一 Core,以 enum 參數區分子型號**
**任一條不成立 → 分立成多個 Core**

| 同族案例 | 條件 1 | 條件 2 | 條件 3 | 決策 |
|---|---|---|---|---|
| MA 族(SMA/EMA/WMA) | ✅ 都是 period | ✅ 單一序列 | ✅ cross/above/below | 合併 `ma_core` |
| Channel 族(Bollinger/Keltner/Donchian) | ❌ params 異構 | ⚠️ 三線結構但語意不同 | ❌ squeeze ≠ breakout ≠ band hold | 分立 |

**跨子類同詞根 Core 必須加領域前綴**:當不同子類出現相近概念(例:個股融資 vs 市場整體融資),命名需明顯區隔(例:`margin_core` vs `market_margin_core`),避免混淆。

> **注意**:本節談的是「**Core 何時合併 vs 分立**」的判準(基於 Params / Output / Fact 三條件)。子類內各 Core 的 **Output 結構同構規範**(統一兩層 `series + events`、Event 四欄)為另一層次要求,見 §7.5。

---

## 三、統一 Trait 介面

所有 Indicator / Chip / Fundamental / Environment Core 實作同一個 trait:

```rust
pub trait IndicatorCore: Send + Sync {
    type Input: Send + Sync;        // 由各 Core 宣告,不限於 OHLCV
    type Params: Default + Clone + Serialize;
    type Output: Serialize;

    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output>;
    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact>;

    /// Core 自宣告所需的暖機「輸入序列單位數」(K 棒數 / 月份數 / 季別數),
    /// 由 input 對應的 loader 與 Pipeline 解讀。
    fn warmup_periods(&self, params: &Self::Params) -> usize;
}
```

### 3.1 trait 設計意義

統一介面後,Orchestrator 對待 30 個 Core 跟 3 個 Core 沒差別。新增 Core 只要實作 trait + 註冊,Orchestrator 不需改動。

### 3.2 Params 與 Output 約束

- `Params: Serialize` — 供 `params_hash` 計算用(canonical JSON keys ASC + blake3,取前 16 hex 字元)
- `Output: Serialize` — 供寫入 `indicator_values` 表的 JSONB 欄位
- `Params: Default` — 提供合理預設值,Workflow toml 不指定參數時使用

### 3.3 Wave Core 的差異

Wave Cores(Neely / Traditional)輸出結構複雜(Scenario Forest、Wave Tree),**不**強制走 `IndicatorCore` trait。它們有自己的 trait(`WaveCore`),但仍遵守相同的命名、註冊、版本控管規範。

`WaveCore` trait 草案於 P0 開發前定稿,設計約束:
- 沿用 `IndicatorCore` 的命名與註冊規範
- `Input` 限定為 `OHLCVSeries`(讀 Silver `price_*_fwd` 表)
- `Output` 為 Scenario Forest 結構,實作 `Serialize` 寫入 `structural_snapshots`
- 提供 `produce_facts` / `name` / `version` / `warmup_periods`

### 3.4 非 OHLCV 輸入 Core 的適配規則

並非所有 Core 都吃 OHLCV。trait 的 `Input` 由各 Core 自行宣告,常見類型:

| Input 類型 | 使用子類 | 對應 loader |
|---|---|---|
| `OHLCVSeries` | Wave Cores / Indicator Cores | `shared/ohlcv_loader/`(讀 Silver `price_*_fwd`) |
| `InstitutionalDailySeries` / `MarginDailySeries` / `ForeignHoldingSeries` / `HoldingSharesPerSeries` / `DayTradingSeries` | Chip Cores | `shared/chip_loader/`(讀 Silver `*_derived`) |
| `MonthlyRevenueSeries` / `ValuationDailySeries` / `FinancialStatementSeries` | Fundamental Cores | `shared/fundamental_loader/`(讀 Silver `*_derived`) |
| `MarketIndexTwSeries` / `MarketIndexUsSeries` / `ExchangeRateSeries` / `FearGreedIndexSeries` / `MarketMarginMaintenanceSeries` | Environment Cores | `shared/environment_loader/`(讀 Silver `*_derived`) |

> **重要**:所有 loader 一律從 Silver 層讀取,不直接讀 Bronze。Silver 層職責清單見 `layered_schema_post_refactor.md` §4。

**warmup_periods 的單位語意**:

`warmup_periods()` 回傳的整數代表「**輸入序列的單位數**」,並非固定「K 棒數」:

- 日頻 OHLCV → K 棒數
- 月頻資料(月營收) → 月份數
- 季頻資料(財報) → 季別數
- 週頻資料(集保持股級距) → 週數

各 loader 解讀 `warmup_periods()` 時依該 Input 的時間粒度取資料窗口。

---

## 四、Core 之間的耦合規範

### 4.1 ✅ 可共用(Shared Infrastructure,不是 Core)

```
shared/
├── ohlcv_loader/           # OHLCV 資料載入(Silver price_*_fwd)
├── chip_loader/            # 籌碼類資料載入(Silver *_derived)
├── fundamental_loader/     # 基本面資料載入(Silver *_derived)
├── environment_loader/     # 環境類資料載入(Silver *_derived)
├── timeframe_resampler/    # 跨 timeframe 對齊輔助
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

### 4.4 上游資料來源:Silver 層

所有 Core 的資料來源為 Silver 層,**不直接讀 Bronze**:

```
Bronze(Silver 層內部使用)
   ↓ Silver builders / Rust binary 處理
Silver(Cores 層讀取目標)
   ↓ Cores 規則 / 算式套用
Cores 層 → 寫入 indicator_values / structural_snapshots / facts
```

Silver 層的具體表結構與處理職責由 `layered_schema_post_refactor.md` 定義,本總綱不重述。

> **歷史備註**:v1.x / v2.0 r1 曾規劃 `TW-Market Core` 作為 Cores 層前置處理,負責漲跌停合併與後復權。v2.0 r2 後此 Core 廢除,所有職責歸 Silver S1_adjustment。詳見 `adr/0001_tw_market_handling.md`。

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

#### 6.1.1 禁用主觀詞彙範例集

跨各子類 Core 共同禁止下列主觀判斷詞彙進入 Fact statement:

| 子類 | 禁用詞彙(範例) |
|---|---|
| 技術面 | 「動能轉強」、「看好突破」、「視覺上的不對勁」、「警示 W5 末端」 |
| 籌碼面 | 「主力進場」、「主力洗盤」、「籌碼面轉強」 |
| 環境面 | 「市場情緒轉樂觀」、「美股拖累台股」、「匯率壓力減輕」 |
| 基本面 | 「動能優異」、「具投資價值」、「基本面強勁」、「成長動能」 |

**判別準則重申**:Fact statement 必須是「給定相同輸入產出相同文字」的機械式陳述,任何帶因果推論、情緒、價值判斷的詞彙皆禁止。子類文件**不再重述**此清單,統一引用本節。

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

#### 6.2.1 stock_id 保留字規範

`stock_id` 為 String 必填欄位,但部分 Core 的事實**與個股無關**(環境變數、市場整體指標等),需使用保留字表示。

**設計原則**:保留字以 **Core 為粒度**指派,代表「該 Core 處理的環境變數類別」。Core 內若有多個子序列(例:`taiex_core` 內的 TAIEX 與 TPEx 是兩個並列大盤;`us_market_core` 內的 SPY 與 VIX 是同源衍生),處理方式見「多序列區分規則」。

| 保留 stock_id | 用途 | 使用 Core |
|---|---|---|
| `_market_` | 台股市場整體統計 | `market_margin_core` |
| `_index_taiex_` | 台股加權指數(TAIEX) | `taiex_core`(TAIEX 序列) |
| `_index_tpex_` | 櫃買指數(TPEx) | `taiex_core`(TPEx 序列) |
| `_index_us_market_` | 美股大盤環境(SPY + VIX) | `us_market_core` |
| `_index_business_` | 台灣景氣指標 | `business_indicator_core` |
| `_global_` | 真·全球變數(無單一指數對應) | `exchange_rate_core` / `fear_greed_core` |

**規則**:
- 保留字以底線前後包夾(`_xxx_`),避免與真實股票代號(純數字)衝突
- **一個保留字僅對應一個 Core 的一條子序列**(`_index_taiex_` 與 `_index_tpex_` 因屬於並列大盤,雖出自同一 Core 也各自獨立保留字)
- 未來新增環境類 Core 若需新保留字,須在本節登記,不可由各 Core 自行挖洞
- 儲存層 spec 沿用本規範,不另立保留字
- 子類文件**不再重述**此清單,統一引用本節

**多序列區分規則**(Core 內有多條子序列時):

| 子序列關係 | 處理方式 | 範例 |
|---|---|---|
| **並列獨立**(語意對等、可獨立解讀) | 拆細保留字,各自一條 | `_index_taiex_` vs `_index_tpex_`(兩個並列大盤) |
| **同源衍生**(子序列彼此引用、共生) | 共用 Core 保留字,以 `metadata.subseries` 區分 | `us_market_core` 用 `_index_us_market_`,Fact metadata 帶 `subseries: "spy"` 或 `"vix"` |

**注意**:Silver 表 PK 中的 `stock_id` 取值(如 `taiex_index_derived` 的 `TAIEX`/`TPEx`)是 **Silver 層內部識別**,與 Cores 層 Fact 寫入時的保留字**屬不同層次**:Silver 用真實代號便於資料對應,Cores 端 Fact 用保留字以保 schema 的「個股無關」語意。Loader 負責兩者轉換。

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

#### 7.3.1 暖機倍數常見慣例

各 Core 的 `warmup_periods()` 回傳值依下列工程慣例計算:

| 計算類型 | 暖機倍數 | 理由 |
|---|---|---|
| SMA / 視窗統計 | `period + 小緩衝(5)` | 視窗滿即精確,無收斂問題 |
| 單層 EMA / ATR / RSI | `period × 4` | EMA 平滑因子 α=2/(N+1),4N 期後權重衰減至 ~0.03%,達收斂 |
| 雙層 EMA 串接(MACD signal) | `period × 6` | 兩層 EMA 誤差累積補償 |
| KD 平滑(含 %K, %D 雙層) | `period × 8` | 多層平滑加碼補償 |
| 累積式指標(OBV / Anchored VWAP) | `0`(從錨點起算)或自訂 | 無暖機概念,起算點即起點 |
| 結構性指標(SR / Trendline) | `lookback_bars + 緩衝(10)` | 由結構回看窗口決定 |

**偏離慣例**:若某 Core 的 warmup 倍數偏離本表,該 Core 文件需在 `warmup_periods()` 段落附**理由說明**(例:財報季頻 Core 用「季別數 × 90 天」是因日頻 batch 需轉換時間粒度)。

### 7.4 params_hash 演算法

```
params_hash = blake3(canonical_json(params, sort_keys=ASC))[..16] (hex)
```

- canonical JSON:鍵升序排序、無 trailing space
- 取 blake3 hex 前 16 字元(64 bits,collision 風險可忽略)
- 保證同一參數組產生同一 hash,供寫入去重與 on-demand 快取查詢

### 7.5 子類內 Output 結構同構規範

同子類(Indicator / Chip / Fundamental / Environment)各 Core 的 Output 結構**強制**統一為兩層:

```rust
pub struct XxxOutput {
    pub series: Vec<XxxPoint>,      // 時間序列數值
    pub events: Vec<XxxEvent>,      // 事件型 Fact 來源
}

pub struct XxxEvent {
    pub date: NaiveDate,            // 區間事件取結束日
    pub kind: XxxEventKind,         // 子類各自 enum
    pub value: f64,                 // 主要數值(供快速 sort/filter)
    pub metadata: serde_json::Value,// 結構化補充資料
}
```

子類文件僅定義 `XxxPoint` 欄位 / `XxxEventKind` enum 變體 / `metadata` 子結構,**兩層 + event 四欄(date / kind / value / metadata)為共通約定**,降低 Aggregation Layer 對各 Core 輸出的 ad-hoc 適配成本。

#### 7.5.1 與 §2.3 的層次區分

- **§2.3**:Core 何時合併 vs 分立(同族判準,基於 Params / Output / Fact 三條件是否同構)
- **§7.5**:子類內各 Core 的 Output **內部結構**(兩層 + event 四欄)應同構

兩節是不同層次,互不取代:§2.3 決定有幾個 Core,§7.5 決定每個 Core 內部 Output 長什麼樣。

#### 7.5.2 為何「強制」而非「建議」

- `chip_cores.md` r2 已實質採用兩層 + event 四欄 pattern,但缺總綱授權,屬「事實上的慣例」
- 將來 `fundamental_cores.md` / `environment_cores.md` 子類若各自發明 Output 結構,Aggregation Layer 將為每個子類寫專屬適配代碼,維護成本指數成長
- 強制統一可一次性給定 `produce_facts()` 從 events 直接透傳的能力,事件 metadata 與 §6.2 Fact schema 對齊

#### 7.5.3 Wave Core 的例外

`WaveCore` trait(見 §3.3)輸出 Scenario Forest 結構複雜,**不**受本節約束。本節僅約束 `IndicatorCore` trait 下的 Indicator / Chip / Fundamental / Environment 四子類。

---

## 八、所有 Core 清單

### 8.1 Wave Cores(波浪體系)

| Core | 文件 | 優先級 |
|---|---|---|
| `neely_core` | `neely_core.md` | P0 |
| `traditional_core` | `traditional_core.md` | P3 |

### 8.2 Indicator Cores(19 個)

| 子類 | Core | 文件 |
|---|---|---|
| **動量 / 趨勢 / 強度類**(9) | `macd_core` / `rsi_core` / `kd_core` / `adx_core` / `ma_core` / `ichimoku_core` / `williams_r_core` / `cci_core` / `coppock_core` | `indicator_cores_momentum.md` |
| **波動 / 通道類**(4) | `bollinger_core` / `keltner_core` / `donchian_core` / `atr_core` | `indicator_cores_volatility.md` |
| **量能類**(3) | `obv_core` / `vwap_core` / `mfi_core` | `indicator_cores_volume.md` |
| **型態 / 價位類**(3) | `candlestick_pattern_core` / `support_resistance_core` / `trendline_core` | `indicator_cores_pattern.md` |

### 8.3 Chip Cores(5 個)

| Core | 文件 |
|---|---|
| `institutional_core` / `margin_core` / `foreign_holding_core` / `shareholder_core` / `day_trading_core` | `chip_cores.md` |

### 8.4 Fundamental Cores(3 個)

| Core | 文件 |
|---|---|
| `revenue_core` / `valuation_core` / `financial_statement_core` | `fundamental_cores.md` |

### 8.5 Environment Cores(6 個)

| Core | 文件 |
|---|---|
| `us_market_core` / `taiex_core` / `exchange_rate_core` / `fear_greed_core` / `market_margin_core` / `business_indicator_core` | `environment_cores.md` |

### 8.6 System Cores

| Core | 職責 |
|---|---|
| `aggregation_layer` | 並排呈現,不整合(即時請求路徑核心) |
| `orchestrator` | Workflow 編排,Core 註冊發現、依序呼叫、結果組裝 |

System Core 的詳細規格屬「即時路徑層」,不在本系列(Core 計算層)文件範圍。

> **歷史備註**:v1.x / v2.0 r1 曾列「Market Cores」類別(`tw_market_core`),v2.0 r2 後廢除,所有職責歸 Silver S1_adjustment。詳見 `adr/0001_tw_market_handling.md`。

---

## 九、開發優先級

| 優先級 | 範圍 | 包含 Core |
|---|---|---|
| **P0** | 基礎能跑 | `neely_core` / `aggregation_layer` / `orchestrator` |
| **P1** | 技術指標 8 個 | `macd_core` / `rsi_core` / `kd_core` / `adx_core` / `ma_core` / `bollinger_core` / `atr_core` / `obv_core` |
| **P2** | 籌碼 + 基本面 + 環境 + 結構性指標 | Chip Cores(5)、Fundamental Cores(3)、Environment Cores(6)、`support_resistance_core` / `trendline_core` / `candlestick_pattern_core` |
| **P3** | 進階指標與離線模組 | `ichimoku_core` / `williams_r_core` / `cci_core` / `coppock_core` / `keltner_core` / `donchian_core` / `vwap_core` / `mfi_core` / `traditional_core` / Learner 離線模組 |

### 9.1 P0 完成後的 Gate

執行五檔股票實測(`0050 / 2330 / 3363 / 6547 / 1312`),校準 Neely Core 的 `forest_max_size` / `compaction_timeout_secs` / `BeamSearchFallback.k` 預設值,結果寫入 `docs/benchmarks/`。

P0 通過後才開始 P1。

> **前置條件**:P0 啟動前,Silver 層 PR #R1~#R6 須完成(見 `data_refactor_plan.md`),`price_daily_fwd` 等 Silver 表必須穩定可讀。

---

## 十、不獨立成 Core 的清單

避免日後重複討論,r3 明確列出**不獨立**成 Core 的項目。

| 項目 | 為何不獨立 | 實際處理方式 |
|---|---|---|
| **Volume(成交量)** | 已存在於 Silver 表(`price_daily_fwd.volume`),無計算邏輯 | Aggregation Layer 直接從 Silver 表查 |
| **Fibonacci** | Neely Core 內部子模組,輸出在 `Scenario.expected_fib_zones` | 屬 Neely Core 範圍 |
| **TTM Squeeze 等跨指標訊號** | 違反零耦合原則 | 詳見第十一章 |
| **MA / SMA / EMA / WMA(同族)** | 演算法相近,差異僅在權重 | 統一為 `ma_core`,以 enum 參數區分子型號 |
| **Tag Core** | 即時路徑禁用 LLM | 改為離線 Learner 模組(P3) |
| **TW-Market 處理**(漲跌停合併、後復權) | 屬複雜計算,違反「Cores 層只做規則 / 算式套用」 | 歸 Silver 層 S1_adjustment Rust binary。詳見 `adr/0001_tw_market_handling.md` |

### 10.1 P1 9 個指標清單澄清

P1 之中的 `volume` 並非新建 `volume_core`,而是「workflow 模板宣告需要 volume 資料」,Aggregation Layer 從 Silver 表撈即可。實質 P1 需新建 Core 為 **8 個**:`macd / rsi / kd / adx / ma / bollinger / atr / obv`。

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

- **Stage 4a**:獨立結構性 Core(Neely / Traditional / `support_resistance_core` / `candlestick_pattern_core`)平行執行
- **Stage 4b**:依賴 4a 的 Core(`trendline_core` / Fib 投影)在 4a 完成後執行

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

#### 13.2.1 跨子類同詞根的命名前綴規則

當同一概念在不同子類出現(個股級 vs 市場級、日頻 vs 月頻等),命名須明顯區隔避免混淆。

| 衝突案例 | 個股 / 個體版 | 市場 / 整體版 |
|---|---|---|
| 融資維持率 | `margin_core`(Chip) | `market_margin_core`(Environment) |

**規則**:
- 環境類 Core 涉及「市場整體」概念,且該概念在個股類已有同名 Core 時,**強制加 `market_` 前綴**
- 未來若出現其他衝突(例:成交量個股 vs 市場),沿用此前綴規則
- 不適用於名稱本身已具區別性的 Core(例:`taiex_core` vs 個股 Indicator,無需前綴)

### 13.3 目錄結構

```
cores/
├── neely_core/
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
├── chip_loader/
├── fundamental_loader/
├── environment_loader/
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

複雜 Core(例:Neely Core)可在此基本結構上擴充子模組,各 Core 文件自行宣告。

---

## 十四、本系列文件未涵蓋的主題

以下主題屬 v2.0 架構但**不在本系列文件範圍**,請參考原始 `neo_pipeline_v2_architecture_decisions_r3.md`:

- **Neely Core 完整規格**(Monowave Detection / Validator / Compaction / Scenario Forest / Power Rating / Fibonacci 子模組)→ `neely_core.md`
- **Silver 層職責清單**(price_*_fwd / *_derived 表結構與 builder 邏輯)→ `layered_schema_post_refactor.md`
- **Bronze / Silver 重構計畫**(PR #R1~#R6)→ `data_refactor_plan.md`
- **儲存層四層架構**(Raw / Indicator Values / Structural Snapshots / Facts)→ 第十四章
- **Batch Pipeline 設計**(Stage 1-4 / on-demand 補算 / single-flight)→ 第十五章
- **Workflow 預設模板**(monthly / quarterly / half_yearly / yearly)→ 第十六章
- **前端職責邊界與錯誤態渲染**→ 第十七章
- **Aggregation Layer 詳細設計**→ 第十二章
- **PyO3 邊界規範**→ 13.3
- **Learner 離線模組界定**→ 附錄 C
- **Indicator kernel 共用化**(`taiex_core` 與個股 Indicator Core 邏輯重複的抽出)→ P3 後考慮,V2 不規劃
- **`financial_statement_core` 拆分**(損益/資產負債/現金流獨立 Core)→ V3 議題,V2 不規劃

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
| Volume 獨立 Core | Aggregation 直接讀 Silver 表即可 |
| `Trigger.on_trigger.ReduceProbability` | 與不使用 probability 原則矛盾 |
| **TW-Market Core 作為 Cores 層的 Market Core** | **r2 廢除**:屬複雜計算,歸 Silver S1。詳見 `adr/0001_tw_market_handling.md` |
