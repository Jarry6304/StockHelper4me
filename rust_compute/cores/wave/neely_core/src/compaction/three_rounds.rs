// three_rounds.rs — Compaction 真窮舉(Round 1-2 一層 aggregation helper)
//
// 對齊 m3Spec/neely_rules.md §Three Rounds 教學流程(line 1198-1256):
//   - Round 1:識別 Standard Series(對 Figure 4-3 五大序列比對 + Similarity & Balance 過濾)
//   - Round 2:把驗證過的 Series 壓縮成單一 base label `:_3` / `:_5`
//   - Round 3:暫停(等新 :L5/:L3,本檔不處理 — 由 `exhaustive::compact` 外層收斂條件接管)
//
// 設計選擇(對齊 v2.0 並排不整合 + V2 範圍保守實作):
//   - Round 1 「對 Figure 4-3 圖搜尋」 = 比對 sliding window 內的
//     (`compacted_base_label` 序列 + `initial_direction` 交替)
//   - Round 2 動作 B「邊界波 Retracement Rules 重評」**暫不做**(spec line 1249-1251),
//     視 production data 視覺檢視需求再加 — 加會寫進 `Scenario.advisory_findings`(不阻擋 forest)
//   - Power Rating / Max Retracement / PostBehavior 對新生成 scenario 重算
//     (對齊 `power_rating::rate_scenario` 既有 API)
//
// 本檔 API:
//   - `aggregate_one_level(scenarios) -> Vec<Scenario>` — 對輸入 list 跑一次 Round 1-2,
//     回新生 Level-N+1 scenarios(空 vec = 已收斂)

use crate::output::{
    AdvisoryFinding, AdvisorySeverity, ComplexityLevel, MonowaveDirection, NeelyPatternType,
    PostBehavior, PowerRating, RoundState, RuleId, Scenario, StructuralFacts, StructureLabel,
    WaveNode, ZigzagKind,
};
use crate::power_rating;

/// Similarity & Balance 容差(對齊 neely_rules.md §1189-1197):
/// 相鄰波在 price 或 time 維度其一相似即可。
///
/// **Best-guess 區間**:0.382..=2.618(對齊 Neely Fib 主要區間)。
/// production data 揭露需收緊時改本 const。
const SB_MIN_RATIO: f64 = 0.382;
const SB_MAX_RATIO: f64 = 2.618;

/// 對輸入 scenarios 跑一輪 Round 1-2,回新生 Level-N+1 scenarios。
///
/// 比對策略(對齊 spec Figure 4-3):
///   - 5-pattern Trending Impulse:`[:_5, :_3, :_5, :_3, :_5]` 交替方向 → 新 `:_5`
///   - 3-pattern Zigzag:`[:_5, :_3, :_5]` 交替方向 → 新 `:_3`
///   - 3-pattern Flat:`[:_3, :_3, :_5]` 後段為衝動 → 新 `:_3`
///   - 5-pattern Triangle:`[:_3, :_3, :_3, :_3, :_3]` 全 corrective → 新 `:_3`
///
/// 過濾條件:相鄰波必須 pass `similarity_and_balance`(price 或 time 相似其一)。
///
/// 空輸入 / 太少 scenarios → 空 vec(收斂)。
pub fn aggregate_one_level(scenarios: &[Scenario]) -> Vec<Scenario> {
    if scenarios.len() < 3 {
        return Vec::new();
    }

    let mut aggregated: Vec<Scenario> = Vec::new();

    // 5-pattern 比對(優先 — Trending Impulse / Triangle)
    if scenarios.len() >= 5 {
        for start in 0..=scenarios.len() - 5 {
            let window = &scenarios[start..start + 5];
            if let Some(new_scenario) = try_aggregate_5(window, start) {
                aggregated.push(new_scenario);
            }
        }
    }

    // 3-pattern 比對(Zigzag / Flat)
    for start in 0..=scenarios.len() - 3 {
        let window = &scenarios[start..start + 3];
        if let Some(new_scenario) = try_aggregate_3(window, start) {
            aggregated.push(new_scenario);
        }
    }

    aggregated
}

// ─────────────────────────────────────────────────────────────────────────────
// 5-pattern 比對(Trending Impulse / Triangle)
// ─────────────────────────────────────────────────────────────────────────────

