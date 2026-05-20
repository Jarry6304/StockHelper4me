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
    AdvisoryFinding, AdvisorySeverity, ComplexityLevel, Monowave, MonowaveDirection,
    NeelyPatternType, PostBehavior, PowerRating, RoundState, RuleId, Scenario, StructuralFacts,
    StructureLabel, WaveNode, ZigzagKind,
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
pub fn aggregate_one_level(scenarios: &[Scenario], monowaves: &[Monowave]) -> Vec<Scenario> {
    if scenarios.len() < 3 {
        return Vec::new();
    }

    let mut aggregated: Vec<Scenario> = Vec::new();

    // 5-pattern 比對(優先 — Trending Impulse / Triangle)
    if scenarios.len() >= 5 {
        for start in 0..=scenarios.len() - 5 {
            let window = &scenarios[start..start + 5];
            if let Some(new_scenario) = try_aggregate_5(window, start, monowaves) {
                aggregated.push(new_scenario);
            }
        }
    }

    // 3-pattern 比對(Zigzag / Flat)
    for start in 0..=scenarios.len() - 3 {
        let window = &scenarios[start..start + 3];
        if let Some(new_scenario) = try_aggregate_3(window, start, monowaves) {
            aggregated.push(new_scenario);
        }
    }

    // 7-pattern 比對(Double-* Combination,P3 Combination 上游補完)
    if scenarios.len() >= 7 {
        for start in 0..=scenarios.len() - 7 {
            let window = &scenarios[start..start + 7];
            if let Some(new_scenario) = try_aggregate_7(window, start, monowaves) {
                aggregated.push(new_scenario);
            }
        }
    }

    // 11-pattern 比對(Triple-* Combination,P3)
    if scenarios.len() >= 11 {
        for start in 0..=scenarios.len() - 11 {
            let window = &scenarios[start..start + 11];
            if let Some(new_scenario) = try_aggregate_11(window, start, monowaves) {
                aggregated.push(new_scenario);
            }
        }
    }

    aggregated
}

// ─────────────────────────────────────────────────────────────────────────────
// 5-pattern 比對(Trending Impulse / Triangle)
// ─────────────────────────────────────────────────────────────────────────────

fn try_aggregate_5(
    window: &[Scenario],
    window_start: usize,
    monowaves: &[Monowave],
) -> Option<Scenario> {
    // v4.8 G1.3 partial Stage 3-4 rerun:邊界波 retracement 超極端 Fib² 範圍 → reject
    if boundary_retracement_extreme(window, monowaves) {
        return None;
    }

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
    if labels == trending_pattern && alternating(&dirs) && all_pairs_pass_sb(window, monowaves) {
        return Some(build_aggregated(
            window,
            window_start,
            StructureLabel::Five,
            NeelyPatternType::Impulse,
            "L_TrendingImpulse",
            monowaves,
        ));
    }

    // Triangle:全 :_3 (5 段都是 corrective);相鄰波方向交替
    let triangle_all_three = labels.iter().all(|l| *l == StructureLabel::Three);
    if triangle_all_three && alternating(&dirs) && all_pairs_pass_sb(window, monowaves) {
        return Some(build_aggregated(
            window,
            window_start,
            StructureLabel::Three,
            NeelyPatternType::Triangle {
                sub_kind: crate::output::TriangleKind::Contracting,
            },
            "L_Triangle",
            monowaves,
        ));
    }

    None
}

// ─────────────────────────────────────────────────────────────────────────────
// 3-pattern 比對(Zigzag / Flat)
// ─────────────────────────────────────────────────────────────────────────────

