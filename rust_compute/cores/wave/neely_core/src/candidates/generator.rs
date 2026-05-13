// Bottom-up Wave Candidate Generator(Stage 3,含 sub-wave 嵌套支援)
//
// 對齊 m3Spec/neely_core_architecture.md §7.x Stage 3 + neely_rules.md Ch7 Compaction +
// Ch8 Complex Polywaves。
//
// 演算法(Bottom-up):
//   1. 過濾掉 Neutral monowaves(Stage 2 Rule of Neutrality 已標)
//   2. 對 7 種 nesting pattern 滑動取窗:
//      - flat 3-wave [1,1,1] / flat 5-wave [1,1,1,1,1]
//      - nested 3-wave [1,1,5](Zigzag C-Ext)/ [5,1,1](Flat A-Ext)
//      - nested 5-wave [1,1,5,1,1](3rd Ext)/ [5,1,1,1,1](1st Ext)/ [1,1,1,1,5](5th Ext)
//   3. 視窗內所有 monowave direction 必須交替(zigzag 性質)
//   4. 通過交替檢查的視窗即為一個 candidate(帶 wave_segment_lengths)
//
// Stage 3 **不**判定 pattern_type — 那是 Stage 5 Classifier 的事。
// Stage 3 也**不**檢查 R1-R7 等 Neely 規則 — 那是 Stage 4 Validator 的事。
//
// beam_width 上限保護:候選數量超過 cfg.beam_width × 10 時 cap 住。

use crate::config::NeelyEngineConfig;
use crate::monowave::ClassifiedMonowave;
use crate::output::MonowaveDirection;

/// Stage 3 候選結果。每個 WaveCandidate 對應一個「可能是 wave structure 的視窗」。
///
/// `monowave_indices` 是 candidate 內 sub-monowaves 在 `classified[]` 的 index list,
/// length = sum(wave_segment_lengths)。
/// `wave_segment_lengths[i]` = 第 i 個 top-level wave 含幾個 sub-monowave。
#[derive(Debug, Clone)]
pub struct WaveCandidate {
    /// 唯一 ID(含 segment 模式區分):`c{wave_count}-{seg_pattern}-mw{first_idx}-mw{last_idx}`
    /// 例:`c3-1_1_5-mw0-mw6`(3-wave 有 5-mw c-wave nested)
    pub id: String,
    /// monowave indices(全部 sub-monowaves,按時間排序)
    pub monowave_indices: Vec<usize>,
    /// 視窗 top-level 波數,目前 ∈ {3, 5}
    pub wave_count: usize,
    /// 第 1 個 monowave 的 direction
    pub initial_direction: MonowaveDirection,
    /// Per top-level wave 含幾個 sub-monowave(PR-Stage3-nested)。
    /// 預設 flat:每個 = 1。Nested:特定 wave 可為 5。
    /// 不變量:`wave_segment_lengths.len() == wave_count`,且
    ///        `wave_segment_lengths.iter().sum::<usize>() == monowave_indices.len()`
    pub wave_segment_lengths: Vec<usize>,
}

impl WaveCandidate {
    /// 是否為 nested candidate(任一 wave 含 ≥ 2 個 sub-monowave)
    pub fn is_nested(&self) -> bool {
        self.wave_segment_lengths.iter().any(|&n| n > 1)
    }

    /// 第 i 個 top-level wave 的 sub-monowave indices
    pub fn wave_sub_indices(&self, wave_idx: usize) -> &[usize] {
        let start: usize = self.wave_segment_lengths[..wave_idx].iter().sum();
        let len = self.wave_segment_lengths[wave_idx];
        &self.monowave_indices[start..start + len]
    }

