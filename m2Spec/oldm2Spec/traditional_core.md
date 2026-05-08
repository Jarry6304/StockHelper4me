# Traditional Core 規格

> **版本**:v2.0 抽出版 r2
> **日期**:2026-05-06
> **配套文件**:`cores_overview.md`(共通規範)、`neely_core.md`、`layered_schema_post_refactor.md`、`adr/0001_tw_market_handling.md`
> **架構原則**:本文件遵循 README「架構原則:計算 / 規則分層」—— Traditional Core 只做傳統派波浪規則套用,所有資料前處理由 Silver 層完成。
> **優先級**:P3
> **狀態**:**第二版範圍,P0 / P1 / P2 階段不開發**

---

## r2 修訂摘要(2026-05-06)

- 上游從「TW-Market Core 處理過的 OHLC」改為「Silver `price_*_fwd`」(§2 / §7 / §8.1 / §8.2)
- §3 補上 `TraditionalCoreParams` 的 `Default` impl(對齊總綱 §3.2 強制規範)
- `TraditionalSchool::Combined` 改名為 `FrostAndRamki`,避免與已棄用「Engine_T + Engine_N 整合公式」混淆
- §5 Fact 章末 / §4 Output 章末加上對總綱 §6.1.1 / §6.2.1 的引用

---

## 目錄

