// output.rs — Traditional Core 自有型別(各帶一份,不 import Neely / cores_shared wave 型別)
//
// 對齊 Traditional Core v2 `references/storage-and-io.md`「自有型別」段 + m3Spec/traditional_rules.md。
//
// 設計原則(對齊 SPEC):
//   - forest **不選 primary**:`scenario_forest: Vec<TraditionalScenario>`,順序為 UI 偏好(preference_score
//     降序),非「主情境」標記
//   - **不引入機率語意**:無 `confidence` / `composite_score`
//   - `preference_score = guidelines_satisfied.len() + qualifiers_met.len()`(engine.md 首選排序鍵)
//   - **無** Neely 專屬的 `power_rating` / `max_retracement` / `post_pattern_behavior`

use chrono::NaiveDate;
use fact_schema::Timeframe; // 平台 enum(非 wave 型別、無 neely dep)
use serde::Serialize;

// ---------------------------------------------------------------------------
// 輸入(自有;非 neely OhlcvSeries)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct TradBar {
    pub date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TradOhlcvSeries {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub bars: Vec<TradBar>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct TimeRange {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

// ---------------------------------------------------------------------------
// Pivot(Stage 1 輸出)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum PivotKind {
    High,
    Low,
}

#[derive(Debug, Clone, Serialize)]
pub struct Pivot {
    pub bar_index: usize,
    pub date: NaiveDate,
    pub price: f64,
    pub kind: PivotKind,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
}

// ---------------------------------------------------------------------------
// 9 級度數(自有;≠ Neely 11 級)— 對齊 traditional_rules.md L1–3
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum Degree {
    GrandSupercycle,
    Supercycle,
    Cycle,
    Primary,
    Intermediate,
    Minor,
    Minute,
    Minuette,
    Subminuette,
}

// ---------------------------------------------------------------------------
// 波浪樹(自有,遞迴)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct WaveNode {
    pub label: String, // "1".."5" / "A".."E" / "W/X/Y/Z"
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub start_price: f64,
    pub end_price: f64,
    pub children: Vec<WaveNode>,
}

// ---------------------------------------------------------------------------
// 形態型別 — 對齊 traditional_rules.md §B + engine.md 對角分類(v2)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum DiagonalKind {
    Leading, // 浪 1 / A
    Ending,  // 浪 5 / C
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum DiagonalShape {
    Contracting,
    Expanding, // R8 rev2:引導對角可呈擴張形
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum DiagonalSub {
    FiveThreeFiveThreeFive, // 5-3-5-3-5
    AllThrees,              // 3-3-3-3-3(R8 rev2:引導對角較常觀察到者)
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum TraditionalPatternType {
    Impulse,
    Diagonal {
        kind: DiagonalKind,
        shape: DiagonalShape,
        sub: DiagonalSub,
    },
    Zigzag,
    Flat,
    Triangle,
    Combination, // Double / Triple Three(v1 未生成,enum 保留供未來)
}

// ---------------------------------------------------------------------------
// 規則 / 指引 / 限定語 ID — 對齊 traditional_rules.md §A
// ---------------------------------------------------------------------------

/// 硬規則(淘汰)+ 待查證/細分(Deferred)+ 工程。`TradRuleId` 直接採規則層 R1–R13 編碼。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum TradRuleId {
    R1Wave2Retracement,   // 浪 2 回撤 ≤ 浪 1 之 100%(硬)
    R2Wave4Retracement,   // 浪 4 回撤 ≤ 浪 3 之 100% [待查證] → Deferred(不淘汰)
    R3Wave3ExceedsWave1,  // 浪 3 必超越浪 1 終點(硬)
    R4Wave3NotShortest,   // 浪 3 在 1/3/5 中永不最短(硬,以百分比)
    R5NoOverlap,          // (衝擊浪)浪 4 不重疊浪 1(硬,僅 Impulse)
    R6ImpulseSubdivision, // 衝擊浪細分 5-3-5-3-5 → Deferred(需遞迴子浪分解,v2 深度)
    R7EndingDiagonalSub,  // 結束對角 3-3-3-3-3 → Deferred(同上)
    R8LeadingDiagonalSub, // 引導對角 5-3-5-3-5 或 3-3-3-3-3 → Deferred(同上)
    R9DiagonalNoFullRetrace, // 對角:無反應子浪完全回撤前行動子浪 + 浪 3 永不最短(硬,僅 Diagonal)
    R10CorrectionNeverFive,  // 修正永遠不是五浪(硬,以候選結構保證)
    R11CorrectiveSubdivision, // 鋸齒 5-3-5 / 平台 3-3-5 / 三角形 3-3-3-3-3 → Deferred(需遞迴)
    R12CombinationConstraint, // 組合 ≤1 鋸齒 / ≤1 三角形 → NotApplicable(v1 未生成組合)
    R13TrianglePosition,      // 三角形 nearly always 位於最終行動浪前(限定語,非硬淘汰)
}

/// 客觀指引(計入 preference_score)— L10–13 + L20–25(§A 指引)。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum GuidelineId {
    Alternation,           // L10 浪 2 ↔ 浪 4 風格交替
    Equality,              // L12 兩非延伸推動浪約等長
    FibWave2Retrace,       // L20 浪 2 回撤 ≈ .618 / .5
    FibWave4Retrace,       // L20 浪 4 回撤 ≈ .382
    FibMotiveMultiple,     // L20 浪 5 對浪 1 ≈ .618 / 1.0 / 1.618 / 2.618
    FibCorrectiveMultiple, // L20 修正浪 C 對 A ≈ equality / .618 / 1.618
}

