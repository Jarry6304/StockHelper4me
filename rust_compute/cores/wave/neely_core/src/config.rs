// NeelyCoreParams + NeelyEngineConfig + OverflowStrategy
// 對齊 m3Spec/neely_core.md §六(2026-05-06 r2)。

use fact_schema::Timeframe;
use serde::Serialize;

/// Workflow toml 可宣告的「使用方選擇」(§6.1)
#[derive(Debug, Clone, Serialize)]
pub struct NeelyCoreParams {
    pub timeframe: Timeframe,
    pub engine_config: NeelyEngineConfig,
}

impl Default for NeelyCoreParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            engine_config: NeelyEngineConfig::default(),
        }
    }
}

/// Core 內部工程參數(§6.3)— 可調但有預設,**不**屬 Neely 規則本身
#[derive(Debug, Clone, Serialize)]
pub struct NeelyEngineConfig {
    /// ATR 計算週期。Rule of Proportion / Neutrality / 45° 判定的計量單位。
    /// 跨 timeframe 統一,屬「約定俗成的工程慣例」非主觀調參(§6.5)。
    pub atr_period: usize,

    /// Bottom-up Candidate Generator 的 beam width
    pub beam_width: usize,

    /// Forest 上限保護:超過此 size 用 BeamSearchFallback
    /// r3 暫定 1000,P0 五檔實測後校準
    pub forest_max_size: usize,

    /// 單檔 Compaction 逾時(秒)
    pub compaction_timeout_secs: u64,

    /// Forest 超過 max_size 時的處理策略
    pub overflow_strategy: OverflowStrategy,

    /// 加權指數套用 Rule of Neutrality 的中性區判定閾值(個股不適用,§10.4)
    /// 單位:%
    pub neutral_threshold_taiex: f64,
}

impl Default for NeelyEngineConfig {
    fn default() -> Self {
        Self {
            atr_period: 14,
            beam_width: 50,
            forest_max_size: 1000,
            compaction_timeout_secs: 60,
            overflow_strategy: OverflowStrategy::BeamSearchFallback { k: 100 },
            neutral_threshold_taiex: 0.5,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum OverflowStrategy {
    /// 用 power_rating 排序保留 top-K,並標記 overflow_triggered
    BeamSearchFallback { k: usize },

    /// 不剪枝(P0 Gate 校準階段使用,生產環境不建議)
    Unbounded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_engine_config_matches_spec_section_6_4() {
        let cfg = NeelyEngineConfig::default();
        assert_eq!(cfg.atr_period, 14);
        assert_eq!(cfg.beam_width, 50);
        assert_eq!(cfg.forest_max_size, 1000);
        assert_eq!(cfg.compaction_timeout_secs, 60);
        assert!((cfg.neutral_threshold_taiex - 0.5).abs() < 1e-9);
        match cfg.overflow_strategy {
            OverflowStrategy::BeamSearchFallback { k } => assert_eq!(k, 100),
            _ => panic!("default overflow_strategy 應為 BeamSearchFallback {{ k: 100 }}"),
        }
    }
}
