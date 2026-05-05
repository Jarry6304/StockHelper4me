# NEO Pipeline v2.0 — 架構轉向決策文件

> **版本**:v2.0-decisions-r3(收斂 P0 工程細節,解決 r2 既有條款的內部矛盾)
> **日期**:2026-04-28
> **基準**:`neo_pipeline_spec_v1_1_1_.md` + `schema_reference.md`(SCHEMA_VERSION=1.1)
> **整合來源**:`tw_stock_mcp_技術指標疊圖系統設計.md`
> **用途**:作為 v2.0 完整 spec 的最終決策紀錄,實作從本文件直接展開
>
> ## r2 → r3 變更摘要
>
> r3 不引入新原則,僅收斂 r2 在「概念對、細節缺」之處的工程未決問題,並修正三處內部矛盾。
>
> **修正的內部矛盾**:
> 1. 第七章「窮舉所有壓縮路徑」與工程現實衝突 → 引入 `forest_max_size` + `BeamSearchFallback`,並聲明 power_rating 截斷不違反「並排不整合」原則
> 2. 第 11.2 節 `inventory::submit!`(編譯期靜態)與第 11.6 坑 2「Core 版本相容性檢查」(runtime)矛盾 → 明確選定 Monolithic binary 模型
> 3. Fibonacci 在 5.2 / 14.2.3 / 8.2 三處歸屬不一致 → 統一為 Neely Core 子模組
>
> **新增 P0 必補章節**(對應 r2 13.8 八項):
> - 13.3 增補 PyO3 邊界與序列化規範
> - 13.8 P0 必須決策清單(8 條,對外作為 P0 動工 checklist)
> - 14.2.2 增補 `params_hash` 演算法定義
> - 14.2.4 增補 facts 去重 unique constraint
> - 14.5 新增 partition + retention 策略
> - 15.1 Stage 4 拆 4a / 4b 兩個子階段
> - 15.6 增補 on-demand single-flight 機制
>
> **新增哲學立場聲明**:
> - 2.4:on-demand 補算為明確認可的 escape hatch,非原則破口
> - 7.5(新增):power_rating 截斷不違反「並排不整合」之論證
>
> **依賴方向修正**:
> - Stage 4 拓撲明確化:`neely_core` → `trendline_core`
> - Fibonacci 不獨立成 Core,從第八章與第十四章移除其獨立 Core 印象
>
> **暫緩決策(等實測數據)**:
> - `NeelyEngineConfig.forest_max_size` / `compaction_timeout_secs` 預設值待五檔股票實測後填入,r3 先給保守佔位值
> - 15.3 batch 時間預算同上,r3 先標註「需依實測修正」

---

## 目錄

