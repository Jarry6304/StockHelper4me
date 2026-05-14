// NeelyCoreOutput + Scenario Forest + Diagnostics
// 對齊 m3Spec/neely_core_architecture.md r5(2026-05-13)+ m3Spec/neely_rules.md
//
// 設計原則:
//   - **Forest 不選 primary**(architecture §8.2 / §9.x):`scenario_forest: Vec<Scenario>`,
//     順序不反映優先級,Aggregation Layer 可依 power_rating 提供 UI 篩選
//   - **不引入機率語意**(architecture §2.1):移除 v1.1 `confidence` / `composite_score` 欄位
//   - **Trigger 不寫 ReduceProbability**(architecture §9.4):改 `WeakenScenario`
//   - **PowerRating enum**(architecture §9.4):取代 v1.1 `i8`
//   - **RuleId 用 Neely 章節編碼**(architecture §9.3):取代 r4 自編號 Core/Flat/Zigzag/Triangle/Wave(u8)
//     — Phase 1 PR 只宣告 Phase 1 用得到的 variants(Ch5_Essential / Ch5_Overlap_* /
//     Ch5_Flat_* / Ch5_Zigzag_* / Ch5_Triangle_* / Ch5_Equality / Ch5_Alternation /
//     Engineering_*),Ch3 / Ch4 / Ch6-Ch12 / Ch11 規則 留後續 PR 補

use chrono::NaiveDate;
use fact_schema::Timeframe;
use serde::Serialize;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Input(§五)
// ---------------------------------------------------------------------------

/// 後復權 OHLC 序列。Silver `price_*_fwd` 表已處理漲跌停合併與後復權。
/// Volume 為選填,Volume Alignment 子規則(§9.1 `volume_alignment`)需要時用。
#[derive(Debug, Clone, Serialize)]
pub struct OhlcvSeries {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub bars: Vec<OhlcvBar>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OhlcvBar {
    pub date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimeRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

// ---------------------------------------------------------------------------
// Output 主結構(§八)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct NeelyCoreOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub data_range: TimeRange,

    /// Forest,**不**附 `primary` 欄位(§9.3)
    pub scenario_forest: Vec<Scenario>,

    /// monowave 序列 — 對外暴露給 trendline_core 唯一例外消費(§8.1)
    pub monowave_series: Vec<Monowave>,

    pub diagnostics: NeelyDiagnostics,

    pub rule_book_references: Vec<RuleReference>,

    /// 資料充分性(歷史不足會導致大量 candidate 被 reject,實際是「無法判斷」)
    pub insufficient_data: bool,

    /// Phase 12.5 新增:Compaction 階段是否觸發 timeout(架構 §8.1「失敗旗標」對稱 insufficient_data)。
    ///
    /// 與 `NeelyDiagnostics.compaction_timeout` 雙寫 — 對齊 spec §8.1 把兩個失敗旗標
    /// 都放 Output 頂層,但保留 Diagnostics 內版本以維持向下相容。
    pub compaction_timeout: bool,

    /// Stage 3.5 Pattern Isolation 識別出的形態邊界(Phase 3 PR)。
    /// 對齊 m3Spec/neely_rules.md §Pattern Isolation Procedures。
    pub pattern_bounds: Vec<PatternBound>,

    /// Stage 3.5 Zigzag DETOUR Test annotation(Phase 3 PR)。
    /// 對齊 m3Spec/neely_rules.md §Zigzag DETOUR Test。
    pub detour_annotations: Vec<DetourAnnotation>,

    /// Phase 8 新增:Three Rounds Round 3 暫停資訊(architecture §8.4)。
    /// 圖中無任何 :L5/:L3 base label → 進入 Round 3 暫停;否則 None。
    pub round3_pause: Option<Round3PauseInfo>,

    /// Phase 9 新增:Stage 9a Missing Wave 偵測結果(spec Ch12 + 1054-1057 missing wave 標記慣例)。
    pub missing_wave_suspects: Vec<MissingWaveSuspect>,

    /// Phase 9 新增:Stage 9b Emulation 偵測結果(spec Ch12 Emulation)。
    pub emulation_suspects: Vec<EmulationSuspect>,

    /// Phase 10 新增:Stage 10.5 Reverse Logic 觀察(spec §Expansion of Possibilities)。
    /// 同一資料序列存在多套合法計數時觸發 — 市場處於某更大形態的中段。
    pub reverse_logic_observation: Option<ReverseLogicObservation>,

    /// Phase 12 新增:Stage 11 Degree Ceiling 推導(architecture §8.5 / §13.3)。
    /// 依資料量自動推導本次分析能達到的最高 Degree。
    pub degree_ceiling: DegreeCeiling,

    /// Phase 12 新增:Stage 12 cross_timeframe_hints(architecture §8.6 / §3.4)。
    /// 各 monowave 的 Structure Label + 價格區間摘要,供 Aggregation Layer 跨 Timeframe 比對。
    pub cross_timeframe_hints: CrossTimeframeHints,
}

/// Three Rounds Round 3 暫停資訊(neely_core_architecture.md §8.4)。
#[derive(Debug, Clone, Serialize)]
pub struct Round3PauseInfo {
    /// Round 3 觸發原因說明
    pub reason: String,
    /// 受影響的 scenario id 數量(全 forest 都標 awaiting_l_label)
    pub affected_scenario_count: usize,
}

// ---------------------------------------------------------------------------
// Phase 9:Missing Wave 偵測(Ch12 + Ch3 missing-wave 標記慣例)
// 對齊 m3Spec/neely_rules.md §Pre-Constructive Logic 細部技術備註(1054-1057 行)
// ---------------------------------------------------------------------------

/// Missing Wave 位置分類(spec 1055-1057 行)。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum MissingWavePosition {
    /// m1 中心 missing wave:左側標 `:5?`、右側標 `x:c3?` 或 `b:F3?`
    M1Center,
    /// m0 中心 missing wave:左側標 `:s5?`、右側標 `x:c3?`
    M0Center,
    /// m2 終點 missing wave:m2 終點標 `x:c3?`
    M2Endpoint,
    /// 模糊位置(bundle 標記成組,具體位置由 Aggregation Layer 判斷)
    Ambiguous,
}