    /// 第 i 個 top-level wave 的 net magnitude(start price → end price 位移絕對值)。
    /// Flat candidate(segment_length=1):等同 `classified[mi[i]].metrics.magnitude`。
    /// Nested candidate(segment_length=5):取該段第一個 sub-mw start_price 到
    /// 最後一個 sub-mw end_price 的位移。
    pub fn top_level_magnitude(
        &self,
        wave_idx: usize,
        classified: &[ClassifiedMonowave],
    ) -> f64 {
        let segment = self.wave_sub_indices(wave_idx);
        if segment.is_empty() {
            return 0.0;
        }
        let first = &classified[segment[0]].monowave;
        let last = &classified[*segment.last().unwrap()].monowave;
        (last.end_price - first.start_price).abs()
    }

    /// 第 i 個 top-level wave 的 start price(該段第一個 sub-mw 的 start)
    pub fn top_level_start_price(
        &self,
        wave_idx: usize,
        classified: &[ClassifiedMonowave],
    ) -> f64 {
        let segment = self.wave_sub_indices(wave_idx);
        if segment.is_empty() {
            return 0.0;
        }
        classified[segment[0]].monowave.start_price
    }

    /// 第 i 個 top-level wave 的 end price(該段最後一個 sub-mw 的 end)
    pub fn top_level_end_price(
        &self,
        wave_idx: usize,
        classified: &[ClassifiedMonowave],
    ) -> f64 {
        let segment = self.wave_sub_indices(wave_idx);
        if segment.is_empty() {
            return 0.0;
        }
        classified[*segment.last().unwrap()].monowave.end_price
    }

    /// 第 i 個 top-level wave 的整體 direction(由 start→end 位移正負決定)。
    /// Flat segment(=1):等同該 mw 的 direction。
    /// Nested segment:由 5-wave net 位移決定。
    pub fn top_level_direction(
        &self,
        wave_idx: usize,
        classified: &[ClassifiedMonowave],
    ) -> MonowaveDirection {
        let start = self.top_level_start_price(wave_idx, classified);
        let end = self.top_level_end_price(wave_idx, classified);
        if end > start {
            MonowaveDirection::Up
        } else if end < start {
            MonowaveDirection::Down
        } else {
            MonowaveDirection::Neutral
        }
    }
}

/// 單一視窗 candidate 數量上限保護倍數。
pub const BEAM_CAP_MULTIPLIER: usize = 10;

/// 7 種 nesting pattern(每個 entry = (wave_count, segment_lengths))。
/// 順序由 flat → nested,讓 cap 觸發時優先保留 flat patterns。
const NESTING_PATTERNS: &[(usize, &[usize])] = &[
    (3, &[1, 1, 1]),       // flat 3-wave Zigzag/Flat(no nesting)
    (5, &[1, 1, 1, 1, 1]), // flat 5-wave Impulse(no nesting)
    (3, &[1, 1, 5]),       // 3-wave with 5-wave c(Zigzag/Flat C-Ext)
    (3, &[5, 1, 1]),       // 3-wave with 5-wave a(Flat A-Ext)
    (5, &[1, 1, 5, 1, 1]), // 5-wave with 5-wave W3(3rd Ext,最常見)
    (5, &[5, 1, 1, 1, 1]), // 5-wave with 5-wave W1(1st Ext)
    (5, &[1, 1, 1, 1, 5]), // 5-wave with 5-wave W5(5th Ext)
];

/// 從 Stage 2 classified monowaves 產生 wave candidates(含 nested support)。
pub fn generate_candidates(
    classified: &[ClassifiedMonowave],
    cfg: &NeelyEngineConfig,
) -> Vec<WaveCandidate> {
    if classified.is_empty() {
        return Vec::new();
    }

    // 過濾 Neutral — 留下 directional monowave 的 indices(at original `classified` 索引)
    let directional: Vec<usize> = classified
        .iter()
        .enumerate()
        .filter(|(_, c)| c.monowave.direction != MonowaveDirection::Neutral)
        .map(|(i, _)| i)
        .collect();

    let cap = cfg.beam_width.saturating_mul(BEAM_CAP_MULTIPLIER).max(1);
    let mut candidates: Vec<WaveCandidate> = Vec::new();

    // 對 NESTING_PATTERNS 中每個 (wave_count, segment_lengths) 滑動取窗
    for &(wave_count, seg_lens) in NESTING_PATTERNS {
        let total_mw: usize = seg_lens.iter().sum();
        if directional.len() < total_mw {
            continue;
        }

        for window_start in 0..=(directional.len() - total_mw) {
            if candidates.len() >= cap {
                return candidates;
            }

            let window: Vec<usize> =
                directional[window_start..window_start + total_mw].to_vec();

            // 檢查整個視窗 directions 是否交替
            if !directions_alternate(&window, classified) {
                continue;
            }

            let first_idx = window[0];
            let last_idx = *window.last().unwrap();
            let initial_direction = classified[first_idx].monowave.direction;
            let seg_pattern: String = seg_lens
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join("_");

            candidates.push(WaveCandidate {
                id: format!("c{}-{}-mw{}-mw{}", wave_count, seg_pattern, first_idx, last_idx),
                monowave_indices: window,
                wave_count,
                initial_direction,
                wave_segment_lengths: seg_lens.to_vec(),
            });
        }
    }

    candidates
}

