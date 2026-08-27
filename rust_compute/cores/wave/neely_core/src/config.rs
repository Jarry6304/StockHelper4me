// NeelyCoreParams + NeelyEngineConfig + OverflowStrategy
// 對齊 m3Spec/neely_core_architecture.md §六(2026-05-06 r2)。

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

    /// Compaction v2 round 引擎:round 內 tiling pool 上限(compaction v2 §5.5 /
    /// 附錄 A;預設 32,Gate 校準)。與 forest_max_size / BeamSearchFallback
    /// 分層並存:前者控 round 內分支爆炸,後者控輸出上限。
    pub round_beam_size: usize,

    /// Compaction 聚合輪數上限(compaction v2 A-8 定案:常數 4 → config 欄;
    /// 工程參數非 Neely 常數,可外部化不違 architecture §6.6)。
    /// 觸頂造成的枚舉缺漏由 `level_cap_hit` 診斷旗標可觀察,動態化另議。
    pub max_compaction_levels: usize,

    /// Forest 上限保護:超過此 size 用 BeamSearchFallback
    ///
    /// **校準歷史**:
    /// - r3 暫定 1000(P0 Gate 校準前 placeholder)
    /// - 2026-05-14 P0 Gate v2(1264 stocks production):forest max=37,p99=16,p95=10
    ///   → 1000 過鬆,降至 **200**(留 5× p99 餘量 + 容受極端股票)
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
            round_beam_size: 32,
            max_compaction_levels: 4,
            // P0 Gate v2 production scale 校準後(1264 stocks):降 1000 → 200(留 5× p99 餘量)
            forest_max_size: 200,
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
        assert_eq!(cfg.round_beam_size, 32); // compaction v2 附錄 A(G2.1)
        assert_eq!(cfg.max_compaction_levels, 4); // compaction v2 A-8 定案
        assert_eq!(cfg.forest_max_size, 200); // P0 Gate v2 校準後(2026-05-14 production 1264 stocks max=37)
        assert_eq!(cfg.compaction_timeout_secs, 60);
        assert!((cfg.neutral_threshold_taiex - 0.5).abs() < 1e-9);
        match cfg.overflow_strategy {
            OverflowStrategy::BeamSearchFallback { k } => assert_eq!(k, 100),
            _ => panic!("default overflow_strategy 應為 BeamSearchFallback {{ k: 100 }}"),
        }
    }
}
