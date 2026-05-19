// pre_constructive — Stage 0:Pre-Constructive Rules of Logic
//
// 對齊 m3Spec/neely_rules.md §Pre-Constructive Rules of Logic(348-1062 行)
//       + m3Spec/neely_core_architecture.md §7.1 Stage 0
//
// **Phase 2 PR**(2026-05-13)— Ch3 Pre-Constructive Logic ~200-245 branches 落地:
//   - 對每個 monowave m1(index i),建構 m(-3)..m5 9-frame context
//   - 量 m2/m1 → 決定 Rule(1-7)
//   - 量 m0/m1 → 決定 Condition(該 Rule 下的 a-f)
//   - 若是 Rule 4,還要量 m3/m2 → 決定 Category(i/ii/iii)
//   - 跑該 (Rule, Condition[, Category]) 的 if-else cascade,逐 branch add/drop
//     Structure Label candidates
//   - 結果存進 classified[i].structure_label_candidates
//
// **r5 容差**(architecture §4.2 三檔表):
//   - 一般近似(approximately):±10%
//   - Fibonacci 比率:±4%
//   - Triangle 同度數腿等價:±5%(本 module 不用 — 三角規則層用)
//
// **缺資料項目**(本 PR best-guess,Phase 2 後校準):
//   - m0/m1/m2 是否含 > 3 sub-monowaves(polywave 偵測)→ 預設 false(走 (B) 分支)
//     [留 Group 1 polywave nested feedback loop 補完]
//   - ~~m1 端點被 m2 突破 → 需 OHLC intraday extreme reference,Phase 2 placeholder false~~
//     **v4.6 G3.1(2026-05-19)已實作**:用 m2.bar_indices 在 bars slice 走
//     intraday high/low extremum,對齊 spec line 247-249
//   - 部分子規則涉及「快/慢回測」+「market returns to wave start」幾何判斷,
//     已以「duration 比較 + retracement_pct + 2-4 line breach」近似實作
//   - 細項對齊 m3Spec/neely_rules.md 1042-1062 行「Pre-Constructive Logic 細部技術備註」

use crate::monowave::ClassifiedMonowave;
use crate::output::{OhlcvBar, Scenario, StructureLabelCandidate};

pub mod context;
pub mod predicates;
/// v4.1 P1.1 #5:Appendix A.3 `is_fifth_of_fifth_extension` 共通函式 — 從
/// rule_3.rs / rule_4.rs 兩處重複實作抽出。
mod fifth_of_fifth_detector;
mod rule_1;
mod rule_2;
mod rule_3;
mod rule_4;
mod rule_5;
mod rule_6;
mod rule_7;

use context::MonowaveContext;
use predicates::{mag_ratio, FIB_1000, FIB_1618, FIB_2618, FIB_382, FIB_618, FIB_TOL};

/// Stage 0 入口:對 classified_monowaves 套用 Pre-Constructive Logic。
///
/// 依序處理每個 m1(i = 0..classified.len()),計算 Structure Label candidates
/// 後存回 `classified[i].structure_label_candidates`。
///
/// 順序遍歷支援「m0 Structure 包含 X」query(早於 m1 處理時 m0 candidates 已填好)。
///
/// **v4.6 G3.1**:`bars` 參數對齊 `MonowaveContext::build`,供 intraday-aware
/// predicates 用(e.g. m1_endpoint_broken_by_m2 在 m2 bar range 找 extrema)。
pub fn run(classified: &mut [ClassifiedMonowave], bars: &[OhlcvBar]) {
    for i in 0..classified.len() {
        let cands = compute_candidates_at(classified, bars, i);
        classified[i].structure_label_candidates = cands;
    }
}

/// **v4.7.2 G1.2(2026-05-19)**:從 Compaction forest 反查 polywave 規模,
/// 標記每個 base classified monowave 的 `polywave_size`。
///
/// 邏輯:
///   - 對 forest 內每個 Level-N+ scenario(wave_tree.children.len() > 0):
///     - 對該 scenario 範圍內的每個 base monowave classified[i]
///       (classified[i].monowave.start_date >= scenario.wave_tree.start AND
///        classified[i].monowave.end_date <= scenario.wave_tree.end):
///       - polywave_size = max(current, scenario.wave_tree.children.len())
///
/// 對齊 spec line 1042-1062「m_N 含 > 3 sub-monowaves」(`polywave_size > 3`
/// 由 `is_polywave` helper 判定);Compaction Three Rounds 將相鄰多個 monowaves
/// aggregated 為一個 wave_tree.children entry,當 children 數 > 3 時對應 region
/// 即視為「polywave region」。
///
/// **2-pass 設計**(對齊 plan §G1.2):
///   - Stage 0 Pre-Constructive Pass 1 → 所有 polywave_size = 0 → rule 1/4/5/6/7
///     polywave checks 全 false(走 (B) 分支,= v4.6 行為)
///   - Stage 8 Compaction 跑完 → 呼叫此 fn 設 polywave_size
///   - Stage 0 Pre-Constructive Pass 2 → polywave checks 反查真實值 → 可走 (A) 分支
pub fn populate_polywave_sizes(
    classified: &mut [ClassifiedMonowave],
    forest: &[Scenario],
) {
    for scenario in forest {
        let n_children = scenario.wave_tree.children.len();
        if n_children == 0 {
            continue; // Level-0 base scenario,無 polywave 資訊
        }
        let start = scenario.wave_tree.start;
        let end = scenario.wave_tree.end;
        for c in classified.iter_mut() {
            if c.monowave.start_date >= start && c.monowave.end_date <= end {
                if n_children > c.polywave_size {
                    c.polywave_size = n_children;
                }
            }
        }
    }
}

