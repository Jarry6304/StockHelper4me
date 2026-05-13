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
//   - m1 端點被 m2 突破 → 需 OHLC intraday extreme reference,Phase 2 placeholder false
//   - 部分子規則涉及「快/慢回測」+「market returns to wave start」幾何判斷,
//     已以「duration 比較 + retracement_pct + 2-4 line breach」近似實作
//   - 細項對齊 m3Spec/neely_rules.md 1042-1062 行「Pre-Constructive Logic 細部技術備註」

use crate::monowave::ClassifiedMonowave;
use crate::output::StructureLabelCandidate;

pub mod context;
pub mod predicates;
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
pub fn run(classified: &mut [ClassifiedMonowave]) {
    for i in 0..classified.len() {
        let cands = compute_candidates_at(classified, i);
        classified[i].structure_label_candidates = cands;
    }
}

/// 對 classified[i] 計算 structure label candidates(不修改 classified)。
fn compute_candidates_at(
    classified: &[ClassifiedMonowave],
    i: usize,
) -> Vec<StructureLabelCandidate> {
    let Some(ctx) = MonowaveContext::build(classified, i) else {
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
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: (end_p - start_p).abs(),
                duration_bars: dur,
                atr_relative: 1.0,
                slope_vs_45deg: 1.0,
            },
            structure_label_candidates: Vec::new(),
        }
    }

    #[test]
    fn run_empty_classified_no_panic() {
        let mut classified: Vec<ClassifiedMonowave> = Vec::new();
        run(&mut classified);
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
        run(&mut classified);
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
        run(&mut classified);
        // m1(i=2)走 Rule 1 Cond 1d → 僅 :5
        let cands = &classified[2].structure_label_candidates;
        assert_eq!(cands.len(), 1);
        assert!(matches!(cands[0].label, StructureLabel::Five));
    }
}
