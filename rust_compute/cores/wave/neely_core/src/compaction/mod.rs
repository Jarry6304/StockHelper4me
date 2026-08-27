// compaction — Stage 8:Compaction v2 tiling-round 引擎 + Forest 上限保護
//
// 對齊 m3Spec/neely_compaction_v2.md(r4)§3/§5/§7
//     + m3Spec/neely_core_architecture.md §三 / §七 Stage 8 / §十一 / §十二。
//
// 子模組:
//   - round_engine.rs — Compaction v2 tiling-round 引擎(G2.4 切換後 serving:
//                       base tiling → W1–W7 階梯 → round 迴圈 → §7 凍結)
//   - beam_search.rs  — Forest 上限保護的 fallback(§12;雙重排序 §10.3)
//
// 關鍵設計:
//   - 純結構壓縮,**不**選最優,不附 primary(§9.3)
//   - 定義域 = tiling(時間軸連續分割,D1);多解讀各產 tiling 分支交 forest
//   - v3.7 遞迴迴圈(exhaustive.rs)與弱比對聚合(three_rounds.rs)已依
//     spec §3.3 於 P0 Gate v3 收案後刪除(2026-08-27,Gate 報告見
//     docs/benchmarks/neely_compaction_v2_gate_results_2026-08-27.md)
//
// Forest 上限保護 / 逾時保護(§7.1 步驟 4 原樣,architecture §10):
//   - 凍結數超過 forest_max_size → BeamSearchFallback(雙重排序 top-K)
//   - elapsed > compaction_timeout_secs → 以當前已凍結內容返回並標
//     compaction_timeout(round_engine 逾時檢查下沉至視窗粒度)

use crate::output::PowerRating;

pub mod beam_search;
pub mod round_engine;

/// PowerRating 排序(對齊 §9.1 enum):StrongBullish > Bullish > SlightBullish >
/// Neutral > SlightBearish > Bearish > StrongBearish。BeamSearch 用「magnitude」
/// 排序(|rating - Neutral|),保留兩端極值;同 magnitude 時保留 Bullish 側。
pub(crate) fn power_rating_magnitude(p: PowerRating) -> i32 {
    match p {
        PowerRating::StrongBullish => 3,
        PowerRating::Bullish => 2,
        PowerRating::SlightBullish => 1,
        PowerRating::Neutral => 0,
        PowerRating::SlightBearish => -1,
        PowerRating::Bearish => -2,
        PowerRating::StrongBearish => -3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_rating_magnitude_ordering() {
        assert_eq!(power_rating_magnitude(PowerRating::StrongBullish), 3);
        assert_eq!(power_rating_magnitude(PowerRating::Neutral), 0);
        assert_eq!(power_rating_magnitude(PowerRating::StrongBearish), -3);
    }
}