/// Missing Wave Suspect — 從 P2 已標的 MissingWaveBundle certainty 萃取的結構化資訊。
///
/// 對齊 spec 1054-1057 行「missing wave 標記慣例 — 標記成組捆綁」。
#[derive(Debug, Clone, Serialize)]
pub struct MissingWaveSuspect {
    /// 對應的 monowave index(P2 標 bundle 的 monowave)
    pub monowave_idx: usize,
    /// missing wave 位置(M1Center / M0Center / M2Endpoint / Ambiguous)
    pub position: MissingWavePosition,
    /// bundle 中所有 missing-wave 標記 labels(必須成組保留)
    pub bundle_labels: Vec<StructureLabel>,
    /// 機械式陳述(對齊 cores_overview §6.1.1 禁主觀詞彙)
    pub message: String,
}

// ---------------------------------------------------------------------------
// Phase 9:Emulation 偵測(Ch12 Emulation)
// 對齊 m3Spec/neely_rules.md §Ch8 Non-Standard Polywaves(1902-1906 行 Running 變體辨識要點)
// ---------------------------------------------------------------------------

/// Emulation 類型 — 視覺上相似但實際結構不同的 pattern 偽裝。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum EmulationKind {
    /// Running Double Three Combination 偽裝 1st Wave Extension Impulse
    /// spec 1905-1906:thrust > 該段「wave-3」+「wave-3 為 :3」可辨識
    RunningDoubleThreeAsImpulse,
    /// Diagonal (Terminal Impulse) 偽裝 Trending Impulse
    DiagonalAsImpulse,
    /// Triangle 偽裝 5-wave Failure pattern
    TriangleAsFailure,
    /// Trending Impulse with 1st Extension 偽裝 Terminal Impulse(wave-2/4 overlap 灰色地帶)
    FirstExtAsTerminal,
    /// 通用 — 視覺上 X 但結構為 Y
    Generic,
}

/// Emulation Suspect — 形態偽裝候選。
#[derive(Debug, Clone, Serialize)]
pub struct EmulationSuspect {
    /// 對應的 Scenario id(若可定位)
    pub scenario_id: Option<String>,
    pub kind: EmulationKind,
    /// 機械式陳述(對齊 cores_overview §6.1.1 禁主觀詞彙)
    pub message: String,
}

// ---------------------------------------------------------------------------
// Phase 10:Stage 10.5 Reverse Logic 觀察
// 對齊 m3Spec/neely_rules.md §Expansion of Possibilities — Reverse Logic Rule
//       (2598-2608 行 — Neely Extension)
// ---------------------------------------------------------------------------

/// Reverse Logic Observation — 多套合法計數場景的觀察輸出。
///
/// 對齊 spec 2599 行核心定理:
///   「同一資料序列存在多個完美合法的計數時,市場必定處於某個修正/衝動形態的中央
///   (b 的 b、3 的 3、或 Non-Standard 複雜修正的 x)。可能性越多,越靠近中央。」
///
/// 操作意涵(spec 2601-2604 行):
///   - 觀察到多套合理計數 → 自動剔除「形態即將完成」的選項,只保留「市場處於中段」的解讀
///   - 若尚未進場 → 等到可能性收斂為一再進場
///   - 若已持倉獲利 → 多套計數出現不代表頂底,而是還有路要走
#[derive(Debug, Clone, Serialize)]
pub struct ReverseLogicObservation {
    /// 同一資料上有效 scenario 的數量(forest.len())
    pub scenario_count: usize,
    /// 是否觸發 Reverse Logic 操作意涵(scenario_count >= 閾值)
    pub triggered: bool,
    /// 機械式陳述(對齊 cores_overview §6.1.1 禁主觀詞彙)
    pub message: String,
    /// 建議過濾掉的「形態即將完成」scenario id 列表
    /// (spec 2602 行「自動剔除『形態即將完成』的選項」操作意涵)
    pub suggested_filter_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// Phase 12:Degree Ceiling(architecture §8.5 / §13.3)
// 對齊 m3Spec/neely_core_architecture.md §13.3 Degree ceiling 推導表
// ---------------------------------------------------------------------------

/// Neely 11 級 Degree 體系(architecture §8.5)。
///
/// 對齊精華版 Ch7 11 級體系,由小至大列序。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum Degree {
    SubMicro,
    Micro,
    SubMinuette,
    Minuette,
    Minute,
    Minor,
    Intermediate,
    Primary,
    Cycle,
    Supercycle,
    GrandSupercycle,
}