1. [架構轉向動機](#一架構轉向動機)
2. [核心設計哲學](#二核心設計哲學)
3. [v1.1 → v2.0 變更總覽](#三v11--v20-變更總覽)
4. [架構分層](#四架構分層)
5. [Neely Core 範圍界定](#五neely-core-範圍界定)
6. [容差系統決策](#六容差系統決策)
7. [Compaction 重新定位](#七compaction-重新定位)
8. [輔助 Cores 清單](#八輔助-cores-清單)
9. [命名規範](#九命名規範)
10. [Core 之間的耦合規範](#十core-之間的耦合規範)
11. [Workflow / Orchestrator 設計](#十一workflow--orchestrator-設計)
12. [Aggregation Layer 設計](#十二aggregation-layer-設計)
13. [尚未決策的問題](#十三尚未決策的問題)
14. [儲存層架構](#十四儲存層架構)
15. [Batch Pipeline 設計](#十五batch-pipeline-設計)
16. [Workflow 預設模板](#十六workflow-預設模板)
17. [前端職責邊界](#十七前端職責邊界)
18. [v1.1 模組去處對照表](#十八v11-模組去處對照表)
19. [附錄 A:已採用的決策清單](#附錄-a已採用的決策清單)
20. [附錄 B:已棄用的方案](#附錄-b已棄用的方案)
21. [附錄 C:Learner 離線模組界定](#附錄-clearner-離線模組界定)
22. [附錄 D:來自疊圖系統文件的整合決策](#附錄-d來自疊圖系統文件的整合決策)

---

## 一、架構轉向動機

### 1.1 v1.1 的核心問題

v1.1 spec 是「**裁決式**」設計:Pipeline 算出單一最優答案、給 primary scenario 與分數,user 看結論。這違反波浪理論的本質與商業可持續性。

### 1.2 v2.0 的核心定位

> **從「Pipeline 是分析師,user 看結論」 → 「Pipeline 是研究助理,user 是決策者」**

Pipeline 的工作是:
- 窮舉所有 Neely 規則允許的 scenarios
- 誠實標註每個 scenario 的客觀屬性
- 並排呈現所有獨立事實

User 的工作是:
- 看完所有並排事實
- 自己連線、自己下決策

### 1.3 決策三大原則

1. **去除 LLM 與主觀黑箱認定** — 不使用 Bayesian likelihood 估計、不使用 LLM 仲裁、不使用 Softmax 動態溫度等「看似客觀的數學包裝主觀判斷」的設計
2. **忠於 Neely 原作** — 規則來自書中明確記載,不自創、不擴充未經驗證的規則
3. **單一職責、切乾淨 by module** — 程式大沒關係,但每個 Core 必須職責單一、可拔可插

---

## 二、核心設計哲學

### 2.1 Pipeline 只做兩件事

1. **窮舉所有 Neely 規則允許的 scenarios**(機械式,不主觀)
2. **誠實標註每個 scenario 的「Neely 內建屬性」**(查書,不主觀)

User 自己用書裡的規則做決策。**Pipeline 不做機率估計、不做仲裁、不做排序建議。**

### 2.2 Scenario 屬性原則

不使用 `probability`、`confidence`、`score` 等暗示「Pipeline 已下判斷」的欄位。改用:

- **Neely 書裡寫死的屬性**(查表):power_rating、max_retracement、post_pattern_behavior
- **規則執行結果**(客觀計數):passed_rules、deferred_rules、rules_passed_count
- **失效條件**(規則的逆向轉譯):invalidation_triggers
- **獨立事實向量**(7 維,不加總):fibonacci_alignment、alternation_facts、channeling_facts...

### 2.3 並排不整合原則

> Aggregation Layer 只做「並排呈現」,不做「加權整合」

**反例**(禁止):
- ❌ 「Neely Core 說 S1 結構好 + Chips Core 說外資買 → 強看好」
- ❌ 「7 個指標投票決定多空」
- ❌ 「綜合評分 8.5 / 10」

**正例**(允許):
- ✅ 把 Neely Core 的 scenarios 與 Chips Core 的事實並列顯示
- ✅ User 看完後自己連線
- ✅ 提供 facet filter 讓 user 折疊不關心的 Core

### 2.4 Batch 預算原則

> **Core 計算結果進 DB,即時請求純讀,前端純展示**

Core 不參與即時請求路徑。每日收盤後 batch 一次性算完所有結果,寫入 Storage Layer。前端任何時間打 API 進來,Aggregation Layer 直接從 DB 讀現成資料組裝回傳。

**設計含義:**
- Core 是純函式,可被獨立測試、並行執行,不需處理併發狀態
- DB 即真相,沒有「快取一致性」問題
- 即時請求毫秒級回應,不受計算時間影響
- Learner 直接讀 DB,無需額外 dump 管線

**例外:on-demand 補算**
- 使用者請求 workflow 預設外的指標參數組合 → 後端臨時算一次 → 寫 DB → 之後 batch 自動納入
- 詳見第十六章

**哲學立場聲明(r3 新增)**:on-demand 補算是**明確認可的 escape hatch**,不是原則破口。理由:
1. 它不破壞「DB 即真相」 — 算完寫進 DB,後續路徑仍純讀
2. 它不破壞 Core 純函式特性 — 仍是 stateless 計算,只是觸發點是即時請求
3. 它不破壞「並排不整合」 — 補算只擴增 indicator 種類,不在 Aggregation Layer 整合
4. 邊界清楚:on-demand 必須走 single-flight + rate limit 雙重保護(詳見 15.6),不可繞過

---

## 三、v1.1 → v2.0 變更總覽

| 項目 | v1.1 做法 | v2.0 做法 | 變更原因 |
|---|---|---|---|
| **架構模型** | 單一 Pipeline,Engine_T + Engine_N 內部整合 | 多 Cores 並列,各自獨立輸出 | 切乾淨,單一職責 |
| **輸出形狀** | `primary` + `alternatives`,有分數 | `Scenario Forest`,無分數無排序 | 去除主觀判斷 |
| **Scorer 7 因子** | 加權加總成單一分數 | 拆成 7 個獨立事實向量,不加總 | 加權本身就是主觀 |
| **Compaction** | 貪心選最高分 + backtrack | 窮舉所有合法壓縮路徑,產出 Forest | 工具僅作輔助判讀 |
| **容差系統** | 絕對偏移 ±4% | **相對偏移 ±4%**(Neely 原意) | 尺度無關、可擴展 |
| **失效條件** | 無 | 每個 Scenario 必須有 invalidation_triggers | 商業核心:user 知道盯什麼 |
| **`[TW-MARKET]` Scorer 微調** | 嵌入 Neely Engine | **移除**(主觀調參) | 違反「忠於原作」 |
| **Combined confidence ×1.1/×0.7** | Engine_T + Engine_N 整合公式 | **移除**(主觀調參) | 違反「並排不整合」 |
| **籌碼 / 基本面 / 環境** | 完全未利用 Collector 大量資料 | 各自獨立 Core,事實標註 | 充分利用 Collector |
| **技術指標** | 無 | 每個指標一個獨立 Core | 用戶要求,且不耦合波浪 |
| **LLM 仲裁層** | Memory:tw_stock_mcp V3 提到 LLM 仲裁 | **不採用** | 黑箱不可審計 |
| **Bayesian 後驗更新** | 中途討論方案 | **不採用** | likelihood 主觀 |
| **執行模式** | 即時計算為主 | **Batch 預算為主**,即時純讀 DB | 台股一日一交易,batch 模式更簡潔 |
| **儲存層** | 無明確設計 | **四層儲存**:Raw / Indicator Values / Structural Snapshots / Facts | DB 即真相,Learner 直讀 |
| **前端職責** | 未明確 | **純讀 API + 渲染圖層**,不參與計算 | Batch 模式下後端職責止於 DB |
| **Tag Core** | 即時模組(疊圖文件草案) | **離線 Learner 模組**(附錄 C) | 即時不依賴 LLM |
| **跨指標訊號** | 未定義 | **不立 Core**,並排呈現由使用者自看 | 維持零耦合原則 |

---

## 四、架構分層

### 4.1 四層架構(v2.0 新增儲存層)

```
┌─────────────────────────────────────────────────────────────┐
│                    Presentation Layer                        │
│   前端圖表 / API,純讀 DB 組裝,無計算邏輯                    │
└─────────────────────────────────────────────────────────────┘
                            ↑ (SQL query)
┌─────────────────────────────────────────────────────────────┐
│                   Aggregation Layer                          │
│   並排呈現,不整合。可提供 facet filter 但不下結論            │
│   即時請求路徑:讀 DB → 組裝 → 回傳                          │
└─────────────────────────────────────────────────────────────┘
                            ↑ (DB read)
┌─────────────────────────────────────────────────────────────┐
│                    Storage Layer (新增)                      │
│   - Raw OHLC / 籌碼 / 財報 / 環境(Collector 寫入)           │
│   - Indicator Values(Batch 寫入,前端直讀)                  │
│   - Structural Snapshots(Wave Forest / Fib / SR)           │
│   - Facts(事件式紀錄,append-only)                          │
└─────────────────────────────────────────────────────────────┘
                            ↑ (Batch write,每日收盤後)
┌─────────────────────────────────────────────────────────────┐
│                         Cores                                │
│   單一職責、可拔可插、Core 之間零語義耦合                     │
│   - Wave Cores (Neely / Traditional)                         │
│   - Market Cores (TW-Market)                                 │
│   - Indicator Cores (MACD / RSI / KD / ADX / ...)            │
│   - Chip Cores (Institutional / Margin / Foreign Holding...) │
│   - Fundamental Cores (Revenue / Valuation / Financials)     │
│   - Environment Cores (US Market / TAIEX / Fear-Greed...)    │
└─────────────────────────────────────────────────────────────┘
                            ↑ (共用基礎)
┌─────────────────────────────────────────────────────────────┐
│                   Shared Infrastructure                      │
│   非 Core,純基礎建設,不做體系判斷                          │
│   - OHLCV Loader                                             │
│   - Timeframe Resampler                                      │
│   - Fact Schema                                              │
│   - Data Reference                                           │
└─────────────────────────────────────────────────────────────┘
```

### 4.1.1 執行模式:Batch 預算為主,即時計算為輔

v2.0 採用 **Batch 預算模式**:

- 每日收盤後 batch:Cores 計算完整結果 → 寫入 Storage Layer
- 即時請求:Aggregation Layer 純讀 DB → 組裝 → 回傳前端
- 前端職責:純展示,無計算

**理由**:台股一日一交易,無即時報價計算壓力;Batch 模式讓 DB 即真相,使用者請求毫秒級回應,Learner 直接讀 DB 無需 dump 管線。

詳見第十五章「儲存層架構」與第十六章「Batch Pipeline 設計」。

### 4.2 命名定義

- **Workflow**(業務概念):一套 Core 的組合流程,例:`tw_stock_standard`、`quick_screening`、`deep_analysis`
- **Orchestrator**(技術元件):Workflow 的執行引擎,負責 Core 註冊發現、依序呼叫、結果組裝
- **Core**(獨立模組):單一職責的純函式黑盒,給定輸入產出輸出
- **Shared Infrastructure**:Core 之間共用的基礎建設,不是 Core

```
Workflow (業務概念)
  ↓ implemented by
Orchestrator (技術元件)
  ↓ composes
Cores (獨立模組)
  ↓ produces
Outputs (標準化資料結構)
  ↓ assembled by
Aggregation Layer (呈現層)
```

---

## 五、Neely Core 範圍界定

### 5.1 進 Neely Core 的條件(三條全中)

1. **Neely 書裡明確記載** — 有頁碼、有書中段落引用
2. **規則式可程式化** — 條件可寫成 if/else,不是「經驗判斷」
3. **不依賴外部資料** — 只吃 OHLC + ATR,不吃籌碼/法人/環境

### 5.2 進 Neely Core 的清單

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

**r3 補充:Fibonacci 的歸屬(統一三處不一致)**

Fibonacci 比率與容差屬 **Neely Core 內部子模組**,輸出在 `Scenario.expected_fib_zones` 欄位,**不獨立成 Core**。
- 8.2 Indicator Cores 清單**不**列入 `fib_core`
- 14.2.3 / 15.1 提及的 `fib_zones` snapshot 是「從 Neely scenario forest 投影出的視圖」,給前端方便消費,不是獨立 Core 計算
- 投影邏輯放在 Aggregation Layer,屬「資料整理」非「計算」,不違反 12.2 原則
- 寫入 `structural_snapshots` 時,若以 `core_name='fib_zones'` 寫,則該 row 必須附 `derived_from_core='neely_core'` 與相應 `snapshot_date`,以保留追溯性

### 5.3 不進 Neely Core 的清單

| 模組 | v1.1 Item | 應該放哪 | 理由 |
|---|---|---|---|
| Scorer 7 因子加權加總 | Item 7 | **不要了** | 加權本身就是主觀 |
| Scorer 7 因子(獨立事實向量) | Item 7 | 進 Neely Core,但**不加總** | 拆解後保留事實層 |
| 連續漲跌停合併 | Item 1.5 | TW-Market Core | 台股市場特性,不是 Neely |
| TAIEX Neutral 閾值調高 | Item 1.5 | TW-Market Core | 同上 |
| 還原指數使用 | Item 17.2 | TW-Market Core | 同上 |
| `[TW-MARKET]` ext_type_prior_3rd | Item 7.4 | **不要了** | 主觀加權 |
| `[TW-MARKET]` alternation_tw_bonus | Item 7.4 | **不要了** | 主觀加權 |
| Engine_T (傳統派) | Item 13, 17 | Traditional Core(獨立並列) | 不是 Neely 體系 |
| Combined confidence ×1.1/×0.7 | Item 17.5 | **不要了** | 主觀調參 |

### 5.4 Neely Core Output 介面契約

```rust
NeelyCoreOutput {
    // 輸入 metadata
    stock_id: String,
    timeframe: Timeframe,
    data_range: TimeRange,

    // 結構性結果(Forest,不是 Tree;不排序、不選 primary)
    scenario_forest: Vec<Scenario>,

    // Neely Core 自己的診斷
    diagnostics: NeelyDiagnostics {
        monowave_count,
        candidate_count,
        validator_pass_count,
        validator_reject_count,
        rejections,            // 含 rule_id, expected, actual, gap, neely_page
        deferred_rules,
        compaction_paths,      // 所有合法壓縮路徑
        elapsed_ms,
    },

    // Neely 書頁追溯
    rule_book_references: Vec<RuleReference>,
}

Scenario {
    id: String,

    // 結構
    wave_tree: WaveNode,
    pattern_type: PatternType,
    structure_label: String,

    // Neely 書裡寫死的屬性(查表)
    neely_power_rating: i8,                 // -3 ~ +3
    neely_max_retracement: f64,
    neely_post_pattern_behavior: PostBehavior,

    // 規則執行結果
    passed_rules: Vec<RuleId>,
    deferred_rules: Vec<RuleId>,

    // 失效條件(Neely 規則的逆向轉譯)
    invalidation_triggers: Vec<Trigger>,

    // 7 個獨立事實向量(不加總)
    structural_facts: StructuralFacts {
        fibonacci_alignment: FibonacciFacts,
        alternation_facts: AlternationFacts,
        channeling_facts: ChannelingFacts,
        equality_facts: EqualityFacts,
        similarity_facts: SimilarityFacts,
        complexity_facts: ComplexityFacts,
        // 注意:沒有「總分」
    },

    // 客觀計數(不是分數)
    rules_passed_count: usize,
    rules_applicable_count: usize,
    deferred_count: usize,

    // 預期區間(規則式產生)
    expected_fib_zones: Vec<FibZone>,
}

Trigger {
    trigger_type: TriggerType,  // PriceBreak / TimeExceeded / VolumeSignal / FibFailure
    description: String,         // "收盤跌破 456.1 (W4 終點)"
    threshold: f64,
    scope: TimeWindow,
    on_trigger: TriggerAction,   // r3 修正:移除 ReduceProbability(違反 2.2 原則)
                                 //         改為 InvalidateScenario / WeakenScenario / PromoteAlternative
                                 //         WeakenScenario 僅標註「該 scenario 進入 deferred」,不引入機率語意
    rule_reference: RuleId,      // 對應 Neely 規則,例 R5
}

// r3 修正:neely_power_rating 由 i8 改為 enum,避免 power_rating = 99 等無效值
pub enum PowerRating {
    StronglyBearish = -3,
    Bearish = -2,
    WeaklyBearish = -1,
    Neutral = 0,
    WeaklyBullish = 1,
    Bullish = 2,
    StronglyBullish = 3,
}
```

---

## 六、容差系統決策

### 6.1 最終決策

- **比率清單**(38.2%、61.8%、100%、161.8% 等)→ Neely 書裡明確列的,**寫死在 Neely Core 常數表**
- **相對 ±4% 容差**→ Neely 原意,**寫死**
- **Waterfall Effect ±5% 例外**→ Neely 書裡特例,**寫死**

**所有 Neely 規則常數不可外部化、不可調**。要調就是改 Neely Core 的代碼,代表刻意偏離原作,需在 commit 訊息明確標註。

### 6.2 相對偏移 vs 絕對偏移

v1.1 用絕對偏移 `[R - 0.04, R + 0.04]`,問題:

| 原始比率 | 絕對 ±4% (v1.1) | 相對 ±4%(v2.0) | 差距 |
|---|---|---|---|
| 38.2% | [34.2%, 42.2%] | [36.7%, 39.7%] | 絕對寬 3.5x |
| 61.8% | [57.8%, 65.8%] | [59.3%, 64.3%] | 絕對寬 2.6x |
| 100% | [96%, 104%] | [96%, 104%] | 一致 |
| 161.8% | [157.8%, 165.8%] | [155.3%, 168.3%] | 相對寬 1.6x |
| 261.8% | [257.8%, 265.8%] | [251.3%, 272.3%] | 相對寬 2.6x |

**v2.0 採用相對偏移**:`[R × (1 - δ), R × (1 + δ)]`,δ = 0.04。意義是**比率本身的相對誤差,跟尺度無關**。未來加新比率(0.236、0.764、2.618、4.236...)邏輯都自動成立。

### 6.3 工程參數例外

**僅以下兩個工程參數**寫在 `NeelyEngineConfig`(可調但有預設):

```rust
pub struct NeelyEngineConfig {
    pub atr_period: usize,        // 工程參數,可調(預設 14)
    pub beam_width: usize,        // 工程參數,可調(預設 5)
}
```

理由:這兩個不是 Neely 規則本身,而是「執行 Neely 規則所需的工程選擇」,可被 Calibration Core 微調。**Fibonacci tolerance 等 Neely 規則沒有對應 setter**。

### 6.4 容差系統優缺點對照(已決策採方案 A)

| 項目 | 寫死(採用) | toml 外部化(已棄) |
|---|---|---|
| 忠於原作 | ✅ 數字就是書裡的數字 | ❌ 誘導偏離 |
| Type-safe | ✅ 編譯期檢查 | ❌ runtime 才爆 |
| 不可調 = 不會被調歪 | ✅ | ❌ 可能被「調出來」結果 |
| 可審計 | ✅ 附 `// Neely p.123` 註解 | ⚠️ 設定檔可能跟代碼不同步 |
| 效能 | ✅ 編譯期常數 | ⚠️ runtime 讀檔 |
| 版本控管 | ✅ git blame 可追 | ❌ 失去 SSOT |

---

## 七、Compaction 重新定位

### 7.1 v1.1 vs v2.0

| 項目 | v1.1 | v2.0 |
|---|---|---|
| 多解處理 | 貪心選最高分 + backtrack | 窮舉所有合法壓縮路徑 |
| 輸出 | 單一壓縮後最優樹 | Scenario Forest(多棵並列) |
| Threshold 角色 | 分數低於 0.3 就丟 | 不再用分數篩選,只用 Neely 規則篩選 |
| 「選擇邏輯」 | Compaction 內建 | **完全移除**(工具僅作輔助判讀) |

### 7.2 設計原則

> **Neely Core 的 Compaction 不做「選擇」,只做「窮舉所有合法的壓縮路徑」**

最後產出的不是「一棵壓縮後的最優樹」,而是「**所有可能的壓縮樹組成的 Forest**」。Forest 的每棵樹都是 Neely 規則允許的合法解讀。

### 7.3 保留的 Neely 規則(進入 Compaction)

- Complexity Rule(差距 ≤ 1 級)— Neely 書裡明確規則
- Deferred 約束(所有 deferred 必須 resolved 才可觸發 Compaction)— v1.1 已有,保留

### 7.4 移除的工程啟發式

- 貪心選分數最高(Greedy)— 移除
- 衝突時 backtrack 取次優(Backtrack)— 移除(因為不再有「最優」這個概念)
- Compaction Threshold(預設 0.3)— 移除

### 7.5 Forest 上限保護(r3 新增)

**問題**:7.2 「窮舉所有合法壓縮路徑」在最壞情況下 Forest 大小可能指數增長,單檔 OOM 或卡住整個 batch。r2 對此無保護機制。

**決策**:在 NeelyEngineConfig 增加 Forest 上限與逾時參數,保留窮舉的精神,但設工程護欄。

```rust
pub struct NeelyEngineConfig {
    pub atr_period: usize,                  // 寫死 14(見 6.3 / 13.7)
    pub beam_width: usize,                  // 工程參數,預設 5
    pub forest_max_size: usize,             // r3 新增,Forest 上限
    pub compaction_timeout_secs: u64,       // r3 新增,單檔逾時
    pub overflow_strategy: OverflowStrategy, // r3 新增,溢出策略
}

pub enum OverflowStrategy {
    /// Forest 超過 max_size 時,用 power_rating 排序保留 top-K
    /// 並在 NeelyDiagnostics.overflow_triggered = true
    BeamSearchFallback { k: usize },

    /// Forest 超過 max_size 時,標記該股 fail,不寫 snapshot
    /// 在 NeelyDiagnostics 完整記錄拒絕原因
    FailWithDiagnostics,

    /// 不限制(僅供開發/測試,production 禁用)
    Unbounded,
}
```

**預設值(暫定,待五檔股票實測後修正)**:
- `forest_max_size = 1000`
- `compaction_timeout_secs = 60`
- `overflow_strategy = BeamSearchFallback { k: 100 }`

**實測指引**:選 5 檔不同型態複雜度的股票(0050 / 2330 / 3363 / 6547 / 1312)跑 5 年日線,測量 P50 / P95 / P99 forest_size 與 elapsed。依以下閾值修正預設值:

| 指標 | 綠燈(預設值合理) | 黃燈(預設值偏緊) | 紅燈(策略需重議) |
|---|---|---|---|
| P95 forest_size | < 200 | 200–1000 | > 1000 |
| P95 elapsed_secs | < 5s | 5–30s | > 30s |
| P95 memory_mb | < 50 | 50–200 | > 200 |

**哲學論證:`BeamSearchFallback` 為何不違反「並排不整合」原則(2.3 條)**

可能被質疑「用 power_rating 排序後砍低分,等於下了主觀判斷」。回應:

1. **power_rating 是 Neely 書裡寫死的查表值**(5.2 表格 / 5.4 NeelyCoreOutput 已決策),不是 Pipeline 計算的主觀分數
2. **截斷不是排序展示**:截斷是 Core 內部資源管理,Aggregation Layer 看到的仍是「forest 已是 Neely 規則允許的合法解讀」,Aggregation 層仍不做加權整合
3. **截斷必須可觀察**:`NeelyDiagnostics.overflow_triggered = true` 時,Aggregation Layer 必須將此狀態傳給前端顯示「此股結構過於複雜,系統呈現 Top K 解讀」,使用者知情
4. **截斷是工程必要,不是哲學偏好**:沒有上限 → OOM → 服務不可用,違反「DB 即真相」承諾比違反「並排不整合」更嚴重

**操作邊界**:
- 截斷僅發生在 Core 層,Aggregation Layer 與前端**永遠不能**對 forest 二次排序或截斷
- 若 P95 真的超過 1000 而需強化截斷,應回頭重審 Compaction 演算法本身,而不是繼續加大 k 值

---

## 八、輔助 Cores 清單

### 8.1 Core 切分原則

> **核心多不是問題,問題是乾淨**(用戶決策)

每個 Core 是 200-500 行的小程式,各自單純,比一個 5000 行的大 Engine 遠遠更好維護。

**r3 補充:不獨立成 Core 的清單**

| 項目 | 為何不獨立 | 實際處理方式 |
|---|---|---|
| Volume(成交量) | 已存在於 Layer 1 raw 表(`price_daily_fwd.volume` 等),無計算邏輯 | Aggregation Layer 直接從 raw 表查 |
| Fibonacci | Neely Core 內部子模組,輸出在 `Scenario.expected_fib_zones` | 詳見 5.2 補充 |
| TTM Squeeze 等跨指標訊號 | 違反零耦合原則 | 詳見 8.6 |
| MA / SMA / EMA / WMA(同族) | 演算法相近,差異僅在權重 | 統一為 `ma_core`,以 enum 參數區分子型號(r3 新增) |

**r3 修正:13.1 P1 清單中的 `volume`**

P1 9 個指標中的 `volume` 並非新建一個 `volume_core`,而是「workflow 模板宣告需要 volume 資料」,Aggregation Layer 從 raw 表撈即可。實質上 P1 需要新建的 Core 是 8 個:`macd / rsi / kd / adx / ma / bollinger / atr / obv`。

### 8.2 完整 Core 清單

```
┌─────────────────────────────────────────────────────────────┐
│ Wave Cores(波浪體系)                                        │
├─────────────────────────────────────────────────────────────┤
│  • neely_core              Neely 科學派波浪                   │
│  • traditional_core        傳統派波浪(獨立並列)              │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Market Cores(市場特性與資料前處理)                           │
├─────────────────────────────────────────────────────────────┤
│  • tw_market_core          台股漲跌停合併、還原指數            │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Indicator Cores(技術指標,各自一個 Core)                     │
├─────────────────────────────────────────────────────────────┤
│  動量 / 趨勢 / 強度類:                                        │
│  • macd_core              MACD                                │
│  • rsi_core               RSI                                 │
│  • kd_core                KD / Stochastic                     │
│  • adx_core               ADX / DMI                           │
│  • ma_core                SMA / EMA(可參數化多週期)           │
│  • ichimoku_core          一目均衡表                           │
│  • williams_r_core        威廉指標                             │
│  • cci_core               CCI                                 │
│  • coppock_core           Coppock Curve                       │
│                                                                │
│  波動 / 通道類:                                                │
│  • bollinger_core         布林通道                             │
│  • keltner_core           Keltner Channel(疊圖文件新增)       │
│  • donchian_core          Donchian Channel(疊圖文件新增)      │
│  • atr_core               ATR(獨立,不依賴 Neely Core)        │
│                                                                │
│  量能類:                                                       │
│  • obv_core               OBV                                 │
│  • vwap_core              VWAP                                │
│  • mfi_core               資金流量指數                         │
│                                                                │
│  型態 / 價位類(疊圖文件新增):                                 │
│  • candlestick_pattern_core  K 線型態(僅嚴格規則式型態)       │
│  • support_resistance_core   靜態撐壓位偵測                    │
│  • trendline_core            趨勢線(消費 monowave 例外)       │
│                                                                │
│  • ...                    未來擴充                             │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Chip Cores(籌碼面,可細分,推薦 5 個獨立)                    │
├─────────────────────────────────────────────────────────────┤
│  • institutional_core      法人買賣(外資/投信/自營)           │
│  • margin_core            融資融券                             │
│  • foreign_holding_core   外資持股比率                         │
│  • shareholder_core       持股級距(籌碼集中度)                │
│  • day_trading_core       當沖統計                             │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Fundamental Cores(基本面)                                    │
├─────────────────────────────────────────────────────────────┤
│  • revenue_core           月營收                               │
│  • valuation_core         PER / PBR / 殖利率                   │
│  • financial_statement_core  財報三表                          │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Environment Cores(環境)                                      │
├─────────────────────────────────────────────────────────────┤
│  • us_market_core         SPY / VIX                           │
│  • taiex_core             加權指數                             │
│  • exchange_rate_core     匯率                                 │
│  • fear_greed_core        恐慌貪婪指數                         │
│  • market_margin_core     市場整體融資維持率                   │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ System Cores                                                  │
├─────────────────────────────────────────────────────────────┤
│  • aggregation_layer      並排呈現(不整合)                   │
│  • orchestrator           Workflow 編排                        │
└─────────────────────────────────────────────────────────────┘
```

### 8.3 Core 與 Collector 資料表對應

| Core | 對應 Collector 表 |
|---|---|
| neely_core | `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd` |
| tw_market_core | `price_limit`、`price_adjustment_events`、`market_index_tw`(還原指數) |
| macd_core / rsi_core / kd_core / ... | `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd` |
| institutional_core | `institutional_daily` |
| margin_core | `margin_daily` |
| foreign_holding_core | `foreign_holding` |
| shareholder_core | `holding_shares_per` |
| day_trading_core | `day_trading` |
| revenue_core | `monthly_revenue` |
| valuation_core | `valuation_daily` |
| financial_statement_core | `financial_statement` |
| us_market_core | `market_index_us` |
| taiex_core | `market_index_tw` |
| exchange_rate_core | `exchange_rate` |
| fear_greed_core | `fear_greed_index` |
| market_margin_core | `market_margin_maintenance` |

### 8.4 Indicator Core 範圍

每個指標一個獨立 Core,trait 統一。技術指標**不耦合波浪**——MACD Core 不知道 Neely 存在,Neely Core 不知道 MACD 存在。

### 8.5 Indicator Core 的「事實」邊界

**只做嚴格規則式事實**,不做經驗判斷式:

| ✅ 進 Core(嚴格規則式) | ❌ 不進 Core(經驗判斷式) |
|---|---|
| `MACD(12,26,9) golden cross at 2026-04-15` | `MACD 顯示動能轉強` |
| `RSI(14) = 78, > 70 for 5 consecutive days` | `RSI 進入超買區,警示 W5 末端` |
| `ADX = 32, +DI > -DI for 20 days` | `ADX 確認趨勢中` |
| `Histogram expanded 8 consecutive bars` | `動能加速,看好突破` |
| `Bearish divergence: price made HH at A, indicator made LH at B, within N bars` | `視覺上的不對勁` |

### 8.6 跨指標訊號(TTM Squeeze 等)的處理原則

**不為「跨指標訊號」設立獨立 Core**(已決策)。

例:TTM Squeeze 需要同時看布林通道與 Keltner Channel——這類訊號不寫成 `ttm_squeeze_core`,理由是違反零耦合。處理方式:

- `bollinger_core` 輸出事實:`bandwidth`、`upper_band`、`lower_band`
- `keltner_core` 輸出事實:`upper_band`、`lower_band`
- Aggregation Layer 並排呈現,使用者自己看出「布林收進 Keltner 內」
- 教學文件提供「如何看出 Squeeze」使用者指引(屬於 UI/教學層,不在架構層)

### 8.7 結構性指標的特殊例外:trendline_core

**原則上 Core 之間零耦合**,但 `trendline_core` 是已知的設計例外:

- 趨勢線需要先做 swing point 偵測,而 swing point 邏輯與 Neely Core 的 monowave detection 在演算法上重複
- **決策**:`trendline_core` 可消費 Neely Core 的 monowave 輸出,而非自行實作
- **代價**:trendline_core 對 neely_core 有讀取依賴(僅讀 monowave,不讀 scenario forest)
- **管控**:此例外需在 trendline_core 的 `Cargo.toml` 明確宣告 `depends_on = ["neely_core"]`,並在 V2 spec 列入「已知耦合」清單

替代方案(若無法接受耦合):把 swing point detection 抽出為 `shared/swing_detector/`,Neely Core 與 trendline_core 都消費 shared module。**第一版傾向直接消費 Neely Core 輸出,第二版視情況重構。**

---

## 九、命名規範

### 9.1 Workflow vs Orchestrator

```
從 user 視角 / 文件視角:Workflow
  例:「TW-Stock-Standard Workflow」、「Quick-Analysis Workflow」、「Deep-Analysis Workflow」

從代碼視角 / 模組命名:Orchestrator
  例:Rust crate 名 = `orchestrator`,struct 名 = `WorkflowOrchestrator`
```

### 9.2 目錄結構

```
cores/
├── neely_core/
│   ├── Cargo.toml
│   ├── src/
│   └── tests/
├── tw_market_core/
├── traditional_core/
├── indicators/
│   ├── macd_core/
│   │   ├── Cargo.toml      # 獨立 crate
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── compute.rs
│   │   │   └── facts.rs
│   │   └── tests/
│   ├── rsi_core/
│   ├── kd_core/
│   └── ...
├── chips/
│   ├── institutional_core/
│   ├── margin_core/
│   ├── foreign_holding_core/
│   ├── shareholder_core/
│   └── day_trading_core/
├── fundamentals/
│   ├── revenue_core/
│   ├── valuation_core/
│   └── financial_statement_core/
├── environment/
│   ├── us_market_core/
│   ├── taiex_core/
│   ├── exchange_rate_core/
│   ├── fear_greed_core/
│   └── market_margin_core/
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

---

## 十、Core 之間的耦合規範

### 10.1 ✅ 可共用(Shared Infrastructure,不是 Core)

```
shared/
├── ohlcv_loader/           # OHLCV 資料載入
├── timeframe_resampler/    # 日線→週線→月線聚合
├── fact_schema/            # Fact 統一資料結構
├── data_ref/               # 資料追溯機制
└── degree_taxonomy/        # 共用的 Degree 詞彙(僅 Neely Core 與少數 Core 用)
```

這些是基礎建設,不是 Core,本身不做任何體系判斷。

### 10.2 ❌ 禁止跨 Core 引用

- **Core 之間不直接 import**:MACD Core 不能 `use rsi_core`,Neely Core 不能 `use chips_core`
- **Core 不知道 Workflow 的存在**:每個 Core 是純函式黑盒,給定輸入產出輸出
- **Core 不互相觸發**:沒有「MACD Core 看到背離通知 RSI Core」這種事

如果發現有需要跨 Core 的邏輯,那是 **Workflow / Orchestrator** 該處理的事,不是 Core 的事。

### 10.3 資料相依處理(坑 1 → 採選項 A)

範例:**TW-Market Core 的「連續漲跌停合併」會改變 monowave 序列,但 Neely Core 也吃 monowave 序列**。

**採用選項 A**:TW-Market Core 在 Neely Core 之前執行,做資料前處理。

```
Raw OHLC → TW-Market Core (合併漲跌停) → 處理過的 OHLC → Neely Core
```

優點:Neely Core 完全不知道台股的存在,純淨。

### 10.4 守則:Aggregation Layer「並排不整合」

> **僅列出提供判斷**(用戶決策)

Aggregation Layer 不做「整合邏輯」,純粹把各 Core 的 output 並排呈現,讓 user 自己連線。

---

## 十一、Workflow / Orchestrator 設計

### 11.1 統一 Trait 介面(草案)

所有 Indicator Core 實作同一個 trait:

```rust
pub trait IndicatorCore: Send + Sync {
    type Params: Default + Clone + Serialize;  // r3 新增 Serialize 約束(供 params_hash 用)
    type Output: Serialize;                    // r3 新增 Serialize 約束(供 JSONB 寫入)

    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn compute(&self, ohlcv: &OHLCVSeries, params: Self::Params) -> Result<Self::Output>;
    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact>;

    /// r3 新增:Core 自宣告所需的暖機 K 線數,供 Batch Pipeline 取資料窗口
    fn warmup_periods(&self, params: &Self::Params) -> usize;
}
```

統一介面後,Orchestrator 對待 30 個 Core 跟 3 個 Core 沒差別。

### 11.2 Core Registry 與部署模型(r3 修正)

**部署模型決策:Monolithic Binary**

r2 的 11.2 用 `inventory::submit!`(編譯期靜態註冊)與 11.6 坑 2「Core 版本相容性檢查」(runtime 機制)概念上矛盾。r3 明確選定:

> **P0 / P1 / P2 階段一律採 Monolithic Binary 部署模型**:所有 Core 編譯在同一 workspace 同一 binary,inventory 收集生效,版本檢查由編譯期保證,不需 runtime check。

```rust
// 每個 Core 在自己的 lib 裡註冊
inventory::submit! {
    CoreRegistration::new("macd_core", "1.0.0", || Box::new(MacdCore::new()))
}

// Orchestrator 啟動時自動發現所有已編譯的 Core
let registry = CoreRegistry::discover();
```

新增 Core 不用改 Orchestrator 代碼,**只要寫好 Core 並編譯進去,自動可用**。

**Monolithic 的代價(誠實標註)**:
- 改任一 Core 需重編全部(實測 ~5 分鐘可接受)
- 無法 hot-fix 單一 Core,但台股一日一交易、batch 模式下沒有這個需求
- Core 不能各自 versioning 對外發版,但 v2.0 沒有第三方 Core 生態,不需要

**未來重議條件**:當以下三條至少滿足兩條,才考慮升級到 Dynamic Loading 或 Subprocess + IPC:
1. Core 數量超過 50 且改動頻率高(目前 36 個,且穩定後改動率低)
2. 出現第三方 Core(社群開發者貢獻 Core)
3. 出現必須 hot-fix 的線上場景(目前 batch 模式不存在)

**取消 11.6 坑 2 的 runtime version check**:由於 Monolithic 編譯期已對齊,r2 11.6 坑 2 描述的「Cargo.toml semver + Orchestrator 啟動時版本相容性檢查」**取消**。`source_version` 欄位仍然寫進 indicator_values / structural_snapshots / facts 三表,但用途改為「審計追溯」(回查某筆資料是哪個版本算出),不做 runtime 相容性檢查。

### 11.3 Workflow Declarative 設定(草案)

```toml
# workflows/tw_stock_standard.toml
name = "Standard TW Stock Analysis"
version = "1.0.0"

[wave_cores]
include = ["neely_core"]

[market_cores]
include = ["tw_market_core"]

[indicator_cores]
include = ["macd", "rsi", "kd", "adx", "ma", "bollinger", "obv", "atr"]
exclude = []

[chip_cores]
include = ["institutional", "margin", "foreign_holding"]

[fundamental_cores]
include = ["revenue", "valuation"]

[environment_cores]
include = ["taiex", "us_market", "fear_greed"]
```

User 可自訂 workflow,要哪些 Core 就 include 哪些。

**r3 補充:`indicator_cores` 的 toml 寫法以 16.3 為準**

11.3 此處的範例是簡化版(僅展示 Core 清單),**真正使用的格式是 16.3 的 table-array 寫法**,因為 indicator 必須帶參數(timeframe / 週期等):

```toml
[[indicator_cores]]
name = "macd"
params = { fast = 12, slow = 26, signal = 9, timeframe = "daily" }

[[indicator_cores]]
name = "ma"
params = { periods = [5, 20, 60], timeframe = "daily" }
```

字串陣列 `include = ["macd", ...]` 只在 `wave_cores / market_cores / chip_cores / fundamental_cores / environment_cores` 等**無參數變化**的 Core 群組使用。

### 11.4 執行順序與兩條路徑

#### 11.4.1 Batch 路徑(每日收盤後,主要計算流)

```
[每日 21:00 觸發]
    ↓
1. Collector 寫入 raw 資料(ohlc / chips / fundamentals / environment)
    ↓
2. TW-Market Core(資料前處理:漲跌停合併、還原指數)
    ↓
3. 對每檔股票並行:
   ├─ Neely Core / Traditional Core(全量重算 → 寫 structural_snapshots)
   ├─ Indicator Cores(取最近 N 天 + 暖機區 → 算 → UPSERT indicator_values)
   ├─ Chip Cores(每日新事實 → INSERT facts)
   ├─ Fundamental Cores(視更新頻率)
   └─ Environment Cores(每日同步)
    ↓
4. 所有 Core 完成後,Batch 結束
    ↓
[DB 即真相,等待即時請求]
```

#### 11.4.2 即時請求路徑(使用者打 API)

```
前端請求 (stock_id, workflow, display_range)
    ↓
Aggregation Layer:
    ├─ SELECT indicator_values WHERE stock_id, date BETWEEN ...
    ├─ SELECT structural_snapshots WHERE stock_id, snapshot_date = latest
    ├─ SELECT facts WHERE stock_id, fact_date BETWEEN ...
    ↓
組裝為標準 Output 結構
    ↓
回傳前端(純資料,無計算)
```

**關鍵原則**:即時路徑**完全不呼叫 Core**,純讀 DB。

#### 11.4.3 On-demand 補算(例外路徑)

當使用者請求 workflow 預設外的指標參數組合(例:預設只算 MACD(12,26,9),使用者想看 MACD(5,35,5)):

```
即時路徑檢測 cache miss
    ↓
觸發單檔單指標 on-demand 計算(非 batch)
    ↓
Core 算完 → UPSERT indicator_values
    ↓
之後 batch 自動納入該參數組合
```

### 11.5 Core 是 stateless 純函式

範例:同一個 macd_core 可能被呼叫多次:

```rust
// Workflow 內
let macd_daily = macd_core.compute(daily_ohlcv, MacdParams::default());
let macd_weekly = macd_core.compute(weekly_ohlcv, MacdParams::default());
let macd_daily_short = macd_core.compute(daily_ohlcv, MacdParams { fast: 5, slow: 35, signal: 5 });
```

三次都產出獨立的 `MacdCoreOutput`,Aggregation Layer 並排顯示。

### 11.6 工程坑提醒

#### 坑 1:Core 之間資料相依 → 採選項 A(已決策)

#### 坑 2:Core 版本相依 ~~已隨 11.2 Monolithic 決策解決,r3 取消~~

r2 此坑提案「Cargo.toml semver + Orchestrator 啟動時 version compatibility check」,在 11.2 採 Monolithic 後失去意義(編譯期已對齊)。`source_version` 欄位仍寫進三表,但僅作審計追溯,不做 runtime 相容性檢查。

#### 坑 3:Aggregation Layer「並排不整合」誘惑 → 採「僅列出提供判斷」(已決策)

需要在 Aggregation Layer 代碼註解明寫,並有 lint rule 或 code review 把關。

**r3 補充強制檢查機制**:
1. `AggregatedOutput` struct 不得有 `score` / `rank` / `primary` / `confidence` / `weight` 等欄位名
2. CI 加 lint:`grep -rE "(score|rank|primary|confidence|weight)" src/aggregation/` 命中即 fail
3. property-based test 驗證 Aggregation 是純資料組裝(同樣輸入永遠輸出 byte-identical 結果)

---

## 十二、Aggregation Layer 設計

### 12.1 統一 Fact Schema(草案)

```rust
Fact {
    category: String,           // "chips" / "momentum" / "fundamentals"
    statement: String,          // "外資連續 8 日淨買超共 12,345 張"
    data_references: Vec<DataRef>,  // 可追溯到具體資料列
    computed_at: DateTime,
    source_core: String,        // 來自哪個 Core
    source_version: String,     // Core 版本
}
```

統一結構讓 Aggregation Layer 處理乾淨,所有 Core 的事實都用同一個結構。

### 12.2 呈現原則

- **僅列出提供判斷**(用戶決策)
- 各 Core output 並列顯示
- 提供 facet filter 讓 user 折疊不關心的 Core
- 不算總分、不下結論、不做加權整合

### 12.3 即時路徑職責(Batch 模式下)

Aggregation Layer 在 v2.0 的職責是**讀 DB + 組裝**,不呼叫 Core:

```rust
pub async fn aggregate(
    stock_id: &str,
    workflow: &Workflow,
    display_range: TimeRange,
) -> Result<AggregatedOutput> {
    // 1. 讀 raw indicator values
    let indicators = db.fetch_indicators(stock_id, workflow.indicator_specs(), display_range).await?;
    
    // 2. 讀 latest structural snapshot
    let snapshots = db.fetch_latest_snapshots(stock_id, workflow.structural_cores()).await?;
    
    // 3. 讀 facts in range
    let facts = db.fetch_facts(stock_id, display_range).await?;
    
    // 4. 並排組裝(不整合、不加權、不計算)
    Ok(AggregatedOutput {
        indicators,
        snapshots,
        facts,
        metadata: build_metadata(workflow),
    })
}
```

**禁止項**:
- ❌ 在此層做任何指標計算
- ❌ 跨 Core 衍生新欄位
- ❌ 加權、評分、排序

---

## 十三、尚未決策的問題

以下問題已提出,但用戶尚未明確決策,在副本實作時需先確認:

### 13.1 第一版 Indicator Cores 範圍 [已決策]

**決策**:採分階段做法,對齊四個預測週期 workflow 的需求(見第十八章)。

- **P1(第一版必做,實質 8 個 Core)**:`macd`、`rsi`、`kd`、`adx`、`ma`、`bollinger`、`atr`、`obv`,加上 `volume`(底圖,**不獨立成 Core**,Aggregation 直接讀 raw 表,見 8.1 補充)
- **P2(第二版,5 個)**:`keltner`、`donchian`、`candlestick_pattern`、`support_resistance`、`vwap`
- **P3(後續,5 個)**:`ichimoku`、`williams_r`、`mfi`、`cci`、`coppock`、`trendline`

P1 滿足月/季/半年/年四個 workflow 的核心需求;P2 補強波動層與型態/價位層;P3 為長期擴充。


### 13.2 籌碼 Core 切分粒度 [已決策]

**決策**:5 個獨立 Core(institutional / margin / foreign_holding / shareholder / day_trading)

理由:符合「單一職責、切乾淨 by module」原則(1.3 條),每個籌碼資料源語義獨立、
更新頻率與 Schema 不同,合併會造成跨資料源耦合。


### 13.3 開發語言 [已決策]

**決策**:Rust + Python 混合(維持 v1.1 路線)

- Rust:Wave Cores / Indicator Cores / Chip Cores 等純計算層(透過 PyO3 暴露)
- Python:Aggregation Layer / Orchestrator / API / Batch Pipeline 排程

理由:Rust 提供計算層的型別安全與效能;Python 在排程、資料整合、Web API 生態
更成熟。混合架構已在 v1.1 驗證可行。

**r3 補充:PyO3 邊界與序列化規範**

確定資料如何跨 Rust ↔ Python,避免 P0 多人開發時各寫各的:

| 項目 | 決策 |
|---|---|
| 邊界形式 | `serde_json::to_string` + Python `json.loads` |
| Rust 端 | 所有 Core Output struct 必須 `#[derive(Serialize)]`,公開函式回傳 JSON 字串 |
| Python 端 | 用 pydantic 重新定義對應 schema,做 runtime 驗證(只在開發模式啟用,production 跳過以省成本) |
| 異常處理 | Rust 端 `Result<T, CoreError>` 失敗時 panic 改為 unwind,Python 端 catch `pyo3::PyException` |
| Numpy 互通 | OHLCV 輸入用 `pyo3-numpy` 零拷貝傳遞 `&[f64]`,避免大 array 序列化 |
| 並行 | Python 端用 `multiprocessing` 切股票分片,**不**在 Python 端做 thread + GIL,Rust 端內部可用 rayon |

**為何選 JSON 而非 PyO3 native classes**:
- Core Output 結構會迭代,native classes 每次改 struct 都要改 PyO3 binding,維護成本高
- batch 模式一日一次,JSON 序列化慢 100ms 在 1800 檔 × N Core 累計仍可接受
- Debug 時 JSON 可讀性最佳

**P2 後若效能瓶頸出現,升級路徑**:
- 第一階段:OHLCV 仍 zero-copy,Output 改 msgpack 或 bincode(預期 3–5x 加速)
- 第二階段:`#[pyclass]` 把熱路徑 Output 改 native class
- 第三階段:整條 batch 路徑搬進 Rust(對應「設計建議 A」)

**禁止項**:
- ❌ 跨邊界傳遞 Rust 原生 mutable reference
- ❌ 在 Python 端做計算(Python 只做排程、組裝、API)
- ❌ Core 函式回傳 Python-specific 型別,必須用通用序列化格式


### 13.4 Aggregation Layer 呈現方式 [已決策]

**決策**:D — Aggregation Layer 提供 facet filter,user UI 上自選

理由:
- 不違反「並排不整合」原則(2.3 條):filter 只折疊顯示,不改變底層資料
- user 自主決定關注哪些 Core,符合「user 是決策者」定位(1.2 條)
- 比方案 C(workflow toml 指定)更彈性:同一 workflow 結果可被不同
  使用情境消費,filter 屬展示層而非計算層

實作:Aggregation Layer 回傳完整 AggregatedOutput,前端依 user UI 選擇折疊
顯示。後端不依 facet 改變查詢內容,避免快取碎片化。


### 13.5 第一版 Core 優先順序 [部分決策]

**P0(必做,基礎設施)**:Neely Core、TW-Market Core、Aggregation Layer、Orchestrator、Storage Layer、Batch Pipeline

**P1(高價值,推薦)**:
- Indicator Cores 9 個(見 13.1)
- Chips Cores:institutional + margin + foreign_holding

**P2(中等)**:
- Indicator Cores 5 個(見 13.1)
- Fundamental Cores:revenue + valuation
- Environment Cores:taiex + us_market

**P3(進階,先卡位)**:
- Traditional Core
- 剩餘 Indicator Cores
- 剩餘 Chip / Fundamental / Environment Cores
- Industry Core
- Learner 離線模組(見附錄 C)

### 13.6 v1.1 Item 11-15(Presentation Layer)的去處 [已決策]

**決策**:獨立成前端模組,不在 Pipeline 範圍。

理由:Batch 模式下,後端職責止於 DB 與 API。圖表渲染、Fibonacci 視覺化、雙色折線等屬前端職責,前端純讀 Aggregation Layer 提供的 API,自行組合圖層。詳見第十九章「前端職責邊界」。

### 13.7 v1.1 Item 19-21(Calibration)的去處 [已決策]

**ATR 校準依據說明**:
ATR 在 Neely 體系中是 Rule of Proportion / Neutrality / 45° 判定的「計量單位」,
Neely 原書未指定 atr_period 具體數值,僅要求「representative period」。
v1.1 的「ATR 自動校準」是 v1.1 自加的工程優化,任何校準目標函數
(monowave 數量合理性、噪訊比、回測勝率)皆屬主觀判準,違反本文件 1.3、5.3 原則。

**決策**:
- atr_period 預設值 = 14(Wilder 1978《New Concepts in Technical Trading Systems》原始定義,
  且為技術分析界事實標準,Neely 書中範例亦多落於此區間,屬「約定俗成的工程慣例」非主觀調參)
- 不做自動校準,保留 NeelyEngineConfig.atr_period 為手動工程參數
- 文件中明確標註「此為工程選擇,非 Neely 規則,改動影響 monowave 切割粒度但不影響規則本身」
- Scorer 權重校準 → 不需要(7 因子改獨立事實向量,無權重)
- Sanity Check → 改造,每個 Core 各自的測試集
- Learner(附錄 C)可離線觀察 atr_period 分佈,但不回寫自動校準閉環

**atr_period 跨 timeframe 統一性**:
- 統一使用 atr_period = 14,不論日線/週線/月線
- 依據:
  (a) Wilder 1978 原始定義為 14,未為不同 timeframe 區分
  (b) Neely 跨 timeframe 使用相同「representative period」的隱含立場
  (c) ATR 的 N 指 K 線根數而非日曆天數,timeframe 切換時 K 線長度
      已自動隨之放大,無需再調整 N
  (d) 跨 timeframe 統一 = 保持 monowave significance 計量單位一致,
      利於跨時框比較分析
- 不允許 workflow toml 覆寫此值(開放即引入主觀調參空間,違反 1.3 原則)
- Storage Layer params_hash 不納入 atr_period(常數,無快取污染)

---

### 13.8 P0 必須決策清單(r3 新增)

以下 8 項是 r2 文件已暗示但未定案、P0 動工第一週就會撞到的工程決策。r3 在此給出明確答案,並 cross-link 到本文件對應章節的詳細展開。

| # | 主題 | r3 決策 | 詳見章節 |
|---|---|---|---|
| 1 | PyO3 邊界與序列化格式 | JSON via serde + Python pydantic 驗證,OHLCV 用 numpy zero-copy | 13.3 補充 |
| 2 | Core 部署模型 | Monolithic Binary,inventory 收集,P3 前不重議 | 11.2 |
| 3 | params_hash 演算法 | canonical JSON keys ASC + blake3 → 取前 16 hex 字元 | 14.2.2 補充 |
| 4 | Storage Partition + Retention | 兩層 partition(年 RANGE + stock_id HASH×8),熱 5 年 SSD,冷 5 年 OLAP | 14.5 |
| 5 | Forest 上限與逾時 | `forest_max_size = 1000`(暫定)+ 60s timeout + BeamSearchFallback{k:100} | 7.5 |
| 6 | on-demand single-flight | Postgres advisory lock + double-check + rate limit(IP 20/min,stock 100/min) | 15.6 |
| 7 | Fact 去重 unique constraint | 複合 unique:(stock_id, fact_date, timeframe, source_core, params_hash, md5(statement)) | 14.2.4 補充 |
| 8 | Stage 4 拓撲拆分 | 拆 4a(獨立)/ 4b(消費 4a 結果),trendline_core 必在 4b | 15.1 |

**狀態欄位約定**:
- 第 1 / 2 / 3 / 7 / 8 項為**最終決策**,r3 已給出實作層細節
- 第 4 項為**結構決策確定 + 細節待調**,partition 切法已定,具體 sub-partition 數量視實際資料分佈微調
- 第 5 / 6 項為**結構決策確定 + 預設值待實測**,參數預設值需依五檔測試結果(見 13.9)修正

### 13.9 五檔股票實測指引(r3 新增)

P0 動工前必須完成的數值實驗,用以校準 13.8 第 5 項的預設值。

**測試標的(5 檔,涵蓋不同型態複雜度)**:

| 股票代號 | 名稱 | 預期型態複雜度 |
|---|---|---|
| 0050 | 元大台灣50 | 低(長期穩定上行) |
| 2330 | 台積電 | 中(多階段結構) |
| 3363 | 上詮 | 中高(五浪推升 + ABC 修正) |
| 6547 | Medigen | 高(暴漲暴跌、結構破碎) |
| 1312 | 國喬 | 高(橫盤震盪) |

**測試流程**:
1. 載入各檔最近 5 年日線 OHLCV(約 1250 K 線)
2. 用「v2.0 模式 = Compaction 不剪枝、`OverflowStrategy::Unbounded`」執行 Neely Core
3. 紀錄每檔的 forest_size、compaction_paths 數量、elapsed 秒數、peak memory MB
4. 計算 P50 / P95 / P99 統計量

**閾值對照**:

| 指標 | 綠燈(預設值合理) | 黃燈(需調預設值) | 紅燈(策略需重議) |
|---|---|---|---|
| P95 forest_size | < 200 | 200–1000 | > 1000 |
| P95 elapsed_secs | < 5 | 5–30 | > 30 |
| P95 memory_mb | < 50 | 50–200 | > 200 |

**處置**:
- 全綠 → 7.5 預設值(forest_max_size=1000、k=100)維持,P0 啟動
- 黃燈 → 預設值依實際 P95 調整(例:實測 P95=600,可降至 800 上限)
- 紅燈 → 7.2 不剪枝決策需回頭重議,Compaction 演算法本身可能要改

**輸出**:測試結果 JSON 提交給專案文件 `docs/benchmarks/forest_size_2026-XX-XX.json`,作為 7.5 預設值決策的支撐證據,寫入附錄 A 的決策歷史。

---

## 十四、儲存層架構

### 14.1 設計原則

> Core 計算結果進 DB,即時請求純讀,前端純展示

v2.0 採用 **Batch 預算 + DB 存讀分離** 架構:

1. 每日收盤後 batch 把所有 Core 算完,結果寫 Storage Layer
2. 即時請求路徑只讀 DB,不觸發任何 Core
3. Learner 直接讀 DB,無需額外 dump 管線

### 14.2 儲存表分類

#### 14.2.1 Layer 1:Raw 資料(由 Collector 寫入)

延用既有 Collector 表結構,不變動:

- `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd`(還原股價)
- `institutional_daily` / `margin_daily` / `foreign_holding` / `holding_shares_per` / `day_trading`
- `monthly_revenue` / `valuation_daily` / `financial_statement`
- `market_index_us` / `market_index_tw` / `exchange_rate` / `fear_greed_index` / `market_margin_maintenance`
- `price_limit` / `price_adjustment_events`

#### 14.2.2 Layer 2:Indicator Values(每日 Batch 寫入)

```sql
CREATE TABLE indicator_values (
    stock_id        VARCHAR(10) NOT NULL,
    date            DATE        NOT NULL,
    timeframe       VARCHAR(10) NOT NULL,   -- daily / weekly / monthly
    indicator_name  VARCHAR(50) NOT NULL,   -- macd / rsi / kd / ...
    params_hash     VARCHAR(16) NOT NULL,   -- r3 修正:VARCHAR(32) → VARCHAR(16),演算法見 14.2.2 補充
    values          JSONB       NOT NULL,   -- 該指標當天的所有值
    core_version    VARCHAR(20) NOT NULL,
    computed_at     TIMESTAMP   NOT NULL,
    PRIMARY KEY (stock_id, date, timeframe, indicator_name, params_hash)
);

CREATE INDEX idx_indicator_lookup
    ON indicator_values (stock_id, indicator_name, params_hash, date DESC);
```

**JSONB values 範例**:
```json
// MACD
{"dif": 12.34, "macd": 10.21, "histogram": 2.13}

// KD
{"k": 78.5, "d": 72.3}

// 布林通道
{"upper": 660.5, "middle": 642.0, "lower": 623.5, "bandwidth": 0.0577}
```

**為何用 JSONB**:不同指標欄位數差異大(MACD 3 欄、KD 2 欄、Ichimoku 5 欄),用 JSONB 避免每個指標一張表的維護成本。代價是查詢時需 JSONB 解析,但 PostgreSQL 對 JSONB 有原生支援,效能可接受。

**r3 補充:`params_hash` 演算法定義**

`params_hash` 是 indicator_values 主鍵的一部分,演算法錯誤會直接導致 cache 分裂。固定演算法:

```rust
fn compute_params_hash<P: Serialize>(params: &P) -> String {
    // Step 1: 序列化為 serde_json::Value
    let value = serde_json::to_value(params).unwrap();
    // Step 2: 遞迴排序所有 object keys(canonical form)
    let canonical = canonicalize_json(&value);
    // Step 3: 序列化 canonical Value 為字串
    let canonical_str = serde_json::to_string(&canonical).unwrap();
    // Step 4: blake3 hash → 取前 16 hex 字元
    let hash = blake3::hash(canonical_str.as_bytes());
    hash.to_hex().as_str()[..16].to_string()
}
```

**r3 細節決策**:

| 項目 | 決策 | 理由 |
|---|---|---|
| 序列化格式 | canonical JSON(keys ASC 排序) | 跨語言通用,Python pydantic 端可重算驗證 |
| 浮點數處理 | 序列化時保留 6 位小數,避免 `0.1 + 0.2 = 0.30000000000000004` 問題 | 確保 Rust ↔ Python 兩端 hash 一致 |
| Hash 演算法 | blake3 取前 16 hex(64-bit) | 比 sha256 快 5–10x,不需密碼學強度;碰撞機率(1 億組合下 ~2.7e-12)夠用 |
| 欄位定義 | `VARCHAR(16)` | r2 的 `VARCHAR(32)` 多餘,改 16 節省空間 |
| atr_period 不納入 | 13.7 已決策 | 常數無 cache 污染 |
| timeframe 不納入 hash | 因為 timeframe 已是主鍵獨立欄位 | 避免重複編碼 |

**r3 修正 schema(VARCHAR 長度調整)**:

```sql
-- 原 r2:params_hash VARCHAR(32)
-- r3 修正:params_hash VARCHAR(16)
```

#### 14.2.3 Layer 3:Structural Snapshots(結構性指標)

```sql
CREATE TABLE structural_snapshots (
    stock_id        VARCHAR(10) NOT NULL,
    snapshot_date   DATE        NOT NULL,
    timeframe       VARCHAR(10) NOT NULL,
    core_name       VARCHAR(50) NOT NULL,   -- neely_core / fib_zones / sr_levels
    snapshot_data   JSONB       NOT NULL,   -- 完整結構快照
    core_version    VARCHAR(20) NOT NULL,
    computed_at     TIMESTAMP   NOT NULL,
    PRIMARY KEY (stock_id, snapshot_date, timeframe, core_name)
);
```

存放每日重算的 Wave Forest、Fibonacci zones、Support/Resistance levels 等**全域性結構**。每天一張完整快照,歷史可回溯。

**範例 snapshot_data**:
```json
// neely_core 的 Wave Forest
{
  "scenario_forest": [
    {
      "id": "S1",
      "wave_tree": {...},
      "neely_power_rating": 2,
      "passed_rules": ["R1", "R2", "R3"],
      "deferred_rules": ["R4"],
      "invalidation_triggers": [...],
      "structural_facts": {...}
    },
    {"id": "S2", ...},
    {"id": "S3", ...}
  ],
  "diagnostics": {...}
}
```

#### 14.2.4 Layer 4:Facts(事件式紀錄)

```sql
CREATE TABLE facts (
    id              BIGSERIAL PRIMARY KEY,
    stock_id        VARCHAR(10) NOT NULL,
    fact_date       DATE        NOT NULL,
    timeframe       VARCHAR(10) NOT NULL,
    category        VARCHAR(50) NOT NULL,   -- momentum / trend / chips / volume / ...
    statement       TEXT        NOT NULL,   -- "MACD(12,26,9) golden cross"
    source_core     VARCHAR(50) NOT NULL,
    source_version  VARCHAR(20) NOT NULL,
    params_hash     VARCHAR(16),            -- r3 修正:VARCHAR(32) → VARCHAR(16)
    data_references JSONB,                  -- 可追溯到具體資料列
    computed_at     TIMESTAMP   NOT NULL
);

CREATE INDEX idx_facts_stock      ON facts (stock_id, fact_date DESC);
CREATE INDEX idx_facts_category   ON facts (category, fact_date DESC);
CREATE INDEX idx_facts_core       ON facts (source_core, params_hash, fact_date DESC);

-- r3 新增:複合 unique constraint,防止 batch 重跑或 on-demand 重算造成 fact 重複
-- 因為 statement 是 TEXT 直接 unique 太大,改 hash 後再 unique
CREATE UNIQUE INDEX idx_facts_unique
    ON facts (stock_id, fact_date, timeframe, source_core,
              COALESCE(params_hash, ''), md5(statement));
```

**append-only**,不更新不刪除。Learner 訓練的主要資料源。

**r3 補充:idempotency 寫入規範**

所有 INSERT 必須帶 `ON CONFLICT DO NOTHING`,避免 batch 重跑或 on-demand 補算重複偵測同一 fact 時報錯:

```sql
INSERT INTO facts (
    stock_id, fact_date, timeframe, category, statement,
    source_core, source_version, params_hash, data_references, computed_at
) VALUES (...)
ON CONFLICT (stock_id, fact_date, timeframe, source_core,
             COALESCE(params_hash, ''), md5(statement))
DO NOTHING;
```

理由:
- batch 中斷 resume(15.4)時,已 INSERT 的 fact 不會重複
- on-demand 補算 MACD 順便偵測金叉,batch 又算一次,fact 不會重複
- `params_hash` 可能為 NULL(籌碼類 fact 沒有指標參數),用 `COALESCE` 避免 NULL 比對問題

**為什麼用 md5(statement) 而非 statement 直接 unique**:
- statement 是 TEXT,可能很長(含詳細描述),直接 unique index 浪費空間
- md5 取 hash 既保證一致又節省空間(128-bit)
- 不需要密碼學強度,md5 衝突機率夠低

### 14.3 儲存量估算(台股 1800 檔,10 年)

**r3 修正**:r2 估算 indicator_values 約 3.78 億 row 偏低。實際 P1 啟用後依 13.1 修正後的 8 個 indicator core(P2 加 5 個 = 13)、16.4 允許多次 include 同 Core(實測平均約 4 組 params_hash)、3 個 timeframe 重新試算。

| 表 | r2 估算 | r3 重新試算 | 試算依據 |
|---|---|---|---|
| Raw OHLC daily | 450 萬 | 450 萬 | 無變化 |
| indicator_values | 3.78 億 | **8.4–13 億** | 1800 × 2500 × 13 indicator × 4 params × 3 timeframe ≈ 7.0 億(P2 完整態);加 P3 後到 13 億 |
| structural_snapshots | 1350 萬 | 1350 萬 | 無變化(結構性 Core 數不變) |
| facts | 2–5 億 | 2–5 億 | 無變化 |

**結論**:r3 試算 indicator_values 比 r2 高約 2–3.4 倍,**Postgres 仍可處理但必須加 partition**(見 14.5)。不加 partition 將在 P1 上線 6 個月後出現 index 失效、VACUUM 跑不完等問題。

### 14.4 增量計算策略

每日 Batch 不需要每天從頭算 10 年,只需算最近一段:

#### 14.4.1 滑動窗口型指標(MA / MACD / RSI / KD / ADX / 布林 / Keltner / Donchian / ATR / OBV)

```
每日 Batch:
  1. 取「最近 N 天 + 暖機區」資料(例:N=30,暖機區=240)
  2. Core 算 → 取最近 N 天結果
  3. UPSERT indicator_values
```

暖機區大小由 Core 自行宣告(透過 IndicatorCore trait 的 `warmup_periods()` method,見 11.1,r3 新增)。

#### 14.4.2 結構性指標(Neely / SR / Trendline)

每天**全量重算**,寫入 `structural_snapshots`。

**r3 修正**:
- r2 結構性指標清單寫「Neely / Fib / SR」,Fibonacci 已確認為 Neely 子模組(5.2 補充),不獨立全量重算。實際清單:`neely_core`、`support_resistance_core`、`trendline_core`(P3+)
- r2 估算「1800 檔 × ~1 秒 = 30 分鐘」與 7.5 Forest 上限保護後的實際耗時可能不一致,需依 13.9 五檔測試結果修正
- 暫時保守估算:單檔 5–15 秒(依複雜度),1800 檔 × 16 並行 worker ≈ **1.5–4.5 小時**,不是 30 分鐘

理由:這類指標一根新 K 線可能改寫整段歷史解讀(例:跌破前低讓 W4 變 W1 頂),無法增量。

#### 14.4.3 Fact 偵測

只算「上次 Fact 截止日 → 今天」這段新事件,append-only,並依 14.2.4 unique constraint 防重複。

### 14.5 Partition + Retention 策略(r3 新增)

#### 14.5.1 Partition 切法

**indicator_values**:兩層 partition

```sql
-- 第一層:RANGE BY date(每年一個 partition)
CREATE TABLE indicator_values (
    stock_id        VARCHAR(10) NOT NULL,
    date            DATE        NOT NULL,
    timeframe       VARCHAR(10) NOT NULL,
    indicator_name  VARCHAR(50) NOT NULL,
    params_hash     VARCHAR(16) NOT NULL,
    values          JSONB       NOT NULL,
    core_version    VARCHAR(20) NOT NULL,
    computed_at     TIMESTAMP   NOT NULL,
    PRIMARY KEY (stock_id, date, timeframe, indicator_name, params_hash)
) PARTITION BY RANGE (date);

-- 第二層:HASH BY stock_id(每年內 8 個 sub-partition)
CREATE TABLE indicator_values_2026 PARTITION OF indicator_values
    FOR VALUES FROM ('2026-01-01') TO ('2027-01-01')
    PARTITION BY HASH (stock_id);

CREATE TABLE indicator_values_2026_p0 PARTITION OF indicator_values_2026
    FOR VALUES WITH (modulus 8, remainder 0);
-- ...重複到 p7
```

**理由**:
- 第一層按 date 切,使典型查詢「某股最近半年」只命中 1 個 partition
- 第二層按 stock_id hash 切,讓 batch 寫入並行不互相鎖
- 8 個 hash sub-partition 對應 batch 16 worker 中的 2 worker,每年內 8 個檔案大小均衡

**facts**:單層 partition,按 fact_date RANGE 切

```sql
CREATE TABLE facts (
    -- 欄位同前
) PARTITION BY RANGE (fact_date);

CREATE TABLE facts_2026 PARTITION OF facts
    FOR VALUES FROM ('2026-01-01') TO ('2027-01-01');
```

**structural_snapshots**:量小,先**不 partition**,P3 後視情況加。

#### 14.5.2 Retention 策略

| 資料層 | 熱資料(SSD) | 冷資料(物件儲存) | 永久保留 |
|---|---|---|---|
| Raw OHLC | 最近 5 年 | 5–10 年 parquet 壓縮 | 月線降解析度永久 |
| indicator_values | 最近 5 年(2021–2026) | 5–10 年(2016–2020)parquet | 不保留 |
| structural_snapshots | 最近 3 年完整 + 5–3 年僅當月最後一筆 | 不保留 | 不保留 |
| facts | 全部保留(append-only,Learner 訓練核心資料) | 5 年以上轉冷儲存 | — |

**冷遷移執行**:每年 1 月由 ops job 執行,熱 partition `pg_dump` 到 parquet 後 `DROP PARTITION`。

#### 14.5.3 索引策略

```sql
-- 主要查詢路徑:某股某指標的時間序列
CREATE INDEX idx_indicator_lookup
    ON indicator_values (stock_id, indicator_name, params_hash, date DESC);

-- JSONB 內部欄位查詢(若需要):用 jsonb_path_ops index
-- 例:查詢所有 MACD histogram > 0 的紀錄
CREATE INDEX idx_indicator_values_jsonb
    ON indicator_values USING GIN (values jsonb_path_ops);
```

JSONB GIN index 在 P0 不建立,**P2 視實際查詢 pattern 再評估是否需要**。

---

## 十五、Batch Pipeline 設計

### 15.1 整體流程

```
[每日 21:00 觸發]
    ↓
Stage 1: Collector(若未在其他時段執行)
    └─ 抓取當日 OHLC、籌碼、基本面、環境資料 → Layer 1 Raw 表
    ↓
Stage 2: TW-Market 前處理
    └─ 漲跌停合併、還原指數計算 → 處理過的 OHLC
    ↓
Stage 3: Indicator Cores(滑動窗口型,並行)
    └─ for each (stock, indicator, params, timeframe):
        取最近 N 天 + 暖機區 → 算 → UPSERT indicator_values
    ↓
Stage 4a: 獨立結構性 Cores(無依賴,並行)— r3 拆分
    ├─ Neely Core(全量重算 → INSERT structural_snapshots core_name='neely_core')
    │   ├─ 含 Fibonacci 子模組,輸出在 Scenario.expected_fib_zones
    │   └─ 受 7.5 forest_max_size 與 timeout 保護
    ├─ Traditional Core(P3 才上)
    └─ Support/Resistance Core(P2)
    ↓
Stage 4b: 依賴 4a 結果的結構性 Cores(讀 4a snapshot,並行)— r3 新增
    ├─ Trendline Core(消費 Neely monowave,P3)
    └─ Fibonacci 投影視圖(可選,從 Neely scenario 投影為 fib_zones snapshot)
    ↓
Stage 5: Chip / Fundamental / Environment Cores(並行)
    └─ 各自讀 raw → 算 → UPSERT 對應表
    ↓
Stage 6: Fact 偵測(並行,跨所有 Core,ON CONFLICT DO NOTHING)
    └─ 偵測新事件 → INSERT facts
    ↓
[Batch 完成,DB 即真相]
```

**Stage 4a/4b 拆分理由(r3)**:
- r2 把 Neely / Trendline 並列在同一 Stage 4,但 8.7 已決策 trendline_core 消費 neely_core monowave,實際存在依賴
- 拆 4a / 4b 後依賴順序明確,Pipeline 編排框架可清楚表達拓撲
- 4a 內部仍可並行(Neely / Traditional / SR 之間零耦合);4b 內部也可並行

### 15.2 並行策略

- **同 Stage 內**:依股票切片並行(例:1800 檔分 16 worker,每 worker 處理 ~112 檔)
- **跨 Stage**:有依賴關係不可並行(Stage 2 必須先於 Stage 3、Stage 4)
- **Core 之間**(同 Stage):理論上可並行(零耦合),實務上建議按 Core 分組以利監控

### 15.3 時間預算(估算,1800 檔)

**r3 修正**:r2 估算的 Neely Core 1–2 秒/檔過於樂觀,實際窮舉 Forest 模式下單檔耗時可能 5–15 秒。以下為 r3 修正後的**保守估算**,實際數字需依 13.9 五檔股票測試結果替換:

| Stage | 單檔耗時(r3 保守) | 並行後總耗時(16 worker) |
|---|---|---|
| Stage 2:TW-Market | ~5ms | < 1 分鐘 |
| Stage 3:Indicator(P1 8 個 + 多參數 + 3 timeframe ≈ 50 次計算) | ~200ms | ~25 分鐘 |
| Stage 4a:Neely Core(含 Fib 子模組,受 7.5 上限保護) | ~5–15 秒 | **~1.5–4.5 小時** |
| Stage 4a:Traditional / SR | ~200ms | ~5 分鐘 |
| Stage 4b:Trendline / Fib 投影(P3) | ~100ms | ~3 分鐘 |
| Stage 5:Chip / Fund / Env | ~50ms | ~10 分鐘 |
| Stage 6:Fact 偵測 | ~100ms | ~15 分鐘 |
| **總計** | — | **~2.5–5.5 小時(P1 全量)** |

收盤後 21:00 啟動,**最壞情況凌晨 02:30 完成,前端隔日早上純讀 DB**。

**若實測超過 6 小時**,啟動以下緩解策略:
1. 增加 Stage 4a 並行度(從 16 提到 32 worker)
2. 引入 Stage 4a 增量化(僅重算 N 天內結構變化的股票)
3. 升級 Neely Core 演算法(評估 Compaction 演算法替代方案)

**r3 標註**:本表的 P95 數字將在 13.9 測試完成後更新為實測值,屆時將標註「[實測 YYYY-MM-DD]」並寫入附錄 A 決策歷史。

### 15.4 失敗處理

- **單檔失敗**:記錄錯誤,跳過此檔,不阻擋其他檔
- **單 Core 失敗**:該 Core 該檔當日資料缺失,Aggregation Layer 容錯顯示「資料未更新」
- **Stage 失敗**:全 Stage 重試 N 次,仍失敗則告警,人工介入
- **Batch 中斷**:支援 resume,以 `(stock_id, core, computed_at)` 為斷點

### 15.5 觀察性需求

每個 Core 在每次 batch 執行時記錄:

```rust
BatchExecutionLog {
    batch_id: Uuid,
    core_name: String,
    core_version: String,
    stock_id: String,
    started_at: DateTime,
    finished_at: DateTime,
    status: enum { Success, Failed, Skipped },
    error_message: Option<String>,
    rows_written: usize,
}
```

供觀察 Batch 執行健康度、效能瓶頸、回溯失敗原因。

### 15.6 On-demand 補算流程(r3 補強 single-flight 與 rate limit)

當使用者請求 workflow 預設外的指標參數組合,執行流程:

```
前端請求 → API 檢查 indicator_values 是否有此 params_hash
    ├─ 有 → 直接讀 DB 回傳
    └─ 無 → 進入 single-flight 補算流程(r3 新增)
            ├─ 1. 取 Postgres advisory lock(key = hash(stock_id, indicator, params_hash))
            ├─ 2. double-check:再讀一次 indicator_values(可能其他 worker 剛算完)
            ├─ 3. 若仍無 → Core 算 → UPSERT indicator_values
            ├─ 4. 釋放 advisory lock
            ├─ 5. 回傳結果
            └─ 6. 註記到 workflow_registry 表(下次 batch 自動納入)
```

**Single-flight 範例(Python 端)**:

```python
async def get_or_compute_indicator(stock_id, indicator, params, timeframe):
    params_hash = compute_params_hash(params)

    # Step 1: 先讀 cache
    result = await db.fetch_indicator(stock_id, indicator, params_hash, timeframe)
    if result:
        return result

    # Step 2: cache miss,取 advisory lock(去重 key 用 64-bit signed int)
    lock_key = stable_hash_64(stock_id, indicator, params_hash, timeframe)

    async with db.advisory_lock(lock_key):
        # Step 3: double-check(可能其他 worker 已算完)
        result = await db.fetch_indicator(stock_id, indicator, params_hash, timeframe)
        if result:
            return result

        # Step 4: 真的需要算
        try:
            output = await call_rust_core(
                core_name=indicator,
                stock_id=stock_id,
                params=params,
                timeframe=timeframe,
                timeout=5.0,  # 同步補算最多等 5 秒
            )
        except TimeoutError:
            # 超過 5 秒改為非同步:寫入 job queue,回傳 job_id 讓前端 polling
            job_id = await enqueue_async_compute(stock_id, indicator, params, timeframe)
            return {"status": "pending", "job_id": job_id}

        await db.upsert_indicator(output)
        await db.upsert_workflow_registry(indicator, params)  # 下次 batch 納入
        return output
```

**Rate limit 規範(r3 補強)**:

| 維度 | 限制 | 理由 |
|---|---|---|
| 單 IP | 20 次 / 分鐘 | 防止單一使用者刷爆 |
| 單 stock_id 全域 | 100 次 / 分鐘 | 防止熱門股(如 2330)被全體刷爆 |
| 單一參數組合全域 | 1 個進行中(advisory lock 保證) | thundering herd 防護 |
| 同步逾時 | 5 秒 | 超過則改非同步 polling |
| 非同步 job queue 容量 | 1000 | 滿則拒絕新請求 |

**workflow_registry 表(r3 新增)**:

```sql
CREATE TABLE workflow_registry (
    indicator_name  VARCHAR(50) NOT NULL,
    params_hash     VARCHAR(16) NOT NULL,
    params_json     JSONB       NOT NULL,
    timeframe       VARCHAR(10) NOT NULL,
    first_seen_at   TIMESTAMP   NOT NULL,
    last_used_at    TIMESTAMP   NOT NULL,
    use_count       BIGINT      NOT NULL DEFAULT 0,
    PRIMARY KEY (indicator_name, params_hash, timeframe)
);
```

每次 on-demand 補算 UPSERT 到此表,batch 啟動時讀取此表,把所有「曾被使用的參數組合」加入該日 batch 計算範圍。**淘汰策略**:30 天未被使用的參數組合自動從 batch 排除(但歷史 indicator_values 不刪)。

**頻率限制理由**:on-demand 補算進入即時路徑(2.4 哲學立場聲明的 escape hatch),必須有嚴格防護,否則濫用會把 Batch 路徑的設計優勢全部抹掉。

---

## 十六、Workflow 預設模板

### 16.1 設計原則

四個預測週期(月/季/半年/年)各對應一個 workflow toml,內容對齊疊圖系統文件第四章。

- workflow toml 是**業務概念**,使用者選擇預測週期 = 載入對應 workflow
- 同一 indicator 可被同一 workflow 包含多次(不同參數)
- workflow 不知道計算順序、不知道 DB 結構,純粹宣告「要哪些 Core 與參數」
- 使用者可自訂 workflow,複製預設模板修改

### 16.2 預設模板總覽

| Workflow | 對應週期 | 主時框 | Indicator 數 | 暖機資料量需求 |
|---|---|---|---|---|
| `tw_stock_monthly` | 月預測(20 交易日) | 日線 | 9 | ~200 天 |
| `tw_stock_quarterly` | 季預測(60 交易日) | 日線 + 週線 | 11 | ~500 天 |
| `tw_stock_half_yearly` | 半年預測(120 交易日) | 週線為主 | 11 | ~1000 天 |
| `tw_stock_yearly` | 年預測(240 交易日) | 週線 + 月線 | 9 | ~2500 天 |

### 16.3 Workflow Toml 範例(月預測)

```toml
# workflows/tw_stock_monthly.toml
name = "TW Stock Monthly Forecast"
version = "1.0.0"
description = "月預測 (約 20 個交易日),日線為主"

[data_requirements]
min_history_days = 200

[wave_cores]
include = ["neely_core"]

[market_cores]
include = ["tw_market_core"]

[[indicator_cores]]
name = "ma"
params = { periods = [5, 20, 60], timeframe = "daily" }

[[indicator_cores]]
name = "macd"
params = { fast = 12, slow = 26, signal = 9, timeframe = "daily" }

[[indicator_cores]]
name = "adx"
params = { period = 14, timeframe = "daily" }

[[indicator_cores]]
name = "kd"
params = { k = 9, d = 3, smooth = 3, timeframe = "daily" }

[[indicator_cores]]
name = "rsi"
params = { period = 14, timeframe = "daily" }

[[indicator_cores]]
name = "bollinger"
params = { period = 20, std = 2, timeframe = "daily" }

[[indicator_cores]]
name = "atr"
params = { period = 14, timeframe = "daily" }

[[indicator_cores]]
name = "obv"
params = { timeframe = "daily" }

[[indicator_cores]]
name = "support_resistance"
params = { lookback_days = 30 }

[chip_cores]
include = ["institutional", "margin"]

[environment_cores]
include = ["taiex"]
```

### 16.4 同 Core 多次 include

例:半年預測同時要日線 MA(60,120,240) 和週線 MA(13,26,52):

```toml
[[indicator_cores]]
name = "ma"
params = { periods = [60, 120, 240], timeframe = "daily" }

[[indicator_cores]]
name = "ma"
params = { periods = [13, 26, 52], timeframe = "weekly" }
```

DB 用 `params_hash` 區分兩組結果,Aggregation Layer 並排呈現。

### 16.5 八層分類的定位

疊圖系統文件提到的「趨勢/強度/震盪/量能/波動/結構/時機/價位」八層分類:

- **僅作為 UI 與使用者教學概念**,不寫入 workflow toml
- **不對應任何代碼結構**(避免誘導跨 Core 耦合)
- 前端可選擇按八層折疊呈現,但這是視覺分組,不是計算分組

---

## 十七、前端職責邊界

### 17.1 前端只做三件事

1. **呼叫 API**:傳 `(stock_id, workflow_name, display_range)` 給後端
2. **接收資料**:取得 Aggregation Layer 組裝好的 `AggregatedOutput`
3. **組合圖表**:把 indicator values、structural snapshots、facts 渲染為圖層

### 17.2 前端不做的事

- ❌ 任何指標計算
- ❌ 跨 Core 衍生欄位(例:不在前端算「MACD 是否金叉」,後端 Fact 已有)
- ❌ 加權、評分、結論
- ❌ 對 raw OHLC 重新計算

### 17.3 圖層組合範例

```
使用者打開 3363 上詮 → 半年預測
  ↓
API: GET /stock/3363/analysis?workflow=tw_stock_half_yearly&from=2025-04-28&to=2026-04-28
  ↓
回傳:
  - indicator_values: MA(13,26,52) 週線、MACD 週線、RSI 週線、布林週線、...
  - structural_snapshots: 最新 Wave Forest、Fib zones、SR levels
  - facts: 該區間內的所有 Fact
  ↓
前端組合:
  圖層 1 (底圖):K 線
  圖層 2:MA 折線(三條)
  圖層 3:布林通道
  圖層 4:Fibonacci 水平線(從 snapshots)
  圖層 5:撐壓位水平線(從 snapshots)
  圖層 6:Fact 標註(金叉、背離等事件 marker)
  副圖 1:MACD
  副圖 2:RSI
```

### 17.4 圖表互動原則

- 使用者切換指標顯示 → 純前端切換圖層,不呼叫後端
- 使用者切換時間範圍 → 重新呼叫 API(改 display_range 參數)
- 使用者改參數(例:MA 改週期)→ 觸發 on-demand 補算路徑
- **使用者切換時間粒度(日線↔週線↔月線)→ 重新呼叫 API**(r3 補強)
  - **不可**從日線 raw OHLC 在前端聚合出週線,因為 raw OHLC 已被 TW-Market Core 前處理(漲跌停合併等),前端聚合會出錯
  - 三個 timeframe 是後端各自算好的獨立資料,前端只能擇一渲染

### 17.5 錯誤態渲染規範(r3 新增)

當某 Core 當日資料缺失(Stage 失敗 fallback,見 15.4),Aggregation Layer 會回傳該 Core 對應欄位為 null 或帶 `error_state` 標記,前端必須有對應 UI 處理:

| 情境 | 後端回傳 | 前端 UI |
|---|---|---|
| 某 indicator 當日無資料 | `indicators.macd = null` 加 `data_warnings: ["macd: not_updated"]` | 該指標圖層顯示半透明灰底 + 「資料延遲」標籤 |
| Neely Core overflow | `snapshots.neely_core.overflow_triggered = true` | 顯示「此股結構過於複雜,呈現 Top 100 解讀」橫幅 |
| 全 Stage 失敗 | API 回 503 + retry-after | 全頁顯示「資料更新中,請稍候」 |
| on-demand 補算 pending | 回 `{status: "pending", job_id: "..."}` | 該指標 placeholder + 自動 polling job 狀態 |

**設計原則**:
- 不靜默失敗,所有缺失情境必須在 UI 上可見
- 不阻擋使用者使用其他正常資料(例:MACD 缺,RSI 仍可顯示)
- 不誤導使用者(例:不可把缺失資料用「上一日值」填補)

---

## 十八、v1.1 模組去處對照表

| v1.1 Item | 內容 | v2.0 去處 | 變更 |
|---|---|---|---|
| Item 1 | Monowave Detection | Neely Core | 保留 |
| Item 1.5 | `[TW-MARKET]` 連續漲跌停合併 | TW-Market Core | 移出 Neely |
| Item 2 | Bottom-up Generator | Neely Core | 保留 |
| Item 3 | Validator | Neely Core | 保留,容差改相對 4% |
| Item 4-6 | Classifier / Flat / Triangle | Neely Core | 保留 |
| Item 7 | Scorer 7 因子 | Neely Core(拆解為事實向量) | **不再加總** |
| Item 7.4 | `[TW-MARKET]` Scorer 微調 | **移除** | 主觀調參 |
| Item 7B | Post-Constructive Validator | Neely Core | 保留 |
| Item 8 | Compaction | Neely Core | **重寫**:窮舉 Forest |
| Item 9 | Missing Wave | Neely Core | 保留 |
| Item 10 | Emulation | Neely Core | 保留 |
| Item 11-15 | Presentation Layer | **獨立成前端模組** | 後端不負責 |
| Item 16 | Output Schema (WaveReport) | Neely Core 改寫 + Aggregation Layer + structural_snapshots | primary/alternatives 改 forest,寫 DB |
| Item 17 | Router 模式分發 | Orchestrator + Workflow | 改 declarative |
| Item 17.2 | 還原指數使用 | TW-Market Core | 移出 |
| Item 17.5 | Combined confidence | **移除** | 主觀調參 |
| Item 18 | async 並行設計 | Orchestrator + Batch Pipeline | 保留思路,擴充 batch |
| Item 19 | ATR 校準 | NeelyEngineConfig.atr_period(預設 14,固定常數) | 移除自動校準，違反 1.3 原則 |
| Item 20 | Scorer 權重校準 | **移除** | 不再有權重 |
| Item 21 | Sanity Check | 改造,每 Core 各自 + Batch 觀察性 | 重寫 |

---

## 附錄 A:已採用的決策清單

### A.1 v2.0 初版決策

1. ✅ Pipeline 從「裁決式」改「輔助判讀」
2. ✅ 去除 LLM 仲裁、去除 Bayesian 後驗、去除 Softmax 動態溫度
3. ✅ 忠於 Neely 原作,規則寫死,不外部化
4. ✅ Fibonacci 容差改相對 4%(Neely 原意)
5. ✅ Compaction 產出 Forest,不做選擇
6. ✅ Scorer 7 因子拆解為獨立事實向量,不加總
7. ✅ 移除 `[TW-MARKET]` Scorer 微調(ext_type_prior_3rd、alternation_tw_bonus)
8. ✅ 移除 Combined confidence ×1.1/×0.7
9. ✅ 切乾淨 by module,核心多不是問題
10. ✅ 技術指標每個一個獨立 Core,不耦合波浪
11. ✅ 籌碼/基本面/環境各自獨立 Core
12. ✅ Aggregation Layer 僅列出提供判斷,不整合
13. ✅ Workflow(業務)/ Orchestrator(技術)雙命名
14. ✅ Core 之間零語義耦合,僅共用基礎建設
15. ✅ TW-Market Core 在 Neely Core 之前做資料前處理(選項 A)
16. ✅ NeelyEngineConfig 僅 atr_period、beam_width 可調

### A.2 整合疊圖系統文件後新增決策

17. ✅ 採用 **Batch 預算模式**:Core 計算結果進 DB,即時請求純讀,前端純展示
18. ✅ Storage Layer 四層分類:Raw / Indicator Values / Structural Snapshots / Facts
19. ✅ Indicator Values 用 JSONB 統一儲存(不為每個指標一張表)
20. ✅ 滑動窗口型指標採「最近 N 天 + 暖機區」增量計算策略
21. ✅ 結構性指標(Neely / SR / Trendline,Fib 為 Neely 子模組)每日全量重算
22. ✅ 新增 5 個 Indicator Core:keltner、donchian、candlestick_pattern、support_resistance、trendline
23. ✅ 跨指標訊號(TTM Squeeze 等)**不立 Core**,並排呈現由使用者自看
24. ✅ trendline_core 例外允許消費 neely_core 的 monowave 輸出
25. ✅ 八層分類(趨勢/強度/震盪/...)僅作 UI 與教學概念,不進架構
26. ✅ Tag Core 不獨立,改為**離線 Learner 模組**(見附錄 C)
27. ✅ 四個預測週期 workflow 預設模板:monthly / quarterly / half_yearly / yearly
28. ✅ 前端職責限於「呼叫 API + 渲染圖層」,不參與計算
29. ✅ On-demand 補算機制:處理 workflow 預設外的指標參數請求
30. ✅ Presentation Layer(v1.1 Item 11-15)獨立為前端模組

### A.3 r3 新增決策(P0 工程細節收斂)

31. ✅ Core 部署模型採 **Monolithic Binary**,inventory 自動註冊,P3 前不重議(11.2)
32. ✅ PyO3 邊界用 **JSON via serde**,OHLCV 用 numpy zero-copy(13.3)
33. ✅ `params_hash` 演算法:**canonical JSON keys ASC + blake3 → 取前 16 hex 字元**(14.2.2)
34. ✅ Storage **partition** 策略:indicator_values 兩層(年 RANGE + stock_id HASH×8),facts 單層(fact_date RANGE)(14.5)
35. ✅ Storage **retention** 策略:熱 5 年 SSD,冷 5 年 OLAP(parquet),facts 全保留 + 5 年以上轉冷(14.5)
36. ✅ Neely Core Forest 上限保護:`forest_max_size = 1000`(暫定)+ 60s timeout + `BeamSearchFallback{k:100}`(7.5)
37. ✅ 聲明 power_rating 截斷不違反「並排不整合」原則,因 power_rating 是 Neely 書裡查表值(7.5)
38. ✅ on-demand 補算 single-flight:**Postgres advisory lock + double-check**,5 秒同步逾時後改非同步 polling(15.6)
39. ✅ Rate limit:單 IP 20/min、單 stock 100/min、async job queue 容量 1000(15.6)
40. ✅ Facts 表 unique constraint:`(stock_id, fact_date, timeframe, source_core, COALESCE(params_hash, ''), md5(statement))` + INSERT ON CONFLICT DO NOTHING(14.2.4)
41. ✅ Stage 4 拆 4a / 4b:4a 為獨立結構性 Core(Neely / Traditional / SR),4b 為依賴 4a 的(Trendline / Fib 投影)(15.1)
42. ✅ Fibonacci 統一為 Neely Core 子模組,**不獨立成 Core**;若以 fib_zones snapshot 寫入,須附 derived_from_core 追溯(5.2)
43. ✅ Volume 不獨立成 Core,Aggregation 直接讀 raw 表(8.1)
44. ✅ `Trigger.on_trigger` enum 移除 `ReduceProbability`,改 `WeakenScenario`,避免機率語意(5.4)
45. ✅ `neely_power_rating` 由 `i8` 改 `enum PowerRating`,避免無效值(5.4)
46. ✅ 13.9 五檔股票實測指引納入 P0 必補項目,測試結果寫入專案 `docs/benchmarks/`
47. ✅ Aggregation Layer 加機械強制檢查:CI lint 禁止 `score|rank|primary|confidence|weight` 欄位名,加 property-based test 驗證純資料組裝(11.6 坑 3)
48. ✅ workflow_registry 表新增,記錄 on-demand 累積的參數組合,30 天未用則自動排除 batch 範圍(15.6)

---

## 附錄 B:已棄用的方案

1. ❌ Bayesian 後驗更新(likelihood 主觀)
2. ❌ Softmax 動態溫度 τ(主觀調參)
3. ❌ LLM 仲裁層(黑箱不可審計)
4. ❌ 容差 toml 外部化(誘導偏離原作)
5. ❌ Compaction 貪心選最高分 + backtrack(違反輔助判讀原則)
6. ❌ Indicator-Wave Linker Core(誘導耦合)
7. ❌ 按 Degree 分層輸出 Indicator(偷偷耦合)
8. ❌ Engine_T + Engine_N 整合公式(主觀調參)
9. ❌ TTM Squeeze 等跨指標訊號獨立 Core(違反零耦合)
10. ❌ Tag Core 即時模組(改為離線 Learner)
11. ❌ 全量寫入 OLTP DB 給 Learner 用(已採 Batch + 同 DB,Learner 直讀)
12. ❌ 八層分類進架構作為 Core 分組(僅作 UI)

### r3 新增棄用

13. ❌ Compaction 完全不剪枝(7.2 r2 立場):工程現實下會 OOM,改為「窮舉但有 forest_max_size 上限保護」
14. ❌ Core 之間 runtime 版本相容性檢查(11.6 r2 坑 2):Monolithic 編譯期已對齊,runtime check 變空話
15. ❌ Fibonacci 獨立 Core(r2 14.2.3 / 15.1 隱含立場):確認為 Neely Core 子模組
16. ❌ Volume 獨立 Core(r2 13.1 P1 列入):Aggregation 直接讀 raw 表即可
17. ❌ `Trigger.on_trigger.ReduceProbability`(r2 5.4):與 2.2「不使用 probability」原則矛盾,改 `WeakenScenario`

---

## 附錄 C:Learner 離線模組界定

### C.1 定位

Learner 是**離線分析 / 訓練模組**,不在即時請求路徑上。職責:

1. 讀 Storage Layer 的 indicator_values、structural_snapshots、facts、raw OHLC
2. 用 LLM 對歷史 Fact 海量標註
3. 透過頻率/相似度/共現分析,收攏成穩定的 Tag Vocabulary(30-80 個標籤)
4. 訓練回測,輸出可被即時路徑消費的「Tag Annotation Model」

### C.2 與即時路徑的關係

```
即時路徑(本 spec 主體):
  Core → Storage Layer → Aggregation Layer → 前端

離線路徑(本附錄):
  Storage Layer → Learner Pipeline → Tag Vocabulary + Annotation Model
                                              ↓
                                       回饋即時路徑(可選):
                                       Aggregation Layer 在 Fact 上
                                       附加 learned tag(僅標註,不下決策)
```

### C.3 Learner 不做的事

- ❌ 不在 Core 層即時推論
- ❌ 不影響 Aggregation Layer 的並排不整合原則
- ❌ 不為 Fact 加分數或排序
- ❌ 不在 Pipeline 即時路徑使用 LLM

### C.4 開發優先級

P3,在 P0/P1 穩定後再開始。第一版可不實作,Storage Layer 已為 Learner 預備好資料來源。

---

## 附錄 D:來自疊圖系統文件的整合決策

整合自 `tw_stock_mcp_技術指標疊圖系統設計.md`,逐項說明在 v2.0 的去向:

| 疊圖文件章節 | v2.0 去處 | 變更 |
|---|---|---|
| 一、產品定位 | 附錄 A 條目 17(Batch 模式)+ 條目 28(前端職責)| 整合到本 spec 哲學層 |
| 二、最終指標組合 13 項 8 層 | 第十六章 Workflow 模板 + 附錄 A 條目 22、25 | 八層降格為 UI,指標補入 Indicator Cores |
| 三、各指標使用判斷 | **不進架構,進使用者文件** | 屬使用者教學 |
| 四、預測週期 × 指標映射 | 第十六章四個 Workflow Toml | 直接轉成預設模板 |
| 五、Tag Core 設計 | 附錄 C(離線 Learner 模組) | 改為離線 |
| 六、LLM 使用點位 | 附錄 C + 即時路徑禁止 LLM(附錄 A 條目 2) | 即時不用 LLM,離線可用 |
| 七、整體架構流程 | 第四章四層架構 + 第十一章執行順序 | 重整為 Batch 模式 |
| 八、後續開發優先順序 | 第 13.5 節 P0/P1/P2/P3 | 對齊 |

### D.1 疊圖文件中**不採納**的內容

- 「ADX 是過濾器,先看 ADX 才決定要不要相信其他訊號」這類**跨 Core 語義耦合**敘述 → 移到使用者教學文件,不寫進 spec
- 「結構/時機/價位三層協作範例」 → 標註為**使用者判讀範例**,不是系統輸出範例
- 「Tag Core」獨立模組 → 已決策併入離線 Learner,即時路徑用 Fact Schema 取代
- 「TTM Squeeze 是布林 + Keltner 同時保留的核心價值」 → 不立 ttm_squeeze_core,改由使用者並排判讀

---

> **下一步(r3 修訂)**:
>
> 1. **執行 13.9 五檔股票實測**(0050 / 2330 / 3363 / 6547 / 1312)
>    → 校準 7.5 `forest_max_size` / `compaction_timeout_secs` / `BeamSearchFallback.k` 預設值
>    → 校準 15.3 batch 時間預算
>    → 結果 JSON 提交至 `docs/benchmarks/forest_size_2026-XX-XX.json`,寫入附錄 A 決策歷史
>
> 2. **基於本決策文件 r3,在新副本中產出 v2.0 完整 spec**:
>    - 目錄結構與各 Core trait 定義(對應第八章 + 11.1)
>    - Workflow toml schema(對應 11.3 + 16.3)
>    - Aggregation API 介面(對應第十二章 + 17.5)
>    - Storage Layer DDL(對應第十四章 + 14.5)
>    - Batch Pipeline 設計(對應第十五章 + 15.6)
>    - PyO3 邊界規範(對應 13.3)
>    - 第一版 P0 / P1 Core 詳細規格
