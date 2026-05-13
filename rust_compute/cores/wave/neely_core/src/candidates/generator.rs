// Bottom-up Wave Candidate Generator(Stage 3)
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 3 / §4.2 Item 2。
//
// 演算法(Bottom-up):
//   1. 過濾掉 Neutral monowaves(Stage 2 Rule of Neutrality 已標)
//   2. 對 directional monowave 序列,滑動取 wave_count ∈ {3, 5} 的連續視窗
//   3. 視窗內 monowave direction 必須交替(zigzag 性質),否則跳過
//   4. 通過交替檢查的視窗即為一個 candidate
//
// Stage 3 **不**判定 pattern_type(Impulse / Zigzag / Flat / Triangle / Combination)
// — 那是 Stage 5 Classifier 的事(neely §七 Stage 5)。
//
// Stage 3 也**不**檢查 R1-R7 等 Neely 規則 — 那是 Stage 4 Validator 的事
// (neely §十)。
//
// beam_width 上限保護:候選數量超過 cfg.beam_width × 10 時 cap 住,避免 Stage 4
// Validator 跑爆。實際 cap 值 P0 Gate 校準後可能調整。
//
// 留後續 PR(對齊 m2Spec/oldm2Spec/neely_core.md §三):
//   - generator.rs 進階:5-wave-of-3 嵌套(Combination 類型需要)
//   - 結合 ProportionMetrics 預先剔除 magnitude 顯著不對稱的視窗

use crate::config::NeelyEngineConfig;
use crate::monowave::ClassifiedMonowave;
use crate::output::MonowaveDirection;

/// Stage 3 候選結果。每個 WaveCandidate 對應一個「可能是 wave structure 的視窗」。
///
/// `monowave_indices` 是 candidate 內 monowaves 在 `classified[]` 的 index list,
/// length 為 wave_count(目前 3 或 5)。Validator(Stage 4)會用 indices 取回對應
/// monowave 做規則檢查。
#[derive(Debug, Clone)]
pub struct WaveCandidate {
    /// 唯一 ID:`c{wave_count}-mw{first_idx}-mw{last_idx}`
    pub id: String,
    /// monowave indices(構成此 candidate 的 directional monowaves,按時間排序)
    pub monowave_indices: Vec<usize>,
    /// 視窗內 monowave 個數,目前 ∈ {3, 5}
    pub wave_count: usize,
    /// 第 1 個 monowave 的 direction(Up 表示上升 wave 序列,Down 表示下降)
    pub initial_direction: MonowaveDirection,
}

/// 單一視窗 candidate 數量上限保護倍數。
/// 實際 cap = `cfg.beam_width * BEAM_CAP_MULTIPLIER`,P0 Gate 校準後可能調。
const BEAM_CAP_MULTIPLIER: usize = 10;

/// 從 Stage 2 classified monowaves 產生 wave candidates。
///
/// 流程:
///   1. 過濾 Neutral
///   2. 對 wave_count ∈ {3, 5} 滑動取窗
///   3. 視窗內 direction 必須交替(Up-Down-Up... 或 Down-Up-Down...)
///   4. 達 beam_width × 10 上限即停止繼續產生
pub fn generate_candidates(
    classified: &[ClassifiedMonowave],
    cfg: &NeelyEngineConfig,
) -> Vec<WaveCandidate> {
    if classified.is_empty() {
        return Vec::new();
    }

    // 過濾 Neutral — 留下 directional monowave 的 indices
    let directional: Vec<usize> = classified
        .iter()
        .enumerate()
        .filter(|(_, c)| c.monowave.direction != MonowaveDirection::Neutral)
        .map(|(i, _)| i)
        .collect();

    let cap = cfg.beam_width.saturating_mul(BEAM_CAP_MULTIPLIER).max(1);
    let mut candidates: Vec<WaveCandidate> = Vec::new();

    // 對每個 wave_count(3 / 5)滑窗
    for &wc in &[3usize, 5] {
        if directional.len() < wc {
            continue;
        }

        for window_start in 0..=(directional.len() - wc) {
            if candidates.len() >= cap {
                return candidates;
            }

            let window: Vec<usize> = directional[window_start..window_start + wc].to_vec();

            // 檢查 direction 是否交替
            if !directions_alternate(&window, classified) {
                continue;
            }

            let first_idx = window[0];
            let last_idx = *window.last().unwrap();
            let initial_direction = classified[first_idx].monowave.direction;

            candidates.push(WaveCandidate {
                id: format!("c{}-mw{}-mw{}", wc, first_idx, last_idx),
                monowave_indices: window,
                wave_count: wc,
                initial_direction,
            });
        }
    }

    candidates
}

