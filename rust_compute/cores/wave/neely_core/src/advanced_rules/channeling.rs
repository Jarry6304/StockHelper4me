// channeling.rs — Ch5 Channeling 分析(5 條 trendlines)
//
// 對齊 m3Spec/neely_rules.md §Channeling(1346-1363 行)
//       + §Realistic Representations - Impulsions(1569-1631 行)
//
// **5 條 trendlines**(對應 RuleId Ch5_Channeling_*):
//   - 0-2:W0(起點)→ W2 連線(Impulse 下通道)
//   - 1-3:W1 終點 → W3 終點 連線(Impulse 上通道)
//   - 2-4:W2 終點 → W4 終點 連線(早突破 5 段警示)
//   - 0-B:wave-a 起點 → wave-b 終點 連線(Zigzag/Flat 通道)
//   - B-D:wave-b 終點 → wave-d 終點 連線(Triangle 通道)
//
// **諮詢性**:每條 trendline 構造後不直接 fail scenario,而是寫
// AdvisoryFinding (rule_id, severity, message) 進 scenario.advisory_findings。

use super::{linear_y_at, pattern_is_correction, pattern_is_impulsive, pattern_is_triangle, scenario_monowaves};
use crate::monowave::ClassifiedMonowave;
use crate::output::{AdvisoryFinding, AdvisorySeverity, MonowaveDirection, RuleId, Scenario};

/// 對 scenario 跑 Channeling 分析,回傳 0..5 條 AdvisoryFinding。
pub fn analyze(scenario: &Scenario, classified: &[ClassifiedMonowave]) -> Vec<AdvisoryFinding> {
    let mut findings = Vec::new();
    let waves = scenario_monowaves(scenario, classified);

    if pattern_is_impulsive(&scenario.pattern_type) && waves.len() >= 5 {
        // Impulse / Diagonal:0-2 / 1-3 / 2-4 三條
        findings.push(analyze_0_2(scenario, waves));
        findings.push(analyze_1_3(scenario, waves));
        findings.push(analyze_2_4(scenario, waves));
    }
    if pattern_is_correction(&scenario.pattern_type) && waves.len() >= 3 {
        // Zigzag / Flat:0-B 一條
        findings.push(analyze_0_b(scenario, waves));
    }
    if pattern_is_triangle(&scenario.pattern_type) && waves.len() >= 5 {
        // Triangle:B-D 一條(0-2/1-3 不適用)
        findings.push(analyze_b_d(scenario, waves));
    }

    findings
}

/// 0-2 trendline:從 W0 起點(W1.start)到 W2 終點。
fn analyze_0_2(scenario: &Scenario, waves: &[ClassifiedMonowave]) -> AdvisoryFinding {
    let w1 = &waves[0];
    let w2 = &waves[1];
    let line_y = linear_y_at(
        w1.monowave.start_date,
        w1.monowave.start_price,
        w2.monowave.end_date,
        w2.monowave.end_price,
        waves.last().unwrap().monowave.end_date,
    );
    let line_str = line_y.map_or("無法計算".to_string(), |y| format!("{:.2}", y));
    AdvisoryFinding {
        rule_id: RuleId::Ch5_Channeling_02,
        severity: AdvisorySeverity::Info,
        message: format!(
            "0-2 trendline 外推至 scenario end:{}(方向={:?})",
            line_str, scenario.initial_direction
        ),
    }
}

/// 1-3 trendline:W1 終點 → W3 終點。
fn analyze_1_3(scenario: &Scenario, waves: &[ClassifiedMonowave]) -> AdvisoryFinding {
    let w1 = &waves[0];
    let w3 = &waves[2];
    let line_y = linear_y_at(
        w1.monowave.end_date,
        w1.monowave.end_price,
        w3.monowave.end_date,
        w3.monowave.end_price,
        waves.last().unwrap().monowave.end_date,
    );
    let w5_end = waves.last().unwrap().monowave.end_price;

    // 對齊 spec 1584(1st Ext):wave-5 通常落在 1-3 延伸的上通道線下方
    let breach = line_y.is_some_and(|y| match scenario.initial_direction {
        MonowaveDirection::Up => w5_end > y * 1.02, // 突破 > 2% 警示
        MonowaveDirection::Down => w5_end < y * 0.98,
        _ => false,
    });
    let severity = if breach {
        AdvisorySeverity::Warning
    } else {
        AdvisorySeverity::Info
    };
    let line_str = line_y.map_or("無法計算".to_string(), |y| format!("{:.2}", y));
    AdvisoryFinding {
        rule_id: RuleId::Ch5_Channeling_13,
        severity,
        message: format!(
            "1-3 trendline at W5.end:{};W5.end_price={:.2}(突破={})",
            line_str, w5_end, breach
        ),
    }
}