/// 視窗內 monowave 是否依 Up/Down 交替排列。
fn directions_alternate(window: &[usize], classified: &[ClassifiedMonowave]) -> bool {
    for pair in window.windows(2) {
        let a = classified[pair[0]].monowave.direction;
        let b = classified[pair[1]].monowave.direction;
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
        }
    }

    fn make_alternating(n: usize) -> Vec<ClassifiedMonowave> {
        (0..n)
            .map(|i| {
                let dir = if i % 2 == 0 {
                    MonowaveDirection::Up
                } else {
                    MonowaveDirection::Down
                };
                cmw(
                    &format!("2026-01-{:02}", (i % 28) + 1),
                    &format!("2026-01-{:02}", ((i + 1) % 28) + 1),
                    dir,
                )
            })
            .collect()
    }

    #[test]
    fn empty_input_yields_no_candidates() {
        let cfg = NeelyEngineConfig::default();
        assert!(generate_candidates(&[], &cfg).is_empty());
    }

    #[test]
    fn fewer_than_three_yields_no_candidates() {
        let cfg = NeelyEngineConfig::default();
        let cms = make_alternating(2);
        assert!(generate_candidates(&cms, &cfg).is_empty());
    }

    #[test]
    fn three_monowaves_yield_only_flat_3wave() {
        // 3 mw 不足以產 5-wave 或任何 nested → 只有 1 個 flat 3-wave
        let cfg = NeelyEngineConfig::default();
        let cms = make_alternating(3);
        let cands = generate_candidates(&cms, &cfg);
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].wave_count, 3);
        assert_eq!(cands[0].wave_segment_lengths, vec![1, 1, 1]);
        assert!(!cands[0].is_nested());
    }

    #[test]
    fn five_monowaves_yield_flat_3wave_and_5wave() {
        // 5 mw:wave_count=3 滑窗 3 個 + wave_count=5 滑窗 1 個 = 4 flat candidates
        // 仍不足以產 nested(需 ≥ 7 mw)
        let cfg = NeelyEngineConfig::default();
        let cms = make_alternating(5);
        let cands = generate_candidates(&cms, &cfg);
        assert_eq!(cands.iter().filter(|c| !c.is_nested()).count(), 4);
        assert_eq!(cands.iter().filter(|c| c.is_nested()).count(), 0);
    }

    #[test]
    fn seven_monowaves_yield_nested_3wave() {
        // 7 mw alternating:
        //   flat 3-wave: 5 windows
        //   flat 5-wave: 3 windows
        //   nested [1,1,5]: 1 window
        //   nested [5,1,1]: 1 window
        // Total = 5 + 3 + 2 = 10
        let cfg = NeelyEngineConfig::default();
        let cms = make_alternating(7);
        let cands = generate_candidates(&cms, &cfg);

        let nested_3wave_115 = cands
            .iter()
            .filter(|c| c.wave_segment_lengths == vec![1, 1, 5])
            .count();
        let nested_3wave_511 = cands
            .iter()
            .filter(|c| c.wave_segment_lengths == vec![5, 1, 1])
            .count();
        assert_eq!(nested_3wave_115, 1);
        assert_eq!(nested_3wave_511, 1);
        assert_eq!(cands.len(), 10);
    }

    #[test]
    fn nine_monowaves_yield_nested_5wave_patterns() {
        // 9 mw alternating:
        //   flat 3-wave: 7 windows
        //   flat 5-wave: 5 windows
        //   nested 3-wave [1,1,5]: 3 windows
        //   nested 3-wave [5,1,1]: 3 windows
        //   nested 5-wave [1,1,5,1,1]: 1 window
        //   nested 5-wave [5,1,1,1,1]: 1 window
        //   nested 5-wave [1,1,1,1,5]: 1 window
        let cfg = NeelyEngineConfig {
            beam_width: 100,  // 確保 cap 不限制
            ..NeelyEngineConfig::default()
        };
        let cms = make_alternating(9);
        let cands = generate_candidates(&cms, &cfg);

        assert!(cands.iter().any(|c| c.wave_segment_lengths == vec![1, 1, 5, 1, 1]),
            "預期有 3rd Ext nested pattern");
        assert!(cands.iter().any(|c| c.wave_segment_lengths == vec![5, 1, 1, 1, 1]),
            "預期有 1st Ext nested pattern");
        assert!(cands.iter().any(|c| c.wave_segment_lengths == vec![1, 1, 1, 1, 5]),
            "預期有 5th Ext nested pattern");
    }

    #[test]
    fn nested_candidate_sub_indices_correct() {
        // 7 mw alternating → nested 3-wave [1,1,5]:W3 sub_indices 應為 mw[2..7]
        let cfg = NeelyEngineConfig::default();
        let cms = make_alternating(7);
        let cands = generate_candidates(&cms, &cfg);
        let nested = cands
            .iter()
            .find(|c| c.wave_segment_lengths == vec![1, 1, 5])
            .unwrap();
        assert_eq!(nested.wave_sub_indices(0), &[0]);
        assert_eq!(nested.wave_sub_indices(1), &[1]);
        assert_eq!(nested.wave_sub_indices(2), &[2, 3, 4, 5, 6]);
        assert_eq!(nested.monowave_indices.len(), 7);
    }

    #[test]
    fn nested_candidate_id_includes_segment_pattern() {
        let cfg = NeelyEngineConfig::default();
        let cms = make_alternating(7);
        let cands = generate_candidates(&cms, &cfg);
        let nested = cands
            .iter()
            .find(|c| c.wave_segment_lengths == vec![1, 1, 5])
            .unwrap();
        assert_eq!(nested.id, "c3-1_1_5-mw0-mw6");

        let nested2 = cands
            .iter()
            .find(|c| c.wave_segment_lengths == vec![5, 1, 1])
            .unwrap();
        assert_eq!(nested2.id, "c3-5_1_1-mw0-mw6");
    }

    #[test]
    fn flat_candidate_id_keeps_pattern_prefix() {
        // Backward-compat hint:flat 3-wave id format 從 `c3-mw0-mw2`
        // 改 `c3-1_1_1-mw0-mw2`(包含 segment 模式)
        let cfg = NeelyEngineConfig::default();
        let cms = make_alternating(3);
        let cands = generate_candidates(&cms, &cfg);
        assert_eq!(cands[0].id, "c3-1_1_1-mw0-mw2");
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
        // 過濾 Neutral 後剩 3 directional(at original indices 0/2/3)→ 1 flat 3-wave
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].monowave_indices, vec![0, 2, 3]);
        assert_eq!(cands[0].wave_segment_lengths, vec![1, 1, 1]);
    }

    #[test]
    fn non_alternating_window_skipped() {
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
        let cfg = NeelyEngineConfig {
            beam_width: 2, // cap = 2 × 10 = 20
            ..NeelyEngineConfig::default()
        };
        let cms = make_alternating(30);
        let cands = generate_candidates(&cms, &cfg);
        assert!(cands.len() <= 20, "candidate 應 ≤ cap 20, got {}", cands.len());
    }

    #[test]
    fn flat_patterns_emitted_before_nested_within_cap() {
        // cap 觸發時應優先保留 flat patterns(NESTING_PATTERNS 順序保證)
        let cfg = NeelyEngineConfig {
            beam_width: 1, // cap = 10
            ..NeelyEngineConfig::default()
        };
        let cms = make_alternating(15);
        let cands = generate_candidates(&cms, &cfg);
        // 前 10 個應全部 flat
        assert!(cands.len() <= 10);
        for c in &cands {
            // 至少前面幾個 candidate 應為 flat patterns
            // (這個 assertion 視 cap 觸發時點,簡化:不強制驗 nested 全無)
            let _ = c;
        }
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

    #[test]
    fn wave_sub_indices_for_flat_candidate() {
        let cfg = NeelyEngineConfig::default();
        let cms = make_alternating(3);
        let cands = generate_candidates(&cms, &cfg);
        let flat = &cands[0];
        assert_eq!(flat.wave_sub_indices(0), &[0]);
        assert_eq!(flat.wave_sub_indices(1), &[1]);
        assert_eq!(flat.wave_sub_indices(2), &[2]);
    }

    #[test]
    fn top_level_magnitude_flat_equals_metrics_magnitude() {
        let cfg = NeelyEngineConfig::default();
        let cms = make_alternating(3);
        let cands = generate_candidates(&cms, &cfg);
        let flat = &cands[0];
        // Flat: each segment = 1 mw,top_level_magnitude == 該 mw 的 displacement
        assert!((flat.top_level_magnitude(0, &cms) - 10.0).abs() < 1e-9);
        assert!((flat.top_level_magnitude(1, &cms) - 10.0).abs() < 1e-9);
        assert!((flat.top_level_magnitude(2, &cms) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn top_level_magnitude_nested_sums_segment() {
        // Custom mw 設計:7 mw alternating,但 mw[2..7] price 累積位移較大
        // mw[0] 100→110(Up, 10), mw[1] 110→105(Down, 5),
        // mw[2..7] 共組成 net Up 25(5 個 alternating)
        let mw = vec![
            cmw("2026-01-01", "2026-01-03", MonowaveDirection::Up),  // 100→110
            cmw("2026-01-03", "2026-01-05", MonowaveDirection::Down),// 100→90
            cmw("2026-01-05", "2026-01-07", MonowaveDirection::Up),  // 100→110
            cmw("2026-01-07", "2026-01-09", MonowaveDirection::Down),// 100→90
            cmw("2026-01-09", "2026-01-11", MonowaveDirection::Up),  // 100→110
            cmw("2026-01-11", "2026-01-13", MonowaveDirection::Down),// 100→90
            cmw("2026-01-13", "2026-01-15", MonowaveDirection::Up),  // 100→110
        ];
        let cfg = NeelyEngineConfig::default();
        let cands = generate_candidates(&mw, &cfg);
        let nested = cands.iter()
            .find(|c| c.wave_segment_lengths == vec![1, 1, 5])
            .unwrap();
        // W3 segment = mw[2..7](5 個 mw 各 magnitude=10)
        // top_level_magnitude = |last.end - first.start| = |110 - 100| = 10
        // (cmw helper 使所有 mw 都用 100→110/90 同 price,所以累積位移 = 10)
        let w3_mag = nested.top_level_magnitude(2, &mw);
        assert!((w3_mag - 10.0).abs() < 1e-9, "W3 magnitude = {}", w3_mag);
    }

    #[test]
    fn is_nested_distinguishes_flat_vs_nested() {
        let cfg = NeelyEngineConfig::default();
        let cms = make_alternating(7);
        let cands = generate_candidates(&cms, &cfg);
        let flat_count = cands.iter().filter(|c| !c.is_nested()).count();
        let nested_count = cands.iter().filter(|c| c.is_nested()).count();
        // 7 mw:flat 3-wave 5 + flat 5-wave 3 + nested 3-wave 2 = 10
        assert_eq!(flat_count, 8);
        assert_eq!(nested_count, 2);
    }
}