/// Stage 11 Degree Ceiling — 依資料量推導本次分析能達到的最高 Degree。
///
/// 對齊 architecture §8.5 + §13.3:
///   - < 1 年 → SubMinuette
///   - 1-3 年 → Minuette / Minute
///   - 3-10 年 → Minor / Intermediate
///   - 10-30 年 → Primary
///   - > 30 年 → Cycle / Supercycle
///
/// **為何重要**(spec §8.5):台股大部分個股上市 5-15 年,根本到不了 Cycle 級別。
/// Aggregation 層可據此自動降低顯示的 Degree 標籤,避免「短歷史股票被標為 Supercycle」誤導。
#[derive(Debug, Clone, Serialize)]
pub struct DegreeCeiling {
    /// 本次分析依資料量可達到的最高 Degree
    pub max_reachable_degree: Degree,
    /// 推導原因說明(例:"data spans 8 years, daily timeframe reaches Minor at best")
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Phase 12:CrossTimeframeHints(architecture §8.6 / §3.4)
// ---------------------------------------------------------------------------

/// Stage 12 cross_timeframe_hints — 各 monowave 摘要,供 Aggregation Layer 跨 Timeframe 比對。
///
/// 對齊 architecture §8.6:Neely Core 本來就有完整 monowave 資訊,直接輸出比 Aggregation
/// 重新解析 `structural_snapshots` 高效。
#[derive(Debug, Clone, Serialize)]
pub struct CrossTimeframeHints {
    /// 本次分析的 Timeframe
    pub timeframe: Timeframe,
    /// 各 monowave 的摘要
    pub monowave_summaries: Vec<MonowaveSummary>,
}

/// 單一 monowave 的跨 Timeframe 摘要(architecture §8.6)。
#[derive(Debug, Clone, Serialize)]
pub struct MonowaveSummary {
    /// 對應 classified_monowaves 的 index
    pub monowave_index: usize,
    /// 起訖日期
    pub date_range: (NaiveDate, NaiveDate),
    /// Structure Label 候選清單(從 P2 Pre-Constructive 標的 candidates)
    /// 例:[":L5", ":F3"]
    pub structure_label_candidates: Vec<String>,
    /// 起訖價格區間(start_price, end_price)
    pub price_range: (f64, f64),
}

// ---------------------------------------------------------------------------
// Diagnostics(§八)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize)]
pub struct NeelyDiagnostics {
    pub monowave_count: usize,
    pub candidate_count: usize,
    pub validator_pass_count: usize,
    pub validator_reject_count: usize,
    /// 完整保留拒絕原因(§18.1):rule_id / expected / actual / gap / neely_page
    pub rejections: Vec<RuleRejection>,
    pub forest_size: usize,
    /// 所有合法壓縮路徑
    pub compaction_paths: usize,
    /// Forest 是否爆量觸發 BeamSearchFallback
    pub overflow_triggered: bool,
    /// Compaction 是否逾時
    pub compaction_timeout: bool,
    /// 各階段耗時(Stage 0-12,**微秒精度**)。
    ///
    /// **2026-05-14 P1 fix**:從 ms → μs(Duration::as_micros)。原 ms 精度對 sub-ms
    /// stages 截斷成 0(P0 Gate §4 §stage_elapsed 全 0 揭露);改 μs 提供 1000× 精度。
    /// SQL caller 用 `(snapshot->'diagnostics'->'stage_elapsed_us'->>'stage_X')::int` 取值。
    pub stage_elapsed_us: HashMap<String, u64>,
    pub elapsed_ms: u64,
    /// 峰值記憶體(P0 Gate 校準用)
    pub peak_memory_mb: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleRejection {
    pub candidate_id: String,
    pub rule_id: RuleId,
    pub expected: String,
    pub actual: String,
    /// 偏離量(百分比或絕對值,依規則而定)
    pub gap: f64,
    /// Neely 書頁追溯,例 "p.123"
    pub neely_page: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleReference {
    pub rule_id: RuleId,
    pub neely_page: String,
    pub description: String,
}

// ---------------------------------------------------------------------------
// Scenario / Wave Tree(§九)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Scenario {
    pub id: String,

    pub wave_tree: WaveNode,
    pub pattern_type: NeelyPatternType,
    /// candidate 起始 monowave direction(Phase 5 PR 新增,供 Power Rating 判 Bullish/Bearish 符號)
    pub initial_direction: MonowaveDirection,
    /// Phase 6 新增:Ch7 Compaction Reassessment 後的 base structure label。
    ///
    /// 對齊 m3Spec/neely_rules.md §Compaction 表(1803-1811 行):
    ///   - Trending Impulse / Terminal Impulse → `:5`
    ///   - Zigzag / Flat / Triangle / 含 x-wave 形態 → `:3`
    ///
    /// 供更高級的 Compaction Round 1 重新評估該 scenario 在更大序列裡的角色。
    pub compacted_base_label: StructureLabel,
    /// 例:"5-3-5 Zigzag in W4 of larger Impulse"
    pub structure_label: String,
    pub complexity_level: ComplexityLevel,

    /// r3 修正:由 i8 改 enum,避免 power_rating = 99 等無效值(§9.4)
    pub power_rating: PowerRating,
    /// Power Rating 對應的最大回測比例(精華版 Ch10 表 2016-2022 行)。
    ///
    /// 對齊 m3Spec/neely_core_architecture.md §9.1:`max_retracement: Option<f64>`
    /// - `None` = 任意回測(Neutral / 或 Triangle/Terminal 內部覆蓋)
    /// - `Some(0.90)` = ±1 級 ≤ 90%
    /// - `Some(0.80)` = ±2 級 ≤ 80%
    /// - `Some(0.65)` = ±3 級 ≤ 60–70%(取中值)
    ///
    /// 由 `power_rating::max_retracement::lookup` 依 PowerRating + in_triangle_context 查表。
    pub max_retracement: Option<f64>,
    pub post_pattern_behavior: PostBehavior,

    /// 客觀計數(取代 v1.1 主觀分數)
    pub passed_rules: Vec<RuleId>,
    pub deferred_rules: Vec<RuleId>,
    pub rules_passed_count: usize,
    pub deferred_rules_count: usize,

    /// 失效條件(Neely 規則的逆向轉譯)
    pub invalidation_triggers: Vec<Trigger>,

    /// Fibonacci 投影區
    pub expected_fib_zones: Vec<FibZone>,

    /// 結構性事實 7 維(Item 7 拆解,不加總)
    pub structural_facts: StructuralFacts,

    /// Phase 7 新增:Stage 7.5 Channeling + Ch9 Advanced Rules 的諮詢性發現。
    /// 對齊 m3Spec/neely_rules.md §Ch5 Channeling + §Ch9 Advanced Rules。
    /// 諮詢性 — 不直接影響 pattern_complete,提供下游 Aggregation Layer 使用。
    pub advisory_findings: Vec<AdvisoryFinding>,

    /// Phase 8 新增:Three Rounds nested parent context — 該 scenario 範圍是否
    /// 完全涵蓋於某 Triangle scenario 內。對齊 m3Spec/neely_rules.md §Ch10
    /// (2021 行「三角形/Terminal 的內部段不傳遞 Power 暗示」例外)。
    /// 由 Stage 8 Compaction 後處理階段填寫;true → Power Rating 套 in_triangle = 0。
    pub in_triangle_context: bool,

    /// Phase 8 新增:Three Rounds Round 3 暫停標記 — 該 scenario 是否在
    /// 「圖中無任何新 :L5/:L3」狀態下標 awaiting。對齊 m3Spec/neely_rules.md
    /// §Round 3(1258-1265 行)+ neely_core_architecture.md §8.4 Round3PauseInfo。
    /// 策略含意:持有原方向,維持原計數,直到新形態具備收尾條件。
    pub awaiting_l_label: bool,

    // ── 群 2 spec §9.1 line 858-863 ─────────────────────────────────────
    //
    // 由 Phase 15 落地。皆「pipeline 已算但未寫進 Scenario」狀態,
    // 由 classifier::classify 階段填寫(對齊 spec §9.1 群 2「Pre-Constructive +
    // Three Rounds 狀態」分組)。

    /// 每 wave-monowave 的 Pre-Constructive Structure Label candidates(spec line 859)。
    /// 1:1 對應 candidate.monowave_indices 順序;從 ClassifiedMonowave.structure_label_candidates wrap。
    pub monowave_structure_labels: Vec<MonowaveStructureLabels>,

    /// Three Rounds 狀態(spec line 860 / §Ch4)— 該 scenario 在 Round 1/2/3 的位置。
    /// 由 Stage 8 Three Rounds 階段 + awaiting_l_label 推導。
    pub round_state: RoundState,

    /// Pattern Isolation 6-step procedure 識別出的 anchors(spec line 862)。
    /// 從 NeelyCoreOutput.pattern_bounds(Stage 3.5)wrap;只篩涵蓋 scenario 範圍的 anchors。
    pub pattern_isolation_anchors: Vec<PatternIsolationAnchor>,

    /// Triplexity 偵測(spec line 863)— Ch8 Triple-grouping(Triple Zigzag / Triple Combination /
    /// Triple Three / Triple Three Combination / Triple Three Running)。
    /// 由 pattern_type 直接推導。
    pub triplexity_detected: bool,
}

/// Phase 15 新增:每 monowave 的 Pre-Constructive structure label set。
///
/// 對齊 m3Spec/neely_core_architecture.md §9.1 line 859。
/// 包裝 `ClassifiedMonowave.structure_label_candidates`,加上 monowave 在 Scenario 內的位置 index。
#[derive(Debug, Clone, Serialize)]
pub struct MonowaveStructureLabels {
    /// 該 monowave 在 scenario candidate 內的索引(0..wave_count)
    pub monowave_index: usize,
    /// Pre-Constructive Logic 給出的 candidate labels(可有多個,不互斥)
    pub labels: Vec<StructureLabelCandidate>,
}

/// Phase 15 新增:Three Rounds 狀態。
///
/// 對齊 m3Spec/neely_rules.md §Ch4(1200-1290 行):
///   - Round 1:初始 Series 識別(從 Pre-Constructive Logic 標的 monowaves 找出 Standard/Non-Standard)
///   - Round 2:Compaction 後重評估(`compacted_base_label` 已填,base label 已 reassign 為 :5 / :3)
///   - Round 3Pause:圖中無新 :L5/:L3,等候(awaiting_l_label = true)
///
/// 由 classifier 階段依 `compacted_base_label` + `awaiting_l_label` 推導:
///   - awaiting_l_label = true → Round3Pause
///   - compacted_base_label 已 reassign(非預設)+ awaiting = false → Round2
///   - 否則 → Round1
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum RoundState {
    Round1,
    Round2,
    Round3Pause,
}

/// Phase 15 新增:Pattern Isolation anchor(spec line 862)。
///
/// 包裝 `PatternBound` 並標 anchor 起點/終點意圖(start_label / end_label),
/// 供 Aggregation Layer 「依 Pre-Constructive Logic 標籤定位 scenario 在 higher-degree 中的位置」。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PatternIsolationAnchor {
    /// 起點 monowave index(inclusive)
    pub start_idx: usize,
    /// 終點 monowave index(inclusive)
    pub end_idx: usize,
    /// 起點 anchor 標籤(F3/XC3/L3/S5/L5 之一)
    pub start_label: StructureLabel,
    /// 終點 anchor 標籤(L5/L3 之一)
    pub end_label: StructureLabel,
    /// Compaction(Ch7)驗證為合法 Elliott 形態(Step 5)
    pub validated: bool,
}

/// Phase 7 — Stage 7.5 諮詢性發現(Channeling / Ch9 Advanced Rules)。
#[derive(Debug, Clone, Serialize)]
pub struct AdvisoryFinding {
    /// 對應的 RuleId(Ch5_Channeling_* / Ch9_*)
    pub rule_id: RuleId,
    /// 嚴重度(Info / Warning / Strong — 對應 spec 「應」 vs 「必」 vs 「絕不」)
    pub severity: AdvisorySeverity,
    /// 人類可讀訊息(機械式陳述,對齊 cores_overview §6.1.1 禁主觀詞彙)
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum AdvisorySeverity {
    /// 資訊性(channeling 標註,無強制力)
    Info,
    /// 警告(spec 「應」或「常」描述,違反屬常見變體)
    Warning,
    /// 強烈警示(spec 「必」、「絕不」,違反通常表示計數錯誤)
    Strong,
}

/// Wave Tree(階層化波浪結構)。具體欄位於後續 PR 補完。
#[derive(Debug, Clone, Serialize)]
pub struct WaveNode {
    pub label: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub children: Vec<WaveNode>,
}

/// Monowave — Neely Core 對外暴露的 raw 結構(§8.1)。
/// 細節欄位於 monowave/ sub-module 實作時補完。
#[derive(Debug, Clone, Serialize)]
pub struct Monowave {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub start_price: f64,
    pub end_price: f64,
    pub direction: MonowaveDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MonowaveDirection {
    Up,
    Down,
    Neutral,
}

// ---------------------------------------------------------------------------
// PowerRating(§9.1)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize)]
pub enum PowerRating {
    StrongBullish,    // +3
    Bullish,          // +2
    SlightBullish,    // +1
    Neutral,          // 0
    SlightBearish,    // -1
    Bearish,          // -2
    StrongBearish,    // -3
}

// ---------------------------------------------------------------------------
// NeelyPatternType + 子型號(§9.1)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub enum NeelyPatternType {
    Impulse,
    Diagonal { sub_kind: DiagonalKind },
    Zigzag { sub_kind: ZigzagKind },
    Flat { sub_kind: FlatKind },
    Triangle { sub_kind: TriangleKind },
    Combination { sub_kinds: Vec<CombinationKind> },
    /// Phase 16 新增:Running Correction 從 Flat sub_kind 上提為頂層 NeelyPatternType。
    ///
    /// 對齊 m3Spec/neely_core_architecture.md §9.1 r5 line 1161:
    /// 「RunningCorrection 屬 Power Rating -3/+3 級別獨立,不再放 FlatKind::Running」。
    /// 對齊 m3Spec/neely_rules.md line 2024-2037:
    /// 「Running Correction:後續必為延伸 Impulse 或 Flat/Zigzag 的延伸 c-wave;
    ///   後續 Impulse 多 > 161.8%(常達 261.8%)」— ±3 strongly favor continuation
    RunningCorrection,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum DiagonalKind {
    Leading,
    Ending,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum ZigzagKind {
    Single,
    Double,
    Triple,
}

/// FlatKind 7-variant(spec r5 line 1161 + neely_rules.md line 2157-2239)。
///
/// Phase 16(2026-05-14)重構:由 r4 3-variant(Regular/Expanded/Running)→ r5 7-variant
/// 具體形態變體 + Running 上提為 NeelyPatternType::RunningCorrection。
///
/// 判定規則(spec line 2157-2239 + 2024-2034):
/// | Variant          | b/a 範圍          | c 行為           | Power |
/// |------------------|-------------------|------------------|-------|
/// | Common           | 81%–100%          | c ≥ 100% × b     | 0     |
/// | BFailure         | 61.8%–81%         | c ≥ 100% × b     | 0     |
/// | CFailure         | 81%–100%          | c < 100% × b     | -1    |
/// | DoubleFailure    | 61.8%–81%         | c < 100% × b     | -2    |
/// | Irregular        | 100%–138.2%       | c ≥ 100% × b     | -1    |
/// | IrregularFailure | > 138.2%          | c < 100% × b     | -2    |
/// | Elongated        | ≥ 138.2%(Triangle/Terminal 內) | c > a | +1   |
///
/// 由 `classifier::flat_classifier::classify_flat` 從 monowave magnitudes 推導。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum FlatKind {
    Common,
    BFailure,
    CFailure,
    DoubleFailure,
    Irregular,
    IrregularFailure,
    Elongated,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum TriangleKind {
    Contracting,
    Expanding,
    Limiting,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum CombinationKind {
    /// Table A 小 x-wave 組合:(5-3-5) + x + (5-3-5) = Double Zigzag
    DoubleZigzag,
    /// Table A:(5-3-5) + x + (3-3-5) 或變體 = Double Combination
    DoubleCombination,
    /// Table A:(3-3-5) + x + (3-3-5) = Double Flat
    DoubleFlat,
    /// Table A:3 Zigzag + 2 x = Triple Zigzag
    TripleZigzag,
    /// Table A:Triple 含 Combination 變體 = Triple Combination
    TripleCombination,
    /// Table B 大 x-wave:(3-3-5) + x + (3-3-5) = Double Three(舊有,保留)
    DoubleThree,
    /// Table B:(3-3-5) + x + (3-3-3-3-3 c.t.) = Double Three Combination(最常見)
    DoubleThreeCombination,
    /// Table B 大 x 且不退至起點:Double Three Running
    DoubleThreeRunning,
    /// Table B:(3-3-5) + x + (3-3-5) + x + (3-3-5) = Triple Three(舊有,保留)
    TripleThree,
    /// Table B:(3-3-5) + x + (3-3-5) + x + (3-3-3-3-3 c.t.) = Triple Three Combination
    TripleThreeCombination,
    /// Table B 大 x:Triple Three Running
    TripleThreeRunning,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum ComplexityLevel {
    Simple,
    Intermediate,
    Complex,
}

/// Neely wave number 標號(Impulse 1-5 + Combination X + Triangle/Flat A-E)。
///
/// 對齊 m3Spec/neely_core_architecture.md §9.2 PostBehavior::ReachesWaveZone 內部欄。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum WaveNumber {
    W1, W2, W3, W4, W5,
    WX,
    WA, WB, WC, WD, WE,
}

/// 後續行為(spec §9.2 line 900–925 r5 結構化 enum)。
///
/// **舊 3-variant(Continuation/Reversal/Indeterminate)由 Phase 14 取代** —
/// 對齊精華版 Ch10 line 2024–2037「各修正暗示重點」14 種具體後續行為。
///
/// 由 `power_rating::post_behavior::lookup(pattern_type, power_rating, in_triangle_context)`
/// 查表得出;後續 Stage 5+ 細化規則時可在 classifier 內 override 為 Composite。
#[derive(Debug, Clone, Serialize)]
pub enum PostBehavior {
    /// 後續必完全回測整段(例:5th Failure、C-Failure、Terminal Impulse)
    FullRetracementRequired,
    /// 後續必回測 ≥ ratio × 整段(0.0..=1.0,例:±1 級 ratio=0.90)
    MinRetracement { ratio: f64 },
    /// 後續必達 wave-X 區(例:Triangle 後續 thrust 須達 wave-D 區)
    ReachesWaveZone { wave: WaveNumber },
    /// 後續 Impulse 必 > ratio × 前一同向 Impulse(例:Running Correction ratio=1.618)
    NextImpulseExceeds { ratio: f64 },
    /// 不會被完全回測(除非為更大級的 5/c — 例:Double Zigzag/Double Flat)
    NotFullyRetracedUnless { exception: String },
    /// 任意後續(Neutral / Power Rating 0 / Triangle 內覆蓋)
    Unconstrained,
    /// 強烈暗示後續形態(例:±1~±3 被回測 100% → Triangle/Terminal)
    HintsAtPattern { suggested_pattern: String, reason: String },
    /// 多模式組合(後續行為跨多條規則 — 例:Irregular Failure = 必完全回測 + 後續 Impulse ≥ 161.8%)
    Composite { behaviors: Vec<PostBehavior> },
}

// ---------------------------------------------------------------------------
// StructuralFacts(§9.1)— Item 7 拆解,不加總
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize)]
pub struct StructuralFacts {
    pub fibonacci_alignment: Option<FibonacciAlignment>,
    pub alternation: Option<AlternationFact>,
    pub channeling: Option<ChannelingFact>,
    pub time_relationship: Option<TimeRelationship>,
    /// 若有 volume 資料才填(§9.1 註)
    pub volume_alignment: Option<VolumeAlignment>,
    pub gap_count: usize,
    pub overlap_pattern: Option<OverlapPattern>,
}

// 以下 placeholder type 在 Stage 5-7 實作時補欄位
#[derive(Debug, Clone, Serialize)]
pub struct FibonacciAlignment {
    pub matched_ratios: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlternationFact {
    pub holds: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChannelingFact {
    pub holds: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimeRelationship {
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VolumeAlignment {
    pub holds: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverlapPattern {
    pub label: String,
}

// ---------------------------------------------------------------------------
// Trigger(§9.2)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Trigger {
    pub trigger_type: TriggerType,
    /// r3 修正:移除 ReduceProbability,改 WeakenScenario(§9.4)
    pub on_trigger: OnTriggerAction,
    pub rule_reference: RuleId,
    pub neely_page: String,
}

#[derive(Debug, Clone, Serialize)]
pub enum TriggerType {
    PriceBreakBelow(f64),
    PriceBreakAbove(f64),
    TimeExceeds(NaiveDate),
    VolumeAnomaly { z_threshold: f64 },
    OverlapWith { wave_id: String },
}

#[derive(Debug, Clone, Serialize)]
pub enum OnTriggerAction {
    InvalidateScenario,
    /// 標註該 scenario 進入 deferred,**不**引入機率語意
    WeakenScenario,
    PromoteAlternative {
        promoted_id: String,
    },
}

// ---------------------------------------------------------------------------
// FibZone(§十四)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct FibZone {
    pub label: String,
    pub low: f64,
    pub high: f64,
    pub source_ratio: f64,
}

// ---------------------------------------------------------------------------
// RuleId(architecture §9.3 — Neely 章節編碼)
// ---------------------------------------------------------------------------

/// Validator 規則 ID,採 Neely 章節編碼(architecture §9.3)。
///
/// 設計目的:RuleId 本身即為書頁追溯,免維護自編號對應表(對齊 architecture §9.3 設計優點)。
///
/// **Phase 1 PR 範圍**:只宣告 Phase 1 用得到的 variants。完整 ~60 variants 對應
/// `m3Spec/neely_core_architecture.md §9.3`,留後續 PR 各 stage 動工時補:
///   - Stage 0(Ch3 Pre-Constructive Logic)→ P2
///   - Stage 3.5(Pattern Isolation / Zigzag DETOUR)→ P3
///   - Stage 5(Ch8 Complex Polywaves)→ P5
///   - Stage 6 / 7(Ch6 Post-Constructive / Ch7 Compaction)→ P6
///   - Stage 7.5(Ch9 Advanced / Channeling)→ P7
///   - Stage 8(Ch4 Three Rounds 遞迴)→ P8
///   - Stage 10 / 10.5(Ch10 Power / Ch12 Reverse Logic)→ P10 / P11
///
/// **設計約束**:
/// - 不 derive `Copy`(預留 Ch9_Exception_Aspect2 { triggered_new_rule: String } 等含 String 的 variant)
/// - 維持 PartialEq + Eq 供 `.contains(&rid)` / `==` 比對
/// - Hash 不需要(無 HashMap/HashSet 用 RuleId 當 key)
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[allow(non_camel_case_types)] // r5 章節編碼採 `Ch5_Essential` 風格(architecture §9.3)
pub enum RuleId {
    // === Ch5 Central Considerations(Essential Construction Rules + Channeling + 變體規則)===
    /// Ch5 Essential Construction Rules R1-R7(neely_rules.md §Impulsion 1291-1300 行)
    /// - R1 必須有 5 個相鄰段
    /// - R2 其中 3 段方向相同
    /// - R3 W2 逆向不得完全回測 W1
    /// - R4 W3 須長於 W2
    /// - R5 W4 逆向不得完全回測 W3
    /// - R6 W5 ≥ 38.2% × W4(短於則稱 5th-Wave Failure)
    /// - R7 W3 絕不可為 W1/W3/W5 中最短
    Ch5_Essential(u8),

    /// Ch5 Overlap Rule — Trending Impulse:W4 不可進入 W2 區
    /// neely_rules.md 1326-1329 行
    Ch5_Overlap_Trending,

    /// Ch5 Overlap Rule — Terminal Impulse:W4 必須部分侵入 W2 區
    /// neely_rules.md 1326-1329 行
    Ch5_Overlap_Terminal,

    /// Ch5 Rule of Equality:1/3/5 中「非延伸的兩個」傾向等價或 Fib 關係
    /// neely_rules.md §Rule of Equality
    Ch5_Equality,

    /// Ch5 Rule of Alternation:同級 W2/W4(或 a/b/c 等)在 axis 之一須不同
    /// neely_rules.md §Rule of Alternation
    Ch5_Alternation { axis: AlternationAxis },

    /// Ch5 Flat 子規則:b ≥ 38.2% × a(neely_rules.md §Flats)
    Ch5_Flat_Min_BRatio,
    /// Ch5 Flat 子規則:c ≥ 38.2% × b(neely_rules.md §Flats)
    Ch5_Flat_Min_CRatio,

    /// Ch5 Zigzag 子規則:b ≤ 61.8% × a(neely_rules.md §Zigzags)
    Ch5_Zigzag_Max_BRetracement,
    /// Ch5 Zigzag 子規則:c-wave Triangle 例外(neely_rules.md §Zigzags)
    Ch5_Zigzag_C_TriangleException,

    /// Ch5 Triangle 子規則:b 的價格範圍約束(neely_rules.md §Triangles)
    Ch5_Triangle_BRange,
    /// Ch5 Triangle 子規則:leg 收斂(Contracting)/ 擴張(Expanding)約束
    Ch5_Triangle_LegContraction,
    /// Ch5 Triangle 子規則:三條同度數腿價格相等性 ±5%(neely_rules.md §Triangles)
    Ch5_Triangle_LegEquality_5Pct,

    /// Ch5 Channeling 0-2 trendline(Impulse 通道,W0→W2 連線)
    Ch5_Channeling_02,
    /// Ch5 Channeling 1-3 trendline(Impulse 通道,W1→W3 連線)
    Ch5_Channeling_13,
    /// Ch5 Channeling 2-4 trendline(Impulse breakout 通道,W2→W4 連線)
    Ch5_Channeling_24,
    /// Ch5 Channeling 0-B trendline(Zigzag/Flat 通道,a 起點→b 終點)
    Ch5_Channeling_0B,
    /// Ch5 Channeling B-D trendline(Triangle 通道,b→d 連線)
    Ch5_Channeling_BD,

    // === Ch9 Advanced Rules(Phase 7 PR)===
    /// Ch9 Trendline Touchpoints Rule(spec 1957-1961 行):
    /// 5+ 點觸線 → 該段不可能是 Impulse
    Ch9_TrendlineTouchpoints,
    /// Ch9 Time Rule(spec 1963-1971 行):
    /// 任何三個相鄰同級波,不可時間皆相等
    Ch9_TimeRule,
    /// Ch9 Independent Rule(spec 1973-1974 行):各規則彼此獨立
    Ch9_Independent,
    /// Ch9 Simultaneous Occurrence(spec 1976-1977 行):同情境所有規則須同時成立
    Ch9_Simultaneous,
    /// Ch9 Exception Rule Aspect 1(spec 1980-1986 行):
    /// 不尋常條件允許單一規則失靈,須符合 Multiwave 結尾 / Terminal w5/c / Triangle 進出三情境之一
    Ch9_Exception_Aspect1 { situation: ExceptionSituation },
    /// Ch9 Exception Rule Aspect 2(spec 1988-1990 行):
    /// 規則失效本身啟動另一規則(例:2-4 線突破 → Terminal;Thrust 超時 → Non-Limiting/Terminal)
    Ch9_Exception_Aspect2 { triggered_new_rule: String },
    /// Ch9 Structure Integrity(spec 1992-1994 行):已壓縮確認的結構不可隨意修改
    Ch9_StructureIntegrity,

    // === 工程護欄(非 Neely 規則,獨立列出 — architecture §9.3 末段)===
    /// 資料量不足(< warmup_periods)→ Stage 1 階段失敗
    Engineering_InsufficientData,
    /// Forest 爆量 → Stage 8 BeamSearchFallback
    Engineering_ForestOverflow,
    /// Compaction 逾時 → Stage 8 中斷,回傳 partial forest
    Engineering_CompactionTimeout,
}

/// Alternation 的「軸」(neely_rules.md §Rule of Alternation)。
/// Phase 1 PR:只 Construction 軸實際被引用(W2/W4 alternation 用 Construction),
/// 其他 axis variant 留後續 PR 用。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum AlternationAxis {
    Price,
    Time,
    Severity,
    Intricacy,
    Construction,
}

/// Ch9 Exception Rule Aspect 1 的三個情境(neely_rules.md 1980-1986 行)。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum ExceptionSituation {
    /// A:Multiwave 或更大形態的結尾
    MultiwaveEnd,
    /// B:Terminal (diagonal triangle) 的 wave-5 或 c-wave
    TerminalW5OrC,
    /// C:進入或離開 Contracting/Expanding Triangle 的位置
    TriangleEntryExit,
}

/// Ch7 Compaction Reassessment 對應的 base label。
/// 對齊 m3Spec/neely_rules.md §Compaction 表(1803-1811 行)。
pub fn compaction_base_label(pattern: &NeelyPatternType) -> StructureLabel {
    match pattern {
        // Trending Impulse → :5
        NeelyPatternType::Impulse => StructureLabel::Five,
        // Terminal Impulse(Diagonal)→ :5
        NeelyPatternType::Diagonal { .. } => StructureLabel::Five,
        // Zigzag(5-3-5)→ :3
        NeelyPatternType::Zigzag { .. } => StructureLabel::Three,
        // Flat(3-3-5)→ :3
        NeelyPatternType::Flat { .. } => StructureLabel::Three,
        // Triangle(3-3-3-3-3)→ :3
        NeelyPatternType::Triangle { .. } => StructureLabel::Three,
        // 含 x-wave 的任何形態 → :3
        NeelyPatternType::Combination { .. } => StructureLabel::Three,
        // RunningCorrection → :3(Non-Standard correction,3-3-5 等基本仍是 corrective)
        NeelyPatternType::RunningCorrection => StructureLabel::Three,
    }
}

// ---------------------------------------------------------------------------
// Structure Label(Ch3 Pre-Constructive Logic + Ch4 Structure Labels)
// 對齊 m3Spec/neely_rules.md §Structure Labels 完整清單(319-346 行)
// ---------------------------------------------------------------------------

/// Structure Label — Ch3 Pre-Constructive Logic 輸出於每個 monowave 的「結構候選」標籤。
///
/// 對應 neely_rules.md §Structure Labels 完整清單(319-346 行)。
/// 命名規則:Position Indicator + Base(`:3` / `:5`),例:
///   - `F3` = `:F3`(First 修正 3 段)
///   - `XC3` = `x:c3`(X-wave 變體 c3)
///   - `BC3` = `b:c3`(Flat b-wave 變體 c3)
///   - `S5` = `:s5`(Special Five — Neely extension)
///
/// Phase 2 PR 範圍只宣告 Ch3 引用的 label。未來 Ch4-Ch12 若引入新 label 需擴 enum。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
#[allow(non_camel_case_types)]
pub enum StructureLabel {
    /// `:5` — 衝動五段(無 Position Indicator)
    Five,
    /// `:3` — 修正三段(無 Position Indicator)
    Three,