/// 對 classified[i] 計算 structure label candidates(不修改 classified)。
fn compute_candidates_at(
    classified: &[ClassifiedMonowave],
    bars: &[OhlcvBar],
    i: usize,
) -> Vec<StructureLabelCandidate> {
    let Some(ctx) = MonowaveContext::build(classified, bars, i) else {
        return Vec::new();
    };

    // m2 不存在 → 無法決定 Rule(m1 為 series 末段)→ 給予 `:?5`/`:?3` UnknownX 作為 placeholder
    let Some(m2) = ctx.m2 else {
        return Vec::new(); // 末段 monowave 跳過(對齊 spec「需 m2 確認」)
    };

    let m2_ratio = mag_ratio(m2, ctx.m1);
    let mut cands = Vec::new();

    // 容差:Rule 3 採 m2/m1 = 61.8% ± 4%
    let r3_lo = FIB_618 * (1.0 - FIB_TOL);
    let r3_hi = FIB_618 * (1.0 + FIB_TOL);

    if m2_ratio < FIB_382 {
        rule_1::run(&ctx, &mut cands);
    } else if (r3_lo..=r3_hi).contains(&m2_ratio) {
        rule_3::run(&ctx, &mut cands);
    } else if m2_ratio < FIB_618 {
        rule_2::run(&ctx, &mut cands);
    } else if m2_ratio < FIB_1000 {
        rule_4::run(&ctx, &mut cands);
    } else if m2_ratio < FIB_1618 {
        rule_5::run(&ctx, &mut cands);
    } else if m2_ratio <= FIB_2618 {
        rule_6::run(&ctx, &mut cands);
    } else {
        rule_7::run(&ctx, &mut cands);
    }

    cands
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::{Monowave, MonowaveDirection, StructureLabel};
    use chrono::NaiveDate;

    fn cmw(start_p: f64, end_p: f64, dir: MonowaveDirection, dur: usize) -> ClassifiedMonowave {
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
                start_price: start_p,
                end_price: end_p,
                direction: dir,
                bar_indices: (0, 0),
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: (end_p - start_p).abs(),
                duration_bars: dur,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
            polywave_size: 0,
        }
    }

    #[test]
    fn run_empty_classified_no_panic() {
        let mut classified: Vec<ClassifiedMonowave> = Vec::new();
        run(&mut classified, &[]);
        assert!(classified.is_empty());
    }

    #[test]
    fn run_populates_candidates() {
        // 5-bar zigzag,m1 是 index 1(direction Down,mag 5,dur 5)
        // m0 = index 0(Up, mag 10);m2 = index 2(Up, mag 12)
        // m2/m1 = 12/5 = 2.4 → Rule 6(161.8-261.8%)
        let mut classified = vec![
            cmw(100.0, 110.0, MonowaveDirection::Up, 5),    // m0(對 i=1)
            cmw(110.0, 105.0, MonowaveDirection::Down, 5),  // m1(i=1)
            cmw(105.0, 117.0, MonowaveDirection::Up, 5),    // m2
            cmw(117.0, 112.0, MonowaveDirection::Down, 3),  // m3
        ];
        run(&mut classified, &[]);
        // m1(i=1)應至少有 1 個候選
        assert!(
            !classified[1].structure_label_candidates.is_empty(),
            "m1 應產生 structure label candidates,實際為空"
        );
    }

    #[test]
    fn run_rule_1_cond_1d_emits_only_five() {
        // 構造 m2/m1 < 38.2% AND m0/m1 > 161.8% 場景
        let mut classified = vec![
            cmw(0.0, 0.0, MonowaveDirection::Up, 1),       // m_minus_1
            cmw(100.0, 80.0, MonowaveDirection::Down, 5),  // m0 (mag 20)
            cmw(80.0, 90.0, MonowaveDirection::Up, 5),     // m1 (mag 10, m0/m1=2.0)
            cmw(90.0, 88.0, MonowaveDirection::Down, 2),   // m2 (mag 2, m2/m1=0.2)
        ];
        run(&mut classified, &[]);
        // m1(i=2)走 Rule 1 Cond 1d → 僅 :5
        let cands = &classified[2].structure_label_candidates;
        assert_eq!(cands.len(), 1);
        assert!(matches!(cands[0].label, StructureLabel::Five));
    }

    // v4.7.2 G1.2 populate_polywave_sizes tests --------------------------

    fn make_scenario_polywave(
        start_date: chrono::NaiveDate,
        end_date: chrono::NaiveDate,
        n_children: usize,
    ) -> crate::output::Scenario {
        use crate::output::{
            ComplexityLevel, NeelyPatternType, PostBehavior, PowerRating, RoundState,
            StructuralFacts, WaveNode,
        };
        let children: Vec<WaveNode> = (0..n_children)
            .map(|i| WaveNode {
                label: format!("c{}", i),
                start: start_date,
                end: end_date,
                children: Vec::new(),
            })
            .collect();
        crate::output::Scenario {
            id: "test".to_string(),
            wave_tree: WaveNode {
                label: "agg".to_string(),
                start: start_date,
                end: end_date,
                children,
            },
            pattern_type: NeelyPatternType::Impulse,
            initial_direction: MonowaveDirection::Up,
            compacted_base_label: StructureLabel::Five,
            structure_label: "agg".to_string(),
            complexity_level: ComplexityLevel::Simple,
            power_rating: PowerRating::Neutral,
            max_retracement: None,
            post_pattern_behavior: PostBehavior::Unconstrained,
            passed_rules: Vec::new(),
            deferred_rules: Vec::new(),
            rules_passed_count: 0,
            deferred_rules_count: 0,
            invalidation_triggers: Vec::new(),
            expected_fib_zones: Vec::new(),
            structural_facts: StructuralFacts::default(),
            advisory_findings: Vec::new(),
            in_triangle_context: false,
            awaiting_l_label: false,
            monowave_structure_labels: Vec::new(),
            round_state: RoundState::Round1,
            pattern_isolation_anchors: Vec::new(),
            triplexity_detected: false,
        }
    }

    fn cmw_at_dates(
        start_date: chrono::NaiveDate,
        end_date: chrono::NaiveDate,
    ) -> ClassifiedMonowave {
        ClassifiedMonowave {
            monowave: Monowave {
                start_date,
                end_date,
                start_price: 100.0,
                end_price: 110.0,
                direction: MonowaveDirection::Up,
                bar_indices: (0, 0),
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: 10.0,
                duration_bars: 5,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
            polywave_size: 0,
        }
    }

    #[test]
    fn populate_polywave_sizes_marks_covered_classified() {
        // 3 classified monowaves covering Jan 1-15
        let mut classified = vec![
            cmw_at_dates(
                chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
            ),
            cmw_at_dates(
                chrono::NaiveDate::from_ymd_opt(2026, 1, 6).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 1, 10).unwrap(),
            ),
            cmw_at_dates(
                chrono::NaiveDate::from_ymd_opt(2026, 1, 11).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            ),
        ];
        // 1 Level-N scenario with 5 children covering Jan 1-15
        let forest = vec![make_scenario_polywave(
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(),
            5,
        )];
        populate_polywave_sizes(&mut classified, &forest);
        // 3 base monowaves should all be marked with polywave_size=5
        for c in &classified {
            assert_eq!(c.polywave_size, 5, "covered base monowave should be marked");
        }
    }

    #[test]
    fn populate_polywave_sizes_skips_level_0_scenarios() {
        let mut classified = vec![cmw_at_dates(
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
        )];
        // Level-0 scenario (no children)
        let forest = vec![make_scenario_polywave(
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
            0,
        )];
        populate_polywave_sizes(&mut classified, &forest);
        assert_eq!(classified[0].polywave_size, 0, "Level-0 不應寫入 polywave_size");
    }

    #[test]
    fn populate_polywave_sizes_keeps_max_when_multiple_levels() {
        let mut classified = vec![cmw_at_dates(
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
        )];
        // 兩個 scenarios:Level-1(3 children)和 Level-2(5 children)
        let forest = vec![
            make_scenario_polywave(
                chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
                3,
            ),
            make_scenario_polywave(
                chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
                5,
            ),
        ];
        populate_polywave_sizes(&mut classified, &forest);
        assert_eq!(classified[0].polywave_size, 5, "應取 max children count");
    }

    #[test]
    fn populate_polywave_sizes_does_not_mark_out_of_range_base() {
        let mut classified = vec![cmw_at_dates(
            chrono::NaiveDate::from_ymd_opt(2026, 2, 1).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 2, 5).unwrap(),
        )];
        // Scenario range 跟 base monowave 不重疊
        let forest = vec![make_scenario_polywave(
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
            5,
        )];
        populate_polywave_sizes(&mut classified, &forest);
        assert_eq!(classified[0].polywave_size, 0, "範圍外不應 mark");
    }

    #[test]
    fn is_polywave_threshold_check() {
        use super::predicates::is_polywave;
        let mut c = cmw_at_dates(
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap(),
        );
        assert!(!is_polywave(&c), "polywave_size=0 → false");
        c.polywave_size = 3;
        assert!(!is_polywave(&c), "polywave_size=3 不算 polywave (> 3 才算)");
        c.polywave_size = 4;
        assert!(is_polywave(&c), "polywave_size=4 → true");
    }
}
