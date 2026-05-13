// NeelyCoreOutput + Scenario Forest + Diagnostics
// 對齊 m3Spec/neely_core_architecture.md r5 §八 / §九 / §十五(2026-05-13 PR-3c-pre)
//
// PR-3c-pre 落地項(對齊 spec r5):
//   - **RuleId chapter-based 重寫**(§9.3):取代 Core(u8)/Flat(u8) 等 simple enum,
//     改用 Neely 章節對應的 variant(Ch3_PreConstructive / Ch11_Triangle_Variant_Rules
//     等),書頁追溯零成本
//   - **TerminalImpulse 取代 Diagonal**(§9.6):Neely 派術語,r2 Diagonal{Leading/Ending}
//     棄用
//   - **PowerRating 方向中性語意**(§9.2):FavorContinuation/AgainstContinuation 取代
//     Bullish/Bearish,避免方向誤用
//   - **PostBehavior 完整 8 variant**(§9.2):取代 r2 簡單 3 variant
//   - **StructuralFacts 8 子欄位**(§9.5):加 extension_subdivision_pair + Alternation5Axes
//   - **NeelyDiagnostics 對齊 spec**(§15.1):加 atr_dual_mode_diff + peak_memory_mb 改 f64
//
// 設計原則保留:
//   - Forest 不選 primary(§8.2):scenario_forest: Vec<Scenario>,順序不反映優先級
//   - 不引入機率語意(§9.4):無 confidence / composite_score
//   - Trigger.on_trigger 用 WeakenScenario 取代 ReduceProbability

use chrono::NaiveDate;
use fact_schema::Timeframe;
use serde::Serialize;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Input(§五)
// ---------------------------------------------------------------------------

/// 後復權 OHLC 序列。Silver `price_*_fwd` 表已處理漲跌停合併與後復權。
/// Volume 為選填,Volume Alignment 子規則(§9.5)需要時用。
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

    /// Forest,**不**附 `primary` 欄位(§8.2)
    pub scenario_forest: Vec<Scenario>,

    /// monowave 序列 — 對外暴露給 trendline_core 唯一例外消費(§8.1)
    pub monowave_series: Vec<Monowave>,

    pub diagnostics: NeelyDiagnostics,

    pub rule_book_references: Vec<RuleReference>,

    /// 資料充分性(歷史不足會導致大量 candidate 被 reject,實際是「無法判斷」)
    pub insufficient_data: bool,

    /// Round 3 Pause:所有 scenario 都等候 :L5/:L3 結束標籤時的整體狀態(§8.4)。
    /// None = 至少一個 scenario 已完成形態識別;Some = 全部暫停。
    pub round3_pause: Option<Round3PauseInfo>,
}

/// Round 3 Pause Info(§8.4)— Three Rounds Compaction 整體暫停狀態摘要。
#[derive(Debug, Clone, Serialize)]
pub struct Round3PauseInfo {
    pub scenarios_affected: usize,
    pub last_l_label_date: Option<NaiveDate>,
    pub strategy_implication: String,
}

// ---------------------------------------------------------------------------
// Diagnostics(§15.1)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize)]
pub struct NeelyDiagnostics {
    pub monowave_count: usize,
    pub candidate_count: usize,
    pub validator_pass_count: usize,
    pub validator_reject_count: usize,
    /// 完整保留拒絕原因(§15.1):rule_id / expected / actual / gap / neely_page
    pub rejections: Vec<RuleRejection>,
    pub forest_size: usize,
    /// 所有合法壓縮路徑
    pub compaction_paths: usize,
    /// Forest 是否爆量觸發 BeamSearchFallback
    pub overflow_triggered: bool,
    /// Compaction 是否逾時
    pub compaction_timeout: bool,
    /// 各階段耗時(Stage 1-10)。spec mandate `HashMap<String, Duration>`,
    /// 為 serde 序列化簡潔以 millis(u64) 表達同等語意。
    pub stage_timings_ms: HashMap<String, u64>,
    /// 整體執行耗時
    pub elapsed_ms: u64,
    /// 峰值記憶體(P0 Gate 校準用,spec §15.1 mandate f64)
    pub peak_memory_mb: f64,
    /// ATR 雙模交叉驗證(P0 Gate 用,§15.1)
    pub atr_dual_mode_diff: Option<AtrDualModeDiff>,
}