    /// `:F3` — First(序列首段)修正 3 段
    F3,
    /// `:c3` — center(序列中段)修正 3 段
    C3,
    /// `:L3` — Last(序列末段)修正 3 段
    L3,
    /// `:?3` — 位置未定 修正 3 段
    UnknownThree,

    /// `:F5` — First(序列首段)衝動 5 段
    F5,
    /// `:L5` — Last(序列末段)衝動 5 段
    L5,
    /// `:?5` — 位置未定 衝動 5 段
    UnknownFive,

    /// `:s5` — special five(Neely extension):可替代 `:L5` 但不需反轉確認
    S5,
    /// `:sL3` — special last three(Neely extension):Triangle 倒二段
    SL3,
    /// `:sL5` — special last five(Neely extension):罕用,功能類似弱化 `:L5`
    SL5,

    /// `x:c3` — X-wave 變體(分隔兩個 Standard 修正的修正波)
    XC3,
    /// `b:c3` — Flat b-wave 變體(Flat 中的 b-wave)
    BC3,
    /// `b:F3` — Flat b-wave F3 變體(missing-wave bundle 用)
    BF3,
}

/// Certainty — Structure Label 的封裝強度(neely_rules.md §封裝慣例 343-346 行)。
///
/// - Primary(無封裝):主要選項(機率最高)
/// - Possible(`(...)`):可能但機率較低
/// - Rare(`[...]`):罕見,僅在極特定條件成立時才考量
/// - MissingWaveBundle(`?` 後綴):missing-wave 場景的束帶標記
///   (對齊 neely_rules.md §1054-1057「missing wave 標記慣例」:成組捆綁,一個被棄整組刪)
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum Certainty {
    /// 主要選項(無封裝)
    Primary,
    /// 可能(`(...)` 圓括)
    Possible,
    /// 罕見(`[...]` 方括)
    Rare,
    /// missing-wave 束帶標記(`?` 後綴)
    MissingWaveBundle,
}

/// Structure Label Candidate — 單一候選 label + certainty。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub struct StructureLabelCandidate {
    pub label: StructureLabel,
    pub certainty: Certainty,
}

// ---------------------------------------------------------------------------
// Pattern Isolation(Stage 3.5)— Ch3 Pattern Isolation Procedures
// 對齊 m3Spec/neely_rules.md §Pattern Isolation Procedures(1064-1126 行)
// ---------------------------------------------------------------------------

/// Pattern Isolation 識別出的「圖上可隔離的 Elliott 形態邊界」。
///
/// (start_idx, end_idx) 是 classified_monowaves 的 index 範圍(inclusive)。
/// start_label / end_label 是 anchor 標籤(F3/XC3/L3/S5/L5 之一作為起點,
/// L5/L3 作為終點)。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PatternBound {
    /// 起點 monowave index(inclusive)
    pub start_idx: usize,
    /// 終點 monowave index(inclusive)
    pub end_idx: usize,
    /// 起點 anchor 標籤(spec 1107 行:F3/XC3/L3/S5/L5 之一)
    pub start_label: StructureLabel,
    /// 終點 anchor 標籤(L5/L3 之一)
    pub end_label: StructureLabel,
    /// 是否通過 Compaction(Ch7)驗證為合法 Elliott 形態(spec Step 5)。
    /// Phase 3 預設 false,Phase 6/8(Compaction Three Rounds)實作後填 true。
    pub validated: bool,
    /// Special Circumstances(spec 1121-1123 行):
    /// price action 超出自身起點 → 強制 base = `:3` corrective
    pub forced_corrective: bool,
}

/// Zigzag DETOUR Test 對 wave_count == 3 candidate 的 annotation
/// 對齊 m3Spec/neely_rules.md §Zigzag DETOUR Test(1283-1285 行)。
#[derive(Debug, Clone, Serialize)]
pub struct DetourAnnotation {
    /// 對應 candidate id
    pub candidate_id: String,
    /// 若 detour 後 5-wave Trending Impulse 結構成立,提供替代 monowave_indices(共 5 個)
    pub impulse_alternative: Option<Vec<usize>>,
}
