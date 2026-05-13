// facts.rs — Fact 產出規則
//
// 對齊 m3Spec/neely_core_architecture.md §15 + cores_overview §6.1.1。
// 從 NeelyCoreOutput 萃取機械式 Fact(禁主觀詞彙)。
//
// **PR-3c-pre 階段(2026-05-13)**:
//   - r2 Diagonal/Bullish 命名改 TerminalImpulse/FavorContinuation(r5 §9.2/9.6)
//   - r2 RuleId::Core(N) → RuleId::Ch5Essential(N)(r5 §9.3)
//   - stage_elapsed_ms 改 stage_timings_ms / peak_memory_mb 改 f64

use crate::output::{
    NeelyPatternType, NeelyCoreOutput, PowerRating, Scenario,
};
use fact_schema::Fact;
use serde_json::json;

/// 從 NeelyCoreOutput 萃取機械式 Fact list。
///
/// **PR-3c-pre 階段**:每 Scenario 一條 + 1 條 forest summary。
pub fn produce(output: &NeelyCoreOutput) -> Vec<Fact> {
    let mut facts = Vec::new();

    for scenario in &output.scenario_forest {
        facts.push(scenario_to_fact(output, scenario));
    }

    // Forest summary fact
    if !output.scenario_forest.is_empty() {
        facts.push(forest_summary_fact(output));
    }

    facts
}

fn scenario_to_fact(output: &NeelyCoreOutput, scenario: &Scenario) -> Fact {
    let pattern_label = pattern_label(&scenario.pattern_type);
    let power_label = power_rating_label(scenario.power_rating);
    let statement = format!(
        "Wave structure: {} from {} to {}, power_rating = {}, rules passed = {}, deferred = {}",
        pattern_label,
        scenario.wave_tree.start,
        scenario.wave_tree.end,
        power_label,
        scenario.rules_passed_count,
        scenario.deferred_rules_count,
    );

    Fact {
        stock_id: output.stock_id.clone(),
        fact_date: output.data_range.end,
        timeframe: output.timeframe,
        source_core: "neely_core".to_string(),
        source_version: "0.7.0".to_string(),
        params_hash: None,
        statement,
        metadata: json!({
            "scenario_id": scenario.id,
            "pattern_type": pattern_label,
            "power_rating": power_label,
            "complexity_level": format!("{:?}", scenario.complexity_level),
            "rules_passed_count": scenario.rules_passed_count,
            "deferred_rules_count": scenario.deferred_rules_count,
            "invalidation_triggers_count": scenario.invalidation_triggers.len(),
            "expected_fib_zones_count": scenario.expected_fib_zones.len(),
        }),
    }
}

fn forest_summary_fact(output: &NeelyCoreOutput) -> Fact {
    let statement = format!(
        "Wave forest: {} scenario(s), {} monowave(s), candidate_count = {}, validator_pass = {}, validator_reject = {}",
        output.scenario_forest.len(),
        output.monowave_series.len(),
        output.diagnostics.candidate_count,
        output.diagnostics.validator_pass_count,
        output.diagnostics.validator_reject_count,
    );

    Fact {
        stock_id: output.stock_id.clone(),
        fact_date: output.data_range.end,
        timeframe: output.timeframe,
        source_core: "neely_core".to_string(),
        source_version: "0.7.0".to_string(),
        params_hash: None,
        statement,
        metadata: json!({
            "kind": "forest_summary",
            "forest_size": output.scenario_forest.len(),
            "monowave_count": output.monowave_series.len(),
            "candidate_count": output.diagnostics.candidate_count,
            "validator_pass_count": output.diagnostics.validator_pass_count,
            "validator_reject_count": output.diagnostics.validator_reject_count,
            "overflow_triggered": output.diagnostics.overflow_triggered,
            "compaction_paths": output.diagnostics.compaction_paths,
        }),
    }
}

fn pattern_label(p: &NeelyPatternType) -> String {
    match p {
        NeelyPatternType::Impulse => "Impulse".to_string(),
        NeelyPatternType::TerminalImpulse => "TerminalImpulse".to_string(),
        NeelyPatternType::RunningCorrection => "RunningCorrection".to_string(),
        NeelyPatternType::Zigzag { sub_kind } => format!("Zigzag({:?})", sub_kind),
        NeelyPatternType::Flat { sub_kind } => format!("Flat({:?})", sub_kind),
        NeelyPatternType::Triangle { sub_kind } => format!("Triangle({:?})", sub_kind),
        NeelyPatternType::Combination { sub_kinds } => format!("Combination({:?})", sub_kinds),
    }
}

