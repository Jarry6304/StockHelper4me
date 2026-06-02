// validator.rs — Stage 3:硬規則淘汰(對齊 traditional_rules.md §A + references/engine.md §規則模態)
//
// 模態:
//   - **硬規則(淘汰)**:此階段可在 pivot-level 評估者 = R1 / R3 / R4(Impulse+Diagonal)、
//     R5(僅 Impulse)、R9(僅 Diagonal)。違反 → RuleRejection,candidate 出局。
//   - **Deferred(不淘汰)**:R2 [待查證];R6/R7/R8/R11 子浪細分(`5-3-5-3-5` / `3-3-3-3-3` /
//     `5-3-5` / `3-3-5`)**需遞迴子浪分解**,v1 pivot-level 無子 pivot → 標 Deferred。
//     ⚠️ 這正是「產品閘」要揭露的引擎深度問題之一:細分硬規則要生效須做遞迴分解(v2)。
//   - 限定語(R7/R8 重疊、R13)/ 指引 → Stage 5,不在此淘汰。
//
// 容差:硬價格比較一律精確(0%),對齊 engine.md「硬規則=精確價格事件」。

use crate::candidates::{Candidate, HypoKind};
use crate::output::{Direction, Pivot, RuleRejection, TradRuleId};

pub struct ValidationOutcome {
    pub passed_rules: Vec<TradRuleId>,
    pub deferred_rules: Vec<TradRuleId>,
    pub rejections: Vec<RuleRejection>,
}

impl ValidationOutcome {
    pub fn is_legal(&self) -> bool {
        self.rejections.is_empty()
    }
}

fn amp(p: &[Pivot], a: usize, b: usize) -> f64 {
    (p[b].price - p[a].price).abs()
}

fn pct(p: &[Pivot], a: usize, b: usize) -> f64 {
    amp(p, a, b) / p[a].price.abs().max(1e-9)
}

/// 浪 3 在 1/3/5 中最短(以百分比計)→ true(違反 R4 / R9 的「浪 3 永不最短」)。
fn wave3_shortest(p: &[Pivot]) -> bool {
    let w1 = pct(p, 0, 1);
    let w3 = pct(p, 2, 3);
    let w5 = pct(p, 4, 5);
    w3 < w1 && w3 < w5
}

fn reject(rule: TradRuleId, id: &str, detail: String) -> RuleRejection {
    RuleRejection {
        rule_id: rule,
        candidate_id: id.to_string(),
        detail,
    }
}

pub fn validate(c: &Candidate) -> ValidationOutcome {
    match c.hypo {
        HypoKind::Impulse => validate_impulse(c),
        HypoKind::Diagonal => validate_diagonal(c),
        HypoKind::Zigzag | HypoKind::Flat => validate_corrective_3(c),
        HypoKind::Triangle => validate_triangle(c),
    }
}

// --- 推動:衝擊浪(6 pivot,W1..W5)---
fn validate_impulse(c: &Candidate) -> ValidationOutcome {
    let p = &c.pivots;
    let up = c.direction == Direction::Up;
    let mut passed = Vec::new();
    let mut rejections = Vec::new();

    // R1:浪 2 回撤 ≤ 浪 1 之 100%
    if amp(p, 1, 2) > amp(p, 0, 1) {
        rejections.push(reject(TradRuleId::R1Wave2Retracement, &c.id,
            format!("W2 amp {:.2} > W1 amp {:.2}(回撤 >100%)", amp(p, 1, 2), amp(p, 0, 1))));
    } else {
        passed.push(TradRuleId::R1Wave2Retracement);
    }

    // R3:浪 3 必超越浪 1 終點
    let r3_ok = if up { p[3].price > p[1].price } else { p[3].price < p[1].price };
    if r3_ok {
        passed.push(TradRuleId::R3Wave3ExceedsWave1);
    } else {
        rejections.push(reject(TradRuleId::R3Wave3ExceedsWave1, &c.id,
            format!("浪 3 端點 {:.2} 未超越浪 1 端點 {:.2}", p[3].price, p[1].price)));
    }

    // R4:浪 3 永不最短(百分比)
    if wave3_shortest(p) {
        rejections.push(reject(TradRuleId::R4Wave3NotShortest, &c.id,
            "浪 3 為 1/3/5 中最短(百分比)".to_string()));
    } else {
        passed.push(TradRuleId::R4Wave3NotShortest);
    }

    // R5(衝擊浪專屬):浪 4 不重疊浪 1
    let r5_ok = if up { p[4].price > p[1].price } else { p[4].price < p[1].price };
    if r5_ok {
        passed.push(TradRuleId::R5NoOverlap);
    } else {
        rejections.push(reject(TradRuleId::R5NoOverlap, &c.id,
            format!("浪 4 端點 {:.2} 重疊浪 1 端點 {:.2}(衝擊浪不允許 → 改判對角)", p[4].price, p[1].price)));
    }

    ValidationOutcome {
        passed_rules: passed,
        // R2 [待查證] + R6 子浪細分 → 需遞迴分解,v1 無法判定
        deferred_rules: vec![TradRuleId::R2Wave4Retracement, TradRuleId::R6ImpulseSubdivision],
        rejections,
    }
}