/// 2-4 trendline:W2 終點 → W4 終點(已在 validator core_rules 用過,本處只報告)。
fn analyze_2_4(_scenario: &Scenario, waves: &[ClassifiedMonowave]) -> AdvisoryFinding {
    let w2 = &waves[1];
    let w4 = &waves[3];
    let line_y = linear_y_at(
        w2.monowave.end_date,
        w2.monowave.end_price,
        w4.monowave.end_date,
        w4.monowave.end_price,
        waves.last().unwrap().monowave.end_date,
    );
    let line_str = line_y.map_or("無法計算".to_string(), |y| format!("{:.2}", y));
    AdvisoryFinding {
        rule_id: RuleId::Ch5_Channeling_24,
        severity: AdvisorySeverity::Info,
        message: format!(
            "2-4 trendline 外推至 scenario end:{}(早突破啟動 Terminal 規則,spec Ch9 Aspect 2)",
            line_str
        ),
    }
}

/// 0-B trendline:wave-a 起點 → wave-b 終點(Zigzag / Flat)。
fn analyze_0_b(scenario: &Scenario, waves: &[ClassifiedMonowave]) -> AdvisoryFinding {
    let wave_a = &waves[0];
    let wave_b = &waves[1];
    let line_y = linear_y_at(
        wave_a.monowave.start_date,
        wave_a.monowave.start_price,
        wave_b.monowave.end_date,
        wave_b.monowave.end_price,
        waves.last().unwrap().monowave.end_date,
    );
    let c_end = waves.last().unwrap().monowave.end_price;
    // Zigzag c-wave 絕不可剛好觸碰平行線(spec 1356)— 本 PR 簡化為「突破 0-B 線」報告
    let line_str = line_y.map_or("無法計算".to_string(), |y| format!("{:.2}", y));
    let breached = line_y.is_some_and(|y| match scenario.initial_direction {
        MonowaveDirection::Up => c_end < y,
        MonowaveDirection::Down => c_end > y,
        _ => false,
    });
    let severity = if breached {
        AdvisorySeverity::Warning // 0-B 線被穿破 → c-wave 與更大形態幾乎結束(spec 1357)
    } else {
        AdvisorySeverity::Info
    };
    AdvisoryFinding {
        rule_id: RuleId::Ch5_Channeling_0B,
        severity,
        message: format!(
            "0-B trendline at c.end:{};c.end_price={:.2}(穿破={})",
            line_str, c_end, breached
        ),
    }
}

/// B-D trendline:wave-b 終點 → wave-d 終點(Triangle)。
fn analyze_b_d(scenario: &Scenario, waves: &[ClassifiedMonowave]) -> AdvisoryFinding {
    let wave_b = &waves[1];
    let wave_d = &waves[3];
    let line_y = linear_y_at(
        wave_b.monowave.end_date,
        wave_b.monowave.end_price,
        wave_d.monowave.end_date,
        wave_d.monowave.end_price,
        waves.last().unwrap().monowave.end_date,
    );
    let e_end = waves.last().unwrap().monowave.end_price;
    // B-D 線被穿破 → Triangle 幾乎結束(spec 1361)
    let breached = line_y.is_some_and(|y| match scenario.initial_direction {
        MonowaveDirection::Up => e_end < y,
        MonowaveDirection::Down => e_end > y,
        _ => false,
    });
    let severity = if breached {
        AdvisorySeverity::Strong
    } else {
        AdvisorySeverity::Info
    };
    let line_str = line_y.map_or("無法計算".to_string(), |y| format!("{:.2}", y));
    AdvisoryFinding {
        rule_id: RuleId::Ch5_Channeling_BD,
        severity,
        message: format!(
            "B-D trendline at e.end:{};e.end_price={:.2}(穿破={};Triangle 結束訊號)",
            line_str, e_end, breached
        ),
    }
}