fn try_aggregate_3(
    window: &[Scenario],
    window_start: usize,
    monowaves: &[Monowave],
) -> Option<Scenario> {
    // v4.8 G1.3 partial Stage 3-4 rerun:邊界波 retracement 超極端 Fib² 範圍 → reject
    if boundary_retracement_extreme(window, monowaves) {
        return None;
    }

    let labels: Vec<StructureLabel> = window.iter().map(|s| s.compacted_base_label).collect();
    let dirs: Vec<MonowaveDirection> = window.iter().map(|s| s.initial_direction).collect();

    // Zigzag:[:_5, :_3, :_5] 交替方向
    let zigzag_pattern = [
        StructureLabel::Five,
        StructureLabel::Three,
        StructureLabel::Five,
    ];
    if labels == zigzag_pattern && alternating(&dirs) && all_pairs_pass_sb(window, monowaves) {
        return Some(build_aggregated(
            window,
            window_start,
            StructureLabel::Three,
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            "L_Zigzag",
            monowaves,
        ));
    }

    // Flat:[:_3, :_3, :_5] 後段衝動,交替方向
    let flat_pattern = [
        StructureLabel::Three,
        StructureLabel::Three,
        StructureLabel::Five,
    ];
    if labels == flat_pattern && alternating(&dirs) && all_pairs_pass_sb(window, monowaves) {
        return Some(build_aggregated(
            window,
            window_start,
            StructureLabel::Three,
            NeelyPatternType::Flat {
                sub_kind: crate::output::FlatKind::Common,
            },
            "L_Flat",
            monowaves,
        ));
    }

    None
}

// ─────────────────────────────────────────────────────────────────────────────
// 7 / 11-pattern 比對(Double-* / Triple-* Combination,P3 Combination 上游補完)
// ─────────────────────────────────────────────────────────────────────────────

/// 7-scenario window = sub_a(3)+ x(1)+ sub_b(3);全 corrective(:_3)交替 → Double Combination。
/// 對齊 classifier `classify_7wave_combination` 的 3+1+3 結構,差別在此 aggregate 的是
/// 已分類的 Level-N scenarios(非 monowaves)。CombinationKind 取通用 DoubleThree,
/// 細分留 P0 Gate#3 校準。
fn try_aggregate_7(
    window: &[Scenario],
    window_start: usize,
    monowaves: &[Monowave],
) -> Option<Scenario> {
    if boundary_retracement_extreme(window, monowaves) {
        return None;
    }
    let labels: Vec<StructureLabel> = window.iter().map(|s| s.compacted_base_label).collect();
    let dirs: Vec<MonowaveDirection> = window.iter().map(|s| s.initial_direction).collect();

    let all_three = labels.iter().all(|l| *l == StructureLabel::Three);
    if all_three && alternating(&dirs) && all_pairs_pass_sb(window, monowaves) {
        return Some(build_aggregated(
            window,
            window_start,
            StructureLabel::Three,
            NeelyPatternType::Combination {
                sub_kinds: vec![crate::output::CombinationKind::DoubleThree],
            },
            "L_DoubleCombination",
            monowaves,
        ));
    }
    None
}