/// ATR 雙模交叉驗證結果(P0 Gate 用,§15.1 + §6.5)
#[derive(Debug, Clone, Default, Serialize)]
pub struct AtrDualModeDiff {
    pub monowave_count_rolling: usize,
    pub monowave_count_fixed: usize,
    pub forest_size_rolling: usize,
    pub forest_size_fixed: usize,
    pub validator_reject_rate_rolling: f64,
    pub validator_reject_rate_fixed: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleRejection {
    pub candidate_id: String,
    pub rule_id: RuleId,
    pub expected: String,
    pub actual: String,
    /// 偏離量(百分比或絕對值,依規則而定)
    pub gap: f64,
    /// Neely 書頁追溯,例 "p.3-48"
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
    /// 例:"5-3-5 Zigzag in W4 of larger Impulse"
    pub structure_label: String,
    pub complexity_level: ComplexityLevel,

    /// PowerRating 為 Ch10 查表結果(§9.2),enum 取代 v1.1 `i8`
    pub power_rating: PowerRating,
    pub max_retracement: f64,
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

    /// 結構性事實 8 維(§9.5 Item 7 拆解,不加總)
    pub structural_facts: StructuralFacts,

    /// 本 Scenario 是否等候 :L5/:L3 結束標籤(§8.4 雙標設計)。
    /// true = 形態未完成,等更多資料;false = 已可判斷形態。
    pub awaiting_l_label: bool,
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
// PowerRating(§9.2)— 方向中性語意,r3 改 enum 取代 i8
// ---------------------------------------------------------------------------

/// Power Rating(精華版 Ch10 查表)
/// 方向中性語意:相對於前一段趨勢方向解釋。
/// r5 §9.2 命名修正:r2 Bullish/Bearish 改 FavorContinuation/AgainstContinuation,
/// 避免方向誤用(任何 pattern 都可能 favor 或 against 既有趨勢)。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum PowerRating {
    StronglyFavorContinuation,    // +3
    ModeratelyFavorContinuation,  // +2
    SlightlyFavorContinuation,    // +1
    Neutral,                      //  0
    SlightlyAgainstContinuation,  // -1
    ModeratelyAgainstContinuation,// -2
    StronglyAgainstContinuation,  // -3
}

// ---------------------------------------------------------------------------
// PostBehavior(§9.2)— 完整 8 variant 結構化 enum
// ---------------------------------------------------------------------------

/// 後續行為(§9.2 Gap 1.2 群 3 PostBehavior = a)
#[derive(Debug, Clone, Serialize)]
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
    HintsAtPattern { suggested_pattern: Box<NeelyPatternType>, reason: String },
    /// 多模式組合(後續行為跨多條規則)
    Composite { behaviors: Vec<PostBehavior> },
}

// ---------------------------------------------------------------------------
// NeelyPatternType + 子型號(§9.6)
// ---------------------------------------------------------------------------

/// Neely Pattern Type。r5 §9.6 修正:
/// - 取代 r2 `Diagonal{Leading/Ending}`(Prechter 派)為 `TerminalImpulse`(Neely 派)
/// - RunningCorrection 獨立 top-level variant(Power Rating ±3 級別)
#[derive(Debug, Clone, Serialize)]
pub enum NeelyPatternType {
    Impulse,
    /// Neely 派術語,取代 Prechter Diagonal Leading/Ending
    TerminalImpulse,
    /// Power Rating ±3 級別獨立 variant
    RunningCorrection,
    Zigzag { sub_kind: ZigzagVariant },
    Flat { sub_kind: FlatVariant },
    Triangle { sub_kind: TriangleVariant },
    Combination { sub_kinds: Vec<CombinationKind> },
}

/// Zigzag 子變體(Ch5 p.5-41~42)
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum ZigzagVariant {
    /// 61.8-161.8% × a(典型)
    Normal,
    /// > 161.8% × a
    Elongated,
    /// 38.2-61.8% × a
    Truncated,
}

/// Flat 子變體(r5 §9.6 修正,7 種 named variant)
/// b 強度(StrongBWave/WeakBWave)不在此 enum,改入 `StructuralFacts` 或 FlatVariant 關聯欄
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum FlatVariant {
    Common,
    BFailure,
    CFailure,
    Irregular,
    IrregularFailure,
    Elongated,
    DoubleFailure,
}

