// config.rs — TraditionalEngineConfig
//
// 對齊 Traditional Core v2 `references/engine.md` §輸入。原書(Frost & Prechter EWP)無
// pivot 演算法、無數值容差 → 以下數值皆 `[工程添加]`,於各 fn doc-comment 標註。

use fact_schema::Timeframe;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TraditionalEngineConfig {
    pub timeframe: Timeframe,
    /// `[工程添加]` ATR 計算週期(Wilder)。pivot 顯著性的計量單位。
    pub atr_period: usize,
    /// `[工程添加]` pivot 顯著性門檻:`|Δ| ≥ ATR × swing_atr_multiplier`。
    pub swing_atr_multiplier: f64,
    /// `[工程添加]` Fibonacci 比率容差(僅計指引,**永不淘汰**;形態優先,比例為輔)。
    pub fib_tolerance: f64,
    /// `[工程添加]` Forest 上限保護(P0 Gate 校準);超過走 beam fallback(beam 鍵 = preference_score)。
    pub forest_max_size: usize,
}

impl Default for TraditionalEngineConfig {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            atr_period: 14,
            swing_atr_multiplier: 3.0,
            fib_tolerance: 0.04,
            forest_max_size: 200,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_spec() {
        let c = TraditionalEngineConfig::default();
        assert_eq!(c.atr_period, 14);
        assert!((c.swing_atr_multiplier - 3.0).abs() < 1e-9);
        assert!((c.fib_tolerance - 0.04).abs() < 1e-9);
        assert_eq!(c.forest_max_size, 200);
    }
}
