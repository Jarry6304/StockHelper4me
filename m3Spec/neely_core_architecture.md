# Neely Core Architecture(r5)

> **版本**:v2.0 architecture r5(精華版分離架構)
> **日期**:2026-05-13
> **基準**:r4 architecture decisions + Gap 1-5 決議
> **配套文件**:
> - `neely_rules.md`(規則層 — 精華版筆記,與原書 Ch1-Ch12 同構)
> - `cores_overview.md`(共通規範)
> - `layered_schema_post_refactor.md`(Silver 層)
> - `adr/0001_tw_market_handling.md`
> **優先級**:**P0**(核心 Core,所有後續 Core 的結構性參考)

---

## r5 修訂摘要(2026-05-13,規則層 / 架構層分離)

> **背景**:r4 嘗試在單一文件中同時描述「Neely 規則」與「v2.0 工程架構」,造成兩大病徵:
>
> 1. **重編號失真** — 把 Neely 的 Rule 1-7、Condition a-f 重編成 `PR0-PR7` / `AL1-AL5` / `C1-C5` / `A1-A6`,每次重編號引入翻譯誤差(發現 5 處與原作衝突的錯誤規則)
> 2. **規則資訊在 spec 內多處重複** — Ch3 同時出現在 §7.4.2 / §10.1 / 附錄 F,三處不一致(典型例子:容差 ±4% 在 §10.4 被刪、在 §F.3 / §F.7 又寫死)
>
> r5 採用「**規則層 / 架構層分離**」策略:
>
> - **architecture.md(本文件)** — 純工程架構決策(輸出形狀、Pipeline 流程、護欄機制、Output 結構)
> - **neely_rules.md(精華版筆記)** — 規則層,與原書 Ch1-Ch12 同構,不重編號
> - 兩者引用關係:架構層引用「精華版 Ch X 第 Y 節」,不重述規則細節

### r5 vs r4 核心改動

| 項 | r4 | r5 |
|---|---|---|
| 規則描述位置 | architecture.md 內(§4.5-4.8、§10.5-10.11、§13.4、§14.8-14.9、附錄 C/D/E/F) | **全部移至 `neely_rules.md`**(精華版同構) |
| RuleId 編碼 | 自編 `R1-R7 / AL1-AL5 / C1-C5 / A1-A6` 等 | **Neely 章節編碼** `Ch3_PreConstructive { rule, condition, category }` |
| 容差立場 | r3 刪除 ±4%,但 §F.3 / §F.7 又寫死(自相矛盾) | **精華版翻譯表為標準**(±10% / ±4% / 10% 三檔),有原書 OCR 依據 |
| OHLC 處理 | 未明確 | **Hybrid** — monowave 切割用 (H+L)/2,Scenario 保留完整 OHLC reference |
| Forest 截斷 | 單純 power_rating 排序 | **雙重排序** — Power Rating 級別分組 + 組內 rules_passed_count |
| NeelyCoreOutput | 結構模糊 | **完整定義** — Forest + 跨 Scenario 警示 + Reverse Logic + Round3 + diagnostics |
| Reverse Logic | 僅 §2.4 哲學描述 | **NeelyCoreOutput 一級欄位** + 可調 threshold |
| ATR 立場 | 「不是 Neely 方法」批判性論述 | **合理詮釋** — 與精華版立場一致,雙模驗證仍跑 |
| P0 Gate 範圍 | 五檔個股 | **六檔**(五檔個股 + TAIEX) |
| 預計篇幅 | 2861 行 | ~700 行 |

### Gap 1-5 決議全部對齊

r5 是 Gap 1-5 提問流程的具體落實。每節若涉及 Gap 決議,於該節開頭以 `[Gap X.Y]` 標記來源。

---

## 目錄

