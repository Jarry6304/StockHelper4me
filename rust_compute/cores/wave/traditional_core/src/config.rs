// config.rs — TraditionalEngineConfig
//
// 對齊 Traditional Core v2 `references/engine.md` §輸入。原書(Frost & Prechter EWP)無
// pivot 演算法、無數值容差 → 以下數值皆 `[工程添加]`,於各 fn doc-comment 標註。

use fact_schema::Timeframe;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TraditionalEngineConfig {
    pub timeframe: Timeframe,
    /// `[工程添加]` Fibonacci 比率容差(僅計指引,**永不淘汰**;形態優先,比例為輔)。
    pub fib_tolerance: f64,
    /// `[工程添加]` Forest 上限保護(P0 Gate 校準);超過走 beam fallback(beam 鍵 = preference_score)。
    pub forest_max_size: usize,
    /// `[工程添加]` v3 monowave 數值雜訊守門:`|Δclose|` 小於 `start×epsilon` 視為平盤(只去數值塵,
    /// **不**抹低度數)。0.0 = 純每根反轉。P0-Gate 可調。
    pub monowave_epsilon: f64,
    /// `[工程添加]` v3 compaction per-round scenario beam(每度數保留 top-N tiling,控 forest 爆炸)。
    pub round_beam_size: usize,
    /// `[工程添加]` v3 compaction 最大度數層數(degree ceiling 的工程硬上限)。
    pub max_degree_levels: usize,
}

impl Default for TraditionalEngineConfig {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            fib_tolerance: 0.04,
            forest_max_size: 200,
            // P0-Gate 校準(2026-06-04):全市場 run-all 在 epsilon=0.0 時每股 ~135s
            // (monowave 不過濾 → base 爆炸 → compaction O(beam×clone) 爆)。0.03 = 3%
            // 反轉雜訊門檻,把 base 砍到 neely 量級。可 env `TRAD_MONOWAVE_EPSILON` 覆寫 sweep。
            monowave_epsilon: 0.03,
            round_beam_size: 64,
            max_degree_levels: 8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_spec() {
        let c = TraditionalEngineConfig::default();
        assert!((c.fib_tolerance - 0.04).abs() < 1e-9);
        assert_eq!(c.forest_max_size, 200);
        assert_eq!(c.round_beam_size, 64);
        assert_eq!(c.max_degree_levels, 8);
    }
}
