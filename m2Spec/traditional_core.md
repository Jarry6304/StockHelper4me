# Traditional Core 規格

> **版本**:v2.0 抽出版 r2(規則補完版)
> **日期**:2026-04-30
> **配套文件**:`cores_overview.md`(共通規範)
> **規則來源**:Frost & Prechter《Elliott Wave Principle》(1978, 10th ed.)、Sid Norris《Wave Notes》、Ramki《Five Waves to Financial Freedom》
> **優先級**:P3
> **狀態**:**第二版範圍,P0 / P1 / P2 階段不開發**

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
11. [**附錄 A:Hard Rules 規則表(剪枝條件)**](#附錄-ahard-rules-規則表剪枝條件)
12. [**附錄 B:Pattern 結構定義(來自書本)**](#附錄-bpattern-結構定義來自書本)
13. [**附錄 C:Fibonacci 理想比率表**](#附錄-cfibonacci-理想比率表)
14. [**附錄 D:Guidelines(教義性指引,非剪枝)**](#附錄-dguidelines教義性指引非剪枝)
15. [**附錄 E:書頁追溯表**](#附錄-e書頁追溯表)

---

## 一、定位

**Traditional Core** 處理傳統派波浪規則(Frost / Prechter / Ramki 體系),與 Neely Core **獨立並列**,兩者不整合。

### 1.1 設計意圖

- 為使用者提供傳統派與 Neely 派**並排對照**的解讀
- 兩派輸出 scenario forest 後,由 Aggregation Layer 並排呈現,**使用者自己連結**
- Pipeline 不做仲裁、不做加權、不選 primary
- **規則完全來自書本**,Core 不發明規則、不引入未在書中出現的閾值

### 1.2 必要規範

- 屬 Wave Core,**不**走 `IndicatorCore` trait
- 與 Neely Core 共用 `WaveCore` trait(草案,P3 確定)
- 輸出 scenario forest 結構與 Neely Core 同形(欄位可能不同),便於 Aggregation Layer 並排處理

### 1.3 「去人為判斷」原則(本版補完核心)

本 Core 嚴格區分兩類規則:

| 類別 | 處理方式 | 對 scenario 的影響 |
|---|---|---|
| **Hard Rules**(書本明示為「必要條件」/ Rule) | Validator 剪枝 | 違反 → 整個 scenario 拒絕,不進 forest |
| **Guidelines**(書本標示為「常見」/「通常」/「Tendency」) | 客觀計數,以 Fact 形式記錄 | **不**參與排序、**不**加權、**不**算分 |

**禁止行為**:
- ❌ Scorer 加權加總(已棄用,見第九章)
- ❌ Confidence 數值產出(已棄用)
- ❌ 「兩個 guideline 命中比一個命中更可信」這類主觀邏輯
- ❌ 任何書中未明示的閾值(若書本有區間,取書本下界作為 Hard Rule、上界作為 guideline 觀察點,而非自行折衷)

---

## 二、輸入

| 輸入 | 說明 |
|---|---|
| `OHLCVSeries` | 經 TW-Market Core 前處理後的 OHLC(漲跌停合併、還原) |
| `Timeframe` | 日線 / 週線 / 月線 |
| `Params` | 見第三章 |

**重要**:Traditional Core 與 Neely Core 一樣,吃的是 TW-Market Core 處理過的 OHLC,**不**直接吃 raw OHLC。

---

## 三、Params

```rust
pub struct TraditionalCoreParams {
    pub atr_period: usize,           // 工程參數,預設 14(僅用於 pivot 偵測,不影響規則判定)
    pub school: TraditionalSchool,   // 規則來源
    pub timeframe: Timeframe,
}

pub enum TraditionalSchool {
    Frost,      // Frost & Prechter (1978) Elliott Wave Principle 為唯一規則來源
    Ramki,      // Ramki Five Waves to Financial Freedom 為唯一規則來源
    Combined,   // Frost + Ramki 同時跑,各自輸出獨立 forest 並排呈現(P3 後考慮)
}
```

### 3.1 Combined 模式語意

`Combined` **不是**將 Frost 與 Ramki 規則合併成單一規則集。它代表:

```
Combined 模式輸出 = Frost forest ∪ Ramki forest(union,不去重)
```

兩派規則互不干擾。同一筆 OHLC 跑兩次,各自獨立剪枝、獨立計數,在 Output 中以 `school` 欄位區分。

### 3.2 已棄用 Params

| 棄用項 | 棄用原因 |
|---|---|
| `tolerance.toml` 外部化容差 | 容差外部化 = 把書本沒寫的閾值留給人調 = 人為判斷 |
| `Scorer` 7 因子 / 9 因子加權加總 | 加權本身即主觀。Guidelines 只計數,不打分 |
| `confidence: f64` 輸出 | 機率語意違反「規則式邊界」(見 cores_overview §2.2) |
| `primary` / `alternatives` 排序 | Forest 不選 primary,排序權交還使用者 |

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
    pub diagnostics: TraditionalDiagnostics,

    // 規則書頁追溯
    pub rule_book_references: Vec<RuleReference>,
}

pub struct TraditionalDiagnostics {
    pub pivot_count: usize,
    pub candidate_count: usize,
    pub validator_pass_count: usize,
    pub validator_reject_count: usize,
    pub rejections: Vec<RuleRejection>,
    pub elapsed_ms: u64,
}

pub struct TraditionalScenario {
    pub id: String,
    pub school: TraditionalSchool,        // Frost or Ramki(Combined 模式下兩派各自獨立 scenario)

    // 結構
    pub wave_tree: WaveNode,
    pub pattern_type: TraditionalPatternType,
    pub structure_label: String,           // e.g. "5-3-5-3-5", "3-3-5"

    // 失效條件(來自書本 Hard Rules,非主觀停損)
    pub invalidation_triggers: Vec<Trigger>,

    // 客觀計數(Guidelines 命中,僅記錄,不加權)
    pub passed_rules: Vec<RuleId>,         // Hard Rules 通過的清單
    pub deferred_rules: Vec<RuleId>,       // 因 wave 未完成而暫無法驗證的 Hard Rules
    pub matched_guidelines: Vec<GuidelineId>,  // Guidelines 命中清單(命中 = 書中描述的「常見」現象出現)
    pub rules_passed_count: usize,
    pub guidelines_matched_count: usize,   // 僅作 Fact 描述用,不影響 forest 排序

    // Fibonacci 投影區(來自附錄 C 的書本比率,非自行設計)
    pub expected_fib_zones: Vec<FibZone>,
}

pub enum TraditionalPatternType {
    // Motive(推動浪)
    Impulse,                  // 5 浪(Frost Ch.1, Ramki Ch.3)
    LeadingDiagonal,          // 楔形 1 浪(Frost Ch.1)
    EndingDiagonal,           // 楔形 5 浪(Frost Ch.1)
    ExpandingDiagonal,        // 擴張楔形(Ramki: 罕見變體)

    // Corrective(修正浪)— Simple
    Zigzag,                   // 5-3-5(Frost Ch.2)
    DoubleZigzag,             // W-X-Y, 兩個 zigzag(Frost Ch.2)
    TripleZigzag,             // W-X-Y-X-Z(Frost Ch.2)
    RegularFlat,              // 3-3-5, B≈A(Frost Ch.2)
    ExpandedFlat,             // 3-3-5, B>A(Frost Ch.2)
    RunningFlat,              // 3-3-5, B>A & C 未過 A 端點(Frost Ch.2)
    ContractingTriangle,      // 3-3-3-3-3, 收斂(Frost Ch.2)
    BarrierTriangle,          // 3-3-3-3-3, 一邊水平(Frost Ch.2)
    ExpandingTriangle,        // 3-3-3-3-3, 擴張(Frost Ch.2)

    // Corrective — Combination
    DoubleThree,              // W-X-Y, 兩個 simple correction(Frost Ch.2)
    TripleThree,              // W-X-Y-X-Z(Frost Ch.2)
}
```

### 4.1 與 Neely Core Scenario 的差異

| 欄位 | Neely Core | Traditional Core |
|---|---|---|
| `power_rating` | ✅ Neely 書裡查表(Ch.7) | ❌ Frost / Ramki 無對應概念 |
| `max_retracement` | ✅ Neely 書裡查表 | ❌ 不適用 |
| `post_pattern_behavior` | ✅ Neely 書裡查表 | ❌ 不適用 |
| `structural_facts` 7 維 | ✅ | ⚠️ 部分適用(Fibonacci / Alternation / Channeling / Personality / Volume / Equality / Extension) |
| `passed_rules` / `matched_guidelines` 分離 | 視 Neely Core 設計 | ✅ 必須分離 |

### 4.2 並排呈現原則

Aggregation Layer 將 Neely Forest 與 Traditional Forest **並排呈現**,**不**做共識比對、不加總、不打分。

---

## 五、Fact 產出規則

每個 scenario 在進入 forest 時產出對應 Fact。**Guideline 命中也產出 Fact,但作為「客觀觀察」而非「信心提升」**。

### 5.1 Scenario 級 Fact

| Fact 範例 | metadata |
|---|---|
| `Traditional(Frost) impulse 5-wave completed at 2026-03-19` | `{ school: "frost", pattern: "impulse", w5_end_date: "2026-03-19", w5_end_price: 640.0 }` |
| `Traditional(Frost) ABC corrective in progress, current at B wave` | `{ school: "frost", pattern: "abc", current_wave: "B" }` |
| `Traditional(Ramki) zigzag completed, expecting flat correction by alternation` | `{ school: "ramki", pattern: "zigzag_completed", alternation_implies: "flat" }` |
| `Traditional(Frost) wave 3 invalidation: W3 < min(W1, W5) at price 488.0` | `{ school: "frost", rule_id: "R03", trigger_price: 488.0 }` |

### 5.2 Guideline 級 Fact(新增)

Guideline 命中時產出獨立 Fact,**不**附 confidence 欄位:

| Fact 範例 | metadata |
|---|---|
| `Traditional(Frost) guideline matched: W2 retracement at 0.382 (high-frequency zone)` | `{ school: "frost", guideline_id: "G_W2_FIB_382", actual_ratio: 0.385 }` |
| `Traditional(Frost) guideline matched: alternation between W2 (sharp) and W4 (sideways)` | `{ school: "frost", guideline_id: "G_ALTERNATION_FORM", w2_form: "sharp", w4_form: "sideways" }` |
| `Traditional(Frost) guideline matched: W3 is the longest among W1/W3/W5` | `{ school: "frost", guideline_id: "G_W3_LONGEST" }` |
| `Traditional(Ramki) guideline matched: Flat C/A ratio at 1.382 (most common per Ramki)` | `{ school: "ramki", guideline_id: "G_FLAT_C_A_138", actual_ratio: 1.379 }` |

### 5.3 Fact 命名規則

統一在 `statement` 開頭加 `Traditional(school)` 標籤。Guidelines 一律以 `guideline matched:` 開頭,Hard Rules 違反以 `invalidation:` 開頭。這讓下游能分辨「規則必然性」與「教義常見性」。

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

實際窗口大小依 P3 開發階段調整。

---

## 七、對應資料表

| 用途 | 資料表 |
|---|---|
| 輸入 OHLC | `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd`(經 TW-Market Core 處理) |
| 寫入結構快照 | `structural_snapshots`,`core_name = 'traditional_core'` |
| 寫入 Fact | `facts`,`source_core = 'traditional_core'` |

---

## 八、與 Neely Core 的關係

### 8.1 兩者並列,不整合

```
Raw OHLC
   ↓
TW-Market Core(資料前處理)
   ↓
   ├──→ Neely Core      → Neely Scenario Forest      ┐
   └──→ Traditional Core → Traditional Scenario Forest ┤
                                                       ↓
                                              Aggregation Layer
                                                  並排呈現
                                                  不整合
```

### 8.2 兩者共用什麼

- 共用 TW-Market Core 處理過的 OHLC
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
| 「兩派共識則加分」邏輯 | 早期討論 | 機率語意,違反 2.2 原則 |
| Composite Confidence 9 因子加權(Personality 0.20、Fibonacci 0.15...) | v1.1 §7.11 | 權重數字本身無書本來源,純人為設定 |
| Flat B 浪 90% 加分 +0.15 | v1.1 P01 §7.2 | 加分數字 0.15 為人為設定;改為純 Fact 記錄 |
| W2 雙峰「不做差異化扣分」隱含的扣分邏輯 | v1.1 P02 | 任何「扣分」皆為人為 |

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

### 10.5 規則書頁追溯義務(本版新增)

每條 Hard Rule 與 Guideline 必須在附錄 E 留下書頁追溯。新增規則時:

1. 必須引用 Frost & Prechter 1978(10th ed.)頁碼,或 Ramki Ch.X 章節
2. 若僅有 Sid Norris《Wave Notes》提及,需標注 `[Sid Norris extracted]` 並說明源章
3. 規則文字若與書本原文有差異(例如數值精度),須在追溯表附註原文 vs 實作差異

---

## 附錄 A:Hard Rules 規則表(剪枝條件)

> **使用方式**:Validator 對每個 candidate scenario 逐條檢查。任一條違反 → scenario 立即拒絕,不進 forest。
> **來源原則**:僅收錄書本明示為「Rule」、「Inviolable」、「Cannot」、「Must」的條目。書本標示為「Usually」、「Tendency」、「Often」者一律入附錄 D Guidelines。

### A.1 Impulse / Motive Wave Rules

| RuleId | 規則內容 | 來源 |
|---|---|---|
| **R01** | Wave 2 不得回撤超過 Wave 1 起點 100%(`W2.end > W1.start`,以方向計) | Frost & Prechter Ch.1; Ramki Ch.3 |
| **R02** | Wave 3 不得是 W1/W3/W5 中最短者(以 `price_len` 計;若 W3 為延伸浪,僅檢查 `W3 ≥ min(W1, W5)`) | Frost & Prechter Ch.1; Ramki Ch.3 |
| **R03** | Wave 4 不得進入 Wave 1 的價格區域(`W4.end` 不得超越 `W1.end`,以方向計) | Frost & Prechter Ch.1 |
| **R04** | W1, W3, W5 中**最多只有一個**為延伸浪(extended,price_len ≥ 1.618 × 同浪平均) | Frost & Prechter Ch.1 |
| **R05** | W3 必須有方向上的明確進展,**不得**在時間或價格上短於 W1(若 W1 為延伸,W3 仍至少等於 W2 + 一個小幅推進) | Frost & Prechter Ch.1 |

### A.2 Diagonal Rules

| RuleId | 規則內容 | 來源 |
|---|---|---|
| **R06** | Leading Diagonal 出現於 W1 或 A 浪;Ending Diagonal 出現於 W5 或 C 浪。其他位置出現的「楔形」一律拒絕 | Frost & Prechter Ch.1 |
| **R07** | Diagonal 內部子浪結構:Leading Diagonal 為 5-3-5-3-5;Ending Diagonal 為 3-3-3-3-3 | Frost & Prechter Ch.1 |
| **R08** | Contracting Diagonal:W3 < W1, W5 < W3, W4 < W2(逐浪縮短) | Frost & Prechter Ch.1 |
| **R09** | Expanding Diagonal:W3 > W1, W5 > W3, W4 > W2(逐浪擴大) | Frost & Prechter Ch.1; Ramki(罕見變體) |
| **R10** | Ending Diagonal 中,W4 **必須**進入 W1 區域(這是與 Impulse 的關鍵差別) | Frost & Prechter Ch.1 |

### A.3 Zigzag Rules

| RuleId | 規則內容 | 來源 |
|---|---|---|
| **R11** | Zigzag 內部結構必須為 5-3-5(A=5 浪、B=3 浪、C=5 浪) | Frost & Prechter Ch.2 |
| **R12** | Zigzag 中 Wave B 不得回撤超過 Wave A 起點(`B.end > A.start`,以方向計) | Frost & Prechter Ch.2 |
| **R13** | Zigzag 中 Wave C 必須超越 Wave A 終點(`C.end > A.end`,以方向計) | Frost & Prechter Ch.2 |

### A.4 Flat Rules

| RuleId | 規則內容 | 來源 |
|---|---|---|
| **R14** | Flat 內部結構必須為 3-3-5(A=3 浪、B=3 浪、C=5 浪) | Frost & Prechter Ch.2 |
| **R15** | Flat 中 Wave B 至少回撤 Wave A 的 90%(註:此為 Frost 嚴格門檻;Ramki 派採 61.8% 見 R15-Ramki) | Frost & Prechter Ch.2 |
| **R15-Ramki** | (僅 Ramki school)Flat 中 Wave B 至少回撤 Wave A 的 61.8% | Ramki Ch.4 |
| **R16** | Regular Flat:Wave B ≈ Wave A 終點(回撤 90%~105%);Wave C ≈ Wave A 長度 | Frost & Prechter Ch.2 |
| **R17** | Expanded Flat:Wave B 超越 Wave A 起點(B 端點越過 A 起點,以方向計);Wave C 超越 Wave A 終點 | Frost & Prechter Ch.2 |
| **R18** | Running Flat:Wave B 超越 Wave A 起點;但 Wave C **未超越** Wave A 終點 | Frost & Prechter Ch.2 |

### A.5 Triangle Rules

| RuleId | 規則內容 | 來源 |
|---|---|---|
| **R19** | Triangle 必須由五個子浪組成(A-B-C-D-E),每個子浪皆為 3 浪結構(3-3-3-3-3) | Frost & Prechter Ch.2 |
| **R20** | Contracting Triangle:後續子浪逐次縮小(C<A, D<B, E<C) | Frost & Prechter Ch.2 |
| **R21** | Barrier Triangle:其中一條邊(B-D 或 A-C)為水平 | Frost & Prechter Ch.2 |
| **R22** | Expanding Triangle:後續子浪逐次擴大(C>A, D>B, E>D) | Frost & Prechter Ch.2 |
| **R23** | Triangle 僅出現於 W4、B、X 位置,**不得**出現於 W2 或末端浪 | Frost & Prechter Ch.2 |

### A.6 Combination Rules

| RuleId | 規則內容 | 來源 |
|---|---|---|
| **R24** | Double Three (W-X-Y):W、Y 為 simple correction(zigzag/flat/triangle),X 為連接浪(任一 corrective) | Frost & Prechter Ch.2 |
| **R25** | Triple Three (W-X-Y-X-Z):同 R24,且第二個 X 連接 Y 與 Z | Frost & Prechter Ch.2 |
| **R26** | Combination 內部最多包含一個 Triangle,且 Triangle 必為最後一個 simple correction(Y 或 Z 位置) | Frost & Prechter Ch.2 |
| **R27** | Double Zigzag (5-3-5-3-5-3-5):W、Y 皆為 zigzag。**注意**:這是 Combination 的一個子集,但 Frost 將其與單一 Zigzag 的延伸視為不同型態 | Frost & Prechter Ch.2 |

### A.7 Degree Rules

| RuleId | 規則內容 | 來源 |
|---|---|---|
| **R28** | 子浪 Degree 必須比父浪低一級(不得跳級) | Frost & Prechter Ch.3 |
| **R29** | 同層 Degree 的浪必須在時間與價格幅度上「大致可比」(此為「相對」規則,實作時以 0.1× ~ 10× 區間視為可比;超出此區間視為 Degree 不一致) | Frost & Prechter Ch.3(實作區間取自 Sid Norris extracted) |

> **R29 實作註**:0.1× ~ 10× 區間並非書本明示。Frost 僅描述「應大致可比」。本實作以 Sid Norris 統計觀察為下界,但若 P3 開發階段發現實測偏差,**應放寬而非收緊**(即:不得自行將區間改為 0.5× ~ 2× 等更嚴格的範圍,因為書本未支持)。

### A.8 規則摘要表

```
總計:29 條 Hard Rules
- Motive(R01-R05):5 條
- Diagonal(R06-R10):5 條
- Zigzag(R11-R13):3 條
- Flat(R14-R18, R15-Ramki):6 條
- Triangle(R19-R23):5 條
- Combination(R24-R27):4 條
- Degree(R28-R29):2 條
```

---

## 附錄 B:Pattern 結構定義(來自書本)

### B.1 Pattern 結構標籤

| Pattern | 子浪結構 | 內部 wave 數 |
|---|---|---|
| Impulse | 5-3-5-3-5 | 5 |
| Leading Diagonal | 5-3-5-3-5 | 5 |
| Ending Diagonal | 3-3-3-3-3 | 5 |
| Expanding Diagonal | 3-3-3-3-3 | 5 |
| Zigzag | 5-3-5 | 3 |
| Regular Flat | 3-3-5 | 3 |
| Expanded Flat | 3-3-5 | 3 |
| Running Flat | 3-3-5 | 3 |
| Contracting Triangle | 3-3-3-3-3 | 5 |
| Barrier Triangle | 3-3-3-3-3 | 5 |
| Expanding Triangle | 3-3-3-3-3 | 5 |
| Double Zigzag | (5-3-5)-(3)-(5-3-5) | 7 |
| Triple Zigzag | 11 | 11 |
| Double Three | 子單元各自為 zigzag/flat/triangle | 7 |
| Triple Three | 同上 | 11 |

### B.2 Pattern 出現位置約束(綜合 R 條規則)

| Pattern | 允許出現位置 |
|---|---|
| Impulse | W1, W3, W5, A, C |
| Leading Diagonal | W1, A |
| Ending Diagonal | W5, C |
| Expanding Diagonal | W5, C(罕見) |
| Zigzag | W2, W4, A, B, X, W, Y, Z |
| Flat(各種) | W2, W4, A, B, X, W, Y, Z |
| Triangle(各種) | W4, B, X(**不可** W2、不可末端浪) |
| Double Three / Triple Three | W2, W4, B, W, Y, Z |

---

## 附錄 C:Fibonacci 理想比率表

> **使用方式**:這些比率是 **書本明示的觀察**,屬 Guidelines。命中時以 Fact 形式記錄,**不**加分、**不**改變 scenario 排序。
> **常見程度標籤**:High / Medium / Low **僅描述書本與統計觀察的頻率**,不是「信心」。

### C.1 W2 回撤 W1

| 比率 | 常見程度 | 來源 |
|---|---|---|
| 0.382 | High | Sid Norris/Swannell 統計:出現頻率最高 |
| 0.500 | High | Ramki Ch.3:實務最常見 W2 回撤位 |
| 0.618 | Medium | Frost Ch.1:深度回撤,Sharp Correction 常見 |
| 0.707 | Low | Sid Norris extracted |
| 0.786 | Low | Sid Norris extracted |

> **雙峰說明**:0.382 與 0.500 同為 High 是書本層面雙來源的客觀記錄。引擎不選邊、不加權。

### C.2 W3 相對 W1

| 比率 | 常見程度 | 意義 |
|---|---|---|
| 1.618 | High | Frost Ch.1:正常 W3 上限 |
| 2.618 | Medium | Frost Ch.1:延伸 |
| 1.000 | Medium | 等幅(W3 最低預期) |
| 1.236 | Medium | Sid Norris extracted |
| 1.382 | Medium | Sid Norris extracted |
| 3.000 | Low | Frost Ch.1:極端延伸 |
| 4.618 | Low | Frost Ch.1:極端延伸 |

### C.3 W4 回撤 W3

| 情境 | 比率 | 常見程度 |
|---|---|---|
| 正常 W3(非延伸) | 0.382 | High(Frost) |
| 正常 W3 | 0.500 | Medium(Frost) |
| 延伸 W3 | 0.236 | High(Frost) |
| 延伸 W3 | 0.382 | Medium(Frost) |

### C.4 W5 相對 P0→P3 與 W1

| 投射 | 比率 | 常見程度 | 條件 |
|---|---|---|---|
| W5 / (P0→P3) | 0.618 | High | Frost Ch.1 |
| W5 / (P0→P3) | 0.382 | Medium | Frost Ch.1 |
| W5 / (P0→P3) | 1.000 | Low | W3 為正常時 |
| W5 / (P0→P3) | 1.618 | Low | W3 為正常時 |
| W5 / W1 | 1.000 | High | W3 為延伸浪時(等幅) |
| W5 / W1 | 0.618 | Medium | W3 為延伸浪時 |

### C.5 Zigzag C/A

| 比率 | 常見程度 |
|---|---|
| 1.000 | High |
| 1.618 | Medium |
| 0.618 | Medium |
| 1.382 | Low |

### C.6 Flat C/A

| 比率 | 常見程度 | 來源 |
|---|---|---|
| 1.382 | High | Ramki Ch.4:Flat C 浪最常見比率 |
| 1.236 | Medium | |
| 1.000 | Medium | Frost Ch.2:Regular Flat 等幅 |
| 1.618 | Low | |

### C.7 Expanded Flat B/A

| 比率 | 常見程度 |
|---|---|
| 1.236 | High |
| 1.382 | Medium |

### C.8 Triangle 收斂率

| 比率 | 常見程度 | 說明 |
|---|---|---|
| 子浪比 ≈ 0.618 | High | 連續子浪比例(C/A, D/B, E/C) |
| 子浪比 ≈ 0.500 | Medium | |
| 子浪比 ≈ 0.382 | Low | |

### C.9 比率命中容差

```rust
const FIB_TOLERANCE: f64 = 0.03;  // ±3%,工程常數
```

> **註**:`FIB_TOLERANCE` 是工程容差,書本未明示具體數值。3% 為 Sid Norris 與 Swannell 統計研究中觀察到的「合理命中區間」。**禁止**在 P3 開發階段為「優化命中率」而調寬此容差(調寬 = 自欺)。

---

## 附錄 D:Guidelines(教義性指引,非剪枝)

> **使用方式**:Guidelines 命中產生 Fact,但**不**參與 forest 排序、**不**算分。每條 Guideline 有唯一 `GuidelineId`,以便 Fact 追溯。

### D.1 Alternation Guidelines(交替法則)

| GuidelineId | 內容 | 來源 |
|---|---|---|
| **G_ALT_FORM** | W2 與 W4 形態交替:W2 若為簡單修正(Zigzag),W4 傾向為複雜修正(Flat/Triangle/Combination),反之亦然 | Frost & Prechter Ch.2 |
| **G_ALT_DEPTH** | W2 與 W4 深度交替:W2 若深度回撤(≥0.5),W4 傾向淺回撤;反之亦然 | Frost & Prechter Ch.2 |
| **G_ALT_TIME** | W2 與 W4 時間交替:W2 若耗時短,W4 傾向耗時長;反之亦然 | Frost & Prechter Ch.2 |
| **G_ALT_INNER** | 修正浪內部交替:Wave A 若為 Zigzag,Wave B 傾向為 Flat;反之亦然 | Frost & Prechter Ch.2 |
| **G_ALT_LONGTERM** | 主要高低點形態交替(irregular bottom → normal bottom) | Frost & Prechter Ch.2 |

### D.2 Extension Guidelines(延伸法則)

| GuidelineId | 內容 | 來源 |
|---|---|---|
| **G_W3_EXTENDED** | 五浪推進中,**最常**延伸的是 W3(統計上 ~60%) | Frost & Prechter Ch.1 |
| **G_W5_EXTENDED** | W5 延伸常出現於商品市場(commodity),股市較少見 | Frost & Prechter Ch.1 |
| **G_W1_EXTENDED** | W1 延伸罕見,出現時 W3、W5 通常為正常或縮短 | Frost & Prechter Ch.1 |
| **G_W3_LONGEST** | 即使非延伸,W3 仍**傾向**為 W1/W3/W5 中最長者 | Frost & Prechter Ch.1 |

### D.3 Channeling Guidelines(通道法則)

| GuidelineId | 內容 | 來源 |
|---|---|---|
| **G_CHANNEL_PARALLEL** | Impulse 浪通常落在平行通道內。畫法:連接 W1 與 W3 的端點作為主軸,過 W2 端點畫平行線 | Frost & Prechter Ch.4 |
| **G_CHANNEL_W4_TOUCH** | W4 終點**傾向**觸及主軸的對側平行線(下軸) | Frost & Prechter Ch.4 |
| **G_CHANNEL_W5_THROWOVER** | W5 在 ending phase 可能 throwover(短暫穿出通道上軸),通常伴隨大量 | Frost & Prechter Ch.4 |
| **G_CHANNEL_LOG** | 長期(月線級以上)趨勢通道應在對數刻度繪製 | Frost & Prechter Ch.4 |

### D.4 Personality Guidelines(浪性格)

| GuidelineId | 內容 | 來源 |
|---|---|---|
| **G_PERS_W1_TENTATIVE** | W1 通常出現於極度悲觀情境,被市場視為反彈而非新趨勢起點 | Frost & Prechter Ch.2; Ramki Ch.5 |
| **G_PERS_W2_RETEST** | W2 通常以強力回測 W1 起點,情緒重回 W1 起點時的悲觀 | Frost & Prechter Ch.2; Ramki Ch.5 |
| **G_PERS_W3_STRONGEST** | W3 為「最強、最快、最具動能」的浪,通常伴隨基本面好轉與成交量放大 | Frost & Prechter Ch.2; Ramki Ch.5 |
| **G_PERS_W3_RSI_PEAK** | W3 在 RSI 上製造最高讀數(本輪行情之最),W5 雖價格更高但 RSI 通常背離 | Ramki Ch.5(Personality 核心觀點) |
| **G_PERS_W4_COMPLEX** | W4 通常為複雜、橫向、令人厭煩的修正,容易被誤判為新趨勢開始 | Frost & Prechter Ch.2 |
| **G_PERS_W5_DIVERGENCE** | W5 通常伴隨技術指標背離(RSI、MACD)、量縮、市場樂觀情緒高漲 | Frost & Prechter Ch.2; Ramki Ch.5 |
| **G_PERS_A_DENIAL** | Wave A 開始時,市場普遍認為這只是修正、主趨勢仍將繼續 | Frost & Prechter Ch.2 |
| **G_PERS_B_FALSE_HOPE** | Wave B 製造「假希望」,常以縮量上揚回測 A 起點附近 | Frost & Prechter Ch.2; Ramki Ch.5 |
| **G_PERS_C_DEVASTATING** | Wave C 為「災難性」一段,動能、成交量、廣度皆強(類似反向 W3) | Frost & Prechter Ch.2; Ramki Ch.5 |

### D.5 Volume Guidelines(成交量)

| GuidelineId | 內容 | 來源 |
|---|---|---|
| **G_VOL_W3_PEAK** | W3 期間成交量達到本輪最高峰 | Frost & Prechter Ch.2 |
| **G_VOL_W5_LIGHTER** | W5 期間成交量通常低於 W3(W3 > W5 in volume) | Frost & Prechter Ch.2 |
| **G_VOL_W5_THROWOVER** | W5 throwover 時可能出現量爆(一次性) | Frost & Prechter Ch.2 |
| **G_VOL_TRIANGLE_DECREASE** | Triangle 整體期間成交量逐漸萎縮,直到 E 浪結束爆量突破 | Frost & Prechter Ch.2 |
| **G_VOL_C_HEAVY** | Zigzag 中 Wave C 通常伴隨大量(C 浪 5 個子浪都有量) | Frost & Prechter Ch.2 |

### D.6 Equality Guidelines(等幅法則)

| GuidelineId | 內容 | 來源 |
|---|---|---|
| **G_EQ_NON_EXTENDED** | 五浪推進中,**未延伸**的兩浪傾向等幅(price 與 time 兩維度) | Frost & Prechter Ch.1 |
| **G_EQ_W1_W5_AFTER_W3_EXT** | W3 延伸後,W1 ≈ W5(price 與 time) | Frost & Prechter Ch.1 |

### D.7 Depth Guidelines(深度法則)

| GuidelineId | 內容 | 來源 |
|---|---|---|
| **G_DEPTH_W2_TYPICAL** | W2 典型回撤 0.382 ~ 0.618 區間 | Frost & Prechter Ch.1; Sid Norris extracted |
| **G_DEPTH_W4_SHALLOW** | W4 典型回撤 0.236 ~ 0.382 區間(較淺) | Frost & Prechter Ch.1 |

### D.8 Structure Guidelines(子浪結構完整度)

| GuidelineId | 內容 | 來源 |
|---|---|---|
| **G_STRUCT_FULL_DECOMPOSITION** | Impulse 內部所有 W1/W2/W3/W4/W5 都能進一步分解為對應子結構(5-3-5-3-5) | Frost & Prechter Ch.3 |
| **G_STRUCT_PARTIAL_DEFERRED** | 進行中(in-progress)的浪允許子結構部分缺失,但完成後必須能完整分解 | Frost & Prechter Ch.3 |

### D.9 Guideline 摘要表

```
總計:約 30 條 Guidelines
- Alternation:5 條
- Extension:4 條
- Channeling:4 條
- Personality:9 條
- Volume:5 條
- Equality:2 條
- Depth:2 條
- Structure:2 條
- (其他 Fibonacci 命中見附錄 C,以 GuidelineId 形式併入計數,例如 G_W2_FIB_382)
```

### D.10 Guidelines 處理偽碼(去人為加權版)

```rust
// ❌ 已棄用做法(v1.1)
// fn composite_confidence(s: &ScoreBreakdown) -> f64 {
//     s.fibonacci * 0.15 + s.alternation * 0.10 + ...
// }

// ✅ 補完版做法(v2.0):僅計數,不加權
fn evaluate_guidelines(scenario: &TraditionalScenario, ohlc: &OHLCVSeries)
    -> Vec<GuidelineMatch>
{
    let mut matches = Vec::new();
    for guideline in ALL_GUIDELINES.iter() {
        if let Some(observation) = guideline.check(scenario, ohlc) {
            matches.push(GuidelineMatch {
                guideline_id: guideline.id(),
                observation,  // 客觀描述,例如 "W2 retraced 0.385"
                fact_statement: guideline.format_fact(observation),
                // 注意:沒有 score 欄位!
            });
        }
    }
    matches
}

// scenario_forest 不依 guidelines_matched_count 排序
// 使用者(或下游 Aggregation Layer)自行決定如何使用這些觀察
```

---

## 附錄 E:書頁追溯表

> **目的**:任何規則或 Guideline 未來被質疑時,可一鍵回溯書本原文。
> **格式**:`(來源, 章/頁, 原文要旨)`

### E.1 Hard Rules 追溯

| RuleId | 來源 | 章/頁 | 原文要旨 |
|---|---|---|---|
| R01 | Frost & Prechter | Ch.1 §"Essential Design" | "Wave 2 never retraces more than 100% of wave 1" |
| R02 | Frost & Prechter | Ch.1 §"Essential Design" | "Wave 3 is never the shortest of waves 1, 3, and 5" |
| R03 | Frost & Prechter | Ch.1 §"Essential Design" | "Wave 4 never enters the price territory of wave 1" |
| R04 | Frost & Prechter | Ch.1 §"Extensions" | "Only one of the three motive waves will normally extend" |
| R05 | Frost & Prechter | Ch.1 §"Wave 3" | Wave 3 always shows progress beyond wave 1's end |
| R06-R10 | Frost & Prechter | Ch.1 §"Diagonal Triangles" | Diagonal 出現位置與內部結構規範 |
| R11-R13 | Frost & Prechter | Ch.2 §"Zigzag" | 5-3-5 結構與位置規範 |
| R14-R18 | Frost & Prechter | Ch.2 §"Flat" | 3-3-5 結構與三種變體定義 |
| R15-Ramki | Ramki | Ch.4 "Flat Correction" | Ramki 經驗放寬至 61.8%(原文以實務案例論證) |
| R19-R23 | Frost & Prechter | Ch.2 §"Triangle" | 3-3-3-3-3 結構、出現位置 |
| R24-R27 | Frost & Prechter | Ch.2 §"Combinations" | W-X-Y 與 W-X-Y-X-Z 結構 |
| R28-R29 | Frost & Prechter | Ch.3 §"Degrees" | Degree 分級與相對可比性 |

### E.2 Guidelines 追溯(節錄)

| GuidelineId | 來源 | 章/頁 | 原文要旨 |
|---|---|---|---|
| G_ALT_FORM | Frost & Prechter | Ch.2 §"Guideline of Alternation" | 形態交替原則 |
| G_W3_EXTENDED | Frost & Prechter | Ch.1 §"Extensions" | "Most often it is wave 3 that is extended" |
| G_W3_LONGEST | Frost & Prechter | Ch.1 §"Wave 3" | W3 為最長浪傾向 |
| G_PERS_W3_RSI_PEAK | Ramki | Ch.5 "Personality" | Ramki Personality 核心:RSI peak 出現於 W3 而非 W5 |
| G_PERS_W5_DIVERGENCE | Frost & Prechter; Ramki | Ch.2; Ch.5 | W5 背離為主要結束訊號 |
| G_VOL_W3_PEAK | Frost & Prechter | Ch.2 §"Volume" | W3 量峰原則 |
| G_VOL_TRIANGLE_DECREASE | Frost & Prechter | Ch.2 §"Triangle" | Triangle 量能萎縮 |
| G_EQ_NON_EXTENDED | Frost & Prechter | Ch.1 §"Equality" | "Two of the motive waves will tend toward equality" |
| G_CHANNEL_PARALLEL | Frost & Prechter | Ch.4 §"Channels" | 通道畫法 |
| G_DEPTH_W2_TYPICAL | Frost & Prechter; Sid Norris | Ch.1; Wave Notes | W2 典型回撤區間 |

> **完整追溯表**:P3 開發時將每條 GuidelineId 補上對應書本確切頁碼(以 10th edition 2005 為基準)。

### E.3 Sid Norris extracted 標注規則

凡標 `[Sid Norris extracted]` 者,為 Sid Norris《Wave Notes》中將 Frost 原文整理為條列式時引入的數值或統計觀察。原書未明示具體數值時,Sid Norris 的整理具參考價值,但標注以示區別。

範例:
- C.1 中 W2 的 0.707 / 0.786 比率
- C.4 中 W3 正常時 W5 / (P0→P3) 的 0.382 比率細分

### E.4 與已棄用設計的對應(供 P3 重寫時對照)

| v1.1 設計 | v2.0 處理 | 對應書本依據 |
|---|---|---|
| Composite Confidence 9 因子 0.20/0.15/... | 改為 Guideline 命中計數 | 書本未提供任何權重數字 |
| Flat B 浪 90% +0.15 加分 | 改為產出 G_FLAT_B_PRECHTER Fact(若 B≥90%) | Frost 原文僅描述「90% 為典型」,未提及「加分」 |
| Engine_T / Engine_N 加權合併 | 並排輸出,Aggregation Layer 不合併 | 兩派為獨立體系,書本層面無「合併」基礎 |
| `tolerance.toml` 外部容差 | 工程容差(FIB_TOLERANCE 3%)硬編碼,不外部化 | 容差為實作細節,非書本內容 |

---

## 附錄 F:開發者 Checklist

P3 開始開發 Traditional Core 前,逐項檢查:

```
[ ] Neely Core 已通過五檔股票實測
[ ] WaveCore trait 草案已固化(P0 完成)
[ ] swing_detector 抽出策略已決議
[ ] 取得 Frost & Prechter 1978(10th ed.)實體或合法 PDF
[ ] 取得 Ramki Five Waves to Financial Freedom 實體或合法 Kindle
[ ] 取得 Sid Norris Wave Notes(elliottwaveplus.com 公開 PDF)
[ ] 附錄 A 的 R01-R29 全部對照書本原文確認文字
[ ] 附錄 D 的 30 條 Guidelines 全部對照書本確認
[ ] 附錄 E 補上每條規則的確切頁碼
[ ] Validator 偽碼通過 5 檔股票回測,Hard Rule 違反率 < 5%(否則表示 pivot detector 有問題,不應放寬規則)
[ ] Output 結構與 Aggregation Layer 契約對齊
[ ] 確認沒有任何「composite_confidence」、「scorer_weight」、「primary 排序」等加權字眼殘留於程式碼
```

---

## 變更紀錄

| 版本 | 日期 | 變更 |
|---|---|---|
| v2.0 r1 | 2026-04-30 | 從 cores_overview 抽出獨立成檔 |
| **v2.0 r2** | **2026-04-30** | **補完核心:新增附錄 A(29 條 Hard Rules)、附錄 B(Pattern 結構表)、附錄 C(Fibonacci 比率表)、附錄 D(30 條 Guidelines)、附錄 E(書頁追溯)、附錄 F(開發者 Checklist);第一章新增 §1.3「去人為判斷」原則;第三章新增 §3.1 Combined 模式語意、§3.2 棄用 Params 細目;第五章 Fact 規則細分為 Scenario 級與 Guideline 級;第九章補三項已棄用整合方式** |

---

**END OF SPEC**