/// 11-scenario window = sub_a(3)+ x1(1)+ sub_b(3)+ x2(1)+ sub_c(3);全 corrective
/// 交替 → Triple Combination。CombinationKind 取通用 TripleThree,細分留 P0 Gate#3。
fn try_aggregate_11(
    window: &[Scenario],
    window_start: usize,
    monowaves: &[Monowave],
) -> Option<Scenario> {
    if boundary_retracement_extreme(window, monowaves) {
        return None;
    }
    let labels: Vec<StructureLabel> = window.iter().map(|s| s.compacted_base_label).collect();
    let dirs: Vec<MonowaveDirection> = window.iter().map(|s| s.initial_direction).collect();

    let all_three = labels.iter().all(|l| *l == StructureLabel::Three);
    if all_three && alternating(&dirs) && all_pairs_pass_sb(window, monowaves) {
        return Some(build_aggregated(
            window,
            window_start,
            StructureLabel::Three,
            NeelyPatternType::Combination {
                sub_kinds: vec![crate::output::CombinationKind::TripleThree],
            },
            "L_TripleCombination",
            monowaves,
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

fn all_pairs_pass_sb(window: &[Scenario], monowaves: &[Monowave]) -> bool {
    for i in 1..window.len() {
        if !similarity_and_balance(&window[i - 1], &window[i], monowaves) {
            return false;
        }
    }
    true
}

/// Similarity & Balance:相鄰波在 price magnitude 或 time duration 維度其一相似即可
/// (對齊 spec §Rule of Similarity & Balance 1189-1197)。
///
/// **v4.7.1 G1.1**:`scenario_price_magnitude` 改 `Option<f64>` —
/// 無 monowave 資料時 price 維度 short-circuit false(原 children.len() fallback
/// 導致 Level-0 placeholder 永遠 similar,違反「price 不可用時純依賴 time」spec)。
fn similarity_and_balance(a: &Scenario, b: &Scenario, monowaves: &[Monowave]) -> bool {
    let price_a = scenario_price_magnitude(a, monowaves);
    let price_b = scenario_price_magnitude(b, monowaves);
    let time_a = scenario_time_days(a);
    let time_b = scenario_time_days(b);

    let price_similar = match (price_a, price_b) {
        (Some(pa), Some(pb)) => ratio_in_range(pa, pb, SB_MIN_RATIO, SB_MAX_RATIO),
        _ => false, // 無 monowave 反查 → price 維度不可用,等 time 判定
    };
    let time_similar = ratio_in_range(time_a, time_b, SB_MIN_RATIO, SB_MAX_RATIO);

    price_similar || time_similar
}

/// 從 scenario.wave_tree.start / end 日期反查 monowaves,取真實 `|end_price - start_price|`。
///
/// **v4.7.1 G1.1(2026-05-19)**:改 `Option<f64>` 返回 — 移除 children.len() fallback
/// (原 v4.4a 引入,Level-0 為 1.0 / Level-N 為 children 數,違反 spec line 204-213
/// 「Compaction 必須依賴實際 monowave price 比對」)。Caller 走 None case 退到純 time
/// 維度判定,對齊 NEoWave 設計精神。
fn scenario_price_magnitude(s: &Scenario, monowaves: &[Monowave]) -> Option<f64> {
    let start_price = find_price_at_date(s.wave_tree.start, monowaves, /*use_end=*/ false)?;
    let end_price = find_price_at_date(s.wave_tree.end, monowaves, /*use_end=*/ true)?;
    let mag = (end_price - start_price).abs();
    if mag > 1e-9 {
        Some(mag)
    } else {
        None
    }
}

/// 從 monowaves 列表反查指定 date 對應的 price。
///
/// 邏輯:
/// - 找 monowave.start_date == date → 回 start_price
/// - 否則找 monowave.end_date == date → 回 end_price
/// - 否則 None
/// - `use_end`:當兩 monowave 都符合 date 時的偏好(start time 取 start_price / end time 取 end_price)
fn find_price_at_date(
    date: chrono::NaiveDate,
    monowaves: &[Monowave],
    use_end: bool,
) -> Option<f64> {
    // 先試精確匹配 start_date / end_date
    for mw in monowaves {
        if !use_end && mw.start_date == date {
            return Some(mw.start_price);
        }
        if use_end && mw.end_date == date {
            return Some(mw.end_price);
        }
    }
    // 再試另一方向(date 可能匹配相鄰 monowave 的 end / start)
    for mw in monowaves {
        if !use_end && mw.end_date == date {
            return Some(mw.end_price);
        }
        if use_end && mw.start_date == date {
            return Some(mw.start_price);
        }
    }
    None
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

/// **v4.8 G1.3 partial rerun**:邊界波 retracement 超出極端 Fib² 範圍 → reject 整段 aggregation。
///
/// 對齊 spec line 1249-1251 完整實作:Stage 3-4 partial rerun ≈ 對 aggregation 結果
/// 套用「邊界波 retracement 必落典型 Fibonacci 區段」rules,違反 → reject(不寫進 next_level)。
///
/// 兩階段 threshold(分別對應 advisory vs reject):
/// - Mild abnormal:ratio < 0.382 或 > 2.618 → 寫 Info advisory(走 build_round_advisories)
/// - Extreme abnormal:ratio < **0.236** 或 > **4.236**(Fib² 範圍外)→ **reject aggregation**
///
/// 0.236 = 1/4.236,4.236 = 2.618 × 1.618(Fib²)— 對齊 spec 對「不可能的 retracement」上限。
fn boundary_retracement_extreme(window: &[Scenario], monowaves: &[Monowave]) -> bool {
    if window.len() < 2 {
        return false;
    }
    let first = window.first().unwrap();
    let second = &window[1];
    let last = window.last().unwrap();
    let second_to_last = &window[window.len() - 2];

    let boundary_pairs = [(first, second), (second_to_last, last)];
    const EXTREME_LOW: f64 = 0.236; // 1 / 4.236
    const EXTREME_HIGH: f64 = 4.236; // 2.618 × 1.618 (Fib²)

    for (a, b) in &boundary_pairs {
        if let (Some(mag_a), Some(mag_b)) = (
            scenario_price_magnitude(a, monowaves),
            scenario_price_magnitude(b, monowaves),
        ) {
            if mag_a > 1e-9 && mag_b > 1e-9 {
                let ratio = mag_b / mag_a;
                if ratio < EXTREME_LOW || ratio > EXTREME_HIGH {
                    return true;
                }
            }
        }
    }
    false
}

/// v4.4a:構造 Compaction advisory 列(含 Round 2 動作 B 邊界波重評)。
///
/// 對齊 spec line 1249-1251「Round 2 動作 B 邊界波 m(-1)/m(+1) Retracement Rules 重評」:
/// 當 aggregation 完成,對 window 的首尾 scenario(邊界波 m(-1)/m(+1))做 retracement 比例
/// 評估,若超出典型 Fibonacci 比例範圍 [0.382, 2.618] → 寫 Info advisory 標示「Round 2 邊界
/// retracement 重評啟動」。
///
/// **v4.8 G1.3 升級**:更極端的 retracement(< 0.236 或 > 4.236 Fib²)由
/// `boundary_retracement_extreme` 在 try_aggregate_* 處 short-circuit reject,
/// 不進入此 fn(故 advisory 對應「[0.382, 2.618] 外但仍在 [0.236, 4.236] 內」mild 區段)。
fn build_round_advisories(
    window: &[Scenario],
    label_prefix: &str,
    monowaves: &[Monowave],
) -> Vec<AdvisoryFinding> {
    let mut findings = vec![AdvisoryFinding {
        rule_id: RuleId::Ch7_Compaction_Reassessment,
        severity: AdvisorySeverity::Info,
        message: format!(
            "Compaction Level-N+1 aggregated from {} sub-scenarios (label_prefix={})",
            window.len(),
            label_prefix
        ),
    }];

    // Round 2 動作 B:邊界波 retracement Rules 重評(spec line 1249-1251)
    if window.len() >= 2 {
        let first = window.first().unwrap();
        let second = &window[1];
        let last = window.last().unwrap();
        let second_to_last = &window[window.len() - 2];

        let boundary_pairs = [
            ("m(-1) / m1(left boundary)", first, second),
            ("m(N) / m(N-1)(right boundary)", second_to_last, last),
        ];

        for (label, a, b) in &boundary_pairs {
            // v4.7.1 G1.1:scenario_price_magnitude 改 Option<f64> → 只有 Some/Some 才比對
            if let (Some(mag_a), Some(mag_b)) = (
                scenario_price_magnitude(a, monowaves),
                scenario_price_magnitude(b, monowaves),
            ) {
                if mag_a > 1e-9 && mag_b > 1e-9 {
                    let ratio = mag_b / mag_a;
                    let typical_range = (0.382, 2.618);
                    if ratio < typical_range.0 || ratio > typical_range.1 {
                        findings.push(AdvisoryFinding {
                            rule_id: RuleId::Ch4_Round2_Compaction,
                            severity: AdvisorySeverity::Info,
                            message: format!(
                                "Ch4 Round 2 動作 B:邊界波 {} retracement ratio = {:.3}(超出典型 [0.382, 2.618])— 邊界 retracement Rules 重評啟動(spec line 1249-1251)",
                                label, ratio
                            ),
                        });
                    }
                }
            }
        }
    }

    findings
}

/// 建構新生 Level-N+1 scenario(整段已 compact)。
///
/// v4.4a:接受 monowaves 參數,在 advisory_findings 加 Round 2 動作 B 邊界波 retracement
/// 重評提示(對齊 spec line 1249-1251)。
fn build_aggregated(
    window: &[Scenario],
    window_start: usize,
    base_label: StructureLabel,
    pattern_type: NeelyPatternType,
    label_prefix: &str,
    monowaves: &[Monowave],
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
        advisory_findings: build_round_advisories(window, label_prefix, monowaves),
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
    fn aggregate_7_double_combination() {
        // P3:7 段全 :_3 corrective + 交替方向 → Double Combination
        let scenarios = vec![
            mk_scenario("s0", StructureLabel::Three, MonowaveDirection::Up, "2026-01-01", "2026-01-08"),
            mk_scenario("s1", StructureLabel::Three, MonowaveDirection::Down, "2026-01-08", "2026-01-15"),
            mk_scenario("s2", StructureLabel::Three, MonowaveDirection::Up, "2026-01-15", "2026-01-22"),
            mk_scenario("s3", StructureLabel::Three, MonowaveDirection::Down, "2026-01-22", "2026-01-29"),
            mk_scenario("s4", StructureLabel::Three, MonowaveDirection::Up, "2026-01-29", "2026-02-05"),
            mk_scenario("s5", StructureLabel::Three, MonowaveDirection::Down, "2026-02-05", "2026-02-12"),
            mk_scenario("s6", StructureLabel::Three, MonowaveDirection::Up, "2026-02-12", "2026-02-19"),
        ];
        let result = aggregate_one_level(&scenarios, &[]);
        let combo = result
            .iter()
            .find(|s| matches!(s.pattern_type, NeelyPatternType::Combination { .. }));
        assert!(combo.is_some(), "7 段全 corrective 應 aggregate 出 Double Combination");
        assert_eq!(combo.unwrap().wave_tree.children.len(), 7);
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
        let result = aggregate_one_level(&scenarios, &[]);
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
        let result = aggregate_one_level(&scenarios, &[]);
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
        let result = aggregate_one_level(&scenarios, &[]);
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
        let result = aggregate_one_level(&scenarios, &[]);
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
        let result = aggregate_one_level(&scenarios, &[]);
        assert!(result.is_empty(), "少於 3 個 scenario 不應 aggregate");
    }

    #[test]
    fn neutral_direction_skipped() {
        let scenarios = vec![
            mk_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-10"),
            mk_scenario("b", StructureLabel::Three, MonowaveDirection::Neutral, "2026-01-10", "2026-01-15"),
            mk_scenario("c", StructureLabel::Five, MonowaveDirection::Down, "2026-01-15", "2026-01-25"),
        ];
        let result = aggregate_one_level(&scenarios, &[]);
        assert!(result.is_empty(), "Neutral 方向 break alternating");
    }

    #[test]
    fn time_similarity_extreme_blocks_aggregation() {
        // time durations 10 / 1 / 10 → 第二段太短不 similar(10/1 = 10 > 2.618;1/10 = 0.1 < 0.382)
        let scenarios = vec![
            mk_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-11"),
            mk_scenario("b", StructureLabel::Three, MonowaveDirection::Down, "2026-01-11", "2026-01-12"),
            mk_scenario("c", StructureLabel::Five, MonowaveDirection::Up, "2026-01-12", "2026-01-22"),
        ];
        let result = aggregate_one_level(&scenarios, &[]);
        // **v4.7.1 G1.1**:scenario_price_magnitude 改 Option,monowaves=[] → None
        //   → price_similar=false → only time_similar 主導
        //   → time 不 similar(10/1 = 10x ratio)→ aggregation blocked
        //   (對齊 spec §Rule of Similarity & Balance:price OR time 至少一個 similar)
        assert!(
            result.is_empty(),
            "time extreme + 無 monowave 反查 → S&B fail → 不應 aggregate"
        );
    }

    #[test]
    fn aggregate_with_real_monowaves_price_similar_passes() {
        // v4.7.1 G1.1:提供真實 monowaves,price magnitude similar → aggregate
        // 即使 time 不 similar 也能透過 price 維度通過 S&B
        let scenarios = vec![
            mk_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-11"),
            mk_scenario("b", StructureLabel::Three, MonowaveDirection::Down, "2026-01-11", "2026-01-12"),
            mk_scenario("c", StructureLabel::Five, MonowaveDirection::Up, "2026-01-12", "2026-01-22"),
        ];
        // monowaves 對 a/b/c 的 wave_tree.start/end 提供 price reference
        // a:100→110(mag 10);b:110→100(mag 10);c:100→110(mag 10)
        let monowaves = vec![
            Monowave {
                start_date: chrono::NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap(),
                end_date: chrono::NaiveDate::parse_from_str("2026-01-11", "%Y-%m-%d").unwrap(),
                start_price: 100.0,
                end_price: 110.0,
                direction: MonowaveDirection::Up,
                bar_indices: (0, 0),
            },
            Monowave {
                start_date: chrono::NaiveDate::parse_from_str("2026-01-11", "%Y-%m-%d").unwrap(),
                end_date: chrono::NaiveDate::parse_from_str("2026-01-12", "%Y-%m-%d").unwrap(),
                start_price: 110.0,
                end_price: 100.0,
                direction: MonowaveDirection::Down,
                bar_indices: (0, 0),
            },
            Monowave {
                start_date: chrono::NaiveDate::parse_from_str("2026-01-12", "%Y-%m-%d").unwrap(),
                end_date: chrono::NaiveDate::parse_from_str("2026-01-22", "%Y-%m-%d").unwrap(),
                start_price: 100.0,
                end_price: 110.0,
                direction: MonowaveDirection::Up,
                bar_indices: (0, 0),
            },
        ];
        let result = aggregate_one_level(&scenarios, &monowaves);
        let zigzag = result
            .iter()
            .find(|s| matches!(s.pattern_type, NeelyPatternType::Zigzag { .. }));
        assert!(
            zigzag.is_some(),
            "price magnitude similar(全部 = 10)→ S&B pass → aggregate"
        );
    }

    #[test]
    fn no_monowave_no_price_similarity_blocks_aggregation_when_time_also_extreme() {
        // v4.7.1 G1.1 regression:確認 monowave=[] + time extreme → 雙不通 → 不 aggregate
        let scenarios = vec![
            mk_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-30"),
            mk_scenario("b", StructureLabel::Three, MonowaveDirection::Down, "2026-01-30", "2026-01-31"),
            mk_scenario("c", StructureLabel::Five, MonowaveDirection::Up, "2026-01-31", "2026-03-01"),
        ];
        let result = aggregate_one_level(&scenarios, &[]);
        assert!(result.is_empty(), "無 monowave + time 不 similar → 不 aggregate");
    }

    // v4.8 G1.3 boundary retracement reject tests --------------------------

    fn mk_mw(start: &str, end: &str, sp: f64, ep: f64, dir: MonowaveDirection) -> Monowave {
        use chrono::NaiveDate;
        Monowave {
            start_date: NaiveDate::parse_from_str(start, "%Y-%m-%d").unwrap(),
            end_date: NaiveDate::parse_from_str(end, "%Y-%m-%d").unwrap(),
            start_price: sp,
            end_price: ep,
            direction: dir,
            bar_indices: (0, 0),
        }
    }

    #[test]
    fn boundary_retracement_extreme_rejects_aggregation_when_first_pair_too_low() {
        // window 5 段 Trending Impulse pattern;monowaves 提供使 first/second 比 = 0.2 < 0.236
        let scenarios = vec![
            mk_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-10"),
            mk_scenario("b", StructureLabel::Three, MonowaveDirection::Down, "2026-01-10", "2026-01-15"),
            mk_scenario("c", StructureLabel::Five, MonowaveDirection::Up, "2026-01-15", "2026-01-25"),
            mk_scenario("d", StructureLabel::Three, MonowaveDirection::Down, "2026-01-25", "2026-01-30"),
            mk_scenario("e", StructureLabel::Five, MonowaveDirection::Up, "2026-01-30", "2026-02-10"),
        ];
        // monowaves:a 大 mag(50)/ b 極短(10)→ 比 = 10/50 = 0.2 < 0.236 → reject
        let monowaves = vec![
            mk_mw("2026-01-01", "2026-01-10", 100.0, 150.0, MonowaveDirection::Up),
            mk_mw("2026-01-10", "2026-01-15", 150.0, 140.0, MonowaveDirection::Down),
            mk_mw("2026-01-15", "2026-01-25", 140.0, 190.0, MonowaveDirection::Up),
            mk_mw("2026-01-25", "2026-01-30", 190.0, 180.0, MonowaveDirection::Down),
            mk_mw("2026-01-30", "2026-02-10", 180.0, 230.0, MonowaveDirection::Up),
        ];
        let result = aggregate_one_level(&scenarios, &monowaves);
        let impulse = result
            .iter()
            .find(|s| matches!(s.pattern_type, NeelyPatternType::Impulse));
        assert!(
            impulse.is_none(),
            "first/second mag ratio = 0.2 < 0.236 → reject(Round 2 動作 B partial rerun)"
        );
    }

    #[test]
    fn boundary_retracement_normal_keeps_aggregation() {
        // window 5 段 Impulse;monowaves 提供 mag 接近 → 比落 [0.236, 4.236] → 不 reject
        let scenarios = vec![
            mk_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-10"),
            mk_scenario("b", StructureLabel::Three, MonowaveDirection::Down, "2026-01-10", "2026-01-15"),
            mk_scenario("c", StructureLabel::Five, MonowaveDirection::Up, "2026-01-15", "2026-01-25"),
            mk_scenario("d", StructureLabel::Three, MonowaveDirection::Down, "2026-01-25", "2026-01-30"),
            mk_scenario("e", StructureLabel::Five, MonowaveDirection::Up, "2026-01-30", "2026-02-10"),
        ];
        // 所有 mag 接近 10 → ratio ≈ 1.0 在 [0.236, 4.236] 內
        let monowaves = vec![
            mk_mw("2026-01-01", "2026-01-10", 100.0, 110.0, MonowaveDirection::Up),
            mk_mw("2026-01-10", "2026-01-15", 110.0, 105.0, MonowaveDirection::Down),
            mk_mw("2026-01-15", "2026-01-25", 105.0, 115.0, MonowaveDirection::Up),
            mk_mw("2026-01-25", "2026-01-30", 115.0, 110.0, MonowaveDirection::Down),
            mk_mw("2026-01-30", "2026-02-10", 110.0, 120.0, MonowaveDirection::Up),
        ];
        let result = aggregate_one_level(&scenarios, &monowaves);
        let impulse = result
            .iter()
            .find(|s| matches!(s.pattern_type, NeelyPatternType::Impulse));
        assert!(
            impulse.is_some(),
            "normal boundary retracement → 仍 aggregate"
        );
    }

    #[test]
    fn boundary_retracement_extreme_rejects_zigzag_when_last_pair_too_high() {
        // 3-pattern Zigzag;monowaves 提供 d/c 比 = 50/10 = 5 > 4.236 → reject
        let scenarios = vec![
            mk_scenario("a", StructureLabel::Five, MonowaveDirection::Up, "2026-01-01", "2026-01-10"),
            mk_scenario("b", StructureLabel::Three, MonowaveDirection::Down, "2026-01-10", "2026-01-15"),
            mk_scenario("c", StructureLabel::Five, MonowaveDirection::Up, "2026-01-15", "2026-01-25"),
        ];
        let monowaves = vec![
            mk_mw("2026-01-01", "2026-01-10", 100.0, 110.0, MonowaveDirection::Up),
            mk_mw("2026-01-10", "2026-01-15", 110.0, 100.0, MonowaveDirection::Down),
            mk_mw("2026-01-15", "2026-01-25", 100.0, 150.0, MonowaveDirection::Up), // mag 50
        ];
        let result = aggregate_one_level(&scenarios, &monowaves);
        let zigzag = result
            .iter()
            .find(|s| matches!(s.pattern_type, NeelyPatternType::Zigzag { .. }));
        assert!(zigzag.is_none(), "last pair ratio 50/10 = 5 > 4.236 → reject");
    }
}