/// 限定語(`almost / nearly always`,計入 preference_score 但不淘汰)— §A。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum QualifierId {
    DiagonalWave1Wave4Overlap, // R7/R8「浪 1/4 重疊」特徵
    TrianglePrecedesFinal,     // R13 三角形位置(v1 無上層 context → 通常不授予)
}

// ---------------------------------------------------------------------------
// 失效條件 / Fib 投影 / 拒絕紀錄
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum TriggerKind {
    PriceBreakBelow,
    PriceBreakAbove,
}

#[derive(Debug, Clone, Serialize)]
pub struct Trigger {
    pub kind: TriggerKind,
    pub price: f64,
    pub rule_reference: TradRuleId,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FibZone {
    pub label: String,
    pub low: f64,
    pub high: f64,
    pub source_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleRejection {
    pub rule_id: TradRuleId,
    pub candidate_id: String,
    pub detail: String,
}

// ---------------------------------------------------------------------------
// Scenario(forest 元素)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct TraditionalScenario {
    pub id: String,
    pub wave_tree: WaveNode,
    pub pattern_type: TraditionalPatternType,
    pub direction: Direction,
    pub structure_label: String,
    pub degree: Degree,
    /// 通過的硬規則(合法性,非排序)
    pub passed_rules: Vec<TradRuleId>,
    /// 無法判定(需遞迴子浪分解 / 待查證)而暫緩的規則
    pub deferred_rules: Vec<TradRuleId>,
    /// 滿足的客觀指引(計入排序)
    pub guidelines_satisfied: Vec<GuidelineId>,
    /// 滿足的限定語(計入排序)
    pub qualifiers_met: Vec<QualifierId>,
    /// = guidelines_satisfied.len() + qualifiers_met.len()(engine.md 首選排序鍵)
    pub preference_score: usize,
    pub invalidation_triggers: Vec<Trigger>,
    pub expected_fib_zones: Vec<FibZone>,
}

// ---------------------------------------------------------------------------
// 診斷 + 主輸出
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct TraditionalDiagnostics {
    pub pivot_count: usize,
    pub candidate_count: usize,
    pub validator_pass_count: usize,
    pub validator_reject_count: usize,
    pub rejections: Vec<RuleRejection>,
    pub forest_overflow_triggered: bool,
    pub insufficient_data: bool,
    pub elapsed_ms: u64,
    /// 引擎版本(traditional_snapshots 無 source_version 欄,由此欄承載;
    /// 舊 row 缺此欄 → 讀取端容缺。首個明確版本常數,對齊 v3 世代命名)
    pub engine_version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraditionalCoreOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub data_range: TimeRange,
    pub pivot_series: Vec<Pivot>,
    pub scenario_forest: Vec<TraditionalScenario>, // 不選 primary
    pub diagnostics: TraditionalDiagnostics,
}