1. [定位](#一定位)
2. [輸入](#二輸入)
3. [Params](#三params)
4. [Output 結構](#四output-結構)
5. [Fact 產出規則](#五fact-產出規則)
6. [warmup_periods](#六warmup_periods)
7. [對應資料表](#七對應資料表)
8. [與 Neely Core 的關係](#八與-neely-core-的關係)
9. [已棄用的整合方式](#九已棄用的整合方式)
10. [開發注意事項](#十開發注意事項)

---

## 一、定位

**Traditional Core** 處理傳統派波浪規則(Frost / Prechter / Ramki 體系),與 Neely Core **獨立並列**,兩者不整合。

### 1.1 設計意圖

- 為使用者提供傳統派與 Neely 派**並排對照**的解讀
- 兩派輸出 scenario forest 後,由 Aggregation Layer 並排呈現,**使用者自己連結**
- Pipeline 不做仲裁、不做加權、不選 primary

### 1.2 必要規範

- 屬 Wave Core,**不**走 `IndicatorCore` trait
- 與 Neely Core 共用 `WaveCore` trait(設計約束見 `cores_overview.md` §3.3,trait 簽章於 P0 開發前定稿)
- 輸出 scenario forest 結構與 Neely Core 同形(欄位可能不同),便於 Aggregation Layer 並排處理
- 嚴格遵循 README「架構原則:計算 / 規則分層」—— 本 Core 只做傳統派規則套用,所有資料前處理由 Silver 層完成

---

## 二、輸入

| 輸入 | 說明 |
|---|---|
| `OHLCVSeries` | Silver 層 `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd`(由 S1_adjustment Rust binary 處理:後復權、漲跌停合併) |
| `Timeframe` | 日線 / 週線 / 月線 |
| `Params` | 見第三章 |

**重要**:Traditional Core 與 Neely Core 一樣,直接讀 Silver 層 `price_*_fwd`,**不**直接讀 Bronze raw OHLC。

> Silver 層的處理職責清單見 `layered_schema_post_refactor.md` §4.1。

---

## 三、Params

```rust
pub struct TraditionalCoreParams {
    pub atr_period: usize,           // 工程參數,預設 14
    pub school: TraditionalSchool,   // 規則來源
    pub timeframe: Timeframe,
}

pub enum TraditionalSchool {
    /// Frost & Prechter (1978) Elliott Wave Principle
    Frost,

    /// Ramki 數浪法
    Ramki,

    /// 並排輸出 Frost + Ramki 兩派 Forest,**兩派各自獨立,不整合**
    /// 與已棄用的 v1.1 Engine_T + Engine_N 加權公式語意不同(見 §9)
    FrostAndRamki,
}

impl Default for TraditionalCoreParams {
    fn default() -> Self {
        Self {
            atr_period: 14,
            school: TraditionalSchool::Frost,
            timeframe: Timeframe::Daily,
        }
    }
}
```

**已棄用**:容差 toml 外部化、Scorer 7 因子加權加總(理由與 Neely Core 同)。

### 3.1 `FrostAndRamki` 的設計意圖

當使用者選擇 `FrostAndRamki` 時:

- Core 會跑兩次計算,分別套用 Frost 規則與 Ramki 規則
- 輸出兩個獨立的 Forest,各自附 `school` 標籤
- Aggregation Layer 並排呈現兩派,**不做交集 / 聯集 / 加權**

此模式僅為「方便使用者一次取得兩派解讀」,語意上仍是兩個獨立 Core 結果並列,與 v1.1 已棄用的「Engine_T + Engine_N 加權整合」(見 §9)截然不同。

---

## 四、Output 結構

```rust
pub struct TraditionalCoreOutput {
    // 輸入 metadata
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub data_range: TimeRange,
    pub school: TraditionalSchool,

    // 結構性結果(Forest,不是 Tree;不排序、不選 primary)
    pub scenario_forest: Vec<TraditionalScenario>,

    // Core 自己的診斷
    pub diagnostics: TraditionalDiagnostics {
        pivot_count: usize,
        candidate_count: usize,
        validator_pass_count: usize,
        validator_reject_count: usize,
        rejections: Vec<RuleRejection>,
        elapsed_ms: u64,
    },

    // 規則書頁追溯
    pub rule_book_references: Vec<RuleReference>,
}

pub struct TraditionalScenario {
    pub id: String,

    // 結構
    pub wave_tree: WaveNode,
    pub pattern_type: TraditionalPatternType,  // Impulse / Diagonal / Zigzag / Flat / Triangle / Double-Three / Triple-Three
    pub structure_label: String,

    // 失效條件
    pub invalidation_triggers: Vec<Trigger>,

    // 客觀計數
    pub passed_rules: Vec<RuleId>,
    pub deferred_rules: Vec<RuleId>,
    pub rules_passed_count: usize,

    // Fibonacci 投影區(傳統派也用 Fibonacci)
    pub expected_fib_zones: Vec<FibZone>,
}
```

### 4.1 與 Neely Core Scenario 的差異

| 欄位 | Neely Core | Traditional Core |
|---|---|---|
| `power_rating` | ✅ Neely 書裡查表 | ❌ 傳統派無對應概念 |
| `max_retracement` | ✅ Neely 書裡查表 | ❌ 不適用 |
| `post_pattern_behavior` | ✅ Neely 書裡查表 | ❌ 不適用 |
| `structural_facts` 7 維 | ✅ | ⚠️ 部分適用(Fibonacci / Alternation / Channeling) |

### 4.2 並排呈現原則

Aggregation Layer 將 Neely Forest 與 Traditional Forest **並排呈現**,**不**做共識比對、不加總、不打分。

### 4.3 共用結構引用

- `FibZone` 結構:P3 開發時決定「沿用 Neely Core §14.3 / 自行定義」,本文件暫保留為佔位
- `Trigger` / `WaveNode` / `RuleId` / `RuleReference` / `TimeRange`:沿用 `shared/` 共用結構
- `stock_id` 編碼:遵循 `cores_overview.md` §6.2.1(Traditional Core 處理個股,實務上使用真實股票代號)

---

## 五、Fact 產出規則

每個 scenario 在進入 forest 時產出對應 Fact。

| Fact 範例 | metadata |
|---|---|
| `Traditional(Frost) impulse 5-wave completed at 2026-03-19, target 700-800` | `{ school: "frost", pattern: "impulse", target_low: 700.0, target_high: 800.0 }` |
| `Traditional(Frost) ABC corrective in progress, current at B wave` | `{ school: "frost", pattern: "abc", current_wave: "B" }` |
| `Traditional(Ramki) zigzag completed, expecting flat correction` | `{ school: "ramki", pattern: "zigzag_completed" }` |

**注意**:`statement` 欄位帶 `school` 標籤,避免與 Neely Core Fact 混淆。

### 5.1 統一規範引用

- Fact statement 詞彙限制遵循 `cores_overview.md` §6.1.1(禁用主觀詞彙)
- Facts 表 unique constraint 與 `params_hash` 演算法遵循 `cores_overview.md` §6.3 / §7.4
- `school` 為 `FrostAndRamki` 時,兩派 scenario 各自產出 Fact,`params_hash` 因 `school` 值不同而不同,自動避免去重衝突

---

## 六、warmup_periods

Traditional Core 屬**結構性指標**,每日全量重算。但仍宣告所需歷史資料量:

```rust
fn warmup_periods(&self, params: &TraditionalCoreParams) -> usize {
    match params.timeframe {
        Timeframe::Daily => 500,    // ~2 年日線
        Timeframe::Weekly => 250,   // ~5 年週線
        Timeframe::Monthly => 120,  // ~10 年月線
    }
}
```

實際窗口大小依 P3 開發階段調整。此值偏離 `cores_overview.md` §7.3.1 的常見慣例,理由:結構性 Core 用「可建構完整波浪結構所需的最短歷史」為基準,非 EMA 倍數收斂。

---

## 七、對應資料表

| 用途 | 資料表 |
|---|---|
| 輸入 OHLC | Silver `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd`(由 S1_adjustment 產出) |
| 寫入結構快照 | `structural_snapshots`,`core_name = 'traditional_core'` |
| 寫入 Fact | `facts`,`source_core = 'traditional_core'` |

> Silver `price_*_fwd` 表結構由 `layered_schema_post_refactor.md` §4.1 定義。

---

## 八、與 Neely Core 的關係

### 8.1 兩者並列,不整合

```
Silver 層
   │
   └─→ price_*_fwd (S1_adjustment 已處理:後復權 + 漲跌停合併)
         │
         ├──→ Neely Core      → Neely Scenario Forest      ┐
         └──→ Traditional Core → Traditional Scenario Forest ┤
                                                            ↓
                                                   Aggregation Layer
                                                       並排呈現
                                                       不整合
```

### 8.2 兩者共用什麼

- 共用 Silver 層 `price_*_fwd`(讀同一份已處理 OHLC)
- 共用 `shared/degree_taxonomy/` 的 Degree 詞彙(若適用)
- 共用 `shared/fact_schema/` 寫 Fact

### 8.3 兩者不共用什麼

- 不共用規則書(Neely 書 vs Frost / Ramki 書)
- 不共用 scenario 結構(欄位不同)
- 不互相觸發、不互相驗證
- **不整合輸出**(已棄用 Combined confidence ×1.1/×0.7 公式)

---

## 九、已棄用的整合方式

| 棄用項 | 來源 | 棄用原因 |
|---|---|---|
| Combined confidence ×1.1/×0.7 | v1.1 Item 17.5 | 主觀調參,違反「並排不整合」 |
| Engine_T 給分 / Engine_N 給分 加權加總 | v1.1 隱含 | 加權本身就是主觀 |
| 「兩派共識則加分」邏輯 | 早期討論 | 機率語意,違反 §2.2 原則 |
| `TraditionalSchool::Combined` 命名 | v2.0 r1 | r2 改名為 `FrostAndRamki`,避免與已棄用整合公式混淆 |

---

## 十、開發注意事項

### 10.1 P3 範圍提醒

Traditional Core 屬 **P3** 範圍,**P0 / P1 / P2 階段不開發**。原因:

1. P0 焦點在 Neely Core 與基礎建設
2. P1 焦點在技術指標
3. P2 焦點在 Chip / Fundamental / Environment Core 與結構性指標
4. Traditional Core 的價值在於提供「另一派解讀」,屬於擴充性功能,非 MVP 必要

### 10.2 開發前置條件

開發 Traditional Core 前,以下三項必須穩定:

1. Neely Core 五檔股票實測通過
2. `WaveCore` trait 草案在 P0 完成後固化
3. `shared/swing_detector/` 是否抽出已決定(影響 Traditional Core 的 pivot 偵測實作策略)

### 10.3 Fact 命名衝突防範

Traditional Core 與 Neely Core 都會產出「波浪相關 Fact」,**統一在 `statement` 開頭加 `Traditional(school)` 或 `Neely` 標籤**,避免下游使用者混淆。

```
✅ "Neely impulse 5-wave detected with power_rating=2"
✅ "Traditional(Frost) impulse 5-wave completed, target 700-800"
❌ "impulse 5-wave detected"  // 不知道是哪派
```

### 10.4 與 Aggregation Layer 的契約

- Aggregation Layer 將兩派 Forest 並排回傳,**不做交集 / 聯集計算**
- 前端 UI 應分兩個 tab 或兩個區塊呈現,**避免視覺上暗示兩派可以「整合」**
- API 回傳格式為 `{ neely: { forest: [...] }, traditional: { forest: [...] } }`,**不做合併陣列**

### 10.5 P3 開發時補完的清單

以下項目於 P3 開發時補完於本文件:

- 進 / 不進 Traditional Core 規則清單(對應 Neely Core §4.2 / §4.3 風格)
- 內部 Pipeline 階段圖(對應 Neely Core §7 風格)
- Validator 規則組(對應 Neely Core §10 風格)
- `FibZone` 結構決定(沿用 Neely / 自行定義)
- 規則書頁追溯規範(對應 Neely Core 附錄 A 風格)

---

## 附錄:Traditional Core 規則來源(P3 開發時補完)

| School | 主要參考 |
|---|---|
| Frost | Frost & Prechter (1978) "Elliott Wave Principle" |
| Ramki | Ramki 數浪法相關專書 |
| FrostAndRamki | (兩派並排,不整合) |

具體規則(R1, R2, ... 等對應條目)在 P3 開發時補入本文件附錄。
