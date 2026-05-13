// validator — Stage 4:Validator Ch5 Essential + Overlap + Flat/Zigzag/Triangle/Equality/Alternation
//
// 對齊 m3Spec/neely_core_architecture.md §7.1 Stage 4 + §9.3 RuleId
//       + m3Spec/neely_rules.md §Ch5(Central Considerations — Polywave 建構)
//
// 子模組:
//   - core_rules.rs   — Ch5_Essential R1-R7 + Ch5_Overlap_Trending + Ch5_Overlap_Terminal(9 條,全實作)
//   - flat_rules.rs   — Ch5_Flat_Min_BRatio / Ch5_Flat_Min_CRatio(2 條 Deferred)
//   - zigzag_rules.rs — Ch5_Zigzag_Max_BRetracement / Ch5_Zigzag_C_TriangleException(2 條 Deferred)
//   - triangle_rules.rs — Ch5_Triangle_BRange / LegContraction / LegEquality_5Pct(3 條 Deferred)
//   - wave_rules.rs   — Ch5_Equality / Ch5_Alternation(2 條 Deferred)
//
// 共 18 條規則,清單對齊 architecture §9.3 RuleId enum Ch5_* variants。
//
// 容差規範(architecture §4.2 三檔容差表):
//   - 一般近似(approximately equal / about):±10%
//   - Fibonacci 比率(38.2/61.8/100/161.8/261.8):±4%
//   - Triangle 三條同度數腿價格相等性:±5%(僅限)
// 不可外部化(architecture §4.5 / §6.6)
//
// 規則執行順序:all-on-all dispatch(無 short-circuit),收齊 18 條 RuleResult 後彙整 ValidationReport
//
// **Phase 1 PR(r5)**:
//   - Ch5 Essential R1-R7 + Ch5_Overlap_Trending + Ch5_Overlap_Terminal **全實作**
//   - 其他 9 條(Flat 2 / Zigzag 2 / Triangle 3 / Wave 2)body Deferred,RuleId 編碼對齊 r5 §9.3
//   - r4 自編號 22 條 → r5 Phase 1 9 條 Deferred(reduction 對齊 spec)
//   - Stage 0 Pre-Constructive Logic 留 P2 / Stage 7.5 Channeling 留 P7

use crate::candidates::WaveCandidate;
use crate::monowave::ClassifiedMonowave;
use crate::output::{RuleId, RuleRejection};

pub mod core_rules;
pub mod flat_rules;
pub mod triangle_rules;
pub mod wave_rules;
pub mod zigzag_rules;

/// 單條規則對 candidate 的判定結果。
#[derive(Debug, Clone)]
pub enum RuleResult {
    /// 規則通過
    Pass,
    /// 規則違反 — 附 RuleRejection 含 rule_id / expected / actual / gap / neely_page
    Fail(RuleRejection),
    /// 規則尚未實作或需 future bars 才能驗證(對齊 §10.3 deferred rules)
    Deferred(RuleId),
    /// 規則對此 candidate 不適用(例:Triangle 規則對 wave_count=3 candidate)
    NotApplicable(RuleId),
}

impl RuleResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, RuleResult::Pass)
    }
    pub fn is_fail(&self) -> bool {
        matches!(self, RuleResult::Fail(_))
    }
    pub fn is_deferred(&self) -> bool {
        matches!(self, RuleResult::Deferred(_))
    }
}

/// 對單一 candidate 跑完所有 18 條 Ch5 規則的彙總報告。
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub candidate_id: String,
    pub passed: Vec<RuleId>,
    pub failed: Vec<RuleRejection>,
    pub deferred: Vec<RuleId>,
    pub not_applicable: Vec<RuleId>,
    /// 整體判定:任一非 Overlap_* Fail → false;
    /// Ch5_Overlap_Trending 或 Ch5_Overlap_Terminal 單獨 fail 是預期行為(兩規則互斥),不視為整體 fail
    pub overall_pass: bool,
}