fn power_rating_label(p: PowerRating) -> &'static str {
    match p {
        PowerRating::StronglyFavorContinuation => "StronglyFavorContinuation",
        PowerRating::ModeratelyFavorContinuation => "ModeratelyFavorContinuation",
        PowerRating::SlightlyFavorContinuation => "SlightlyFavorContinuation",
        PowerRating::Neutral => "Neutral",
        PowerRating::SlightlyAgainstContinuation => "SlightlyAgainstContinuation",
        PowerRating::ModeratelyAgainstContinuation => "ModeratelyAgainstContinuation",
        PowerRating::StronglyAgainstContinuation => "StronglyAgainstContinuation",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;
    use chrono::NaiveDate;
    use fact_schema::Timeframe;
    use std::collections::HashMap;

    fn make_output(num_scenarios: usize) -> NeelyCoreOutput {
        let date = NaiveDate::parse_from_str("2026-01-05", "%Y-%m-%d").unwrap();
        let scenarios = (0..num_scenarios)
            .map(|i| Scenario {
                id: format!("s{}", i),
                wave_tree: WaveNode {
                    label: "test".to_string(),
                    start: date,
                    end: date,
                    children: Vec::new(),
                },
                pattern_type: NeelyPatternType::Impulse,
                structure_label: "test".to_string(),
                complexity_level: ComplexityLevel::Simple,
                power_rating: PowerRating::ModeratelyFavorContinuation,
                max_retracement: 0.0,
                post_pattern_behavior: PostBehavior::Unconstrained,
                passed_rules: vec![
                    RuleId::Ch5Essential(1),
                    RuleId::Ch5Essential(2),
                    RuleId::Ch5Essential(3),
                ],
                deferred_rules: vec![RuleId::Ch3PreConstructive {
                    rule: 4, condition: 'a', category: None, sub_rule_index: None,
                }],
                rules_passed_count: 3,
                deferred_rules_count: 1,
                invalidation_triggers: Vec::new(),
                expected_fib_zones: Vec::new(),
                structural_facts: StructuralFacts::default(),
            awaiting_l_label: false,
            })
            .collect();

        NeelyCoreOutput {
            stock_id: "2330".to_string(),
            timeframe: Timeframe::Daily,
            data_range: TimeRange {
                start: date,
                end: date,
            },
            scenario_forest: scenarios,
            monowave_series: Vec::new(),
            diagnostics: NeelyDiagnostics {
                monowave_count: 0,
                candidate_count: num_scenarios,
                validator_pass_count: num_scenarios,
                validator_reject_count: 0,
                rejections: Vec::new(),
                forest_size: num_scenarios,
                compaction_paths: num_scenarios,
                overflow_triggered: false,
                compaction_timeout: false,
                stage_timings_ms: HashMap::new(),
                elapsed_ms: 0,
                peak_memory_mb: 0.0,
                atr_dual_mode_diff: None,
            },
            rule_book_references: Vec::new(),
            insufficient_data: false,
            round3_pause: None,
        }
    }

    #[test]
    fn empty_forest_yields_no_facts() {
        let out = make_output(0);
        assert!(produce(&out).is_empty());
    }

    #[test]
    fn single_scenario_yields_two_facts() {
        // 1 scenario fact + 1 forest summary fact
        let out = make_output(1);
        let facts = produce(&out);
        assert_eq!(facts.len(), 2);
        assert!(facts[0].statement.contains("Impulse"));
        assert!(facts[0].statement.contains("ModeratelyFavorContinuation"));
        assert!(facts[1].statement.contains("forest"));
    }

    #[test]
    fn fact_metadata_has_pattern_info() {
        let out = make_output(1);
        let facts = produce(&out);
        let scenario_fact = &facts[0];
        let meta = &scenario_fact.metadata;
        assert_eq!(meta["pattern_type"], "Impulse");
        assert_eq!(meta["power_rating"], "ModeratelyFavorContinuation");
        assert_eq!(meta["rules_passed_count"], 3);
    }

    #[test]
    fn fact_source_core_is_neely() {
        let out = make_output(1);
        let facts = produce(&out);
        for f in &facts {
            assert_eq!(f.source_core, "neely_core");
            assert_eq!(f.stock_id, "2330");
        }
    }
}
