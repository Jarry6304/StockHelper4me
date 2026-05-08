# NEO Pipeline v2.0 — 架構轉向決策文件

> **版本**:v2.0-decisions(架構決策階段)
> **日期**:2026-04-28
> **基準**:`neo_pipeline_spec_v1_1_1_.md` + `schema_reference.md`(SCHEMA_VERSION=1.1)
> **用途**:作為 v2.0 完整 spec 的前置決策紀錄,供另開副本實作參考

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
14. [v1.1 模組去處對照表](#十四v11-模組去處對照表)

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

---

## 四、架構分層

### 4.1 三層架構

```
┌─────────────────────────────────────────────────────────────┐
│                    Aggregation Layer                         │
│   並排呈現,不整合。可提供 facet filter 但不下結論            │
└─────────────────────────────────────────────────────────────┘
                            ↑ (各 Core 獨立 output)
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
    on_trigger: TriggerAction,   // InvalidateScenario / ReduceProbability / PromoteAlternative
    rule_reference: RuleId,      // 對應 Neely 規則,例 R5
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
    pub atr_period: usize,        // 工程參數,可調(預設 20)
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

---

## 八、輔助 Cores 清單

### 8.1 Core 切分原則

> **核心多不是問題,問題是乾淨**(用戶決策)

每個 Core 是 200-500 行的小程式,各自單純,比一個 5000 行的大 Engine 遠遠更好維護。

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
│  • macd_core              MACD                                │
│  • rsi_core               RSI                                 │
│  • kd_core                KD / Stochastic                     │
│  • adx_core               ADX / DMI                           │
│  • ma_core                SMA / EMA(可參數化多週期)           │
│  • bollinger_core         布林通道                             │
│  • ichimoku_core          一目均衡表                           │
│  • obv_core               OBV                                 │
│  • atr_core               ATR(獨立,不依賴 Neely Core)        │
│  • vwap_core              VWAP                                │
│  • williams_r_core        威廉指標                             │
│  • mfi_core               資金流量指數                         │
│  • cci_core               CCI                                 │
│  • coppock_core           Coppock Curve                       │
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
    type Params: Default + Clone;
    type Output;

    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn compute(&self, ohlcv: &OHLCVSeries, params: Self::Params) -> Result<Self::Output>;
    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact>;
}
```

統一介面後,Orchestrator 對待 30 個 Core 跟 3 個 Core 沒差別。

### 11.2 Core Registry(自動註冊)

```rust
// 每個 Core 在自己的 lib 裡註冊
inventory::submit! {
    CoreRegistration::new("macd_core", "1.0.0", || Box::new(MacdCore::new()))
}

// Orchestrator 啟動時自動發現所有已編譯的 Core
let registry = CoreRegistry::discover();
```

新增 Core 不用改 Orchestrator 代碼,**只要寫好 Core 並編譯進去,自動可用**。

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

### 11.4 執行順序(草案)

```
TW-Market Core(資料前處理)
    ↓
Neely Core / Traditional Core(波浪 Cores,可並行)
    ↓
Indicators / Chips / Fundamentals / Environment Cores(可並行)
    ↓
Aggregation Layer(並排組裝)
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

#### 坑 2:Core 版本相依

每個輔助 Core 要宣告依賴的 NeelyCore 版本範圍(類似 Cargo.toml 的 semver),Orchestrator 啟動時做 version compatibility check。

#### 坑 3:Aggregation Layer「並排不整合」誘惑 → 採「僅列出提供判斷」(已決策)

需要在 Aggregation Layer 代碼註解明寫,並有 lint rule 或 code review 把關。

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

---

## 十三、尚未決策的問題

以下問題已提出,但用戶尚未明確決策,在副本實作時需先確認:

### 13.1 第一版 Indicator Cores 範圍

- 全做 14 個? 或只做 6 個最常用(MACD、RSI、KD、ADX、MA、Bollinger),其餘後補?
- 用戶若選後者,風險低、迭代快

### 13.2 籌碼 Core 切分粒度

- 5 個獨立(institutional / margin / foreign_holding / shareholder / day_trading)?(推薦,符合「乾淨」原則)
- 3 個整合?
- 2 個整合?

### 13.3 開發語言

v1.1 是 Rust + Python 混合(PyO3、Rust ComputeCore)。v2.0 想用什麼?

- 全 Rust?
- Rust(Wave/Indicator/Chip)+ Python(Aggregation / API)?
- 暫不決定?

### 13.4 Aggregation Layer 呈現方式

- A:全部攤平,user 自己捲動
- B:按 Category 折疊(Wave / Indicator / Chip / Fundamental / Environment)
- C:user 在 workflow toml 指定關注 Core,只顯示那些
- D:Aggregation Layer 提供 facet filter,user UI 上自選

### 13.5 第一版 Core 優先順序

建議:

- **P0(必做)**:Neely Core、TW-Market Core、Aggregation Layer、Orchestrator
- **P1(高價值,推薦)**:Chips Core(institutional + margin + foreign_holding)、Indicator Cores(6 個常用)
- **P2(中等)**:Fundamental Cores、Environment Cores
- **P3(進階,先卡位)**:Industry Core、Traditional Core、剩餘 Indicator Cores

### 13.6 v1.1 Item 11-15(Presentation Layer)的去處

v1.1 spec 有 Presentation Layer(K 線疊圖、Fibonacci 視覺化、雙色折線等),v2.0 應該:

- 進 Aggregation Layer?
- 獨立成前端模組,不在 Pipeline 範圍?
- 沿用 v1.1 設計?

### 13.7 v1.1 Item 19-21(Calibration)的去處

v1.1 有 ATR 校準、Scorer 權重校準、Sanity Check。v2.0:

- ATR 校準仍需要(`atr_period` 是 NeelyEngineConfig 內可調工程參數)
- Scorer 權重校準 → **不需要**(因為 Scorer 7 因子改成獨立事實向量,不加總,沒有權重要校)
- Sanity Check → 改造,每個 Core 各自的測試集

---

## 十四、v1.1 模組去處對照表

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
| Item 11-15 | Presentation Layer | (未決策) | 待定 |
| Item 16 | Output Schema (WaveReport) | Neely Core 改寫 + Aggregation Layer | primary/alternatives 改 forest |
| Item 17 | Router 模式分發 | Orchestrator + Workflow | 改 declarative |
| Item 17.2 | 還原指數使用 | TW-Market Core | 移出 |
| Item 17.5 | Combined confidence | **移除** | 主觀調參 |
| Item 18 | async 並行設計 | Orchestrator | 保留思路 |
| Item 19 | ATR 校準 | Calibration(僅 atr_period) | 縮小範圍 |
| Item 20 | Scorer 權重校準 | **移除** | 不再有權重 |
| Item 21 | Sanity Check | 改造,每 Core 各自 | 重寫 |

---

## 附錄 A:已採用的決策清單

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

---

> **下一步**:基於本決策文件,在新副本中產出 v2.0 完整 spec(目錄結構 + 各 Core trait 定義 + Workflow toml schema + Aggregation 介面 + 第一版 P0/P1 Core 詳細規格)。
> 
> 副本實作前需先解決「[尚未決策的問題](#十三尚未決策的問題)」中的 7 項。