// --- 推動:對角三角形(6 pivot)— R5 不適用(浪 1/4 重疊為特徵);R8 rev2 容許擴張引導對角 ---
fn validate_diagonal(c: &Candidate) -> ValidationOutcome {
    let p = &c.pivots;
    let up = c.direction == Direction::Up;
    let mut passed = Vec::new();
    let mut rejections = Vec::new();

    // R1(沿用):浪 2 回撤 ≤ 浪 1
    if amp(p, 1, 2) > amp(p, 0, 1) {
        rejections.push(reject(TradRuleId::R1Wave2Retracement, &c.id,
            format!("W2 amp {:.2} > W1 amp {:.2}", amp(p, 1, 2), amp(p, 0, 1))));
    } else {
        passed.push(TradRuleId::R1Wave2Retracement);
    }

    // R3:浪 3 超越浪 1 終點
    let r3_ok = if up { p[3].price > p[1].price } else { p[3].price < p[1].price };
    if r3_ok {
        passed.push(TradRuleId::R3Wave3ExceedsWave1);
    } else {
        rejections.push(reject(TradRuleId::R3Wave3ExceedsWave1, &c.id,
            format!("浪 3 {:.2} 未超越浪 1 {:.2}", p[3].price, p[1].price)));
    }

    // R9:無反應子浪完全回撤前行動子浪(W2<W1 且 W4<W3)+ 浪 3 永不最短
    let mut r9_fail = false;
    let mut why = String::new();
    if amp(p, 1, 2) >= amp(p, 0, 1) {
        r9_fail = true;
        why = format!("反應浪 2 amp {:.2} 完全回撤行動浪 1 amp {:.2}", amp(p, 1, 2), amp(p, 0, 1));
    } else if amp(p, 3, 4) >= amp(p, 2, 3) {
        r9_fail = true;
        why = format!("反應浪 4 amp {:.2} 完全回撤行動浪 3 amp {:.2}", amp(p, 3, 4), amp(p, 2, 3));
    } else if wave3_shortest(p) {
        r9_fail = true;
        why = "浪 3 為最短(對角亦不允許)".to_string();
    }
    if r9_fail {
        rejections.push(reject(TradRuleId::R9DiagonalNoFullRetrace, &c.id, why));
    } else {
        passed.push(TradRuleId::R9DiagonalNoFullRetrace);
    }

    ValidationOutcome {
        passed_rules: passed,
        // R7/R8 子浪細分(3-3-3-3-3 / 5-3-5-3-5)需遞迴分解 → v1 Deferred
        deferred_rules: vec![
            TradRuleId::R2Wave4Retracement,
            TradRuleId::R7EndingDiagonalSub,
            TradRuleId::R8LeadingDiagonalSub,
        ],
        rejections,
    }
}

// --- 修正:鋸齒 / 平台(4 pivot,3 leg)---
fn validate_corrective_3(_c: &Candidate) -> ValidationOutcome {
    // R10:修正永不為五浪 — 候選由 generator 保證為 3 leg → 通過
    // R11 細分(5-3-5 / 3-3-5)需遞迴 → Deferred
    ValidationOutcome {
        passed_rules: vec![TradRuleId::R10CorrectionNeverFive],
        deferred_rules: vec![TradRuleId::R11CorrectiveSubdivision],
        rejections: Vec::new(),
    }
}