/// 視窗內 monowave 是否依 Up/Down 交替排列。
/// (Neutral 已在 generate_candidates 過濾,這裡視窗內不含 Neutral。)
fn directions_alternate(window: &[usize], classified: &[ClassifiedMonowave]) -> bool {
    for pair in window.windows(2) {
        let a = classified[pair[0]].monowave.direction;
        let b = classified[pair[1]].monowave.direction;
        // 已過濾 Neutral,但 defensive:任一方為 Neutral 視為「不交替」
        if a == MonowaveDirection::Neutral || b == MonowaveDirection::Neutral {
            return false;
        }
        if a == b {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::Monowave;
    use chrono::NaiveDate;

    fn cmw(start_date: &str, end_date: &str, dir: MonowaveDirection) -> ClassifiedMonowave {
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: NaiveDate::parse_from_str(start_date, "%Y-%m-%d").unwrap(),
                end_date: NaiveDate::parse_from_str(end_date, "%Y-%m-%d").unwrap(),
                start_price: 100.0,
                end_price: if dir == MonowaveDirection::Up { 110.0 } else { 90.0 },
                direction: dir,
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: 10.0,
                duration_bars: 5,
                atr_relative: 10.0,
                slope_vs_45deg: 2.0,
            },
            structure_label_candidates: Vec::new(),
        }
    }

    #[test]
    fn empty_input_yields_no_candidates() {
        let cfg = NeelyEngineConfig::default();
        assert!(generate_candidates(&[], &cfg).is_empty());
    }

    #[test]
    fn fewer_than_three_directional_monowaves_yield_no_candidates() {
        let cfg = NeelyEngineConfig::default();
        let cms = vec![
            cmw("2026-01-01", "2026-01-05", MonowaveDirection::Up),
            cmw("2026-01-05", "2026-01-08", MonowaveDirection::Down),
        ];
        assert!(generate_candidates(&cms, &cfg).is_empty());
    }

    #[test]
    fn three_alternating_monowaves_yield_one_candidate() {
        let cfg = NeelyEngineConfig::default();
        let cms = vec![
            cmw("2026-01-01", "2026-01-05", MonowaveDirection::Up),
            cmw("2026-01-05", "2026-01-08", MonowaveDirection::Down),
            cmw("2026-01-08", "2026-01-12", MonowaveDirection::Up),
        ];
        let cands = generate_candidates(&cms, &cfg);
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].wave_count, 3);
        assert_eq!(cands[0].monowave_indices, vec![0, 1, 2]);
        assert_eq!(cands[0].initial_direction, MonowaveDirection::Up);
        assert_eq!(cands[0].id, "c3-mw0-mw2");
    }

    #[test]
    fn five_alternating_monowaves_yield_three_candidates() {
        // 5 個 alternating monowaves(U-D-U-D-U)→
        //   wave_count=3:window 起點 0/1/2 → 3 個 candidate
        //   wave_count=5:window 起點 0     → 1 個 candidate
        // 共 4 個
        let cfg = NeelyEngineConfig::default();
        let cms = vec![
            cmw("2026-01-01", "2026-01-03", MonowaveDirection::Up),
            cmw("2026-01-03", "2026-01-05", MonowaveDirection::Down),
            cmw("2026-01-05", "2026-01-07", MonowaveDirection::Up),
            cmw("2026-01-07", "2026-01-09", MonowaveDirection::Down),
            cmw("2026-01-09", "2026-01-11", MonowaveDirection::Up),
        ];
        let cands = generate_candidates(&cms, &cfg);
        assert_eq!(cands.len(), 4);
        assert_eq!(cands.iter().filter(|c| c.wave_count == 3).count(), 3);
        assert_eq!(cands.iter().filter(|c| c.wave_count == 5).count(), 1);
        // wave_count=5 candidate 的 indices 應為 [0,1,2,3,4]
        let five = cands.iter().find(|c| c.wave_count == 5).unwrap();
        assert_eq!(five.monowave_indices, vec![0, 1, 2, 3, 4]);
        assert_eq!(five.initial_direction, MonowaveDirection::Up);
    }

    #[test]
    fn neutral_monowaves_are_filtered_out() {
        let cfg = NeelyEngineConfig::default();
        let cms = vec![
            cmw("2026-01-01", "2026-01-03", MonowaveDirection::Up),
            cmw("2026-01-03", "2026-01-04", MonowaveDirection::Neutral),
            cmw("2026-01-04", "2026-01-06", MonowaveDirection::Down),
            cmw("2026-01-06", "2026-01-08", MonowaveDirection::Up),
        ];
        let cands = generate_candidates(&cms, &cfg);
        // Neutral 被濾掉後剩 3 個 directional(U-D-U)→ 1 個 wave_count=3 candidate
        assert_eq!(cands.len(), 1);
        // monowave_indices 對應到原始 classified 的 indices(0, 2, 3),不是 directional 序列 indices
        assert_eq!(cands[0].monowave_indices, vec![0, 2, 3]);
    }

    #[test]
    fn non_alternating_window_skipped() {
        // U-U-D 不交替(U 跟 U 連續) → 不產生 candidate
        let cfg = NeelyEngineConfig::default();
        let cms = vec![
            cmw("2026-01-01", "2026-01-03", MonowaveDirection::Up),
            cmw("2026-01-03", "2026-01-05", MonowaveDirection::Up),
            cmw("2026-01-05", "2026-01-07", MonowaveDirection::Down),
        ];
        let cands = generate_candidates(&cms, &cfg);
        assert!(cands.is_empty());
    }

    #[test]
    fn beam_width_cap_limits_candidates() {
        // 製造大量 alternating monowaves,驗 cap 生效
        let cfg = NeelyEngineConfig {
            beam_width: 2,  // cap = 2 × 10 = 20
            ..NeelyEngineConfig::default()
        };
        // 30 個 alternating monowaves → 理論 wave_count=3 視窗 28 個 + wave_count=5 視窗 26 個 = 54 個
        let mut cms = Vec::new();
        for i in 0..30 {
            let dir = if i % 2 == 0 { MonowaveDirection::Up } else { MonowaveDirection::Down };
            cms.push(cmw(
                &format!("2026-01-{:02}", (i % 28) + 1),
                &format!("2026-01-{:02}", (i % 28) + 2),
                dir,
            ));
        }
        let cands = generate_candidates(&cms, &cfg);
        assert!(cands.len() <= 20, "candidate count 應 ≤ beam_width × 10 = 20,實際 {}", cands.len());
    }

    #[test]
    fn descending_initial_direction_recorded() {
        let cfg = NeelyEngineConfig::default();
        let cms = vec![
            cmw("2026-01-01", "2026-01-03", MonowaveDirection::Down),
            cmw("2026-01-03", "2026-01-05", MonowaveDirection::Up),
            cmw("2026-01-05", "2026-01-07", MonowaveDirection::Down),
        ];
        let cands = generate_candidates(&cms, &cfg);
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].initial_direction, MonowaveDirection::Down);
    }
}