1. [定位](#一定位)
2. [設計哲學:並排不整合](#二設計哲學並排不整合)
3. [三層文件架構](#三三層文件架構)
4. [容差規範](#四容差規範)
5. [輸入](#五輸入)
6. [NeelyEngineConfig](#六neelyengineconfig)
7. [Pipeline 階段](#七pipeline-階段)
8. [NeelyCoreOutput 結構](#八neelycoreoutput-結構)
9. [Scenario 內部結構](#九scenario-內部結構)
10. [Forest 上限保護](#十forest-上限保護)
11. [Power Rating 截斷哲學](#十一power-rating-截斷哲學)
12. [Fact 產出規則](#十二fact-產出規則)
13. [warmup 與 Degree ceiling](#十三warmup-與-degree-ceiling)
14. [對應資料表](#十四對應資料表)
15. [診斷與可觀察性](#十五診斷與可觀察性)
16. [P0 Gate 六檔實測](#十六p0-gate-六檔實測)
17. [已棄用設計](#十七已棄用設計)
18. [與其他 Core 的關係](#十八與其他-core-的關係)
19. [Appendix A:精華版引用對照表](#appendix-a精華版引用對照表)
20. [Appendix B:Gap 5 核實後續工作清單](#appendix-bgap-5-核實後續工作清單)

---

## 一、定位

**Neely Core** 是 v2.0 架構的**核心結構性 Core**,實作 Glenn Neely《Mastering Elliott Wave》(NEoWave)的全套規則,輸出 **Scenario Forest**(多棵並列、不選 primary、無分數)。

### 1.1 設計意圖

- 將 Neely 體系的所有規則集中於單一 Core,作為「最權威的波浪結構解讀引擎」
- 輸出 Forest 而非 Tree:**所有 Neely 規則允許的合法解讀並列呈現**,不替使用者選最佳解
- v2.0 整體架構從「裁決式」轉向「展示式」的核心體現

### 1.2 必要規範

- 屬 **Wave Core**,**不**走 `IndicatorCore` trait
- 走 `WaveCore` trait(設計約束見 `cores_overview.md` §3.3,trait 簽章於 P0 開發前定稿)
- P0 階段最複雜、開發優先級最高的 Core
- 與 Traditional Core(P3)獨立並列,**不整合**
- 嚴格遵循「**計算 / 規則分層**」原則 —— 本 Core 只做 Neely 規則套用,所有資料前處理由 Silver 層完成

### 1.3 唯一允許的耦合

`trendline_core`(P2)是全 Core 系統中**唯一允許消費 Neely Core 輸出的例外**,僅讀 `NeelyCoreOutput.monowave_series`(不讀 `scenario_forest`)。詳見 §18.3。

---

## 二、設計哲學:並排不整合

[Gap 1.1 A]

### 2.1 v1.1 → v2.0 核心轉向

從 v1.1 的「**裁決式**」轉向 v2.0 的「**展示式**」:

| 項目 | v1.1(已棄用) | v2.0(本文件) |
|---|---|---|
| 輸出形狀 | `primary` + `alternatives`,有分數 | `Scenario Forest`,無分數無排序 |
| Compaction | 貪心選最高分 + backtrack | 窮舉所有合法壓縮路徑,產出 Forest |
| 容差系統 | 全域絕對偏移 ±4% | 精華版翻譯表(±10% / ±4% / 10% 三檔,§4) |
| 失效條件 | 無 | 每個 Scenario 必有 `invalidation_triggers` |
| 主觀加權 | Scorer 7 因子加總 | **移除** — 拆解為 `structural_facts`,不加總 |

### 2.2 Scenario 屬性原則

- ❌ **不**做「Pipeline 算的分數」(主觀加權)
- ❌ **不**做 probability(主觀分布假設)
- ✅ **客觀計數**:`rules_passed_count` / `deferred_rules_count`(誰多誰少使用者自看)
- ✅ **Neely 書中查表屬性**:`power_rating` / `max_retracement` / `post_pattern_behavior`
- ✅ **失效條件**:`invalidation_triggers`(規則的逆向轉譯)

### 2.3 並排不整合的具體禁止

❌ **以下行為禁止**:
- 「Neely Core 說 S1 結構好 + Chips Core 說外資買 → 強看好」
- 「兩派共識則加分」
- 「Engine_T 給分 + Engine_N 給分 加權加總」
- 「VIX 高時調降結構分數」

✅ **正確做法**:把 Neely Core 的 scenarios 與其他 Core 的事實**並列**輸出至 Aggregation Layer,由使用者(或 LLM agent)自己連線。

### 2.4 Reverse Logic Rule

> **核心命題(Neely Ch12 p.12-50)**:當同一資料序列上存在**多個完美合法的計數**時,市場必定處於某個修正/衝動形態的**中央**(b 的 b、3 的 3、或 Non-Standard 的 x)。**可能性越多,越靠近中央。**

#### 對 Forest 設計的具體意涵

- Forest size 越大,「即將完成」的解讀越不可信
- Forest 大時截斷反而是「**中段訊號**」,而非「主觀選擇」
- 這與 §11 Power Rating 截斷哲學一致

#### 操作意涵 → Output 一級欄位

[Gap 1.3 B = a]

Reverse Logic 是 Neely 體系的核心訊號之一,**不應被歸類為「工程診斷」**。本 spec 將其提升為 `NeelyCoreOutput.reverse_logic: ReverseLogicObservation` 頂層欄位(見 §8)。

#### Forest size 與 Reverse Logic 的協同

| Forest size | Reverse Logic 解讀 | 系統行為 |
|---|---|---|
| 1-2 | 形態接近完成,單一解讀可靠 | 正常輸出 |
| 3 ~ reverse_logic_threshold | 多套可能,可能進入中段 | 正常輸出 + Reverse Logic Fact |
| > reverse_logic_threshold | 強烈中段訊號 | 正常輸出 + `reverse_logic.activated = true` |
| > forest_max_size | 中段訊號 + 系統資源護欄觸發 | BeamSearchFallback + 雙旗標(`reverse_logic_activated` + `overflow_triggered`) |

`reverse_logic_threshold` 屬可調工程參數(§6),P0 Gate 校準。

---

## 三、三層文件架構

> r5 的核心結構性變動。將「規則」與「工程決策」徹底分離。

### 3.1 三層文件職責

```
┌──────────────────────────────────────────────────────────────┐
│ neely_core_architecture.md(本文件,r5,~700 行)            │
│  - 工程架構決策(Output 形狀、Pipeline、護欄、Config)      │
│  - 不重述 Neely 規則,只引用「精華版 Ch X 第 Y 節」         │
│  - 規則查詢請看 neely_rules.md                              │
└──────────────────────────────────────────────────────────────┘
                           │ 引用
                           ↓
┌──────────────────────────────────────────────────────────────┐
│ neely_rules.md(精華版筆記,~2600 行)                      │
│  - 與原書 Glenn Neely《Mastering Elliott Wave》Ch1-Ch12 同構│
│  - 不重編號:Rule 1-7、Condition a-f、Category i/ii/iii    │
│    保持 Neely 原命名,直接作為 Rust enum 值                 │
│  - 每章末尾加「→ 實作位置:src/xxx/yyy.rs」(節層級)        │
│  - 標記 5 處精華版待原書二次核實(已完成,見 Appendix B)    │
└──────────────────────────────────────────────────────────────┘
                           │ 引用
                           ↓
┌──────────────────────────────────────────────────────────────┐
│ Rust 程式碼(P0 開發產出)                                  │
│  - 每個 fn 的 doc-comment 引用「精華版 Ch X 第 Y 節」       │
│  - RuleId enum 對應精華版章節編碼                          │
└──────────────────────────────────────────────────────────────┘
```

### 3.2 為何採用此架構

**規則層採用「精華版同構」的好處**:

1. **規則本身就是規格** — 翻譯層消失,無 r4 「重編號失真」風險
2. **書頁追溯零成本** — RuleId = Neely 章節編碼,doc-comment 直接引用
3. **未來修訂幾乎不動規則層** — Neely 沒發新版,規則永遠不變
4. **架構修訂時只動 architecture.md** — 規則層穩定,架構演進獨立

**精華版的合理性依據**:

- 精華版由原書 PDF OCR 整理,有原書文字依據
- 對「approximately」、「about」等近似詞的量化整理(±10% / ±4%)為合理詮釋,**不是工程妥協**
- Gap 5 已對精華版自身做原書二次核實,結果見 Appendix B(6 條核實項中 3 條完全對齊原書,3 條需精華版補強,無一條為錯誤詮釋)

### 3.3 引用約定

architecture.md 引用精華版時使用以下格式:

| 引用層級 | 格式 | 範例 |
|---|---|---|
| 章層級 | `精華版 Ch X` | `精華版 Ch3` |
| 節層級 | `精華版 Ch X 第 Y 節` | `精華版 Ch3「Retracement Rules」` |
| 規則層級 | `精華版 Ch X Rule Z` | `精華版 Ch3 Rule 5 Cond 5b` |

---


## 四、容差規範

[Gap 2.1 = a / 2.2 = c / 2.3 = b / 2.4 = b]

### 4.1 容差來源宣告

本 Core 的容差規則來自精華版筆記 Ch3 末段「容差/近似詞翻譯表」。該翻譯表為原書 Neely 描述近似詞的量化整理,**有原書 PDF OCR 依據**,屬合理詮釋(非工程妥協)。

> **r3 立場撤回說明**:r3 曾以「全書搜尋 ±4% 零次出現」為由刪除容差,但該搜尋方法本身有問題 —— Neely 原書用文字描述近似關係(「approximately」、「very close to」等),不會直接寫「±4%」符號。精華版做的是「**把近似詞統一量化為容差**」的整理,屬合理詮釋。Gap 2 採信精華版立場。

### 4.2 三檔容差表

| 詞 / 情境 | 量化容差 | 適用 | 依據 |
|---|---|---|---|
| `approximately equal` / `about` / `close to` / `very close to` / `almost` | **±10%** | 一般近似關係描述 | 原書 Neely 一般近似詞 |
| 具體 Fibonacci 比率(38.2% / 61.8% / 100% / 161.8% / 261.8% 等) | **±4%** | Pre-Constructive Logic、Fib 投影區、Validator 規則 | 原書 Neely Fibonacci 描述 |
| Triangle 三條同度數腿價格相等性 | **±5%** | T 規則組(僅限) | 精華版 Ch11 原文逐字 `give or take 5%` |

### 4.3 Rule 3 臨界區容差

| 項 | 內容 |
|---|---|
| Rule 3 適用範圍 | `m2/m1 = 61.8% ± 4%`(即 m2/m1 ∈ [58%, 66%]) |
| 依據 | 精華版詮釋 — 對應原書 Ch3「臨界,差異最細」描述 |
| doc-comment 標註 | `[精華版詮釋,對應原書 Ch3 臨界區描述]` |

### 4.4 Exception Rule(Ch9 規則違反容忍)

**Gap 5 核實結論**:原書 Ch9 p.9-7「Exception Rule」**完全沒有量化幅度**(無「a small distance」具體百分比、無「less than X%」、無「10%」)。Exception Rule 在原書中**純粹是質性規則**,豁免依據為 Aspect 1 三項情境之一(Multiwave 結尾 / Terminal 5th/c / Triangle 進出),非定量幅度。

#### 觸發條件(質性)

- 單一規則違反(非多條同時違反)
- 失靈幅度遠小於規則特徵尺度
- 符合 Aspect 1 的 A/B/C 情境

#### 「失靈幅度」量化

依各規則的特徵尺度而定,**非固定百分比**:

| 範例 | 規則特徵尺度 | 失靈幅度判定 |
|---|---|---|
| R_OVERLAP_TRENDING | wave-2 價區寬度 | wave-4 進入 wave-2 區深度 < 10% × wave-2 寬度 |
| Ch9 Time Rule | 三段中最長段時長 | 三段時間差異 < 10% × 最長段時長 |
| Triangle wave-e 上限 | wave-d 長度 | wave-e 突破 wave-d 上限 < 10% × wave-d |

各規則特徵尺度與失靈閾值在 `neely_rules.md` 該規則 doc-comment 中個別建檔。

#### r4 「10%」數字的處置

r4 §10.7.1 寫「A5 EXCEPTION_ASPECT1 允許單一規則 < 10% 失靈」,該 10% 數字屬 r4/精華版詮釋。Rust 程式碼 doc-comment 必須明標:

```rust
/// Exception Rule(Ch9 Aspect 1):允許單一規則失靈
/// [r4/精華版詮釋,原書無量化依據]
/// 「失靈幅度」依該規則特徵尺度而定,本範例採 10% × wave-2 寬度
const OVERLAP_EXCEPTION_RATIO: f64 = 0.10;
```

### 4.5 不可外部化原則

容差規則一律**寫死於 Rust 常數**,**不可**從以下來源讀取:

- toml 設定檔
- 環境變數
- `NeelyEngineConfig`(§6 列出的工程參數**不含**容差)

要調整 = 刻意偏離 Neely 詮釋,需在 commit 訊息明確標註,並附原書頁與依據。

### 4.6 容差與三類規則的對應

| 規則類別 | 容差來源 |
|---|---|
| Pre-Constructive Rules of Logic(精華版 Ch3) | 4.2 三檔表 + 4.3 Rule 3 ±4% |
| Essential Construction Rules(精華版 Ch5 R1-R7) | 各規則 doc-comment 個別建檔(多為 ±4%) |
| Triangle 規則組(精華版 Ch5/Ch11) | 4.2 ±5%(僅限三腿等價特例) |
| Advanced Rules(精華版 Ch9 A1-A6) | 4.4 Exception Rule + 各規則特徵尺度 |
| Validator 工程護欄(forest_max_size 等) | §6 NeelyEngineConfig(屬工程,非規則) |

---

## 五、輸入

[Gap 3.1 = d / 3.2 = a / 3.4 = c]

### 5.1 輸入來源

| 輸入 | 來源 |
|---|---|
| `OHLCVSeries` | Silver 層 `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd`(由 S1_adjustment Rust binary 產出) |
| `Timeframe` | 日線 / 週線 / 月線(單一 Timeframe per `compute()` 呼叫) |
| `NeelyCoreParams` | §6.2 |

### 5.2 Silver 層職責邊界

Neely Core 直接讀 Silver 層 `price_*_fwd` 表,**所有資料前處理已由 Silver S1 完成**:

- 後復權處理
- 連續漲跌停合併(精華版未提,但漲跌停日 OHLC 是 `O=H=L=C` 違反 monowave「方向變化」前提 → S1 合併)
- TAIEX 加權指數的開盤前失真處理(若採用該指數;個股不需)

**Neely Core 完全不知道台股的存在**,純淨執行 Neely 規則。

> Silver 層處理職責清單見 `layered_schema_post_refactor.md` §4.1。漲跌停合併規則與後復權倒推算法的設計溯源見 `adr/0001_tw_market_handling.md` 附錄 B。

### 5.3 OHLC 處理策略(Hybrid)

**核心設計**:

```
monowave 切割演算法           → 用 (H+L)/2(對齊精華版「單日一筆價」)
Scenario / Monowave 保留資訊  → 完整 OHLC(供 invalidation trigger、LLM 推理使用)
```

#### 為何採用 Hybrid

**精華版 Ch2「該用什麼資料」原則**:

1. 嚴禁只用收盤價(close 不可獨用)
2. 不建議用線形 bar chart(同時含 H/L 無法做唯一比較)
3. **首選**:Cash 資料,單日一筆價(可用 (H+L)/2)

**本專案的補強**(因為「不做日內預測」前提):

- OHLC 之間的**日內順序資訊永遠遺失**(O→H→L→C 還是 O→L→H→C 都無法事後還原)
- 「依次序高低」方法(每日展開為 2 個點)需要日內順序假設 → **不可行**
- OHLC 全部進 monowave 切割(每日 4 個點)需要更強的日內順序假設 → **更不可行**
- **唯一可靠做法**:monowave 切割用 (H+L)/2(無歧義),但 Scenario / Monowave 保留 OHLC reference(完整資訊不丟)

#### 實作結構

```rust
pub struct Monowave {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,

    // monowave 切割使用值
    pub start_price_midpoint: f64,    // (H+L)/2 at start_date
    pub end_price_midpoint: f64,      // (H+L)/2 at end_date
    pub direction: Direction,

    // 完整 OHLC reference(P0 必要)
    pub start_ohlc: Ohlc,
    pub end_ohlc: Ohlc,
    pub intermediate_days: Vec<NaiveDate>, // monowave 中間經過的日期(可選查表)
}

pub struct Ohlc {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: u64,
}
```

#### LLM agent / Aggregation 端的使用

- 用 `*_price_midpoint` 做技術分析計算(對齊精華版)
- 用 `*_ohlc.close` 做收盤級別的 invalidation trigger
- 用 `*_ohlc.high` / `.low` 做高低點級別的精準支撐壓力分析
- 引用 `intermediate_days` 查表獲得 monowave 內部每日 OHLC(若需要)

### 5.4 Timeframe 處理(單 Timeframe per call)

**Gap 3.4 = c**:單 Timeframe 進 `compute()`,但 `NeelyCoreOutput` 包含 `cross_timeframe_hints` 欄位。

```rust
pub fn compute(
    series: OHLCVSeries,     // 單一 Timeframe
    params: NeelyCoreParams,
) -> Result<NeelyCoreOutput, NeelyCoreError>;
```

Aggregation Layer 呼叫三次(daily / weekly / monthly)後,自行做跨 Timeframe 比對。Core 內部不協同 Timeframe(避免「三個 Timeframe 結果衝突時誰優先」這種無解問題)。

`cross_timeframe_hints` 欄位提供「此 monowave 在當前 Timeframe 的 Structure Label」訊息,給 Aggregation 層比對使用(見 §8)。


---

## 六、NeelyEngineConfig

[Gap 4.3 = c / 4.4 = c]

### 6.1 設計理念:外部 Params vs 內部 Config

- **NeelyCoreParams**:Workflow toml 可宣告,屬「使用方選擇」
- **NeelyEngineConfig**:Core 內部工程參數,可調但有預設,**不**屬 Neely 規則本身

「Neely 規則」與「執行 Neely 規則所需的工程選擇」**嚴格區分**。Neely 規則一律寫死(§4.5),NeelyEngineConfig 只裝工程護欄參數。

### 6.2 NeelyCoreParams

```rust
pub struct NeelyCoreParams {
    pub timeframe: Timeframe,
    pub engine_config: NeelyEngineConfig,
}

pub enum Timeframe { Daily, Weekly, Monthly }
```

### 6.3 NeelyEngineConfig(8 個可調工程參數)

```rust
pub struct NeelyEngineConfig {
    /// ATR 計算週期(Rule of Proportion / Neutrality / 45° 判定的計量單位)
    /// 預設 14,跨 timeframe 統一(技術分析界事實標準)
    /// 屬「視覺幾何判定的量化代理」,合理詮釋(見 §6.5)
    pub atr_period: usize,                      // 預設 14

    /// Bottom-up Candidate Generator 的 beam width
    pub beam_width: usize,                      // 預設 50

    /// Forest 上限保護:超過此 size 用 BeamSearchFallback
    /// **v1.34 P0 Gate v2 校準後**:1000(r2 保守佔位)→ **200**
    /// production 1264 stocks 實測 max=37 / p99=16,留 ~5× p99 餘量
    pub forest_max_size: usize,                 // 預設 200(v1.34 校準前 1000)

    /// 單檔 Compaction 逾時(秒)
    pub compaction_timeout_secs: u64,           // 預設 60

    /// Forest 超過 max_size 時的處理策略
    pub overflow_strategy: OverflowStrategy,

    /// 加權指數套用 Rule of Neutrality 的中性區判定閾值(個股不適用)
    /// 預設 0.5(%)
    pub neutral_threshold_taiex: f64,           // 預設 0.5

    /// 啟用「固定參考尺度」mode 取代滾動 ATR(見 §6.5)
    /// 預設 false(走 ATR);true 時用 warmup 窗內 |close[t]-close[t-1]| 中位數
    pub use_fixed_reference_scale: bool,        // 預設 false

    /// Reverse Logic Fact 觸發的 forest_size 閾值
    /// 預設 5(P0 Gate 校準)
    /// 屬經驗閾值,精華版 Ch12 無具體數字(只說「越多越靠近中段」)
    pub reverse_logic_threshold: usize,         // 預設 5
}

pub enum OverflowStrategy {
    /// 用「Power Rating 級別分組 + 組內 rules_passed_count」排序保留 top-K
    /// 並標記 overflow_triggered = true
    BeamSearchFallback { k: usize },            // 預設 k = 100

    /// 不剪枝(P0 Gate 校準階段使用,生產環境不建議)
    Unbounded,
}
```

### 6.4 預設值

```rust
impl Default for NeelyEngineConfig {
    fn default() -> Self {
        Self {
            atr_period: 14,
            beam_width: 50,
            forest_max_size: 200,                // v1.34 P0 Gate v2 校準(原 1000)
            compaction_timeout_secs: 60,
            overflow_strategy: OverflowStrategy::BeamSearchFallback { k: 100 },
            neutral_threshold_taiex: 0.5,
            use_fixed_reference_scale: false,
            reverse_logic_threshold: 5,
        }
    }
}
```

### 6.5 ATR 工程審視

> **立場宣告**:r5 視 ATR 為「**Rule of Proportion 視覺判定的工程量化代理**」,屬合理詮釋,**不再聲明「ATR 不是 Neely 方法」**(與 §4 容差立場一致)。

#### ATR 在 Neely Core 中的三個用途

| 用途 | 精華版方法 | 本專案以 ATR 代理 | 評估 |
|---|---|---|---|
| Monowave 切割(direction change detection) | 收盤價逐點掃描(精華版 Ch3) | 噪訊濾波門檻(`Δclose < k·ATR` 視為同方向) | 補充用 |
| Rule of Proportion 45° 判定 | 圖表縮放使典型走勢約 45°(視覺幾何) | 將 `Δprice / Δtime` 換算到「ATR 為價軸單位、1 bar 為時軸單位」 | 合理代理 |
| Rule of Neutrality 水平段判定 | 「水平延伸不顯著」(精華版 Ch3) | `|單日漲跌幅| < k·ATR` 視為中性 | 合理代理 |

#### 滾動 ATR vs 固定參考尺度

精華版 Ch3 的「圖表縮放」是 **for the whole chart, fixed once**。本專案預設用滾動 ATR(14)會隨時間漂移。為兼顧:

- **預設用滾動 ATR**:對齊技術分析界慣例與既有實作慣性
- **新增 `use_fixed_reference_scale` 開關**:啟用後在 warmup 窗算出一次固定尺度
- **P0 Gate 雙模驗證**:在六檔上跑兩個 mode,差異 ≥ 10% 改採固定參考尺度為預設

固定參考尺度公式(供開關使用):

```rust
fixed_reference_scale = median(|close[t] - close[t-1]|) over warmup window
```

#### 文件責任

`monowave/pure_close.rs`、`monowave/proportion.rs`、`monowave/neutrality.rs` 三個檔案的 module-level doc-comment 必須標註:

```rust
//! Engineering quantification proxy for Neely's visual 45° chart scaling
//! (Rule of Proportion, Ch3). ATR is the default quantification unit.
//! For an alternative closer to Neely's "fixed chart proportion" philosophy,
//! set `use_fixed_reference_scale = true`. See architecture.md §6.5.
```

### 6.6 不可外部化的 Neely 規則

NeelyEngineConfig **僅含上述 8 個工程參數**,以下一律寫死:

- Fibonacci 比率清單(精華版 Ch5/Ch12 + r5 §11.3)
- 容差規則(§4)
- Validator 各規則的硬閾值(精華版相應章節)
- Power Rating 查表值(精華版 Ch10)

要改任一項 = 刻意偏離 Neely 詮釋,需在 commit 訊息明確標註。

> **`neutral_threshold_taiex` 例外說明**:此參數不屬於 Neely 規則本身,而是「Rule of Neutrality 套用於加權指數」的工程選擇(加權指數波動天然較個股小,沿用個股閾值會誤判)。精華版未列加權指數特例。

---

## 七、Pipeline 階段

[Gap 3.4 / 4.1 / 4.2]

### 7.1 完整 Pipeline 流程

```
OHLCVSeries(單一 Timeframe)
    ↓
[Stage 1] Monowave Detection
    - 精華版 Ch3「Monowave 的辨識」
    - 用 (H+L)/2 做轉折偵測(§5.3 Hybrid)
    - 套用 Rule of Neutrality Aspect 1/2(精華版 Ch3)
    ↓
monowave_series: Vec<Monowave>(已含完整 OHLC reference)
    ↓
[Stage 2] Rule of Proportion / Directional-NonDirectional 標註
    - 精華版 Ch3「Rule of Proportion」
    ↓
classified_monowaves
    ↓
[Stage 0] Pre-Constructive Logic
    - 精華版 Ch3「Pre-Constructive Rules of Logic」
    - 對每個 monowave 跑 Retracement Rules(7 條)+ Conditions(a-f)+ Rule 4 Categories(i/ii/iii)
    - ~200 分支的 if-else 條件樹(精華版 Ch3 p.3-34~3-60)
    ↓
labeled_monowaves(每個 monowave 帶 Structure Label 候選清單)
    ↓
[Stage 3] Bottom-up Candidate Generator
    - 精華版 Ch4「Monowave Groups → Polywaves」
    - 對已 Structure Labelled 的 monowave 序列搜尋 Figure 4-3 五大 Standard + 三大 Non-Standard Series
    - 套用 Rule of Similarity & Balance 過濾(精華版 Ch4)
    ↓
candidates: Vec<WaveCandidate>
    ↓
[Stage 3.5] Pattern Isolation + Zigzag DETOUR Test
    - 精華版 Ch3「Pattern Isolation Procedures」(6 步驟)
    - 精華版 Ch4「Zigzag DETOUR Test」
    ↓
isolated_candidates
    ↓
[Stage 4] Validator(Essential Construction + 變體規則)
    - 精華版 Ch5 R1-R7(Essential Construction Rules)
    - 精華版 Ch5/Ch11 Flat / Zigzag / Triangle 變體規則
    - 精華版 Ch5 Overlap / Equality / Alternation
    ↓
valid_candidates
    ↓
[Stage 5] Classifier
    - 精華版 Ch5「Realistic Representations」對齊命名
    - 精華版 Ch8 Complex Polywaves / Non-Standard 分類
    - 精華版 Ch10 Power Ratings 變體對齊
    ↓
classified_scenarios
    ↓
[Stage 6] Post-Constructive Validator(W1-W2)
    - 精華版 Ch6「衝動波兩階段確認 / 修正波確認 / Triangle 確認」
    ↓
post_validated_scenarios
    ↓
[Stage 7] Complexity Rule + Triplexity
    - 精華版 Ch7「Complexity Rule」+「Triplexity」
    ↓
complexity_filtered_scenarios
    ↓
[Stage 7.5] Channeling + Advanced Rules
    - 精華版 Ch5 + Ch12 Channeling(0-2 / 2-4 / 1-3 / 0-B / b-d)
    - 精華版 Ch9 Advanced Rules(Trendline Touchpoints / Time Rule / Independent / Simultaneous / Exception / Structure Integrity)
    ↓
advanced_validated_scenarios
    ↓
[Stage 8] Compaction — Three Rounds 遞迴
    - 精華版 Ch4「Three Rounds 教學流程」+ Ch7 Compaction
    - Round 1(Series 識別)→ Round 2(Compaction + 邊界重評)
    - 若仍存在 :L5/:L3 → 遞迴回 Round 1(更高級層面)
    - 若不存在 :L5/:L3 → Round 3(暫停,scenario 標 awaiting_l_label)
    - Forest 上限保護(§10)
    ↓
scenario_forest: Vec<Scenario>
    ↓
[Stage 9] Missing Wave 偵測 + Emulation 辨識
    - 精華版 Ch12「Missing Waves」+「Emulation」
    - 跨 Scenario 層級警示(放入 NeelyCoreOutput 而非單一 Scenario)
    ↓
augmented_output(含 emulation_suspects, missing_wave_suspects)
    ↓
[Stage 10] Power Rating 查表 + Max Retracement + Fibonacci 投影 + Invalidation Triggers
    - 精華版 Ch10 Power Rating 表 + Max Retracement 對應
    - 精華版 Ch12「Advanced Fibonacci Relationships」Internal / External
    - 精華版 Ch12「Waterfall Effect」階梯
    ↓
[Stage 10.5] Reverse Logic 偵測
    - 精華版 Ch12「Reverse Logic Rule」
    - 若 forest_size >= reverse_logic_threshold → reverse_logic.activated = true
    ↓
[Stage 11] Degree Ceiling 推導(r5 新增)
    - 依資料量決定本次分析能達到的最高 Degree(精華版 Ch7 11 級體系)
    ↓
[Stage 12] cross_timeframe_hints 計算(r5 新增)
    - 為每個 monowave 整理「在當前 Timeframe 的 Structure Label」訊息
    - 供 Aggregation 層跨 Timeframe 比對使用
    ↓
NeelyCoreOutput(§8)
```

### 7.2 階段失敗處理(Hybrid 失敗模型)

[Gap 1.3 D = c]

| 階段失敗 | 失敗類型 | 處理 |
|---|---|---|
| Stage 1(輸入格式錯誤) | 工程失敗 | `Err(NeelyCoreError::InvalidInput)` |
| Stage 1(資料不足) | 規則失敗 | `Ok(NeelyCoreOutput { insufficient_data: true, scenario_forest: vec![], ... })` |
| Stage 0(monowave 不足以套 Retracement Rules) | 規則失敗 | 該 monowave Structure Label 標 `:?`(未定),寫入 diagnostics |
| Stage 4(全部 reject) | 規則失敗 | `Ok(NeelyCoreOutput { scenario_forest: vec![], diagnostics: ..., ... })` |
| Stage 8(Forest 爆量) | 規則失敗 | 套用 `OverflowStrategy::BeamSearchFallback`,保留 top-K |
| Stage 8(逾時) | 規則失敗 | 中斷並回傳 `Ok(NeelyCoreOutput { compaction_timeout: true, scenario_forest: <partial>, ... })` |
| Stage 8(Round 3 暫停) | 規則狀態 | Forest 中所有 scenario 標 `awaiting_l_label = true`,Output 加 `round3_pause` 摘要 |
| OOM / Panic / 其他工程失敗 | 工程失敗 | `Err(NeelyCoreError::Engine(...))` |

#### Hybrid 失敗模型的設計理由

- **工程失敗(`Err`)**:呼叫端必須處理,不能繼續往下走
- **規則失敗(`Ok` + 旗標)**:仍是合法 Neely 判讀結果(「Neely 認為現在無解」),呼叫端可選擇繼續顯示空 Forest + 警示

### 7.3 階段可觀察性

每階段的耗時、輸入輸出計數、reject 原因皆寫入 `NeelyDiagnostics`(§15)。

### 7.4 Three Rounds 對應 Pipeline

[Gap 1.2 群 2 = a]

| Round | 對應 Pipeline Stage | 動作 | 輸出 |
|---|---|---|---|
| **Round 1** | Stage 3 + 3.5 | 搜尋 Series + Similarity & Balance + DETOUR | Series 候選清單 |
| **Round 2** | Stage 4 ~ 8 | Validator + Compaction 為 `:_3/:_5` + 邊界 m(-1)/m(+1) 重評 | 壓縮後 Series + 更新邊界 Structure Label |
| **(遞迴)** | 回 Stage 3 | 把壓縮後 Series 視為更大級 monowave,更高級層面重做 Round 1 | 更高級 Forest |
| **Round 3** | Stage 8 後若無 :L5/:L3 | 暫停判讀 → Scenario 標 `awaiting_l_label = true` | Forest with awaiting flag |

**Round 3 策略含意**:當所有 scenario 都標 `awaiting_l_label = true`,Output 的 `round3_pause` 欄位提供整體摘要,Aggregation Layer 顯示「**等待新形態具備收尾條件**」橫幅。

### 7.5 Special Circumstances(Compacted 超出自身起點)

若某個 Compacted 形態的 price action 在完成前**已超出該形態自身的起點**,該 Compacted 形態的 base Structure **強制為 `:3`**(修正性),無論 Pre-Constructive Rules 建議什麼。

詳見精華版 Ch3「Special Circumstances」。實作位置:`structure_labeler/special_circumstances.rs`,在 Round 2 遞迴前優先檢查。

---


## 八、NeelyCoreOutput 結構

[Gap 1.3 完整定案]

### 8.1 頂層結構

```rust
pub struct NeelyCoreOutput {
    // === 主結果(平等並列,無 primary)===
    pub scenario_forest: Vec<Scenario>,

    // === 跨 Scenario 警示(Gap 1.2 群 6 拆分過來)===
    pub emulation_suspects: Vec<EmulationCandidate>,
    pub missing_wave_suspects: Vec<MissingWaveCandidate>,

    // === 中間層產物(供 trendline_core 等下游使用)===
    pub monowave_series: Vec<Monowave>,

    // === Reverse Logic 觀察(一級訊號)===
    pub reverse_logic: ReverseLogicObservation,

    // === Round 3 暫停狀態(整體摘要,Scenario 內仍標 awaiting_l_label)===
    pub round3_pause: Option<Round3PauseInfo>,

    // === Degree ceiling(資料量推導)===
    pub degree_ceiling: DegreeCeiling,

    // === 跨 Timeframe 比對提示(Aggregation Layer 用)===
    pub cross_timeframe_hints: CrossTimeframeHints,

    // === 診斷 ===
    pub diagnostics: NeelyDiagnostics,

    // === 元資料 ===
    // 注:source_version / params_hash / computed_at 屬 caller-side metadata,
    // 不直接在 NeelyCoreOutput struct 內,由 tw_cores binary 序列化進
    // structural_snapshots 表時填:
    //   - source_version → 直接讀 NeelyCore::version() 寫進表
    //   - params_hash    → caller 用 fact_schema::params_hash(&params) 算
    //   - computed_at    → 表的 created_at 欄位 TIMESTAMPTZ DEFAULT NOW() 自動填
    // 設計依據:避免 compute() 變 impure(Utc::now() 呼叫)+ 避免引入 caller-only
    // 資料的空殼欄位(對齊 cores_overview §四 禁止抽象)。
    pub source_version: String,                 // "neely_core 1.0.0"(實作:caller 寫表時填)

    // === 失敗旗標(Hybrid 失敗模型)===
    pub insufficient_data: bool,                // 規則失敗:資料不足
    pub compaction_timeout: bool,               // 規則失敗:Compaction 逾時(v0.20.0 上提至頂層,
                                                //   對稱 insufficient_data;NeelyDiagnostics 保留雙寫)
}

pub fn compute(
    series: OHLCVSeries,
    params: NeelyCoreParams,
) -> Result<NeelyCoreOutput, NeelyCoreError>;

pub enum NeelyCoreError {
    InvalidInput(String),
    Engine(String),       // OOM / Panic / 其他工程失敗
    SilverUnavailable,
}
```

### 8.2 Forest 不選 primary 的具體實作

- `scenario_forest: Vec<Scenario>` **不附** `primary: Scenario` 欄位
- 順序**不**反映優先級(可按 `id` 字典序或 `power_rating` 排序顯示,語意上平等)
- Aggregation Layer 可依 `power_rating` 提供 UI 篩選,但**不在 Core 層做選擇**

### 8.3 ReverseLogicObservation

```rust
pub struct ReverseLogicObservation {
    pub forest_size: usize,
    pub threshold: usize,                       // 來自 NeelyEngineConfig.reverse_logic_threshold
    pub activated: bool,                        // forest_size >= threshold
    pub interpretation: Option<String>,
        // 例:"forest_size=42 suggests market in middle of larger pattern"
    pub neely_page: String,                     // "Ch12_p12-50"
}
```

### 8.4 Round3PauseInfo

```rust
pub struct Round3PauseInfo {
    pub scenarios_affected: usize,              // 所有暫停的 scenario 數
    pub last_l_label_date: Option<NaiveDate>,   // 上次出現 :L5/:L3 的日期
    pub strategy_implication: String,
        // "持有原方向,等候新形態收尾條件"
}
```

**雙標設計理由**:Scenario 內標 `awaiting_l_label` = 個別 Scenario 的事實(可能只部分暫停);Output 頂層 `round3_pause` = 整體判讀狀態摘要(全部暫停時呈現給使用者)。

### 8.5 DegreeCeiling

[Gap 3.5 = c]

```rust
pub struct DegreeCeiling {
    pub max_reachable_degree: Degree,
        // 本次分析依資料量可達到的最高 Degree(精華版 Ch7 11 級)
    pub reason: String,
        // 例:"data spans 8 years, monthly compaction reaches Minor at best"
}

pub enum Degree {
    SubMicro, Micro, SubMinuette, Minuette, Minute,
    Minor, Intermediate, Primary, Cycle, Supercycle, GrandSupercycle,
}
```

**為何需要**:台股大部分個股上市 5-15 年,根本到不了 Cycle 級別。Aggregation 層可據此自動降低顯示的 Degree 標籤,避免「短歷史股票被標為 Supercycle」的誤導。

### 8.6 CrossTimeframeHints

[Gap 3.4 = c]

```rust
pub struct CrossTimeframeHints {
    pub timeframe: Timeframe,                   // 本次分析的 Timeframe
    pub monowave_summaries: Vec<MonowaveSummary>,
}

pub struct MonowaveSummary {
    pub monowave_index: usize,
    pub date_range: (NaiveDate, NaiveDate),
    pub structure_label_candidates: Vec<String>,
        // 例:[":L5", ":F3"]
    pub price_range: (f64, f64),
}
```

**為何放在 Output 而非 Aggregation 自己算**:Neely Core 本來就有 monowave 完整資訊,直接輸出比 Aggregation 重新解析 `structural_snapshots` 高效。

### 8.7 v1.1 → v2.0 / r5 欄位變動清單

| v1.1 / r4 欄位 | r5 處理 | 理由 |
|---|---|---|
| `primary: Scenario` | 移除 | 不選最優(§2) |
| `alternatives: Vec<Scenario>` | 改為 `scenario_forest` | 平等並列 |
| `confidence: f64` | 移除 | 機率語意(§2.2) |
| `composite_score: f64` | 移除 | 主觀加權 |
| `emulation_suspects` / `missing_wave_suspects`(r4 在 Scenario 內) | 上移至 Output 頂層 | 跨 Scenario 警示,非單一 Scenario 屬性 |
| `monowave_series`(r4 不明確) | 明確列 Output 頂層欄位 | trendline_core 直接讀(§18.3) |
| `reverse_logic`(r4 僅 §2.4 哲學) | 升級為頂層欄位 | Neely 核心訊號,非工程診斷 |
| `degree_ceiling` / `cross_timeframe_hints` | r5 新增 | 對應 Gap 3.4 / 3.5 |
| `Result<Output, Error>` 失敗模型 | r5 採 Hybrid(§7.2) | 工程失敗 vs 規則失敗分離 |

---

## 九、Scenario 內部結構

[Gap 1.2 完整定案]

### 9.1 完整結構

```rust
pub struct Scenario {
    // === 群 1:結構識別 ===
    pub id: String,
    pub wave_tree: WaveNode,
    pub pattern_type: NeelyPatternType,
    pub structure_label: String,            // 例:"5-3-5 Zigzag in W4 of larger Impulse"
    pub complexity_level: ComplexityLevel,

    // === 群 2:Pre-Constructive + Three Rounds 狀態 ===
    pub monowave_structure_labels: Vec<MonowaveStructureLabels>,
    pub round_state: RoundState,
    pub awaiting_l_label: bool,
    pub pattern_isolation_anchors: Vec<PatternIsolationAnchor>,
    pub triplexity_detected: bool,

    // === 群 3:Neely 書中查表屬性 ===
    pub power_rating: PowerRating,
    pub max_retracement: Option<f64>,           // None = 任意(Triangle/Terminal 內覆蓋為 None)
    pub post_pattern_behavior: PostBehavior,

    // === 群 4:客觀計數 ===
    pub passed_rules: Vec<RuleId>,
    pub deferred_rules: Vec<RuleId>,
    pub rules_passed_count: usize,
    pub deferred_rules_count: usize,

    // === 群 5:失效條件 ===
    pub invalidation_triggers: Vec<Trigger>,

    // === 群 6:Fibonacci + 結構性事實 ===
    pub expected_fib_zones: Vec<FibZone>,
    pub structural_facts: StructuralFacts,
}
```

### 9.2 群 3 — PowerRating + PostBehavior

```rust
/// Power Rating(精華版 Ch10)
/// 方向中性語意:相對於前一段趨勢方向解釋
pub enum PowerRating {
    StronglyFavorContinuation,    // +3
    ModeratelyFavorContinuation,  // +2
    SlightlyFavorContinuation,    // +1
    Neutral,                      //  0
    SlightlyAgainstContinuation,  // -1
    ModeratelyAgainstContinuation,// -2
    StronglyAgainstContinuation,  // -3
}

/// 後續行為(結構化 enum,Gap 1.2 群 3 PostBehavior = a)
pub enum PostBehavior {
    /// 必完全回測整段(例:5th Failure、C-Failure、Terminal)
    FullRetracementRequired,

    /// 必回測 ≥ N% 整段
    MinRetracement { ratio: f64 },

    /// 必達 wave-X 區
    ReachesWaveZone { wave: WaveNumber },

    /// 後續 Impulse 必 > N% × 前一同向 Impulse
    NextImpulseExceeds { ratio: f64 },

    /// 不會被完全回測(除非為更大級的 5/c)
    NotFullyRetracedUnless { exception: String },

    /// 任意後續(Neutral)
    Unconstrained,

    /// 強烈暗示後續形態(例:±1~±3 被回測 100% → Triangle/Terminal)
    HintsAtPattern { suggested_pattern: NeelyPatternType, reason: String },

    /// 多模式組合(後續行為跨多條規則)
    Composite { behaviors: Vec<PostBehavior> },
}
```

### 9.3 群 4 — RuleId(Neely 章節編碼)

[Gap 1.2 群 4 = a]

> **r5 範圍說明**(v0.20.0 落地時新增,2026-05-13):
>
> 本節列出完整 76 個 RuleId variants 作為**文件追溯參照**。實作體系實際 dispatch
> 進 `RuleRejection.rule_id` 的 variants **限縮為三組**:
>
>   - `Ch5_*`(Essential / Overlap_Trending / Overlap_Terminal / Equality /
>     Alternation / Flat_* / Zigzag_* / Triangle_* / Channeling_*)
>   - `Ch9_*`(TrendlineTouchpoints / TimeRule / Independent / Simultaneous /
>     Exception_Aspect{1,2} / StructureIntegrity)
>   - `Engineering_*`(InsufficientData / ForestOverflow / CompactionTimeout)
>
> 其他章節的規則「結果」改用 **domain-specific enums / fields** 取代 RuleId variants:
>
> | 章節 | 對應 domain-specific 資料結構 |
> |---|---|
> | Ch3 Pre-Constructive Logic | `StructureLabel` candidates(寫入 ClassifiedMonowave.structure_label_candidates) |
> | Ch4 Three Rounds | `Scenario.compacted_base_label` + `Scenario.in_triangle_context` |
> | Ch6 Post-Constructive | `pattern_complete: bool`(Stage 6 過濾 forest) |
> | Ch7 Compaction Reassessment | `Scenario.compacted_base_label`(`:5` / `:3`) |
> | Ch8 Complex Polywaves | `CombinationKind` enum 11 variants(對齊 Table A/B) |
> | Ch10 Power Ratings | `PowerRating` enum 7-level table(±3..±3) |
> | Ch11 Wave-by-Wave 變體 | 直接寫進 `RuleRejection.gap`(無 RuleId variant) |
> | Ch12 Missing Wave | `MissingWaveSuspect` + `MissingWavePosition` enum |
> | Ch12 Emulation | `EmulationSuspect` + `EmulationKind` enum 5 variants |
> | Ch12 Reverse Logic | `ReverseLogicObservation`(scenario_count / triggered / suggested_filter_ids) |
> | Ch12 Fibonacci | `FibZone`(Internal + External 兩組) |
> | Ch5/Ch9 Channeling 諮詢性發現 | `AdvisoryFinding`(severity + message) |
>
> **設計依據**:`m3Spec/cores_overview.md §四`(禁止抽象)+ `§十四`(prematurely
> declare 未實際 dispatch 的 RuleId 不該做)。下方完整 RuleId 清單保留作為:
>
>   1. 章節追溯文件 — 看哪條規則對應哪個 Neely 書頁
>   2. P0 Gate 後若 production SQL 需「按 Chapter 統計拒絕原因」,可批量補 missing
>      variants 進 Rust enum(目前 56 個 spec-only variants 在 code 不存在)

```rust
/// Rule 編碼採 Neely 章節對應,而非自編序號
/// 設計目的:RuleId 本身即為書頁追溯,免維護自編號對應表
pub enum RuleId {
    // === Ch3 Pre-Constructive Rules of Logic ===
    Ch3_PreConstructive {
        rule: u8,                               // 1-7
        condition: char,                        // 'a'-'f'
        category: Option<char>,                 // 'i'/'ii'/'iii'(僅 Rule 4)
        sub_rule_index: Option<u8>,             // 同 Condition 內的多條子規則
    },
    Ch3_Proportion_Directional,
    Ch3_Proportion_NonDirectional,
    Ch3_Neutrality_Aspect1,
    Ch3_Neutrality_Aspect2,
    Ch3_PatternIsolation_Step(u8),              // 1-6 步驟
    Ch3_SpecialCircumstances,                   // Compacted 超出自身起點 → :3

    // === Ch4 Intermediary Observations ===
    Ch4_SimilarityBalance_Price,
    Ch4_SimilarityBalance_Time,
    Ch4_Round1_Series,
    Ch4_Round2_Compaction,
    Ch4_Round3_Pause,
    Ch4_ZigzagDetour,

    // === Ch5 Central Considerations ===
    Ch5_Essential(u8),                          // R1-R7
    Ch5_Equality,
    Ch5_Extension,
    Ch5_Extension_Exception1,                   // 1st 最長例外
    Ch5_Extension_Exception2,                   // 3rd 最長但 < 161.8% × 1st
    Ch5_Overlap_Trending,
    Ch5_Overlap_Terminal,
    Ch5_Alternation { axis: AlternationAxis },
    Ch5_Channeling_02,
    Ch5_Channeling_24,
    Ch5_Channeling_13,
    Ch5_Channeling_0B,
    Ch5_Channeling_BD,
    Ch5_Flat_Min_BRatio,
    Ch5_Flat_Min_CRatio,
    Ch5_Zigzag_Max_BRetracement,
    Ch5_Zigzag_C_TriangleException,
    Ch5_Triangle_BRange,
    Ch5_Triangle_LegContraction,
    Ch5_Triangle_LegEquality_5Pct,

    // === Ch6 Post-Constructive Rules ===
    Ch6_Impulse_Stage1,
    Ch6_Impulse_Stage2 { extension: ImpulseExtension },
    Ch6_Correction_BSmall_Stage1,
    Ch6_Correction_BSmall_Stage2,
    Ch6_Correction_BLarge_Stage1,
    Ch6_Correction_BLarge_Stage2,
    Ch6_Triangle_Contracting_Stage1,
    Ch6_Triangle_Contracting_Stage2,
    Ch6_Triangle_Expanding_NonConfirmation,

    // === Ch7 Conclusions ===
    Ch7_Compaction_Reassessment,
    Ch7_Complexity_Difference,
    Ch7_Triplexity,

    // === Ch8 Complex Polywaves ===
    Ch8_NonStandard_Cond1,                      // 中介修正 < 61.8%
    Ch8_NonStandard_Cond2,                      // 中段 ≥ 161.8%
    Ch8_XWave_InternalStructure,
    Ch8_LargeXWave_NoZigzag,
    Ch8_ExtensionSubdivision_Independence,
    Ch8_Multiwave_Construction,

    // === Ch9 Advanced Rules ===
    Ch9_TrendlineTouchpoints,
    Ch9_TimeRule,
    Ch9_Independent,
    Ch9_Simultaneous,
    Ch9_Exception_Aspect1 { situation: ExceptionSituation },
    Ch9_Exception_Aspect2 { triggered_new_rule: String },
    Ch9_StructureIntegrity,

    // === Ch10 Advanced Logic Rules ===
    Ch10_PowerRating_Lookup,
    Ch10_MaxRetracement_Lookup,
    Ch10_TriangleTerminal_PowerOverride,

    // === Ch11 Advanced Progress Label Application ===
    Ch11_Impulse_WaveByWave { ext: ImpulseExtension, wave: WaveNumber },
    Ch11_Terminal_WaveByWave { ext: ImpulseExtension, wave: WaveNumber },
    Ch11_Flat_Variant_Rules { variant: FlatVariant, wave: WaveAbc },
    Ch11_Zigzag_WaveByWave { wave: WaveAbc },
    Ch11_Triangle_Variant_Rules { variant: TriangleVariant, wave: TriangleWave },

    // === Ch12 Advanced Neely Extensions ===
    Ch12_Channeling_RunningDoubleThree,
    Ch12_Channeling_TriangleEarlyWarning,
    Ch12_Channeling_TerminalEarlyWarning,
    Ch12_Fibonacci_Internal,
    Ch12_Fibonacci_External,
    Ch12_WaterfallEffect,
    Ch12_MissingWave_MinDataPoints,
    Ch12_Emulation { kind: EmulationKind },
    Ch12_ReverseLogic,
    Ch12_LocalizedChanges,

    // === 工程護欄(非 Neely 規則,獨立列出)===
    Engineering_ForestOverflow,
    Engineering_CompactionTimeout,
    Engineering_InsufficientData,
}

pub enum AlternationAxis { Price, Time, Severity, Intricacy, Construction }
pub enum ExceptionSituation { MultiwaveEnd, TerminalW5OrC, TriangleEntryExit }
pub enum WaveAbc { A, B, C }
pub enum TriangleWave { A, B, C, D, E }
pub enum TriangleVariant {
    HorizontalLimiting, IrregularLimiting, RunningLimiting,
    HorizontalNonLimiting, IrregularNonLimiting, RunningNonLimiting,
    HorizontalExpanding, IrregularExpanding, RunningExpanding,
}
```

**RuleId 設計優點**:

- doc-comment 引用零成本 — `Ch3_PreConstructive { rule: 5, condition: 'b', .. }` 直接對應精華版 Ch3 Rule 5 Cond 5b
- 不需維護自編號 ↔ Neely 章節的對應表
- 未來精華版條目擴充時,新增 enum variant 即可,無需重編號

### 9.4 群 5 — Trigger 結構

```rust
pub struct Trigger {
    pub trigger_type: TriggerType,
    pub on_trigger: OnTriggerAction,
    pub rule_reference: RuleId,
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
    WeakenScenario,                              // 進入 deferred,不引入機率語意
    PromoteAlternative { promoted_id: String },
}
```

### 9.5 群 6 — StructuralFacts(8 子欄位)

```rust
pub struct StructuralFacts {
    pub fibonacci_alignment: FibonacciAlignment,
    pub alternation: Alternation5Axes,           // 5 軸展開
    pub channeling: ChannelingFact,
    pub time_relationship: TimeRelationship,     // 對應 Ch9 Time Rule 三種關係
    pub volume_alignment: VolumeAlignment,
    pub gap_count: usize,
    pub overlap_pattern: OverlapPattern,         // Trending(禁止) / Terminal(必須) / None
    pub extension_subdivision_pair: ExtensionSubdivisionPair,
}

pub struct Alternation5Axes {
    pub price: AlternationCheck,
    pub time: AlternationCheck,
    pub severity: AlternationCheck,              // 僅 Impulse 2/4
    pub intricacy: AlternationCheck,
    pub construction: AlternationCheck,
}

pub enum AlternationCheck {
    AlternatePresent { evidence: String },
    AlternateAbsent { suggested_pattern: String },
    NotApplicable,
}

/// Extension 與 Subdivision 獨立記錄(精華版 Ch8)
pub struct ExtensionSubdivisionPair {
    pub extension_wave: WaveNumber,              // 最長
    pub subdivision_wave: WaveNumber,            // 細分最多
    pub independent: bool,                       // false = 同一波
    pub terminal_hint: bool,                     // 若 3-Ext 但 5 細分 → true
}
```

### 9.6 NeelyPatternType / FlatVariant 等型別

完整 `enum` 定義對齊精華版 Ch5/Ch8/Ch10/Ch11 命名。詳細變體清單與規則對應見精華版相應章節。本 spec 不在此重述以避免重複(對應 r4 §9.1 內容已移至精華版規則層)。

> **r5 修正 r4 §9.8.1 設計分歧**:`FlatVariant` 不再包含 `StrongBWave` / `WeakBWave`(這兩個是 b 強度**分類軸**,不是具體形態變體)。改為具體形態變體:`Common / BFailure / CFailure / Irregular / IrregularFailure / Elongated / DoubleFailure`(7 種 named)+ top-level `NeelyPatternType::RunningCorrection`(1 種,屬 Power Rating -3/+3 級別獨立)。b-wave 強度資訊改放 `Scenario.structural_facts` 或 `FlatVariant` 的關聯欄位中。

---


## 十、Forest 上限保護

[Gap 4.1 = c / 4.2 = a]

### 10.1 工程現實衝突

第七章「窮舉所有壓縮路徑」與工程現實衝突:某些股票的解讀可能上千棵 Forest,記憶體與時間都會爆炸。

### 10.2 護欄機制

```rust
pub struct NeelyEngineConfig {
    pub forest_max_size: usize,                  // 預設 200(v1.34 P0 Gate v2 校準前 1000)
    pub compaction_timeout_secs: u64,            // 預設 60
    pub overflow_strategy: OverflowStrategy,
}

pub enum OverflowStrategy {
    /// 雙重排序保留 top-K(Gap 4.1 = c)
    /// 1. 先按 PowerRating 級別分組(±3 / ±2 / ±1 / 0)
    /// 2. 組內按 rules_passed_count 排序
    /// 保留:強訊號級別優先 + 組內通過規則多者優先
    /// 標記 NeelyDiagnostics.overflow_triggered = true
    BeamSearchFallback { k: usize },             // 預設 k = 100

    /// 不剪枝(P0 Gate 校準階段使用)
    Unbounded,
}
```

### 10.3 雙重排序的設計理由

[Gap 4.1 = c]

**為何不單純用 power_rating**:容易被指責「power_rating 是 Neely 主觀分類」(儘管 §11 已論證為查表非主觀)。

**為何不單純用 rules_passed_count**:通過規則多不等於結構更可靠(有些規則互相蘊含,通過數虛高)。

**雙重排序取兩者之長**:

```
排序鍵 1:PowerRating 級別(±3 / ±2 / ±1 / 0)
   - 保證「強訊號 Scenario 不被弱訊號擠掉」
排序鍵 2(組內):rules_passed_count
   - 提供同級別內的二次篩選依據
```

當 forest 爆量時,先丟掉「同級別中通過規則少」的 Scenario,而不是直接丟掉某個級別。

### 10.4 護欄執行流程

```
Compaction 執行中
   ↓
forest 累積 → 持續監控 size 與 elapsed
   ↓
若 size > forest_max_size:
   套用 overflow_strategy
   - BeamSearchFallback { k }:雙重排序保留 top-K + overflow_triggered = true
   - Unbounded:繼續累積(僅 P0 Gate 用)
   ↓
若 elapsed > compaction_timeout_secs:
   中斷,回傳 Ok(Output) 含已處理部分 forest + compaction_timeout = true
   ↓
所有拒絕原因寫入 NeelyDiagnostics.rejections(完整保留)
```

### 10.5 BeamSearchFallback k 值上界

**警告**:k 值不應無限放大。若 P95 真的超過 1000 而需強化截斷,應**回頭重審 Compaction 演算法本身**(§16 紅燈條件)。

持續加大 k 值代表演算法本身有設計問題,需根本性重構,**不可繼續以 k 規避**。

### 10.6 預設值理由

| 參數 | 預設 | 理由 |
|---|---|---|
| `forest_max_size` | **200** | **v1.34 P0 Gate v2(2026-05-14)校準後**:production 1264 stocks 實測 max=37 / p99=16,從 r2 保守佔位 1000 降至 200(留 ~5× p99 餘量);超過會走 BeamSearchFallback k=100 |
| `compaction_timeout_secs` | 60 | 保守佔位 |
| `BeamSearchFallback { k }` | 100 | 100 棵已遠超人類能消化的解讀數 |

---

## 十一、Power Rating 截斷哲學

### 11.1 可能質疑

> 「用 power_rating 排序後砍低分,等於下了主觀判斷」

### 11.2 回應

#### 11.2.1 power_rating 不是主觀分數

- power_rating 是**精華版 Ch10 寫死的查表值**
- 是 Neely 自己定義的「型態強度等級」,**不是 Pipeline 計算的主觀分數**
- 任何 scenario 的 power_rating 都是查表得出,與 Pipeline 算法無關
- enum 命名 `FavorContinuation` / `AgainstContinuation` 為方向中性語意,避免「Bullish/Bearish」造成方向誤用

#### 11.2.2 截斷不是排序展示

- 截斷是 **Core 內部資源管理**(避免 OOM)
- Aggregation Layer 看到的仍是「Forest 已是 Neely 規則允許的合法解讀」
- Aggregation 層仍**不做加權整合**,使用者看到「Top K 都是合法解讀,只是因系統資源限制只呈現這 K 個」

#### 11.2.3 截斷必須可觀察

- `NeelyDiagnostics.overflow_triggered = true` 時,Aggregation Layer **必須**將此狀態傳給前端
- 前端顯示「此股結構過於複雜,系統呈現 Top K 解讀」橫幅
- 使用者**知情**,可選擇相信或不相信

### 11.3 為何不退回 Unbounded

- Unbounded 模式 P95 可能 OOM 或超時,生產環境不可接受
- v2.0 哲學「不替使用者選擇」 ≠ 「無視工程現實」
- Forest 上限保護是「窮舉精神 + 工程可行性」之間的合理折衷

### 11.4 Power Rating × Max Retracement 對應

詳細對應表見**精華版 Ch10**。本 spec 不重述,但實作位置:`power_rating/max_retracement.rs`。

關鍵覆蓋規則:形態出現在 **Contracting Triangle 內部**或 **Terminal Impulse 內部**時,Power Rating 覆蓋為 0(對應精華版 Ch10「(in a Triangle = 0)」)。

### 11.5 各修正 Power Rating 後續行為

詳細對應表見**精華版 Ch10「各修正暗示重點」**(精華版完整列出 14 條,涵蓋 Triple Zigzag 至 Triple Three Running)。

> **Gap 5 核實補充**:r4 §13.4.4 漏列三條:Truncated Zigzag、Common Flat、Elongated 在 Triangle/Terminal 內的特殊行為。已於精華版 Ch10 補入(見 Appendix B)。

---

## 十二、Fact 產出規則

### 12.1 Fact 的時機

每個 scenario 在進入 forest 時產出對應 Fact。每日 batch 重算後,新出現的 scenario 對應產出新 Fact;消失的 scenario 對應舊 Fact 自動標記為失效(透過 `invalidation_triggers` 觸發)。

### 12.2 Fact 範例

| Fact statement | metadata |
|---|---|
| `Neely Impulse 5-wave detected with power_rating=StronglyFavorContinuation` | `{ pattern: "impulse", power_rating: "strongly_favor_continuation", scenario_id: "..." }` |
| `Neely Zigzag (Normal) in W4 of larger Impulse, currently at B wave` | `{ pattern: "zigzag", length: "normal", current_position: "wave_b" }` |
| `Neely Triangle Contracting Limiting Horizontal, 3 of 5 sub-waves completed` | `{ pattern: "triangle", contraction: "contracting", confinement: "limiting", variation: "horizontal", sub_waves_completed: 3 }` |
| `Neely Reverse Logic activated: forest_size=42 suggests market in middle of larger pattern` | `{ event: "reverse_logic_activated", forest_size: 42, threshold: 5, neely_page: "Ch12_p12-50" }` |
| `Neely Round 3 awaiting: no :L5/:L3 in current data, hold original direction` | `{ event: "round3_awaiting", scenarios_affected: 12 }` |
| `Neely Missing Wave suspected: pattern=DoubleZigzag, data_points=8 (min required 10)` | `{ event: "missing_wave_suspected", pattern: "double_zigzag", actual_points: 8, min_required: 10, likely_missing: "x_wave" }` |
| `Neely Emulation suspected: Double Zigzag emulating Impulse, channel too perfect` | `{ event: "emulation_suspected", emulation_kind: "double_triple_zigzag_as_impulse", evidence: ["channel_perfect", "alternation_absent_24"] }` |
| `Neely Triplexity detected at 5th Extension subdivision` | `{ event: "triplexity_detected", location: "5th_ext_subdivision" }` |
| `Neely Localized change: wave-5 of prior Impulse demoted to wave-1 of larger Impulse` | `{ event: "localized_change", original_label: "wave_5", new_label: "wave_1_larger", scenario_id_new: "..." }` |
| `Neely Pre-Constructive: monowave[12] labeled :L5 by Rule 5 Cond 5a` | `{ event: "structure_labeled", monowave_index: 12, label: ":L5", rule: "Ch3_PreConstructive{r:5,c:'a'}", neely_page: "Ch3_p3-53" }` |
| `Neely Degree ceiling: max reachable = Minor (data spans 8 years)` | `{ event: "degree_ceiling", max_degree: "Minor", reason: "data_span" }` |

### 12.3 Fact statement 命名衝突防範

Neely Core 與 Traditional Core 都會產出「波浪相關 Fact」,**統一在 statement 開頭加標籤**:

```
✅ "Neely Impulse 5-wave detected with power_rating=StronglyFavorContinuation"
✅ "Traditional(Frost) impulse 5-wave completed, target 700-800"
❌ "Impulse 5-wave detected"   // 不知道是哪派
```

### 12.4 不入 Fact 表的內容

- 每個 Scenario 的完整結構 → 寫 `structural_snapshots`(JSONB)
- monowave 序列 → 寫 `structural_snapshots`
- diagnostics → 寫 `structural_snapshots`

只有「事件型」的事實寫 facts(scenario 出現 / 消失 / 失效、forest 摘要等)。

### 12.5 統一規範引用

- Fact statement 詞彙限制遵循 `cores_overview.md` §6.1.1(禁用主觀詞彙)
- `stock_id` 編碼遵循 `cores_overview.md` §6.2.1(保留字規範)
- Facts 表 unique constraint 與 `params_hash` 演算法遵循 `cores_overview.md` §6.3 / §7.4

---

## 十三、warmup 與 Degree ceiling

[Gap 3.5 = c]

### 13.1 warmup_periods

Neely Core 屬**結構性指標**,每日全量重算。但仍宣告所需歷史資料量:

```rust
fn warmup_periods(&self, params: &NeelyCoreParams) -> usize {
    match params.timeframe {
        Timeframe::Daily => 500,        // ~2 年日線
        Timeframe::Weekly => 250,       // ~5 年週線
        Timeframe::Monthly => 120,      // ~10 年月線
        Timeframe::Quarterly => 60,     // ~15 年季線(2026-05-10 v1.30 加 Quarterly Timeframe)
    }
}
```

實際窗口大小依 P0 Gate 六檔股票實測校準。

### 13.2 為何需要這麼多歷史

Neely 體系判斷 W5 是否完成、ABC 是否在更大 degree 的 W4 中等,都需追溯至更大 degree 的歷史。歷史資料不足會導致大量 candidate 被 reject(實際上是「無法判斷」而非「規則不符」)。

P0 Gate 六檔實測時應記錄「資料不足」的拒絕比例,作為 warmup 校準依據。

### 13.3 Degree ceiling 推導

依資料量自動推導本次分析能達到的最高 Degree:

| 歷史長度(Daily) | max_reachable_degree | 理由 |
|---|---|---|
| < 1 年 | SubMinuette | 資料量僅夠識別最小結構 |
| 1-3 年 | Minute | 中期結構可識別(code 保守取中段而非 Minuette / Minute 並列) |
| 3-10 年 | Minor | 大級別需要更長歷史(code 保守偏小而非 Minor / Intermediate 並列) |
| 10-30 年 | Primary | Cycle 級需歷史 30+ 年 |
| 30-100 年 | Cycle | TAIEX、加權指數歷史夠用 |
| > 100 年 | Supercycle | 罕見資料量(全球主要市場長期指數) |

> **實作對應**:`rust_compute/cores/wave/neely_core/src/degree/mod.rs::classify_degree()`
> 用上表的閾值。spec 原列「1-3 年 → Minuette / Minute」「3-10 年 → Minor / Intermediate」
> 「> 30 年 → Cycle / Supercycle」是並列範圍;code 取**保守值**(較小 Degree)避免
> Aggregation 層誤標為更大級別。2026-05-13 spec amendment 對齊。

`DegreeCeiling.max_reachable_degree` 由 Stage 11 推導(§7.1),寫入 `NeelyCoreOutput`。

**為何重要**:台股大部分個股上市 5-15 年,根本到不了 Cycle 級別。Aggregation 層可據此自動降低顯示的 Degree 標籤,避免誤導。

### 13.4 與 §6.5 固定參考尺度的關係

若 `use_fixed_reference_scale = true`,固定參考尺度的計算窗口即為 `warmup_periods`:在 warmup 區間內計算 `median(|Δclose|)`,之後整個分析窗口都用此單一尺度做 45° 判定與 Neutrality 閾值。比滾動 ATR 更貼近精華版「fixed chart scaling」精神。

---

## 十四、對應資料表

| 用途 | 資料表 |
|---|---|
| 輸入 OHLC | Silver `price_daily_fwd` / `price_weekly_fwd` / `price_monthly_fwd`(由 S1_adjustment Rust binary 產出) |
| 寫入結構快照 | `structural_snapshots`,`core_name = 'neely_core'` |
| 寫入 Fibonacci 投影視圖(可選) | `structural_snapshots`,`core_name = 'fib_zones'` + `derived_from_core = 'neely_core'` |
| 寫入 Fact | `facts`,`source_core = 'neely_core'` |

> Silver `price_*_fwd` 表結構與處理邏輯由 `layered_schema_post_refactor.md` §4.1 定義。Neely Core 不關心 Silver 內部如何產出,只消費結果。

### 14.1 structural_snapshots JSONB 範例

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
        "pattern_type": { "kind": "impulse", "extension": "wave3_extended" },
        "power_rating": "strongly_favor_continuation",
        "rules_passed_count": 18,
        "deferred_rules_count": 1,
        "complexity_level": 3,
        "round_state": "Round2",
        "awaiting_l_label": false,
        "triplexity_detected": false
      }
    ],
    "monowave_count": 245,
    "forest_size": 42,
    "overflow_triggered": false,
    "reverse_logic": {
      "forest_size": 42,
      "threshold": 5,
      "activated": true
    },
    "degree_ceiling": {
      "max_reachable_degree": "Minor",
      "reason": "data spans 8 years"
    },
    "engine_config": {
      "atr_period": 14,
      "use_fixed_reference_scale": false,
      "reverse_logic_threshold": 5
    }
  }
}
```

---


## 十五、診斷與可觀察性

### 15.1 NeelyDiagnostics 完整保留拒絕原因

```rust
pub struct NeelyDiagnostics {
    pub stage_timings: HashMap<String, Duration>,
    pub rejections: Vec<RuleRejection>,
    pub overflow_triggered: bool,
    pub compaction_timeout: bool,
    pub atr_dual_mode_diff: Option<AtrDualModeDiff>,  // P0 Gate 用
    pub peak_memory_mb: f64,
    pub elapsed_ms: u64,
}

pub struct RuleRejection {
    pub candidate_id: String,
    pub rule_id: RuleId,                              // 哪條規則
    pub expected: String,                             // 規則要求
    pub actual: String,                               // 實際情況
    pub gap: f64,                                     // 偏離量
    pub neely_page: String,                           // 書頁追溯
}

/// ATR 雙模交叉驗證結果(P0 Gate 用)
pub struct AtrDualModeDiff {
    pub monowave_count_rolling: usize,
    pub monowave_count_fixed: usize,
    pub forest_size_rolling: usize,
    pub forest_size_fixed: usize,
    pub validator_reject_rate_rolling: f64,
    pub validator_reject_rate_fixed: f64,
}
```

### 15.2 P0 Gate 階段的觀察重點

P0 Gate 六檔股票實測階段必須記錄:

1. **forest_size 分布** — P50 / P95 / P99 / max
2. **compaction_paths 分布** — 同上
3. **elapsed_ms 分布** — 同上
4. **peak_memory_mb 分布** — 同上
5. **overflow_triggered 比例** — 多少檔 / 多少時間點觸發
6. **compaction_timeout 比例** — 多少檔逾時
7. **資料不足拒絕比例** — 校準 warmup_periods
8. **滾動 ATR vs 固定參考尺度的差異** — 同檔同期跑兩 mode 比較(§6.5)
9. **reverse_logic_activated 比例** — 校準 `reverse_logic_threshold` 預設值
10. **degree_ceiling 分布** — 確認 11 級體系自動推導合理

### 15.3 紅燈條件

P0 Gate 結果若出現以下情況之一,**回頭重議 Compaction 演算法**:

- forest_size P95 > 1000
- elapsed_ms P95 > 30 秒
- peak_memory_mb P95 > 1 GB
- overflow_triggered 比例 > 20%

不可繼續加大 `forest_max_size` 或 `BeamSearchFallback.k` 規避(§10.5)。

### 15.4 ATR 雙模紅燈條件

P0 Gate 跑「滾動 ATR」與「固定參考尺度」雙模驗證,若以下任一情況出現,**改採固定參考尺度為預設**:

- monowave 切割數量差異 ≥ 10%
- Validator reject 率差異 ≥ 10%
- Scenario Forest size 差異 ≥ 20%

### 15.5 前端可觀察性串接

`NeelyDiagnostics.overflow_triggered = true` 必須傳到前端,顯示「此股結構過於複雜,系統呈現 Top K 解讀」橫幅。使用者知情,不可隱藏。

`reverse_logic.activated = true` 同樣必須傳到前端,顯示「**多套合理計數共存,市場可能處於更大形態中段**」橫幅(這是 Neely 規則訊號,非工程診斷)。

---

## 十六、P0 Gate 六檔實測

[Gap 4.5 = b]

### 16.1 Gate 範圍

P0 完成後執行六檔股票實測,校準 NeelyEngineConfig 預設值。

**實測標的**:`0050 / 2330 / 3363 / 6547 / 1312 / TAIEX`

| 標的 | 類別 | 涵蓋情境 |
|---|---|---|
| 0050 | ETF / 大型成熟 | 流動性高、波動穩定 |
| 2330 | 半導體龍頭 | 國際性、高關注度 |
| 3363 | 上漲趨勢股 | 強勢推升、Wave 識別關鍵案例 |
| 6547 | 震盪整理股 | 高 forest size 壓力測試 |
| 1312 | 傳產類股 | 低波動、長週期形態 |
| **TAIEX** | 加權指數 | **驗證 `neutral_threshold_taiex` 工程參數** + 長歷史(Cycle 級別) |

### 16.2 實測流程

1. 取每標的完整歷史(自上市 / 指數起算以來)
2. 用「v2.0 模式 = Compaction 不剪枝、`OverflowStrategy::Unbounded`」執行 Neely Core
3. 記錄每檔的 forest_size、compaction_paths 數量、elapsed 秒數、peak memory MB
4. 各時間框架(日 / 週 / 月)分別測試
5. **同時跑 `use_fixed_reference_scale = false` 與 `true` 雙模**(§6.5)
6. 結果寫入 `docs/benchmarks/p0_gate_results.md`

### 16.3 校準產出

依 P95 結果校準:

- `forest_max_size` 預設值
- `compaction_timeout_secs` 預設值
- `BeamSearchFallback.k` 預設值
- `reverse_logic_threshold` 預設值(觀察各檔 reverse_logic_activated 比例)
- `use_fixed_reference_scale` 預設值(依 §15.4 紅燈條件決定)

### 16.4 Gate 通過條件

綠燈(全通過):

- forest_size P95 ≤ 1000
- elapsed_ms P95 ≤ 30 秒
- peak_memory_mb P95 ≤ 1 GB
- 六檔皆能產出至少一個 Scenario(無資料不足)
- ATR 雙模差異各項皆 < §15.4 紅燈門檻(或改採固定參考尺度為預設)
- TAIEX 在月線級別能達到 `degree_ceiling = Cycle` 或更高(驗證 Degree 推導正確性)

紅燈條件見 §15.3 / §15.4。Gate 通過才開始 P1 開發。

---

## 十七、已棄用設計

| 棄用項 | 來源 | 棄用原因 |
|---|---|---|
| Compaction 貪心選最高分 + backtrack | v1.1 | 違反輔助判讀原則 |
| `composite_score` 加總 | v1.1 Item 7 | 主觀加權 |
| `confidence` 機率語意 | v1.1 | 違反 §2.2 不使用 probability |
| Compaction Threshold 0.3 剪枝 | v1.1 | 改用 Neely 規則篩選 |
| 容差 toml 外部化 | 早期討論 | 誘導偏離原作 |
| Compaction 完全不剪枝 | r2 §7.2 | 工程現實下 OOM,改為「窮舉但有護欄」 |
| `[TW-MARKET]` Scorer 微調 | v1.1 Item 7.4 | 主觀加權,違反「忠於原作」 |
| `Trigger.on_trigger.ReduceProbability` | r2 | 與 §2.2 不使用 probability 矛盾,改 `WeakenScenario` |
| `neely_power_rating: i8` | r2 | 改 `enum PowerRating`,避免 99 等無效值 |
| Engine_T + Engine_N 整合公式 | v1.1 Item 17.5 | 主觀調參,Traditional Core 改為獨立並列 |
| 自動校準 atr_period | (討論) | 校準準則皆主觀,違反 §1.3 |
| TW-Market Core 嵌在 Cores 層 | v2.0 r1 | r2 廢除:屬複雜計算,歸 Silver S1 |
| `pattern_type::Diagonal` | r2 | 非 Neely 用語(Prechter 派),改 `TerminalImpulse` |
| `Flat { Regular/Expanded/Running }` | r2 | 非 Neely 用語,改用 Neely Ch10 Power Ratings 表的具名變體 |
| `Triangle { Contracting/Expanding/Limiting }` 平行列舉 | r2 | Limiting 是 Contracting/Expanding 的子分類,改 2 層 enum |
| `Zigzag { Single/Double/Triple }` | r2 | 概念混淆,單 Zigzag 改 `length` 變體,Double/Triple 為獨立 top-level |
| `PowerRating::Bullish/Bearish` | r2 | 方向相依語意丟失,改 `FavorContinuation/AgainstContinuation` |
| 全域 `FIB_TOLERANCE_RELATIVE = 0.04` | r2 | r3 一度刪除,**r5 撤回該刪除**(Gap 2 確認精華版翻譯表有 OCR 依據,應採用 ±4% 為 Fibonacci 比率容差) |
| `WATERFALL_TOLERANCE = 0.05` | r2 | r3 已澄清:Waterfall Effect 是行為模式(Ch12),Triangle 三腿 5% 是另一概念(Ch11),拆開 |
| FIB_RATIOS 含 0.786 / 1.272 | r2 | r3 已移除(精華版零次出現,屬諧波交易派);補入 1.382 |
| **r4 自編 RuleId 系統**(R1-R7 / AL1-AL5 / C1-C5 / A1-A6 等) | r4 | **r5 改用 Neely 章節編碼**,RuleId 本身即書頁追溯,免維護對應表 |
| **r4 spec 內重述 Neely 規則細節**(§4.5-4.8 / §10.5-10.11 / §13.4 / §14.8-14.9 / 附錄 C-F) | r4 | **r5 改三層架構**,規則細節全部移至 `neely_rules.md`(精華版同構) |
| **r4 「ATR 不是 Neely 方法」批判性論述** | r4 §6.5.1 | **r5 撤回**(與 §4 容差立場一致):ATR 視為 Rule of Proportion 視覺判定的合理量化代理 |
| **r4 `FlatVariant { StrongBWave, WeakBWave }` 概念性變體** | r4 §9.1 | **r5 修正**:這兩個是 b 強度分類軸,非具體形態變體 |

---

## 十八、與其他 Core 的關係

### 18.1 上游

- **Silver 層 S1_adjustment**(資料層,非 Core)— Neely Core 直讀 Silver `price_*_fwd` 表
- 所有資料前處理(後復權、漲跌停合併、加權指數開盤前失真處理)由 S1 Rust binary 完成
- Neely Core 不知道 Silver 內部如何處理,只消費結果

> 歷史備註:v1.x / v2.0 r1 曾規劃 `TW-Market Core` 作為 Neely Core 上游,r2 後此 Core 廢除。詳見 `adr/0001_tw_market_handling.md`。

### 18.2 下游(Aggregation Layer)

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
使用者(或 LLM agent)自己連線
```

### 18.3 唯一允許的耦合:trendline_core

`trendline_core`(P2)是**全 Core 系統中唯一允許消費 Neely Core 輸出的例外**:

- 僅讀 `NeelyCoreOutput.monowave_series`(不讀 `scenario_forest`)
- `trendline_core/Cargo.toml` 明確宣告 `depends_on = ["neely_core"]`
- 在 V2 spec 列入「已知耦合」清單

詳細管控規則見 `cores_overview.md` §12 與 `indicator_cores_pattern.md` §5。

### 18.4 與 Traditional Core 的關係

- Neely Core 與 Traditional Core **獨立並列**,**不整合**
- 兩者讀同一份 Silver `price_*_fwd`(已由 S1_adjustment 後復權與漲跌停合併)
- 兩者各自輸出 Forest,Aggregation Layer 並排呈現
- v1.1 的 Combined confidence 整合公式已棄用

詳見 `traditional_core.md` §8。

### 18.5 ATR 的雙重身份

Neely Core 內嵌 ATR 計算(`NeelyEngineConfig.atr_period = 14`),用於 Rule of Proportion / Neutrality / 45° 判定的**工程量化代理**(§6.5)。

`atr_core` 是獨立 Indicator Core,對外輸出 ATR 值與相關 Fact。

**兩者不互相 import**:

- Neely Core 不依賴 `atr_core`(計算邏輯內嵌)
- `atr_core` 不依賴 Neely Core(對外服務)
- 兩者數值相同但實作獨立,維持零耦合

詳見 `indicator_cores_volatility.md` §2.4。

---

## Appendix A:精華版引用對照表

> 本對照表協助 P0 開發者快速從 Pipeline 階段定位到精華版章節與 Rust 實作檔案。

### A.1 Pipeline 階段 → 精華版章節 → Rust 模組

| Stage | 動作 | 精華版章節 | Rust 模組 |
|---|---|---|---|
| 1 | Monowave Detection | Ch3「Monowave 的辨識」+「Rule of Neutrality」 | `monowave/pure_close.rs` + `monowave/neutrality.rs` |
| 2 | Rule of Proportion 標註 | Ch3「Rule of Proportion」(Directional/NonDirectional) | `monowave/proportion.rs` |
| 0 | Pre-Constructive Logic | Ch3「Pre-Constructive Rules of Logic」(Rule 1-7) | `structure_labeler/{retracement_rules,conditions,categories,decision_tree}.rs` |
| 3 | Candidate Generator + Similarity & Balance | Ch4「Monowave Groups → Polywaves」+「Similarity & Balance」 | `candidates/{generator,similarity_balance}.rs` |
| 3.5 | Pattern Isolation + Zigzag DETOUR | Ch3「Pattern Isolation Procedures」+ Ch4「DETOUR Test」 | `pattern_isolation/mod.rs` + `candidates/zigzag_detour.rs` |
| 4 | Validator | Ch5 R1-R7 + Flat/Zigzag/Triangle 變體規則 + Overlap/Equality/Alternation | `validator/{core_rules,flat_rules,zigzag_rules,triangle_rules,overlap,equality,alternation}.rs` |
| 5 | Classifier | Ch5「Realistic Representations」+ Ch8 Complex + Ch10 變體 | `classifier/{flat,triangle,combination,realistic_repr}.rs` |
| 6 | Post-Constructive Validator | Ch6 W1-W2 四形態 × Stage 1/2 | `validator/wave_rules.rs` |
| 7 | Complexity + Triplexity | Ch7 Complexity Rule + Triplexity | `complexity/mod.rs` |
| 7.5 | Channeling + Advanced Rules | Ch5 + Ch12 Channeling + Ch9 A1-A6 | `channeling/{impulse,correction,triangle}.rs` + `advanced_rules/{trendline_touchpoints,time_rule,independent,simultaneous,exception,structure_integrity}.rs` |
| 8 | Compaction Three Rounds | Ch4 Three Rounds + Ch7 Compaction | `compaction/{exhaustive,three_rounds,beam_search}.rs` |
| 9 | Missing Wave + Emulation | Ch12「Missing Waves」+「Emulation」 | `missing_wave/mod.rs` + `emulation/mod.rs` |
| 10 | Power Rating + Fib + Triggers | Ch10 Power Ratings + Ch12 Fibonacci Internal/External + Waterfall | `power_rating/{table,max_retracement}.rs` + `fibonacci/{ratios,projection,internal_external,waterfall}.rs` + `triggers/mod.rs` |
| 10.5 | Reverse Logic | Ch12「Reverse Logic Rule」 | `reverse_logic/mod.rs` |
| 11 | Degree Ceiling | Ch7 Degree 11 級體系 | `degree/mod.rs` |
| 12 | cross_timeframe_hints | (r5 新增,非精華版規則) | `output.rs` |

### A.2 RuleId enum → 精華版章節 對應

完整 RuleId enum 列於 §9.3,每個 variant 對應精華版章節由 enum 命名直接表達:

- `Ch3_PreConstructive { rule: 5, condition: 'b', .. }` → 精華版 Ch3 Rule 5 Cond 5b
- `Ch5_Essential(3)` → 精華版 Ch5 R3(Essential Construction Rules 第 3 條)
- `Ch9_Exception_Aspect1 { situation: TriangleEntryExit }` → 精華版 Ch9 Exception Rule Aspect 1 Situation C

### A.3 跨 Rule 共通函數(r5 新增)

> 來源:Gap 5 核實項 E 發現可收斂為跨 Rule meta-rule。

| 共通函數 | 適用 RuleId | 精華版來源 |
|---|---|---|
| `is_fifth_of_fifth_extension(context)` | `Ch3_PreConstructive { rule: 3, condition: 'a', .. }`, `Ch3_PreConstructive { rule: 4, condition: 'b', .. }`, 其他「m1 may be 5th wave of 5th Extension」 | 精華版 Ch3 [:L5] add 共通條件 |

實作位置:`structure_labeler/fifth_of_fifth_detector.rs`,被多個 Rule/Condition 引用。

---

## Appendix B:Gap 5 核實後續工作清單

> Gap 5 已完成原書二次核實(共 6 項),結果見「Gap5_原書核實決議.md」。本附錄為 r5 完成後**立即**執行的精華版補強清單。

### B.1 必補(r5 完成後立即執行)

| 項 | 主題 | 動作 |
|---|---|---|
| **A** | Strong b-wave 分流邏輯 | 精華版 Ch5「Strong B-wave」段補入:① 123.6% 中間檻(c 完整回測機率轉折點)② c-wave vs 161.8%×a 對 Irregular / Elongated Flat 的二分 |
| **E** | PCRL [:L5] add 跨 Rule 分布 | 精華版 Rule 4b 條目補入 [:L5] add 規則 + 提取共通函數 `is_fifth_of_fifth_extension(context)`(實作 `structure_labeler/fifth_of_fifth_detector.rs`) |
| **F** | Zigzag c 在 Triangle 內例外 | 精華版 Ch11 Wave-c (Zigzag) 改寫為:「Zigzag 在 Triangle(僅 1-2 個更高級)內,c 可超出 61.8%-161.8% 區間(任一方向),不強制。若 c 超出區間,反為 Triangle 形成的強烈訊號。」 |

### B.2 不需改動(已對齊原書)

| 項 | 主題 | 結論 |
|---|---|---|
| **C** | Triangle wave-e ≥ 38.2% 例外 | 精華版已對齊原書 `excluding wave-e`,無需修改 |
| **D** | Wave-2 不可為 Running Correction | 精華版正確(hard fail 強度);P0 開發時補入副規則(若見 Zigzag,優先重新標為 wave-a of larger Flat,soft warning) |

### B.3 doc-comment 標註(r5 開發期間執行)

| 項 | 主題 | doc-comment 內容 |
|---|---|---|
| **B** | Exception Rule 量化 | 各 Rust 規則對應「失靈幅度」常數必標 `[r4/精華版詮釋,原書無量化依據]`,例:`const OVERLAP_EXCEPTION_RATIO: f64 = 0.10; // [r4/精華版詮釋,原書無量化依據]` |

### B.4 P0 開發中執行(若需要)

- 精華版若有其他未察覺的不完整處,在 P0 編碼遭遇時即時補核實
- 任何新發現的「精華版偏離原書」記入 `docs/neely_rules_discrepancies.md`

---

> 本規格 r5 修訂完成後,後續變動以 r6+ commit 訊息明確標註,並附精華版章節 + 原書頁追溯。
> **任何規則的調整都須走 PR review,且需提供「對齊精華版章節 + 必要時對齊原書原文段落」的引用證據**。