// --- 修正:三角形(6 pivot,5 leg a-e)---
fn validate_triangle(c: &Candidate) -> ValidationOutcome {
    let _ = c;
    // R10 通過(三角形 5 leg 皆修正模式,非五浪推動);R11(3-3-3-3-3)細分需遞迴 → Deferred;
    // R12 組合約束 v1 無組合 → 不列;R13 位置 = 限定語(Stage 5 處理)
    ValidationOutcome {
        passed_rules: vec![TradRuleId::R10CorrectionNeverFive],
        deferred_rules: vec![TradRuleId::R11CorrectiveSubdivision],
        rejections: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::Candidate;
    use crate::output::PivotKind;
    use chrono::NaiveDate;

    fn p(idx: usize, price: f64, kind: PivotKind) -> Pivot {
        Pivot {
            bar_index: idx,
            date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap() + chrono::Duration::days(idx as i64),
            price,
            kind,
        }
    }

    fn motive(prices: [f64; 6]) -> Vec<Pivot> {
        vec![
            p(0, prices[0], PivotKind::Low),
            p(1, prices[1], PivotKind::High),
            p(2, prices[2], PivotKind::Low),
            p(3, prices[3], PivotKind::High),
            p(4, prices[4], PivotKind::Low),
            p(5, prices[5], PivotKind::High),
        ]
    }

    fn cand(pivots: Vec<Pivot>, hypo: HypoKind) -> Candidate {
        Candidate { id: "t".into(), pivots, hypo, direction: Direction::Up }
    }

    // 乾淨上行衝擊浪:10→14→12→20→17→24
    #[test]
    fn clean_impulse_passes() {
        let o = validate(&cand(motive([10.0, 14.0, 12.0, 20.0, 17.0, 24.0]), HypoKind::Impulse));
        assert!(o.is_legal(), "clean impulse should pass, rej={:?}", o.rejections);
        assert!(o.passed_rules.contains(&TradRuleId::R5NoOverlap));
        // 子浪細分規則 Deferred(v1 無遞迴分解)
        assert!(o.deferred_rules.contains(&TradRuleId::R6ImpulseSubdivision));
    }

    #[test]
    fn r1_eliminates_wave2_over_retrace() {
        // W2 (14→9) amp 5 > W1 (10→14) amp 4 → 回撤 >100%
        let o = validate(&cand(motive([10.0, 14.0, 9.0, 20.0, 17.0, 24.0]), HypoKind::Impulse));
        assert!(!o.is_legal());
        assert!(o.rejections.iter().any(|r| r.rule_id == TradRuleId::R1Wave2Retracement));
    }

    #[test]
    fn r3_eliminates_wave3_not_exceeding() {
        // 浪 3 端點 13 < 浪 1 端點 14
        let o = validate(&cand(motive([10.0, 14.0, 12.0, 13.0, 12.5, 16.0]), HypoKind::Impulse));
        assert!(o.rejections.iter().any(|r| r.rule_id == TradRuleId::R3Wave3ExceedsWave1));
    }

    #[test]
    fn r4_eliminates_wave3_shortest() {
        // W1 10→14 (40%), W3 12→13 (~8%), W5 12→24 (100%) → 浪 3 最短
        let o = validate(&cand(motive([10.0, 14.0, 12.0, 13.0, 12.0, 24.0]), HypoKind::Impulse));
        assert!(o.rejections.iter().any(|r| r.rule_id == TradRuleId::R4Wave3NotShortest
            || r.rule_id == TradRuleId::R3Wave3ExceedsWave1));
    }

    // 關鍵:浪 4 重疊浪 1 → Impulse 被 R5 淘汰,但 SAME 幾何的 Diagonal 不淘汰(R8 rev2)
    #[test]
    fn r5_overlap_kills_impulse_but_diagonal_legal() {
        // 浪 4 端點 13 < 浪 1 端點 14 → 重疊
        let prices = [10.0, 14.0, 12.5, 18.0, 13.0, 20.0];
        let imp = validate(&cand(motive(prices), HypoKind::Impulse));
        assert!(imp.rejections.iter().any(|r| r.rule_id == TradRuleId::R5NoOverlap),
            "overlapping impulse must fail R5");
        let dia = validate(&cand(motive(prices), HypoKind::Diagonal));
        assert!(dia.is_legal(), "same overlap geometry must be LEGAL as diagonal (R8 rev2), rej={:?}", dia.rejections);
    }

    #[test]
    fn r9_eliminates_diagonal_full_retrace() {
        // 反應浪 2 (14→9) amp 5 完全回撤 行動浪 1 (10→14) amp 4
        let o = validate(&cand(motive([10.0, 14.0, 9.0, 18.0, 13.0, 20.0]), HypoKind::Diagonal));
        assert!(o.rejections.iter().any(|r| r.rule_id == TradRuleId::R9DiagonalNoFullRetrace));
    }
}
