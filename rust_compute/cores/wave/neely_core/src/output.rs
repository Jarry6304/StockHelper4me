// NeelyCoreOutput + Scenario Forest + Diagnostics
// 對齊 m3Spec/neely_core.md §五 §八 §九 §十(2026-05-06 r2)
//
// 設計原則:
//   - **Forest 不選 primary**(§9.3):`scenario_forest: Vec<Scenario>`,
//     順序不反映優先級,Aggregation Layer 可依 power_rating 提供 UI 篩選
//   - **不引入機率語意**(§9.4):移除 v1.1 `confidence` / `composite_score` 欄位
//   - **Trigger 不寫 ReduceProbability**(§9.4):改 `WeakenScenario`
//   - **PowerRating enum**(§9.4):取代 v1.1 `i8`,避免 99 等無效值

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
    /// 各階段耗時(Stage 1-10)
    pub stage_elapsed_ms: HashMap<String, u64>,
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
    /// 例:"5-3-5 Zigzag in W4 of larger Impulse"
    pub structure_label: String,
    pub complexity_level: ComplexityLevel,

    /// r3 修正:由 i8 改 enum,避免 power_rating = 99 等無效值(§9.4)
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

    /// 結構性事實 7 維(Item 7 拆解,不加總)
    pub structural_facts: StructuralFacts,
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

#[derive(Debug, Clone, Copy, Serialize)]
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

#[derive(Debug, Clone, Copy, Serialize)]
pub enum FlatKind {
    Regular,
    Expanded,
    Running,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum TriangleKind {
    Contracting,
    Expanding,
    Limiting,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum CombinationKind {
    DoubleThree,
    TripleThree,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum ComplexityLevel {
    Simple,
    Intermediate,
    Complex,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum PostBehavior {
    Continuation,
    Reversal,
    Indeterminate,
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
// RuleId(§十)
// ---------------------------------------------------------------------------

/// Validator 規則 ID。R / F / Z / T / W 五組,具體規則內容於 validator/ 子模組。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum RuleId {
    /// 通用核心規則 R1-R7
    Core(u8),
    /// Flat 子規則 F1-F2
    Flat(u8),
    /// Zigzag 子規則 Z1-Z4
    Zigzag(u8),
    /// Triangle 子規則 T1-T10
    Triangle(u8),
    /// Wave 通用規則 W1-W2
    Wave(u8),
}
