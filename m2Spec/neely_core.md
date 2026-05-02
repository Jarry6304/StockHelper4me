# Neely Core 規格

> **版本**:v2.0 抽出版 r1
> **日期**:2026-04-30
> **基準**:`neo_pipeline_v2_architecture_decisions_r3.md`
> **配套文件**:`cores_overview.md`(共通規範)
> **優先級**:**P0**(核心 Core,所有後續 Core 的結構性參考)

---

## 目錄

1. [定位](#一定位)
2. [設計哲學:並排不整合](#二設計哲學並排不整合)
3. [模組組成](#三模組組成)
4. [Neely 規則邊界](#四neely-規則邊界)
5. [輸入](#五輸入)
6. [Params 與 NeelyEngineConfig](#六params-與-neelyengineconfig)
7. [內部 Pipeline 階段](#七內部-pipeline-階段)
8. [Output 結構](#八output-結構)
9. [Scenario Forest 結構](#九scenario-forest-結構)
10. [Validator 規則組](#十validator-規則組)
11. [Compaction 重新定位](#十一compaction-重新定位)
12. [Forest 上限保護機制](#十二forest-上限保護機制)
13. [Power Rating 截斷哲學論證](#十三power-rating-截斷哲學論證)
14. [Fibonacci 子模組](#十四fibonacci-子模組)
15. [Fact 產出規則](#十五fact-產出規則)
16. [warmup_periods](#十六warmup_periods)
17. [對應資料表](#十七對應資料表)
18. [診斷與可觀察性](#十八診斷與可觀察性)
19. [P0 Gate:五檔股票實測](#十九p0-gate五檔股票實測)
20. [已棄用的設計](#二十已棄用的設計)
21. [與其他 Core 的關係](#二十一與其他-core-的關係)

---

## 一、定位

**Neely Core** 是 v2.0 架構的 **核心結構性 Core**,實作 Glenn Neely《Mastering Elliott Wave》(NEoWave)的全套規則,輸出 **Scenario Forest**(多棵並列、不選 primary、無分數)。

### 1.1 設計意圖

- 將 Neely 體系的所有規則集中於單一 Core,作為「最權威的波浪結構解讀引擎」
- 輸出 Forest 而非 Tree:**所有 Neely 規則允許的合法解讀並列呈現**,不替使用者選最佳解
- 是 v2.0 整個架構從「裁決式」轉向「展示式」的核心體現

### 1.2 必要規範

- 屬 Wave Core,**不**走 `IndicatorCore` trait
- 走 `WaveCore` trait(草案,P0 確定)
- 是 P0 階段最複雜、開發優先級最高的 Core
- 與 Traditional Core(P3)獨立並列,**不整合**

### 1.3 Neely Core 的特殊地位

雖然其他結構性 Core(Trendline / SR)的設計皆遵循「零耦合」,**`trendline_core` 是唯一例外**,允許讀取 Neely Core 的 monowave 輸出(僅 monowave,不讀 scenario forest)。詳見 `cores_overview.md` §12。

---

## 二、設計哲學:並排不整合

### 2.1 核心原則

從 v1.1 的「**裁決式**」轉向 v2.0 的「**展示式**」。具體差異:

| 項目 | v1.1(裁決式,已棄用) | v2.0(展示式) |
|---|---|---|
| 輸出形狀 | `primary` + `alternatives`,有分數 | `Scenario Forest`,無分數無排序 |
| Compaction | 貪心選最高分 + backtrack | 窮舉所有合法壓縮路徑,產出 Forest |
| 容差系統 | 絕對偏移 ±4% | **相對偏移 ±4%**(Neely 原意) |
| 失效條件 | 無 | 每個 Scenario 必須有 invalidation_triggers |
| `[TW-MARKET]` Scorer 微調 | 嵌入 Neely Engine | **移除**(主觀調參) |

### 2.2 Scenario 屬性原則

- ❌ **不**做「Pipeline 算的分數」(主觀加權)
- ❌ **不**做 probability(主觀分布假設)
- ✅ **客觀計數**:`rules_passed_count`、`deferred_rules_count`(誰多誰少使用者自看)
- ✅ **Neely 書裡寫死的屬性**(查表):`power_rating`、`max_retracement`、`post_pattern_behavior`
- ✅ **失效條件**:`invalidation_triggers`(規則的逆向轉譯)

### 2.3 並排不整合的具體禁止

❌ **以下行為禁止寫進 Neely Core**:

- 「Neely Core 說 S1 結構好 + Chips Core 說外資買 → 強看好」
- 「兩派共識則加分」
- 「Engine_T 給分 + Engine_N 給分 加權加總」
- 「VIX 高時調降結構分數」

✅ **正確做法**:

- 把 Neely Core 的 scenarios 與 Chips Core 的事實**並列**輸出至 Aggregation Layer
- 由使用者自己連線

---

## 三、模組組成

Neely Core 內部包含以下子模組,**全部封裝於 `neely_core` crate**,對外只暴露單一 `compute()` 入口:

```
neely_core/
├── Cargo.toml
├── src/
│   ├── lib.rs                          # WaveCore trait impl + inventory::submit!
│   ├── monowave/                       # Monowave Detection
│   │   ├── mod.rs
│   │   ├── pure_close.rs               # Pure Close + ATR 演算法
│   │   ├── proportion.rs               # Rule of Proportion(45° + ATR)
│   │   └── neutrality.rs               # Rule of Neutrality(水平段判定)
│   ├── candidates/                     # Bottom-up Candidate Generator
│   │   ├── mod.rs
│   │   └── generator.rs
│   ├── validator/                      # Validator R1-R7 / F1-F2 / Z1-Z4 / T1-T10 / W1-W2
│   │   ├── mod.rs
│   │   ├── core_rules.rs               # R1-R7
│   │   ├── flat_rules.rs               # F1-F2
│   │   ├── zigzag_rules.rs             # Z1-Z4
│   │   ├── triangle_rules.rs           # T1-T10
│   │   └── wave_rules.rs               # W1-W2
│   ├── classifier/                     # Pattern Classifier
│   │   ├── mod.rs
│   │   ├── flat.rs
│   │   ├── triangle.rs
│   │   └── combination.rs
│   ├── post_validator/                 # Post-Constructive Validator
│   │   └── mod.rs
│   ├── complexity/                     # Complexity Rule
│   │   └── mod.rs
│   ├── compaction/                     # Compaction(窮舉 Forest)
│   │   ├── mod.rs
│   │   ├── exhaustive.rs               # 窮舉模式
│   │   └── beam_search.rs              # Forest 上限保護的 fallback
│   ├── missing_wave/                   # Missing Wave 偵測
│   │   └── mod.rs
│   ├── emulation/                      # Emulation 辨識
│   │   └── mod.rs
│   ├── power_rating/                   # Power Rating 查表
│   │   ├── mod.rs
│   │   └── table.rs                    # Neely 書裡的 power rating 表(寫死)
│   ├── fibonacci/                      # Fibonacci 子模組
│   │   ├── mod.rs
│   │   ├── ratios.rs                   # 比率清單(寫死)
│   │   └── projection.rs               # expected_fib_zones 計算
│   ├── triggers/                       # Invalidation Triggers
│   │   └── mod.rs
│   ├── degree/                         # Degree 詞彙(可能與 shared/ 共用)
│   │   └── mod.rs
│   ├── facts.rs                        # Fact 產生規則
│   ├── output.rs                       # NeelyCoreOutput 組裝
│   └── config.rs                       # NeelyEngineConfig
└── tests/
    ├── unit/
    ├── golden/                         # 對照 Neely 書中經典範例
    └── benchmark/                      # P0 Gate 五檔股票實測
```

### 3.1 模組職責切分意圖

- **monowave** 負責「把 OHLC 切成 monowave 序列」,是後續所有處理的基礎
- **candidates** 把 monowave 序列窮舉成所有可能的「波浪結構候選」
- **validator** 用 Neely 規則篩選候選
- **classifier** 給通過 validator 的結構命名(Impulse / Diagonal / Flat / Zigzag / Triangle / Combination)
- **compaction** 把多種解讀路徑窮舉成 Forest
- **power_rating / fibonacci** 是 Neely 內建子模組,輸出附在每個 Scenario

---

## 四、Neely 規則邊界

### 4.1 進 Neely Core 的條件(三條全中)

1. **Neely 書裡明確記載** — 有頁碼、有書中段落引用
2. **Neely 體系內建** — 是 Neely 派專屬,非通用結構分析
3. **不需主觀判斷** — 規則化、可重現

### 4.2 進 Neely Core 的清單

| 模組 | v1.1 Item | 進 Neely Core? | 備註 |
|---|---|---|---|
| Monowave Detection (Pure Close + ATR) | Item 1.1-1.4 | ✅ | Neely 原書方法 |
| Rule of Proportion (45° + ATR) | Item 1.2 | ✅ | Neely 原書 |
| Rule of Neutrality (水平段判定) | Item 1.3 | ✅ | Neely 原書 |
| Bottom-up Candidate Generator | Item 2 | ✅ | Neely 體系內建窮舉邏輯 |
| Validator R1-R7 / F1-F2 / Z1-Z4 / T1-T10 / W1-W2 | Item 3 | ✅ | Neely 硬規則 |
| Classifier (Flat / Triangle / Combination 子類型) | Item 4-6 | ✅ | Neely 決策樹 |
| Post-Constructive Validator | Item 7B | ✅ | Neely 「型態完成必要條件」 |
| Complexity Rule | Item 8.1 | ✅ | Neely Complexity Level |
| Compaction (純結構壓縮,產出 Forest) | Item 8 部分 | ✅ | **重寫**:去除「貪心選分數」 |
| Missing Wave 偵測 | Item 9 | ✅ | Neely 原書 fallback |
| Emulation 辨識 | Item 10 | ✅ | Neely 原書 |
| Power Rating (查表) | Item 7 子模組 | ✅ | Neely 書裡列表 |
| Fibonacci 比率與容差 | (Neely 內建) | ✅ | **Neely 子模組,不獨立成 Core** |

### 4.3 不進 Neely Core 的清單

| 模組 | v1.1 Item | 去哪 | 理由 |
|---|---|---|---|
| Scorer 7 因子加總 | Item 7 | 進 Neely Core,但**不加總** | 拆解後保留事實層 |
| 連續漲跌停合併 | Item 1.5 | TW-Market Core | 台股市場特性,不是 Neely |
| `[TW-MARKET]` Scorer 微調 | Item 7.4 | **棄用** | 主觀加權 |
| 容差 toml 外部化 | (討論) | **棄用** | 誘導偏離原作 |
| Engine_T (傳統派) | Item 13, 17 | Traditional Core(獨立並列) | 不是 Neely 體系 |
| Engine_T + Engine_N 整合公式 | Item 17.5 | **棄用** | 主觀調參 |

### 4.4 Neely 規則寫死原則

- **比率清單**(38.2%、61.8%、100%、161.8% 等)→ Neely 書裡明確列的,**寫死在 Neely Core 常數表**
- **相對 ±4% 容差** → Neely 原意,**寫死**
- **Waterfall Effect ±5% 例外** → Neely 書裡特例,**寫死**

**所有 Neely 規則常數不可外部化、不可調**。要調就是改 Neely Core 的代碼,代表刻意偏離原作,需在 commit 訊息明確標註並附 Neely 書頁追溯。

---

## 五、輸入

| 輸入 | 來源 |
|---|---|
| `OHLCVSeries` | `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd`(經 TW-Market Core 處理) |
| `Timeframe` | 日線 / 週線 / 月線 |
| `NeelyCoreParams` | 見第六章 |

**重要**:Neely Core 吃的是 TW-Market Core 處理過的 OHLC(漲跌停合併、後復權),**不**直接吃 raw OHLC。Neely Core 完全不知道台股的存在。

---

## 六、Params 與 NeelyEngineConfig

### 6.1 設計理念:外部 Params vs 內部 Config

- **NeelyCoreParams**:Workflow toml 可宣告,屬「使用方選擇」
- **NeelyEngineConfig**:Core 內部工程參數,可調但有預設,**不**屬 Neely 規則本身

「Neely 規則」與「執行 Neely 規則所需的工程選擇」**嚴格區分**。

### 6.2 NeelyCoreParams

```rust
pub struct NeelyCoreParams {
    pub timeframe: Timeframe,
    pub engine_config: NeelyEngineConfig,
}
```

### 6.3 NeelyEngineConfig

```rust
pub struct NeelyEngineConfig {
    /// ATR 計算週期,Rule of Proportion / Neutrality / 45° 判定的計量單位
    /// 預設 14,跨 timeframe 統一(技術分析界事實標準)
    pub atr_period: usize,                      // 預設 14

    /// Bottom-up Candidate Generator 的 beam width
    /// 預設 50
    pub beam_width: usize,                      // 預設 50

    /// Forest 上限保護:超過此 size 用 BeamSearchFallback
    /// r3 暫定 1000,P0 五檔實測後校準
    pub forest_max_size: usize,                 // 預設 1000

    /// 單檔 Compaction 逾時(秒)
    /// 預設 60
    pub compaction_timeout_secs: u64,           // 預設 60

    /// Forest 超過 max_size 時的處理策略
    pub overflow_strategy: OverflowStrategy,
}

pub enum OverflowStrategy {
    /// 用 power_rating 排序保留 top-K,並標記 overflow_triggered
    BeamSearchFallback { k: usize },            // 預設 k = 100

    /// 不剪枝(P0 Gate 校準階段使用,生產環境不建議)
    Unbounded,
}
```

### 6.4 NeelyEngineConfig 預設值

```rust
impl Default for NeelyEngineConfig {
    fn default() -> Self {
        Self {
            atr_period: 14,
            beam_width: 50,
            forest_max_size: 1000,
            compaction_timeout_secs: 60,
            overflow_strategy: OverflowStrategy::BeamSearchFallback { k: 100 },
        }
    }
}
```

### 6.5 atr_period = 14 的特別說明

ATR 在 Neely 體系中是 **Rule of Proportion / Neutrality / 45° 判定的計量單位**。Neely 原書未指定 atr_period 具體數值,僅要求 "representative period"。

**為何固定 14**:

1. 14 是技術分析界事實標準,Neely 書中範例多落於此區間,屬「約定俗成的工程慣例」非主觀調參
2. **不做自動校準**:任何「自動找最佳 atr_period」的設計都涉及主觀判準(monowave 數量合理性、噪訊比、回測勝率),違反 v2.0 §1.3、§5.3 原則
3. 跨 timeframe 統一 = 保持 monowave significance 計量單位一致
4. 文件中明確標註「此為工程選擇,非 Neely 規則,改動影響 monowave 切割粒度但不影響規則本身」

### 6.6 Fibonacci tolerance 等 Neely 規則沒有對應 setter

NeelyEngineConfig **僅** `atr_period` / `beam_width` / `forest_max_size` / `compaction_timeout_secs` / `overflow_strategy` 五個工程參數可調。

❌ 不可外部化的 Neely 規則:

- Fibonacci 比率清單(寫死常數)
- 相對 ±4% 容差(寫死)
- Waterfall Effect ±5% 例外(寫死)
- Validator 各規則的硬閾值(寫死)
- Power Rating 查表值(寫死)

要改任一項就是「刻意偏離 Neely 原作」,需在 commit 訊息明確標註,**不可透過設定檔規避**。

---

## 七、內部 Pipeline 階段

Neely Core 的 `compute()` 內部依序執行以下階段:

```
OHLCVSeries
    ↓
[Stage 1] Monowave Detection(Pure Close + ATR)
    ↓
monowave_series: Vec<Monowave>
    ↓
[Stage 2] Rule of Proportion / Neutrality 標註
    ↓
classified_monowaves
    ↓
[Stage 3] Bottom-up Candidate Generator
    ↓
candidates: Vec<WaveCandidate>(可能上千)
    ↓
[Stage 4] Validator(R1-R7 / F1-F2 / Z1-Z4 / T1-T10 / W1-W2)
    ↓
valid_candidates(通常剩 100-300 個)
    ↓
[Stage 5] Classifier(命名 + 子類型)
    ↓
classified_scenarios
    ↓
[Stage 6] Post-Constructive Validator
    ↓
post_validated_scenarios
    ↓
[Stage 7] Complexity Rule 篩選
    ↓
complexity_filtered_scenarios
    ↓
[Stage 8] Compaction(窮舉 Forest)+ Forest 上限保護
    ↓
scenario_forest: Vec<Scenario>
    ↓
[Stage 9] Missing Wave 偵測 + Emulation 辨識(對 Forest 中各 Scenario)
    ↓
augmented_forest
    ↓
[Stage 10] Power Rating 查表 + Fibonacci 投影 + Invalidation Triggers 生成
    ↓
NeelyCoreOutput
```

### 7.1 階段間的失敗處理

- **Stage 1 失敗**(資料不足):回傳 `NeelyCoreOutput { scenario_forest: vec![], diagnostics: ..., insufficient_data: true }`
- **Stage 4 全部 reject**:回傳空 forest + 完整 rejections list
- **Stage 8 Forest 爆量**:套用 `OverflowStrategy::BeamSearchFallback`,保留 top-K
- **Stage 8 逾時**:中斷並回傳 `compaction_timeout = true` + 已處理部分 forest

### 7.2 階段可觀察性

每個階段的耗時、輸入輸出計數、reject 原因皆寫入 `NeelyDiagnostics`,P0 Gate 五檔股票實測階段尤其重要。

---

## 八、Output 結構

```rust
pub struct NeelyCoreOutput {
    // 輸入 metadata
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub data_range: TimeRange,

    // 結構性結果(Forest,不是 Tree;不排序、不選 primary)
    pub scenario_forest: Vec<Scenario>,

    // monowave 序列(供 trendline_core 等下游消費)
    pub monowave_series: Vec<Monowave>,

    // Neely Core 自己的診斷
    pub diagnostics: NeelyDiagnostics,

    // Neely 書頁追溯
    pub rule_book_references: Vec<RuleReference>,

    // 資料充分性
    pub insufficient_data: bool,
}

pub struct NeelyDiagnostics {
    pub monowave_count: usize,
    pub candidate_count: usize,
    pub validator_pass_count: usize,
    pub validator_reject_count: usize,
    pub rejections: Vec<RuleRejection>,         // 含 rule_id, expected, actual, gap, neely_page
    pub forest_size: usize,
    pub compaction_paths: usize,                 // 所有合法壓縮路徑
    pub overflow_triggered: bool,                // Forest 是否爆量觸發 BeamSearchFallback
    pub compaction_timeout: bool,                // Compaction 是否逾時
    pub stage_elapsed_ms: HashMap<String, u64>,  // 各階段耗時
    pub elapsed_ms: u64,                         // 總耗時
    pub peak_memory_mb: u64,                     // 峰值記憶體(P0 Gate 校準用)
}

pub struct RuleRejection {
    pub candidate_id: String,
    pub rule_id: RuleId,                         // 例:R5 / F2 / Z3 / T7
    pub expected: String,                        // 規則要求
    pub actual: String,                          // 實際情況
    pub gap: f64,                                // 偏離量(百分比或絕對值,依規則而定)
    pub neely_page: String,                     // Neely 書頁追溯,例:"p.123"
}

pub struct RuleReference {
    pub rule_id: RuleId,
    pub neely_page: String,
    pub description: String,
}
```

### 8.1 monowave_series 對外暴露

`monowave_series` 是 Neely Core 對外暴露的 raw 結構,供 `trendline_core`(唯一例外)讀取。其他下游 Core 不應消費此欄位。

---

## 九、Scenario Forest 結構

### 9.1 Scenario 完整結構

```rust
pub struct Scenario {
    pub id: String,

    // 結構
    pub wave_tree: WaveNode,
    pub pattern_type: NeelyPatternType,         // Impulse / Diagonal / Zigzag / Flat / Triangle / Combination
    pub structure_label: String,                // 例:"5-3-5 Zigzag in W4 of larger Impulse"
    pub complexity_level: ComplexityLevel,      // Neely Complexity Level

    // Neely 書裡寫死的屬性(查表)
    pub power_rating: PowerRating,              // r3 修正:由 i8 改 enum,避免 power_rating = 99 等無效值
    pub max_retracement: f64,
    pub post_pattern_behavior: PostBehavior,

    // 客觀計數(取代 v1.1 主觀分數)
    pub passed_rules: Vec<RuleId>,
    pub deferred_rules: Vec<RuleId>,
    pub rules_passed_count: usize,
    pub deferred_rules_count: usize,

    // 失效條件(Neely 規則的逆向轉譯)
    pub invalidation_triggers: Vec<Trigger>,

    // Fibonacci 投影區
    pub expected_fib_zones: Vec<FibZone>,

    // 結構性事實 7 維(Item 7 拆解,不加總)
    pub structural_facts: StructuralFacts,
}

pub enum PowerRating {
    StrongBullish,    // +3
    Bullish,          // +2
    SlightBullish,    // +1
    Neutral,          // 0
    SlightBearish,    // -1
    Bearish,          // -2
    StrongBearish,    // -3
}

pub enum NeelyPatternType {
    Impulse,
    Diagonal { sub_kind: DiagonalKind },        // Leading / Ending
    Zigzag { sub_kind: ZigzagKind },            // Single / Double / Triple
    Flat { sub_kind: FlatKind },                // Regular / Expanded / Running
    Triangle { sub_kind: TriangleKind },        // Contracting / Expanding / Limiting
    Combination { sub_kinds: Vec<CombinationKind> },
}

pub struct StructuralFacts {
    pub fibonacci_alignment: FibonacciAlignment,
    pub alternation: AlternationFact,
    pub channeling: ChannelingFact,
    pub time_relationship: TimeRelationship,
    pub volume_alignment: VolumeAlignment,      // 若有 volume 資料
    pub gap_count: usize,
    pub overlap_pattern: OverlapPattern,
}
```

### 9.2 Trigger 結構

```rust
pub struct Trigger {
    pub trigger_type: TriggerType,
    pub on_trigger: OnTriggerAction,            // r3 修正:移除 ReduceProbability,改 WeakenScenario
    pub rule_reference: RuleId,                 // 對應 Neely 規則,例 R5
    pub neely_page: String,
}

pub enum TriggerType {
    PriceBreakBelow(f64),
    PriceBreakAbove(f64),
    TimeExceeds(NaiveDate),
    VolumeAnomaly { z_threshold: f64 },
    OverlapWith { wave_id: String },
}

pub enum OnTriggerAction {
    InvalidateScenario,
    WeakenScenario,                              // 標註該 scenario 進入 deferred,不引入機率語意
    PromoteAlternative { promoted_id: String },
}
```

### 9.3 Forest 不選 primary 的具體實作

- `scenario_forest: Vec<Scenario>` **不**附 `primary: Scenario` 欄位
- 順序**不**反映優先級(可按 `id` 字典序或 `power_rating` 排序顯示,但語意上是平等的)
- Aggregation Layer 可依 `power_rating` 提供 UI 篩選功能,但**不**在 Core 層做選擇

### 9.4 v1.1 → v2.0 結構欄位變動清單

| v1.1 欄位 | v2.0 處理 | 理由 |
|---|---|---|
| `primary: Scenario` | 移除 | 不選最優 |
| `alternatives: Vec<Scenario>` | 改為 `scenario_forest` | 平等並列 |
| `confidence: f64` | 移除 | 機率語意 |
| `composite_score: f64` | 移除 | 主觀加權 |
| `scorer_factors: ...` | 拆解為 `structural_facts` | 拆解後保留事實層,不加總 |
| `Trigger.on_trigger.ReduceProbability` | 改 `WeakenScenario` | 與 §2.2 不使用 probability 矛盾 |
| `neely_power_rating: i8` | 改 `enum PowerRating` | 避免 99 等無效值(r3) |

---

## 十、Validator 規則組

### 10.1 規則分組

| 規則組 | 適用 | 規則數 | 來源 |
|---|---|---|---|
| **R1-R7** | 通用核心規則 | 7 | Neely 書 |
| **F1-F2** | Flat 子規則 | 2 | Neely 書 |
| **Z1-Z4** | Zigzag 子規則 | 4 | Neely 書 |
| **T1-T10** | Triangle 子規則 | 10 | Neely 書 |
| **W1-W2** | Wave 通用規則 | 2 | Neely 書 |

每條規則的具體內容(門檻、容差、書頁)在 P0 開發時逐條建檔於 `validator/*.rs` 註解中,並對照 Neely 書頁。

### 10.2 規則執行順序

```
candidate
   ↓
通用核心規則 R1-R7(全部須過)
   ↓
若 candidate 是 Flat → 套 F1-F2
若 candidate 是 Zigzag → 套 Z1-Z4
若 candidate 是 Triangle → 套 T1-T10
   ↓
通用波浪規則 W1-W2(全部須過)
   ↓
通過 → 進入 valid_candidates
不通過 → 寫入 RuleRejection,附 rule_id / expected / actual / gap / neely_page
```

### 10.3 Deferred 規則處理

部分規則需「等到後續 K 棒出現才能驗證」(例:某型態完成後預期接續的 wave 行為)。處理方式:

- 暫時通過,但記錄 `deferred_rules: Vec<RuleId>`
- 寫入 Scenario 的 `deferred_rules` 欄位
- Aggregation Layer 對使用者明示「此 scenario 有 N 條規則待驗證」
- **重要**:所有 deferred rules 必須 resolved(通過或拒絕)才可觸發 Compaction

### 10.4 容差規範

- **相對 ±4% 容差**(Neely 原意)— 所有比例比較使用相對偏移
- **Waterfall Effect ±5% 例外** — Neely 書裡特例
- **不**接受絕對偏移容差(v1.1 已棄用)

```
✅ 規則:W3 至少為 W1 的 100%
✅ 容差:W3 / W1 ≥ 0.96(相對 -4%)
❌ 容差:W3 - W1 ≥ -X(絕對)
```

---

## 十一、Compaction 重新定位

### 11.1 與 v1.1 的差異

| 項目 | v1.1 | v2.0 |
|---|---|---|
| 輸出 | 單一壓縮後最優樹 | Scenario Forest(多棵並列) |
| Threshold 角色 | 分數低於 0.3 就丟 | 不再用分數篩選,只用 Neely 規則篩選 |
| 「選擇邏輯」 | Compaction 內建 | **完全移除**(工具僅作輔助判讀) |

### 11.2 v2.0 Compaction 核心定位

> **Neely Core 的 Compaction 不做「選擇」,只做「窮舉所有合法的壓縮路徑」**

最後產出的不是「一棵壓縮後的最優樹」,而是「**所有可能的壓縮樹組成的 Forest**」。Forest 的每棵樹都是 Neely 規則允許的合法解讀。

### 11.3 保留的 Neely 規則(進入 Compaction)

- **Complexity Rule**(差距 ≤ 1 級)— Neely 書裡明確規則
- **Deferred 約束**(所有 deferred 必須 resolved 才可觸發 Compaction)— v1.1 已有,保留

### 11.4 移除的 v1.1 內容

- ❌ Compaction Threshold(預設 0.3)
- ❌ 貪心選最高分
- ❌ Backtrack 選次高分
- ❌ 任何基於 score 的剪枝

---

## 十二、Forest 上限保護機制

### 12.1 工程現實衝突

第七章「窮舉所有壓縮路徑」與工程現實衝突:某些股票的解讀可能上千棵 Forest,記憶體與時間都會爆炸。

### 12.2 護欄機制

在 `NeelyEngineConfig` 增加 Forest 上限與逾時參數,**保留窮舉精神,但設工程護欄**:

```rust
pub struct NeelyEngineConfig {
    pub forest_max_size: usize,             // r3 暫定 1000
    pub compaction_timeout_secs: u64,       // r3 暫定 60
    pub overflow_strategy: OverflowStrategy,
}

pub enum OverflowStrategy {
    /// Forest 超過 max_size 時,用 power_rating 排序保留 top-K
    /// 並在 NeelyDiagnostics.overflow_triggered = true
    BeamSearchFallback { k: usize },        // 預設 k = 100

    /// 不剪枝(P0 Gate 校準階段使用)
    Unbounded,
}
```

### 12.3 護欄執行流程

```
Compaction 執行中
   ↓
forest 累積 → 持續監控 size 與 elapsed
   ↓
若 size > forest_max_size:
   套用 overflow_strategy
   - BeamSearchFallback { k }:依 power_rating 排序保留 top-K,標記 overflow_triggered
   - Unbounded:繼續累積(僅 P0 Gate 用)
   ↓
若 elapsed > compaction_timeout_secs:
   中斷,回傳已處理部分,標記 compaction_timeout = true
   ↓
所有拒絕原因寫入 NeelyDiagnostics.rejections(完整保留)
```

### 12.4 r3 預設值理由

- `forest_max_size = 1000` — 保守佔位,P0 五檔股票實測後校準
- `compaction_timeout_secs = 60` — 保守佔位
- `BeamSearchFallback { k = 100 }` — 100 棵已遠超人類能消化的解讀數

### 12.5 BeamSearchFallback k 值上界

**警告**:k 值不應無限放大。

- 若 P95 真的超過 1000 而需強化截斷,應**回頭重審 Compaction 演算法本身**,而不是繼續加大 k 值
- 持續加大 k 值代表演算法本身有設計問題,需根本性重構

---

## 十三、Power Rating 截斷哲學論證

### 13.1 可能質疑

> 「用 power_rating 排序後砍低分,等於下了主觀判斷」

### 13.2 回應

#### 13.2.1 power_rating 不是主觀分數

- power_rating 是 **Neely 書裡寫死的查表值**(§5.2 表格 / §5.4 NeelyCoreOutput 已決策)
- 是 Neely 自己定義的「型態強度等級」,**不**是 Pipeline 計算的主觀分數
- 任何 scenario 的 power_rating 都是書中查表得出,與 Pipeline 算法無關

#### 13.2.2 截斷不是排序展示

- 截斷是 **Core 內部資源管理**(避免 OOM)
- Aggregation Layer 看到的仍是「forest 已是 Neely 規則允許的合法解讀」
- Aggregation 層仍**不做加權整合**,使用者看到的是「Top K 都是合法解讀,只是因系統資源限制只呈現這 K 個」

#### 13.2.3 截斷必須可觀察

- `NeelyDiagnostics.overflow_triggered = true` 時,Aggregation Layer **必須**將此狀態傳給前端
- 前端顯示「此股結構過於複雜,系統呈現 Top K 解讀」橫幅
- 使用者**知情**,可選擇相信或不相信

### 13.3 為何不退回 Unbounded

- Unbounded 模式 P95 可能 OOM 或超時,生產環境不可接受
- v2.0 §1.3 哲學「不替使用者選擇」 ≠ 「無視工程現實」
- Forest 上限保護是「在窮舉精神 + 工程可行性」之間的合理折衷

---

## 十四、Fibonacci 子模組

### 14.1 Fibonacci 不獨立成 Core

Fibonacci 比率與容差屬 **Neely Core 內部子模組**,**不獨立成 Core**。

### 14.2 為何不獨立

- Fibonacci 投影強依賴 Neely Core 的 wave 結構(從 W4 終點到 W5 投影、ABC 修正的回撤位等)
- 離開波浪結構單獨計算 Fibonacci 沒有意義
- 統一在 Neely Core 內,避免散布造成歸屬不一致(v1.1 spec 在 §5.2 / §14.2.3 / §8.2 三處歸屬不一致,r3 已統一)

### 14.3 fibonacci 子模組職責

```rust
// neely_core/src/fibonacci/

pub struct FibonacciSubmodule;

impl FibonacciSubmodule {
    /// 從 Scenario 的 wave 結構計算預期 Fibonacci 投影區
    pub fn project_zones(scenario: &Scenario) -> Vec<FibZone> { ... }
}

pub struct FibZone {
    pub zone_kind: FibZoneKind,                 // Retracement / Extension / Projection
    pub ratio: f64,                              // 38.2 / 50.0 / 61.8 / 100.0 / 161.8 / ...
    pub price_low: f64,                         // 含 ±4% 容差後的下限
    pub price_high: f64,                        // 含 ±4% 容差後的上限
    pub source_wave: String,                    // 從哪一段 wave 投影出來
}
```

### 14.4 比率清單寫死

```rust
// neely_core/src/fibonacci/ratios.rs

pub const FIB_RATIOS: &[f64] = &[
    0.236, 0.382, 0.500, 0.618, 0.786,          // 回撤
    1.000, 1.272, 1.618, 2.000, 2.618,          // 延伸
    // ... Neely 書裡明確列的比率
];

pub const FIB_TOLERANCE_RELATIVE: f64 = 0.04;   // ±4% 相對容差
pub const WATERFALL_TOLERANCE: f64 = 0.05;      // Waterfall Effect ±5% 例外
```

### 14.5 expected_fib_zones 在 Scenario 中的位置

每個 Scenario 都附 `expected_fib_zones: Vec<FibZone>`,表示「依此 scenario 的 wave 結構,預期下一段價位會走到哪些 Fibonacci 投影區」。

### 14.6 fib_zones 寫入 structural_snapshots

若以 `core_name='fib_zones'` 寫入 `structural_snapshots`,該 row **必須**附:

- `derived_from_core = 'neely_core'`
- 對應的 `snapshot_date`

確保即使從 snapshot 資料看,也能追溯到原始 Neely Core 計算。

### 14.7 Fibonacci 投影視圖屬資料整理層

從 Neely scenario 投影為 `fib_zones` snapshot 的視圖,屬「**資料整理**」非「**計算**」,放在 Aggregation Layer 不違反「並排不整合」原則。

---

## 十五、Fact 產出規則

### 15.1 Fact 的時機

每個 scenario 在進入 forest 時產出對應 Fact。每日 batch 重算後,新出現的 scenario 對應產出新 Fact;消失的 scenario 對應舊 Fact 自動標記為失效(透過 invalidation_triggers 觸發)。

### 15.2 Fact 範例

| Fact statement | metadata |
|---|---|
| `Neely Impulse 5-wave detected with power_rating=Bullish` | `{ pattern: "impulse", power_rating: "bullish", scenario_id: "..." }` |
| `Neely Zigzag in W4 of larger Impulse, currently at B wave` | `{ pattern: "zigzag", current_position: "wave_b" }` |
| `Neely Triangle Contracting, 3 of 5 sub-waves completed` | `{ pattern: "triangle_contracting", sub_waves_completed: 3 }` |
| `Neely Flat Expanded forming, deferred rule W2 pending verification` | `{ pattern: "flat_expanded", deferred_rules: ["W2"] }` |
| `Neely scenario forest size=42, overflow_triggered=false` | `{ event: "forest_summary", size: 42 }` |
| `Neely scenario invalidated: price broke below 456.1 on 2026-04-28` | `{ event: "scenario_invalidated", broken_price: 456.1, scenario_id: "..." }` |

### 15.3 Fact statement 命名衝突防範

Neely Core 與 Traditional Core 都會產出「波浪相關 Fact」,**統一在 statement 開頭加標籤**:

```
✅ "Neely Impulse 5-wave detected with power_rating=Bullish"
✅ "Traditional(Frost) impulse 5-wave completed, target 700-800"
❌ "Impulse 5-wave detected"   // 不知道是哪派
```

### 15.4 不入 Fact 表的內容

- 每個 Scenario 的完整結構 → 寫 `structural_snapshots`(JSONB),**不**寫 facts
- monowave 序列 → 寫 `structural_snapshots`,**不**寫 facts
- diagnostics → 寫 `structural_snapshots`,**不**寫 facts

只有「事件型」的事實寫 facts(scenario 出現 / 消失 / 失效、forest 摘要等)。

---

## 十六、warmup_periods

Neely Core 屬**結構性指標**,每日全量重算。但仍宣告所需歷史資料量:

```rust
fn warmup_periods(&self, params: &NeelyCoreParams) -> usize {
    match params.timeframe {
        Timeframe::Daily => 500,    // ~2 年日線(完整 Impulse + Correction 至少需此量)
        Timeframe::Weekly => 250,   // ~5 年週線
        Timeframe::Monthly => 120,  // ~10 年月線
    }
}
```

實際窗口大小依 P0 Gate 五檔股票實測校準。

### 16.1 為何需要這麼多歷史

Neely 體系判斷 W5 是否完成、ABC 是否在更大 Impulse 的 W4 中等,都需追溯至更大 degree 的歷史。歷史資料不足會導致大量 candidate 被 reject(實際上是「無法判斷」而非「規則不符」)。

P0 Gate 五檔股票實測時應記錄「資料不足」的拒絕比例,作為 warmup_periods 校準依據。

---

## 十七、對應資料表

| 用途 | 資料表 |
|---|---|
| 輸入 OHLC | `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd`(經 TW-Market Core 處理) |
| 寫入結構快照 | `structural_snapshots`,`core_name = 'neely_core'` |
| 寫入 Fibonacci 投影視圖(可選) | `structural_snapshots`,`core_name = 'fib_zones'` + `derived_from_core = 'neely_core'` |
| 寫入 Fact | `facts`,`source_core = 'neely_core'` |

### 17.1 structural_snapshots JSONB 範例

```json
{
  "stock_id": "3363",
  "snapshot_date": "2026-04-30",
  "core_name": "neely_core",
  "source_version": "1.0.0",
  "params_hash": "a3f8b2c1d4e5f6a7",
  "snapshot": {
    "scenario_forest": [
      {
        "id": "S001",
        "pattern_type": "impulse",
        "power_rating": "bullish",
        "rules_passed_count": 18,
        ...
      },
      ...
    ],
    "monowave_count": 245,
    "forest_size": 42,
    "overflow_triggered": false
  }
}
```

---

## 十八、診斷與可觀察性

### 18.1 NeelyDiagnostics 完整保留拒絕原因

每個被 Validator 拒絕的 candidate 都記錄在 `rejections`,**附**:

- `rule_id`(哪條規則)
- `expected`(規則要求)
- `actual`(實際情況)
- `gap`(偏離量)
- `neely_page`(書頁追溯)

### 18.2 P0 Gate 階段的觀察重點

P0 Gate 五檔股票實測階段必須記錄:

1. **forest_size 分布** — P50 / P95 / P99 / max
2. **compaction_paths 分布** — 同上
3. **elapsed_ms 分布** — 同上
4. **peak_memory_mb 分布** — 同上
5. **overflow_triggered 比例** — 多少檔 / 多少時間點觸發
6. **compaction_timeout 比例** — 多少檔逾時
7. **資料不足拒絕比例** — 校準 warmup_periods

### 18.3 紅燈條件

P0 Gate 結果若出現以下情況之一,**回頭重議 Compaction 演算法**:

- forest_size P95 > 1000
- elapsed_ms P95 > 30 秒
- peak_memory_mb P95 > 1 GB
- overflow_triggered 比例 > 20%

不可繼續加大 `forest_max_size` 或 `BeamSearchFallback.k` 規避(§12.5)。

### 18.4 前端可觀察性串接

`NeelyDiagnostics.overflow_triggered = true` 必須傳到前端,顯示「此股結構過於複雜,系統呈現 Top K 解讀」橫幅。使用者知情,不可隱藏。

---

## 十九、P0 Gate:五檔股票實測

### 19.1 Gate 範圍

P0 完成後,執行五檔股票實測,校準 NeelyEngineConfig 預設值。

**實測股票**:`0050 / 2330 / 3363 / 6547 / 1312`

涵蓋:大型成熟股、半導體龍頭、上漲趨勢股、震盪整理股、傳產類股,各類市場結構皆有代表。

### 19.2 實測流程

1. 取每檔股票完整歷史(自上市以來)
2. 用「v2.0 模式 = Compaction 不剪枝、`OverflowStrategy::Unbounded`」執行 Neely Core
3. 紀錄每檔的 forest_size、compaction_paths 數量、elapsed 秒數、peak memory MB
4. 各時間框架(日 / 週 / 月)分別測試
5. 結果寫入 `docs/benchmarks/p0_gate_results.md`

### 19.3 校準產出

依 P95 結果校準:

- `forest_max_size` 預設值
- `compaction_timeout_secs` 預設值
- `BeamSearchFallback.k` 預設值

### 19.4 Gate 通過條件

綠燈(全通過):

- forest_size P95 ≤ 1000
- elapsed_ms P95 ≤ 30 秒
- peak_memory_mb P95 ≤ 1 GB
- 五檔股票皆能產出至少一個 Scenario(無資料不足)

紅燈條件見 §18.3。Gate 通過才開始 P1 開發。

---

## 二十、已棄用的設計

| 棄用項 | 來源 | 棄用原因 |
|---|---|---|
| Compaction 貪心選最高分 + backtrack | v1.1 | 違反輔助判讀原則 |
| `composite_score` 加總 | v1.1 Item 7 | 主觀加權 |
| `confidence` 機率語意 | v1.1 | 違反 §2.2 不使用 probability |
| Compaction Threshold 0.3 剪枝 | v1.1 | 改用 Neely 規則篩選 |
| 容差 toml 外部化 | 早期討論 | 誘導偏離原作 |
| Compaction 完全不剪枝 | r2 §7.2 | 工程現實下 OOM,改為「窮舉但有 forest_max_size 護欄」 |
| `[TW-MARKET]` Scorer 微調 | v1.1 Item 7.4 | 主觀加權,違反「忠於原作」 |
| 漲跌停處理嵌在 Neely Engine | v1.1 Item 1.5 | 違反單一職責,移至 TW-Market Core |
| `Trigger.on_trigger.ReduceProbability` | r2 §5.4 | 與 §2.2 不使用 probability 矛盾,改 `WeakenScenario` |
| `neely_power_rating: i8` | r2 §5.4 | 改 `enum PowerRating`,避免 99 等無效值 |
| Scorer 7 因子加權加總 | v1.1 Item 7 | 進 Neely Core 但**不加總**,拆解為 `structural_facts` |
| Engine_T + Engine_N 整合公式 | v1.1 Item 17.5 | 主觀調參,Traditional Core 改為獨立並列 |
| 自動校準 atr_period | (討論) | 校準準則(monowave 數量合理性等)皆主觀,違反 §1.3 |
| Fibonacci 獨立 Core | r2 §14.2.3 / §15.1 隱含 | 確認為 Neely Core 子模組 |

---

## 二十一、與其他 Core 的關係

### 21.1 上游

- **TW-Market Core**(P0,前置)— Neely Core 吃 TW-Market Core 處理過的 OHLC

### 21.2 下游(Aggregation Layer)

Neely Core 的 Forest 與其他所有 Core 的事實**並排呈現**:

```
Neely Core Forest
   +
Traditional Core Forest(P3 後加入)
   +
Indicator Cores Facts
   +
Chip Cores Facts
   +
Fundamental Cores Facts
   +
Environment Cores Facts
   ↓
Aggregation Layer 並排呈現
   ↓
使用者自己連線
```

### 21.3 唯一允許的耦合:trendline_core

`trendline_core`(P2)是**全 Core 系統中唯一允許消費 Neely Core 輸出的例外**:

- 僅讀 `monowave_series`(不讀 `scenario_forest`)
- `trendline_core/Cargo.toml` 明確宣告 `depends_on = ["neely_core"]`
- 在 V2 spec 列入「已知耦合」清單

詳細管控規則見 `cores_overview.md` §12 與 `indicator_cores_pattern.md` §5。

### 21.4 與 Traditional Core 的關係

- Neely Core 與 Traditional Core **獨立並列**,**不整合**
- 兩者吃同一份 TW-Market Core 處理過的 OHLC
- 兩者各自輸出 Forest,Aggregation Layer 並排呈現
- v1.1 的 Combined confidence 整合公式已棄用

詳見 `traditional_core.md` §8。

### 21.5 ATR 的雙重身份

Neely Core 內嵌 ATR 計算(`NeelyEngineConfig.atr_period = 14`),用於 Rule of Proportion / Neutrality / 45° 判定。

`atr_core` 是獨立 Indicator Core,對外輸出 ATR 值與相關 Fact。

**兩者不互相 import**:

- Neely Core 不依賴 `atr_core`(計算邏輯內嵌)
- `atr_core` 不依賴 Neely Core(對外服務)
- 兩者數值相同但實作獨立,維持零耦合

詳見 `indicator_cores_volatility.md` §2.4。

---

## 附錄 A:Neely 書頁追溯規範

每條規則的代碼註解必須附 Neely 書頁追溯:

```rust
/// Validator R5: Wave 4 不能與 Wave 1 重疊(在 Impulse 中)
/// Neely p.123-125
/// 容差:相對 ±4%
fn rule_r5(&self, candidate: &WaveCandidate) -> RuleResult { ... }
```

`RuleRejection` 的 `neely_page` 欄位由此註解的書頁取出,確保使用者可回溯原作驗證。

---

## 附錄 B:Power Rating 查表(待 P0 開發補完)

```rust
// neely_core/src/power_rating/table.rs

pub fn lookup(pattern: &NeelyPatternType, position: WavePosition) -> PowerRating {
    match (pattern, position) {
        (NeelyPatternType::Impulse, WavePosition::EndOfW3) => PowerRating::StrongBullish,
        (NeelyPatternType::Impulse, WavePosition::EndOfW5) => PowerRating::SlightBearish,
        (NeelyPatternType::Zigzag { .. }, WavePosition::EndOfC) => PowerRating::Bullish,
        // ... 完整對照表 P0 開發時補完,所有條目附 Neely 書頁
        _ => PowerRating::Neutral,
    }
}
```

具體 power rating 對照表(每個 pattern × 每個 position)在 P0 開發時逐條建檔,**附 Neely 書頁**。