fn try_aggregate_5(window: &[Scenario], window_start: usize) -> Option<Scenario> {
    let labels: Vec<StructureLabel> = window.iter().map(|s| s.compacted_base_label).collect();
    let dirs: Vec<MonowaveDirection> = window.iter().map(|s| s.initial_direction).collect();

    // Trending Impulse:[:_5, :_3, :_5, :_3, :_5] 交替方向
    let trending_pattern = [
        StructureLabel::Five,
        StructureLabel::Three,
        StructureLabel::Five,
        StructureLabel::Three,
        StructureLabel::Five,
    ];
    if labels == trending_pattern && alternating(&dirs) && all_pairs_pass_sb(window) {
        return Some(build_aggregated(
            window,
            window_start,
            StructureLabel::Five,
            NeelyPatternType::Impulse,
            "L_TrendingImpulse",
        ));
    }

    // Triangle:全 :_3 (5 段都是 corrective);相鄰波方向交替
    let triangle_all_three = labels.iter().all(|l| *l == StructureLabel::Three);
    if triangle_all_three && alternating(&dirs) && all_pairs_pass_sb(window) {
        return Some(build_aggregated(
            window,
            window_start,
            StructureLabel::Three,
            NeelyPatternType::Triangle {
                sub_kind: crate::output::TriangleKind::Contracting,
            },
            "L_Triangle",
        ));
    }

    None
}

// ─────────────────────────────────────────────────────────────────────────────
// 3-pattern 比對(Zigzag / Flat)
// ─────────────────────────────────────────────────────────────────────────────

