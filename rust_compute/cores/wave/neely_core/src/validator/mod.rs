// validator — Stage 4:Validator R1-R7 / F1-F2 / Z1-Z4 / T1-T10 / W1-W2
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 4 / §十(規則組)。
//
// 子模組:
//   - core_rules.rs   — R1-R7(通用核心規則)
//   - flat_rules.rs   — F1-F2(Flat 子規則)
//   - zigzag_rules.rs — Z1-Z4(Zigzag 子規則)
//   - triangle_rules.rs — T1-T10(Triangle 子規則)
//   - wave_rules.rs   — W1-W2(通用波浪規則)
//
// 容差規範(§10.4):相對 ±4%(寫死)+ Waterfall Effect ±5% 例外(寫死)
// — 不可外部化(§4.4 / §6.6)
//
// 規則執行順序(§10.2):
//   candidate
//      ↓ R1-R7 全部須過
//   通過 candidate → 套子規則(F/Z/T,依 candidate pattern 假設)
//      ↓ W1-W2 全部須過
//   通過 → ValidationReport.overall_pass = true
//   不通過 → 寫 RuleRejection 附 rule_id / expected / actual / gap / neely_page
//
// **M3 PR-3b 階段**:
//   - R1-R3 完整實作(Elliott Wave 教科書通用規則,跨派系一致性高)
//   - R4-R7 + F1-F2 + Z1-Z4 + T1-T10 + W1-W2 全部回 Deferred
//     (具體門檻 oldm2Spec/ §10.1 寫「P0 開發時逐條建檔」沒列細節,
//     等 user 在 m3Spec/ 寫最新 neely_core spec 後 batch 補)

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

/// 對單一 candidate 跑完所有 25 條規則的彙總報告。
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub candidate_id: String,
    pub passed: Vec<RuleId>,
    pub failed: Vec<RuleRejection>,
    pub deferred: Vec<RuleId>,
    pub not_applicable: Vec<RuleId>,
    /// 整體判定:任一 Fail → false;任一 Deferred 仍可 true(對齊 §10.3 deferred 暫時通過)
    pub overall_pass: bool,
}

/// 對單一 candidate 跑完整 25 條規則,回傳彙整報告。
///
/// 邏輯:
///   1. 跑 R1-R7(core_rules,通用核心)
///   2. 跑 F1-F2 / Z1-Z4 / T1-T10(子規則,目前全 Deferred)
///   3. 跑 W1-W2(通用波浪)
///   4. 任一 Fail → overall_pass = false
pub fn validate_candidate(
    candidate: &WaveCandidate,
    classified: &[ClassifiedMonowave],
) -> ValidationReport {
    let mut report = ValidationReport {
        candidate_id: candidate.id.clone(),
        ..Default::default()
    };

    // 收集 25 條規則結果
    let mut results: Vec<RuleResult> = Vec::with_capacity(25);
    results.extend(core_rules::run(candidate, classified));
    results.extend(flat_rules::run(candidate, classified));
    results.extend(zigzag_rules::run(candidate, classified));
    results.extend(triangle_rules::run(candidate, classified));
    results.extend(wave_rules::run(candidate, classified));

    let mut has_fail = false;
    for result in results {
        match result {
            RuleResult::Pass => {
                // R1-R7 / W1-W2 等 universal 規則 pass 沒記錄 RuleId(設計假設:有實作的規則才會 emit Pass);
                // 留 PR-4 補完整 Pass 紀錄(目前 passed 只記 R1/R2/R3 的具體 RuleId 由 core_rules 決定)
            }
            RuleResult::Fail(rej) => {
                has_fail = true;
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

    report.overall_pass = !has_fail;
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
        }
    }

    fn make_5wave_impulse_up() -> Vec<ClassifiedMonowave> {
        // W1 100→110 / W2 110→104 / W3 104→125 / W4 125→118 / W5 118→132
        // R1: W2 endpoint 104 > W1 start 100 ✓(未完全回測)
        // R2: W3 magnitude 21 > min(W1=10, W5=14) = 10 ✓(W3 不最短)
        // R3: W4 終點 118 > W1 終點 110 ✓(W4 不重疊 W1)
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
            "well-formed 5-wave Up impulse 應 overall_pass = true,failed = {:?}",
            report.failed
        );
        assert!(report.failed.is_empty());
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
        // 5-wave 應 pass(R1-R3 都通,4-25 條 deferred)
        assert!(reports[0].overall_pass);
        // 3-wave 沒 W3/W4/W5 → R2/R3 N/A,應仍 pass(因 R1 通過 + 其他 NotApplicable)
        assert!(reports[1].overall_pass);
    }
}