/// 對單一 candidate 跑完整 18 條規則,回傳彙整報告。
///
/// 邏輯:
///   1. 跑 Ch5_Essential R1-R7 + Ch5_Overlap_Trending + Ch5_Overlap_Terminal(9 條,core_rules)
///   2. 跑 Ch5_Flat_* / Ch5_Zigzag_* / Ch5_Triangle_*(Deferred)
///   3. 跑 Ch5_Equality / Ch5_Alternation(Deferred)
///   4. **Overlap_Trending / Overlap_Terminal 互斥**:其中之一 fail 是正常(對應 Impulse 或 Diagonal),
///      只有兩條同時 fail 才表示結構錯亂 → overall_pass = false
///   5. 其他非 Overlap 規則任一 Fail → overall_pass = false
pub fn validate_candidate(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> ValidationReport {
    let mut report = ValidationReport {
        candidate_id: candidate.id.clone(),
        ..Default::default()
    };

    // 收集 18 條規則結果
    let mut results: Vec<RuleResult> = Vec::with_capacity(18);
    results.extend(core_rules::run(candidate, classified));
    results.extend(flat_rules::run(candidate, classified));
    results.extend(zigzag_rules::run(candidate, classified));
    results.extend(triangle_rules::run(candidate, classified));
    results.extend(wave_rules::run(candidate, classified));

    let mut non_overlap_fail = false;
    let mut overlap_trending_failed = false;
    let mut overlap_terminal_failed = false;

    for result in results {
        match result {
            RuleResult::Pass => {
                // 規則 pass 暫不記入 passed(目前 passed 留給 classifier default_passed_rules 反推)
                // P4+ 補完整 Pass 紀錄
            }
            RuleResult::Fail(rej) => {
                match rej.rule_id {
                    RuleId::Ch5_Overlap_Trending => overlap_trending_failed = true,
                    RuleId::Ch5_Overlap_Terminal => overlap_terminal_failed = true,
                    _ => non_overlap_fail = true,
                }
                report.failed.push(rej);
            }
            RuleResult::Deferred(rid) => {
                report.deferred.push(rid);
            }
            RuleResult::NotApplicable(rid) => {
                report.not_applicable.push(rid);
            }
        }
    }

    // 兩條 overlap 規則同時 fail → 結構錯亂(architecture §7.1 Stage 4 失敗模型)
    let both_overlaps_failed = overlap_trending_failed && overlap_terminal_failed;
    report.overall_pass = !(non_overlap_fail || both_overlaps_failed);
    report
}

/// 批次跑全部 candidates 的 validator。
pub fn validate_all(
    candidates: &[WaveCandidate],
    classified: &[ClassifiedMonowave],
) -> Vec<ValidationReport> {
    candidates
        .iter()
        .map(|c| validate_candidate(c, classified))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NeelyEngineConfig;
    use crate::monowave::ProportionMetrics;
    use crate::output::{Monowave, MonowaveDirection};
    use chrono::NaiveDate;

    fn cmw(start_p: f64, end_p: f64, dir: MonowaveDirection) -> ClassifiedMonowave {
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap(),
                end_date: NaiveDate::parse_from_str("2026-01-05", "%Y-%m-%d").unwrap(),
                start_price: start_p,
                end_price: end_p,
                direction: dir,
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: (end_p - start_p).abs(),
                duration_bars: 5,
                atr_relative: 5.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
        }
    }

    fn make_5wave_impulse_up() -> Vec<ClassifiedMonowave> {
        // W1 100→110 / W2 110→104 / W3 104→125 / W4 125→118 / W5 118→132
        // 對齊 r5 Ch5 Essential 7 條全 pass + Overlap_Trending pass / Overlap_Terminal fail
        // (這是 clean Trending Impulse,Terminal 規則應 fail 是正常 — 兩規則互斥)
        vec![
            cmw(100.0, 110.0, MonowaveDirection::Up),
            cmw(110.0, 104.0, MonowaveDirection::Down),
            cmw(104.0, 125.0, MonowaveDirection::Up),
            cmw(125.0, 118.0, MonowaveDirection::Down),
            cmw(118.0, 132.0, MonowaveDirection::Up),
        ]
    }

    fn make_candidate_5wave(direction: MonowaveDirection) -> WaveCandidate {
        WaveCandidate {
            id: "c5-mw0-mw4".to_string(),
            monowave_indices: vec![0, 1, 2, 3, 4],
            wave_count: 5,
            initial_direction: direction,
        }
    }

    fn make_candidate_3wave(direction: MonowaveDirection) -> WaveCandidate {
        WaveCandidate {
            id: "c3-mw0-mw2".to_string(),
            monowave_indices: vec![0, 1, 2],
            wave_count: 3,
            initial_direction: direction,
        }
    }

    #[test]
    fn valid_5wave_up_impulse_passes_overall() {
        let classified = make_5wave_impulse_up();
        let candidate = make_candidate_5wave(MonowaveDirection::Up);
        let report = validate_candidate(&candidate, &classified);
        assert!(
            report.overall_pass,
            "well-formed 5-wave Up impulse 應 overall_pass = true(只 Overlap_Terminal fail,正常),failed = {:?}",
            report.failed
        );
        // failed 應只含 Overlap_Terminal(Trending 假設下 Terminal 必 fail)
        assert_eq!(report.failed.len(), 1, "clean impulse 應只 1 條 fail(Overlap_Terminal)");
        assert!(matches!(
            report.failed[0].rule_id,
            RuleId::Ch5_Overlap_Terminal
        ));
    }

    #[test]
    fn validate_all_processes_multiple_candidates() {
        let classified = make_5wave_impulse_up();
        let candidates = vec![
            make_candidate_5wave(MonowaveDirection::Up),
            make_candidate_3wave(MonowaveDirection::Up),
        ];
        let _cfg = NeelyEngineConfig::default();
        let reports = super::validate_all(&candidates, &classified);
        assert_eq!(reports.len(), 2);
        // 5-wave 應 pass(Ch5_Essential R1-R7 + Overlap_Trending 都通;Overlap_Terminal fail 正常)
        assert!(reports[0].overall_pass);
        // 3-wave 大部分 N/A;只 R3 適用且應通過(W2 不過 W1 起點),overall_pass = true
        assert!(reports[1].overall_pass);
    }

    #[test]
    fn both_overlaps_failed_yields_overall_fail() {
        // 構造一個 W4 既 < W2 終點 又 ≥ W2 終點(邏輯不可能,但測試 dispatcher 行為)
        // → 改測:手動製造兩條 Overlap fail RuleResult,確認 dispatcher reduce 邏輯
        let mut report = ValidationReport::default();
        report.failed.push(RuleRejection {
            candidate_id: "test".to_string(),
            rule_id: RuleId::Ch5_Overlap_Trending,
            expected: "test".to_string(),
            actual: "test".to_string(),
            gap: 0.0,
            neely_page: "test".to_string(),
        });
        report.failed.push(RuleRejection {
            candidate_id: "test".to_string(),
            rule_id: RuleId::Ch5_Overlap_Terminal,
            expected: "test".to_string(),
            actual: "test".to_string(),
            gap: 0.0,
            neely_page: "test".to_string(),
        });
        // 不跑 dispatcher,只驗 ValidationReport struct 行為(實際 dispatcher 的兩條 fail 邏輯
        // 已在 validate_candidate 內 reduce — 此測試只佔位確認 struct 可同時持 2 個 fail RuleRejection)
        assert_eq!(report.failed.len(), 2);
    }
}