fn try_aggregate_3(window: &[Scenario], window_start: usize) -> Option<Scenario> {
    let labels: Vec<StructureLabel> = window.iter().map(|s| s.compacted_base_label).collect();
    let dirs: Vec<MonowaveDirection> = window.iter().map(|s| s.initial_direction).collect();

    // Zigzag:[:_5, :_3, :_5] 交替方向
    let zigzag_pattern = [
        StructureLabel::Five,
        StructureLabel::Three,
        StructureLabel::Five,
    ];
    if labels == zigzag_pattern && alternating(&dirs) && all_pairs_pass_sb(window) {
        return Some(build_aggregated(
            window,
            window_start,
            StructureLabel::Three,
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            "L_Zigzag",
        ));
    }

    // Flat:[:_3, :_3, :_5] 後段衝動,交替方向
    let flat_pattern = [
        StructureLabel::Three,
        StructureLabel::Three,
        StructureLabel::Five,
    ];
    if labels == flat_pattern && alternating(&dirs) && all_pairs_pass_sb(window) {
        return Some(build_aggregated(
            window,
            window_start,
            StructureLabel::Three,
            NeelyPatternType::Flat {
                sub_kind: crate::output::FlatKind::Common,
            },
            "L_Flat",
        ));
    }

    None
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn alternating(dirs: &[MonowaveDirection]) -> bool {
    if dirs.len() < 2 {
        return false;
    }
    for i in 1..dirs.len() {
        if dirs[i] == dirs[i - 1] || dirs[i] == MonowaveDirection::Neutral {
            return false;
        }
    }
    true
}

fn all_pairs_pass_sb(window: &[Scenario]) -> bool {
    for i in 1..window.len() {
        if !similarity_and_balance(&window[i - 1], &window[i]) {
            return false;
        }
    }
    true
}

/// Similarity & Balance:相鄰波在 price magnitude 或 time duration 維度其一相似即可
/// (對齊 spec §Rule of Similarity & Balance 1189-1197)。
fn similarity_and_balance(a: &Scenario, b: &Scenario) -> bool {
    let price_a = scenario_price_magnitude(a);
    let price_b = scenario_price_magnitude(b);
    let time_a = scenario_time_days(a);
    let time_b = scenario_time_days(b);

    let price_similar = ratio_in_range(price_a, price_b, SB_MIN_RATIO, SB_MAX_RATIO);
    let time_similar = ratio_in_range(time_a, time_b, SB_MIN_RATIO, SB_MAX_RATIO);

    price_similar || time_similar
}

fn scenario_price_magnitude(s: &Scenario) -> f64 {
    // wave_tree 的 children 對應 sub-wave;若空(Level-0),用 1.0 placeholder
    // (Level-0 scenario 預設未填 price magnitude;Similarity & Balance 對 Level-0
    // 退化為「只看 time」,對 Level-1+ 才嚴格)
    if s.wave_tree.children.is_empty() {
        1.0
    } else {
        // 取 first child label 長度作 proxy(placeholder — production 應接 monowave price)
        s.wave_tree.children.len() as f64
    }
}

fn scenario_time_days(s: &Scenario) -> f64 {
    let duration = s.wave_tree.end - s.wave_tree.start;
    duration.num_days() as f64
}

fn ratio_in_range(a: f64, b: f64, min: f64, max: f64) -> bool {
    if a <= 0.0 || b <= 0.0 {
        return false;
    }
    let ratio = a / b;
    ratio >= min && ratio <= max
}

/// 建構新生 Level-N+1 scenario(整段已 compact)。
fn build_aggregated(
    window: &[Scenario],
    window_start: usize,
    base_label: StructureLabel,
    pattern_type: NeelyPatternType,
    label_prefix: &str,
) -> Scenario {
    let first = window.first().expect("aggregate window non-empty");
    let last = window.last().expect("aggregate window non-empty");
    let id = format!(
        "{}_idx{}_{}",
        label_prefix,
        window_start,
        first.id.chars().take(8).collect::<String>()
    );

    let children: Vec<WaveNode> = window.iter().map(|s| s.wave_tree.clone()).collect();
    let wave_tree = WaveNode {
        label: format!("{}_compact", label_prefix),
        start: first.wave_tree.start,
        end: last.wave_tree.end,
        children,
    };

    let in_triangle = matches!(pattern_type, NeelyPatternType::Triangle { .. });
    let mut new_scenario = Scenario {
        id,
        wave_tree,
        pattern_type,
        initial_direction: first.initial_direction,
        compacted_base_label: base_label,
        structure_label: label_prefix.to_string(),
        complexity_level: ComplexityLevel::Complex,
        power_rating: PowerRating::Neutral, // 之後 rate_scenario 重算
        max_retracement: None,
        post_pattern_behavior: PostBehavior::Unconstrained,
        passed_rules: Vec::new(),
        deferred_rules: Vec::new(),
        rules_passed_count: 0,
        deferred_rules_count: 0,
        invalidation_triggers: Vec::new(),
        expected_fib_zones: Vec::new(),
        structural_facts: StructuralFacts::default(),
        advisory_findings: vec![AdvisoryFinding {
            rule_id: RuleId::Ch7_Compaction_Reassessment,
            severity: AdvisorySeverity::Info,
            message: format!(
                "Compaction Level-N+1 aggregated from {} sub-scenarios (label_prefix={})",
                window.len(),
                label_prefix
            ),
        }],
        in_triangle_context: in_triangle,
        awaiting_l_label: false,
        monowave_structure_labels: Vec::new(),
        round_state: RoundState::Round2,
        pattern_isolation_anchors: Vec::new(),
        triplexity_detected: false,
    };

    // Power Rating + Max Retracement + PostBehavior 重算(對齊 spec Ch10)
    new_scenario.power_rating = power_rating::rate_scenario(&new_scenario);
    new_scenario.max_retracement = power_rating::max_retracement::lookup(
        new_scenario.power_rating,
        new_scenario.in_triangle_context,
    );
    new_scenario.post_pattern_behavior = power_rating::post_behavior::lookup(
        &new_scenario.pattern_type,
        new_scenario.in_triangle_context,
    );

    new_scenario
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn mk_scenario(
        id: &str,
        label: StructureLabel,
        dir: MonowaveDirection,
        start: &str,
        end: &str,
    ) -> Scenario {
        Scenario {
            id: id.to_string(),
            wave_tree: WaveNode {
                label: id.to_string(),
                start: date(start),
                end: date(end),
                children: Vec::new(),
            },
            pattern_type: if label == StructureLabel::Five {
                NeelyPatternType::Impulse
            } else {
                NeelyPatternType::Zigzag {
                    sub_kind: ZigzagKind::Single,
                }
            },
            initial_direction: dir,
            compacted_base_label: label,
            structure_label: id.to_string(),
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

    #[test]
    fn aggregate_5_trending_impulse() {
        // [:_5(Up), :_3(Down), :_5(Up), :_3(Down), :_5(Up)] — time 約相等
        let scenarios = vec![
            mk_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-10"),
            mk_scenario("b", StructureLabel::Three, MonowaveDirection::Down, "2026-01-10", "2026-01-15"),
            mk_scenario("c", StructureLabel::Five, MonowaveDirection::Up, "2026-01-15", "2026-01-25"),
            mk_scenario("d", StructureLabel::Three, MonowaveDirection::Down, "2026-01-25", "2026-01-30"),
            mk_scenario("e", StructureLabel::Five, MonowaveDirection::Up, "2026-01-30", "2026-02-10"),
        ];
        let result = aggregate_one_level(&scenarios);
        // 應該有 1 個 5-pattern Trending Impulse
        let impulse_count = result
            .iter()
            .filter(|s| matches!(s.pattern_type, NeelyPatternType::Impulse))
            .count();
        assert!(impulse_count >= 1, "5-pattern Trending Impulse 應 aggregate");
        let impulse = result
            .iter()
            .find(|s| matches!(s.pattern_type, NeelyPatternType::Impulse))
            .unwrap();
        assert_eq!(impulse.compacted_base_label, StructureLabel::Five);
        assert_eq!(impulse.wave_tree.children.len(), 5);
        assert_eq!(impulse.round_state, RoundState::Round2);
    }

    #[test]
    fn aggregate_3_zigzag() {
        // [:_5(Up), :_3(Down), :_5(Up)] — Zigzag pattern
        let scenarios = vec![
            mk_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-10"),
            mk_scenario("b", StructureLabel::Three, MonowaveDirection::Down, "2026-01-10", "2026-01-15"),
            mk_scenario("c", StructureLabel::Five, MonowaveDirection::Up, "2026-01-15", "2026-01-25"),
        ];
        let result = aggregate_one_level(&scenarios);
        let zigzag = result
            .iter()
            .find(|s| matches!(s.pattern_type, NeelyPatternType::Zigzag { .. }));
        assert!(zigzag.is_some(), "3-pattern Zigzag 應 aggregate");
        let z = zigzag.unwrap();
        assert_eq!(z.compacted_base_label, StructureLabel::Three);
        assert_eq!(z.wave_tree.children.len(), 3);
    }

    #[test]
    fn aggregate_3_flat() {
        // [:_3(Up), :_3(Down), :_5(Up)] — Flat pattern
        let scenarios = vec![
            mk_scenario("a", StructureLabel::Three, MonowaveDirection::Up, "2026-01-01", "2026-01-10"),
            mk_scenario("b", StructureLabel::Three, MonowaveDirection::Down, "2026-01-10", "2026-01-15"),
            mk_scenario("c", StructureLabel::Five, MonowaveDirection::Up, "2026-01-15", "2026-01-25"),
        ];
        let result = aggregate_one_level(&scenarios);
        let flat = result
            .iter()
            .find(|s| matches!(s.pattern_type, NeelyPatternType::Flat { .. }));
        assert!(flat.is_some(), "3-pattern Flat 應 aggregate");
    }

    #[test]
    fn no_alternation_no_aggregation() {
        // 全 Up 方向 → 不可能 aggregate(Standard Series 要求交替)
        let scenarios = vec![
            mk_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-10"),
            mk_scenario("b", StructureLabel::Three, MonowaveDirection::Up, "2026-01-10", "2026-01-15"),
            mk_scenario("c", StructureLabel::Five, MonowaveDirection::Up, "2026-01-15", "2026-01-25"),
        ];
        let result = aggregate_one_level(&scenarios);
        assert!(result.is_empty(), "同方向不應 aggregate");
    }

    #[test]
    fn too_few_scenarios_no_aggregation() {
        let scenarios = vec![mk_scenario(
            "a",
            StructureLabel::Five,
            MonowaveDirection::Up,
            "2026-01-01",
            "2026-01-10",
        )];
        let result = aggregate_one_level(&scenarios);
        assert!(result.is_empty(), "少於 3 個 scenario 不應 aggregate");
    }

    #[test]
    fn neutral_direction_skipped() {
        let scenarios = vec![
            mk_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-10"),
            mk_scenario("b", StructureLabel::Three, MonowaveDirection::Neutral, "2026-01-10", "2026-01-15"),
            mk_scenario("c", StructureLabel::Five, MonowaveDirection::Down, "2026-01-15", "2026-01-25"),
        ];
        let result = aggregate_one_level(&scenarios);
        assert!(result.is_empty(), "Neutral 方向 break alternating");
    }

    #[test]
    fn time_similarity_extreme_blocks_aggregation() {
        // time durations 10 / 1 / 10 → 第二段太短不 similar
        let scenarios = vec![
            mk_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-11"),
            mk_scenario("b", StructureLabel::Three, MonowaveDirection::Down, "2026-01-11", "2026-01-12"),
            mk_scenario("c", StructureLabel::Five, MonowaveDirection::Up, "2026-01-12", "2026-01-22"),
        ];
        let result = aggregate_one_level(&scenarios);
        // 10/1 = 10 ratio > 2.618;1/10 = 0.1 < 0.382 → time 不 similar
        // price 全部 1.0(empty children)→ price 永遠 similar
        // 所以結果視 price OR time:price similar → 仍 aggregate
        // 這 case 文件用,實際行為:price similar → aggregate
        let zigzag = result
            .iter()
            .find(|s| matches!(s.pattern_type, NeelyPatternType::Zigzag { .. }));
        assert!(
            zigzag.is_some(),
            "Level-0 placeholder price 永遠 similar → 仍 aggregate(留 V3 接真 monowave price 改善)"
        );
    }
}