/// Triangle 子變體(spec r5 §9.3 line 1040-1044,9 種)
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum TriangleVariant {
    HorizontalLimiting,
    IrregularLimiting,
    RunningLimiting,
    HorizontalNonLimiting,
    IrregularNonLimiting,
    RunningNonLimiting,
    HorizontalExpanding,
    IrregularExpanding,
    RunningExpanding,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum CombinationKind {
    DoubleThree,
    TripleThree,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum ComplexityLevel {
    Simple,
    Intermediate,
    Complex,
}

// ---------------------------------------------------------------------------
// 共用 enum(§9.3 line 1036-1044)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum AlternationAxis { Price, Time, Severity, Intricacy, Construction }

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum ExceptionSituation { MultiwaveEnd, TerminalW5OrC, TriangleEntryExit }

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum WaveAbc { A, B, C }

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum TriangleWave { A, B, C, D, E }

/// Impulse Wave 編號(1-5),供 Ch11 wave-by-wave 規則 + ExtensionSubdivisionPair 用。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum WaveNumber {
    One,
    Two,
    Three,
    Four,
    Five,
}

/// Impulse Extension 類別(Ch5 / Ch11)
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum ImpulseExtension {
    /// 1st Wave Extended
    FirstExt,
    /// 3rd Wave Extended(最常見)
    ThirdExt,
    /// 5th Wave Extended
    FifthExt,
    /// 無特定延長波
    NonExt,
    /// 5th Wave 未創新高/低
    FifthFailure,
}

/// Emulation 類別(Ch12,5 種模仿條件)
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum EmulationKind {
    /// Double Failure 模仿 Triangle
    DoubleFailureAsTriangle,
    /// Double Flat 模仿 Impulse(缺 x-wave)
    DoubleFlatAsImpulse,
    /// Double/Triple Zigzag 模仿 Impulse
    MultiZigzagAsImpulse,
    /// 1st Ext 缺 wave-4 看起來像 c ≤ a 的 Zigzag
    FirstExtAsZigzag,
    /// 5th Ext 缺 wave-2 看起來像 c > a 的 Zigzag
    FifthExtAsZigzag,
}

// ---------------------------------------------------------------------------
// StructuralFacts(§9.5)— 8 子欄位 Item 7 拆解,不加總
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize)]
pub struct StructuralFacts {
    pub fibonacci_alignment: FibonacciAlignment,
    /// 5 軸展開(§9.5)
    pub alternation: Alternation5Axes,
    pub channeling: ChannelingFact,
    /// 對應 Ch9 Time Rule 三種關係
    pub time_relationship: TimeRelationship,
    pub volume_alignment: VolumeAlignment,
    pub gap_count: usize,
    /// Trending(禁止)/ Terminal(必須)/ None
    pub overlap_pattern: OverlapPattern,
    /// Extension 與 Subdivision 獨立記錄(精華版 Ch8,第 8 子欄位)
    pub extension_subdivision_pair: ExtensionSubdivisionPair,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FibonacciAlignment {
    pub matched_ratios: Vec<f64>,
}

/// Alternation 5 軸展開(§9.5)
#[derive(Debug, Clone, Default, Serialize)]
pub struct Alternation5Axes {
    pub price: AlternationCheck,
    pub time: AlternationCheck,
    /// 僅 Impulse 2/4 適用
    pub severity: AlternationCheck,
    pub intricacy: AlternationCheck,
    pub construction: AlternationCheck,
}

#[derive(Debug, Clone, Serialize)]
pub enum AlternationCheck {
    AlternatePresent { evidence: String },
    AlternateAbsent { suggested_pattern: String },
    NotApplicable,
}

impl Default for AlternationCheck {
    fn default() -> Self { AlternationCheck::NotApplicable }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ChannelingFact {
    pub holds: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TimeRelationship {
    pub label: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct VolumeAlignment {
    pub holds: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct OverlapPattern {
    pub label: String,
}

/// Extension 與 Subdivision 獨立記錄(§9.5,精華版 Ch8)
#[derive(Debug, Clone, Serialize)]
pub struct ExtensionSubdivisionPair {
    pub extension_wave: WaveNumber,
    pub subdivision_wave: WaveNumber,
    /// false = 同一波(典型 Impulse);true = Ch8 Independence
    pub independent: bool,
    /// 若 3-Ext 但 wave-5 細分多 → true(Terminal 暗示)
    pub terminal_hint: bool,
}

impl Default for ExtensionSubdivisionPair {
    fn default() -> Self {
        Self {
            extension_wave: WaveNumber::Three,
            subdivision_wave: WaveNumber::Three,
            independent: false,
            terminal_hint: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Trigger(§9.4)
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
// RuleId(§9.3)— Neely 章節編碼
// ---------------------------------------------------------------------------

/// Rule 編碼採 Neely 章節對應(精華版 Ch3-Ch12),而非自編序號。
/// 設計目的:RuleId 本身即為書頁追溯,免維護自編號對應表(§9.3 設計優點)。
///
/// **PR-3c-pre 階段**:enum variant 已完整定義(對齊 spec r5 §9.3),
/// 但具體規則邏輯多數仍為 Deferred(各 validator/*.rs stub),留 PR-3c-1~3 補。
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
pub enum RuleId {
    // === Ch3 Pre-Constructive Rules of Logic ===
    Ch3PreConstructive {
        rule: u8,                        // 1-7
        condition: char,                 // 'a'-'f'
        category: Option<char>,          // 'i'/'ii'/'iii'(僅 Rule 4)
        sub_rule_index: Option<u8>,      // 同 Condition 內多條子規則
    },
    Ch3ProportionDirectional,
    Ch3ProportionNonDirectional,
    Ch3NeutralityAspect1,
    Ch3NeutralityAspect2,
    /// 1-6 步驟
    Ch3PatternIsolationStep(u8),
    /// Compacted 超出自身起點 → :3
    Ch3SpecialCircumstances,

    // === Ch4 Intermediary Observations ===
    Ch4SimilarityBalancePrice,
    Ch4SimilarityBalanceTime,
    Ch4Round1Series,
    Ch4Round2Compaction,
    Ch4Round3Pause,
    Ch4ZigzagDetour,

    // === Ch5 Central Considerations ===
    /// R1-R7(spec line 952)
    Ch5Essential(u8),
    Ch5Equality,
    Ch5Extension,
    /// 1st 最長例外
    Ch5ExtensionException1,
    /// 3rd 最長但 < 161.8% × 1st
    Ch5ExtensionException2,
    Ch5OverlapTrending,
    Ch5OverlapTerminal,
    Ch5Alternation { axis: AlternationAxis },
    Ch5Channeling02,
    Ch5Channeling24,
    Ch5Channeling13,
    Ch5Channeling0B,
    Ch5ChannelingBD,
    Ch5FlatMinBRatio,
    Ch5FlatMinCRatio,
    Ch5ZigzagMaxBRetracement,
    Ch5ZigzagCTriangleException,
    Ch5TriangleBRange,
    Ch5TriangleLegContraction,
    Ch5TriangleLegEquality5Pct,

    // === Ch6 Post-Constructive Rules ===
    Ch6ImpulseStage1,
    Ch6ImpulseStage2 { extension: ImpulseExtension },
    Ch6CorrectionBSmallStage1,
    Ch6CorrectionBSmallStage2,
    Ch6CorrectionBLargeStage1,
    Ch6CorrectionBLargeStage2,
    Ch6TriangleContractingStage1,
    Ch6TriangleContractingStage2,
    Ch6TriangleExpandingNonConfirmation,

    // === Ch7 Conclusions ===
    Ch7CompactionReassessment,
    Ch7ComplexityDifference,
    Ch7Triplexity,

    // === Ch8 Complex Polywaves ===
    /// 中介修正 < 61.8%
    Ch8NonStandardCond1,
    /// 中段 ≥ 161.8%
    Ch8NonStandardCond2,
    Ch8XWaveInternalStructure,
    Ch8LargeXWaveNoZigzag,
    Ch8ExtensionSubdivisionIndependence,
    Ch8MultiwaveConstruction,

    // === Ch9 Advanced Rules ===
    Ch9TrendlineTouchpoints,
    Ch9TimeRule,
    Ch9Independent,
    Ch9Simultaneous,
    Ch9ExceptionAspect1 { situation: ExceptionSituation },
    Ch9ExceptionAspect2 { triggered_new_rule: String },
    Ch9StructureIntegrity,

    // === Ch10 Advanced Logic Rules ===
    Ch10PowerRatingLookup,
    Ch10MaxRetracementLookup,
    Ch10TriangleTerminalPowerOverride,

    // === Ch11 Advanced Progress Label Application ===
    Ch11ImpulseWaveByWave { ext: ImpulseExtension, wave: WaveNumber },
    Ch11TerminalWaveByWave { ext: ImpulseExtension, wave: WaveNumber },
    Ch11FlatVariantRules { variant: FlatVariant, wave: WaveAbc },
    Ch11ZigzagWaveByWave { wave: WaveAbc },
    Ch11TriangleVariantRules { variant: TriangleVariant, wave: TriangleWave },

    // === Ch12 Advanced Neely Extensions ===
    Ch12ChannelingRunningDoubleThree,
    Ch12ChannelingTriangleEarlyWarning,
    Ch12ChannelingTerminalEarlyWarning,
    Ch12FibonacciInternal,
    Ch12FibonacciExternal,
    Ch12WaterfallEffect,
    Ch12MissingWaveMinDataPoints,
    Ch12Emulation { kind: EmulationKind },
    Ch12ReverseLogic,
    Ch12LocalizedChanges,

    // === 工程護欄(非 Neely 規則,獨立列出)===
    EngineeringForestOverflow,
    EngineeringCompactionTimeout,
    EngineeringInsufficientData,
}
