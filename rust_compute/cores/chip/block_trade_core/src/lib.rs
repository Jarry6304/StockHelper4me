// block_trade_core(P2)— 對齊 m3Spec/chip_cores.md §十一(v3.21 拍版)
//
// 4 EventKind:LargeBlockTrade / Accumulation / Distribution / MatchingTradeSpike
//
// Reference:
// - matching_trade_share_threshold = 0.80:Cao, Field & Hanka (2009),
//   "Block Trading and Stock Prices" *Journal of Empirical Finance* 16:1-25
//   — matched trades 通常占 block trade volume 50-70%(成熟市場),>80% 視為異常集中

use anyhow::Result;
use chip_loader::{BlockTradeRaw, BlockTradeSeries};
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "block_trade_core", "0.1.0", core_registry::CoreKind::Chip, "P2",
        "Block Trade Core(LargeBlock / Accumulation / Distribution / MatchingSpike)",
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockTradeParams {
    pub timeframe: Timeframe,
    pub min_trading_money_threshold: i64,     // 預設 100_000_000 NTD
    pub z_score_threshold: f64,               // 預設 2.5
    pub lookback_for_z: usize,                // 預設 60
    pub streak_min_days: usize,               // 預設 3
    pub matching_trade_share_threshold: f64,  // 預設 0.80(Cao 2009)
}

impl Default for BlockTradeParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            min_trading_money_threshold: 100_000_000,
            z_score_threshold: 2.5,
            lookback_for_z: 60,
            streak_min_days: 3,
            matching_trade_share_threshold: 0.80,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockTradeOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub events: Vec<BlockTradeEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BlockTradeEvent {
    pub date: NaiveDate,
    pub kind: BlockTradeEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum BlockTradeEventKind {
    LargeBlockTrade,
    BlockTradeAccumulation,
    BlockTradeDistribution,
    MatchingTradeSpike,
}

pub struct BlockTradeCore;
impl BlockTradeCore { pub fn new() -> Self { BlockTradeCore } }
impl Default for BlockTradeCore { fn default() -> Self { BlockTradeCore::new() } }

/// rolling z-score 對 total_trading_money(對個股自身分布)
fn rolling_z(values: &[f64], window: usize, idx: usize) -> Option<f64> {
    if idx < window || window < 2 { return None; }
    let slice = &values[idx - window .. idx];
    let n = slice.len() as f64;
    let mean = slice.iter().sum::<f64>() / n;
    let var = slice.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    let std = var.sqrt();
    if std <= f64::EPSILON { return None; }
    Some((values[idx] - mean) / std)
}

impl IndicatorCore for BlockTradeCore {
    type Input = BlockTradeSeries;
    type Params = BlockTradeParams;
    type Output = BlockTradeOutput;

    fn name(&self) -> &'static str { "block_trade_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn warmup_periods(&self, params: &Self::Params) -> usize { params.lookback_for_z + 10 }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let mut events = Vec::new();
        let n = input.points.len();
        let monies: Vec<f64> = input.points.iter()
            .map(|p| p.total_trading_money.unwrap_or(0) as f64)
            .collect();

        // 對 BlockTradeAccumulation / Distribution(連續 streak 偵測,基於 total_volume sign):
        // 這裡簡化為:total_volume > 0 視為「上方流入」,持續 streak_min_days 同向 → fire。
        // (block trade 沒有「淨流入」概念;v1 用 trading_money 連續 ≥ min_trading_money_threshold
        // 視為「持續大宗活動」,Acc vs Dist 由 matching_share 偏向判斷)
        let mut streak_days: usize = 0;
        let mut streak_start: Option<NaiveDate> = None;
        let mut cum_volume: i64 = 0;

        for i in 0..n {
            let p: &BlockTradeRaw = &input.points[i];

            // 1. LargeBlockTrade(z-score edge trigger)
            let cur_money = monies[i];
            if let Some(z) = rolling_z(&monies, params.lookback_for_z, i) {
                let prev_z = if i > 0 { rolling_z(&monies, params.lookback_for_z, i - 1) } else { None };
                let prev_below = prev_z.map(|v| v < params.z_score_threshold).unwrap_or(true);
                if z >= params.z_score_threshold
                    && prev_below
                    && (cur_money as i64) >= params.min_trading_money_threshold
                {
                    events.push(BlockTradeEvent {
                        date: p.date,
                        kind: BlockTradeEventKind::LargeBlockTrade,
                        value: z,
                        metadata: json!({
                            "z_score": z,
                            "total_trading_money": cur_money as i64,
                            "matching_share": p.matching_share,
                            "largest_single_trade_money": p.largest_single_trade_money,
                            "trade_type_count": p.trade_type_count,
                        }),
                    });
                }
            }

            // 2/3. Accumulation / Distribution(streak)
            let active = (cur_money as i64) >= params.min_trading_money_threshold;
            if active {
                if streak_days == 0 { streak_start = Some(p.date); cum_volume = 0; }
                streak_days += 1;
                cum_volume += p.total_volume.unwrap_or(0);
                // 在 streak 結束日 fire(若 streak_days >= min);本實作在「連續 streak_min_days
                // 達標當日」fire edge trigger(對齊 institutional_core NetBuyStreak 慣例)
                if streak_days == params.streak_min_days {
                    let kind = if p.matching_share.unwrap_or(0.0) >= 0.5 {
                        BlockTradeEventKind::BlockTradeAccumulation  // 配對主導 → 視為機構建倉
                    } else {
                        BlockTradeEventKind::BlockTradeDistribution  // 非配對主導 → 視為公開賣出
                    };
                    events.push(BlockTradeEvent {
                        date: p.date,
                        kind,
                        value: cum_volume as f64,
                        metadata: json!({
                            "start_date": streak_start.map(|d| d.format("%Y-%m-%d").to_string()),
                            "end_date": p.date.format("%Y-%m-%d").to_string(),
                            "days": streak_days,
                            "cumulative_volume": cum_volume,
                        }),
                    });
                }
            } else {
                streak_days = 0;
                streak_start = None;
                cum_volume = 0;
            }

            // 4. MatchingTradeSpike(配對交易單日佔比 > threshold)
            if let Some(share) = p.matching_share {
                if share >= params.matching_trade_share_threshold {
                    events.push(BlockTradeEvent {
                        date: p.date,
                        kind: BlockTradeEventKind::MatchingTradeSpike,
                        value: share,
                        metadata: json!({
                            "matching_share": share,
                            "matching_trading_money": p.matching_trading_money,
                            "total_trading_money": p.total_trading_money,
                        }),
                    });
                }
            }
        }

        Ok(BlockTradeOutput {
            stock_id: input.stock_id.clone(),
            timeframe: params.timeframe,
            events,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(),
            fact_date: e.date,
            timeframe: output.timeframe,
            source_core: "block_trade_core".to_string(),
            source_version: "0.1.0".to_string(),
            params_hash: None,
            statement: format!("BlockTrade {:?} on {}: value={:.4}", e.kind, e.date, e.value),
            metadata: fact_schema::with_event_kind(e.metadata.clone(), &e.kind),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(date: &str, money: i64, share: f64) -> BlockTradeRaw {
        BlockTradeRaw {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            total_volume: Some(50_000),
            total_trading_money: Some(money),
            matching_volume: Some(30_000),
            matching_trading_money: Some((money as f64 * share) as i64),
            matching_share: Some(share),
            largest_single_trade_money: Some(money / 2),
            trade_type_count: Some(2),
        }
    }

    #[test]
    fn name_warmup() {
        let core = BlockTradeCore::new();
        assert_eq!(core.name(), "block_trade_core");
        assert_eq!(core.warmup_periods(&BlockTradeParams::default()), 70);
    }

    #[test]
    fn matching_trade_spike_above_80_pct() {
        let core = BlockTradeCore::new();
        let input = BlockTradeSeries {
            stock_id: "2330".to_string(),
            points: vec![pt("2025-01-02", 200_000_000, 0.85)],
        };
        let out = core.compute(&input, BlockTradeParams::default()).unwrap();
        let spikes: Vec<_> = out.events.iter()
            .filter(|e| e.kind == BlockTradeEventKind::MatchingTradeSpike).collect();
        assert_eq!(spikes.len(), 1);
    }

    #[test]
    fn streak_accumulation_3_days() {
        let core = BlockTradeCore::new();
        let mut points = Vec::new();
        for i in 0..3 {
            let d = format!("2025-01-{:02}", i + 1);
            points.push(pt(&d, 150_000_000, 0.6));  // matching > 50% → Accumulation
        }
        let input = BlockTradeSeries { stock_id: "2330".to_string(), points };
        let out = core.compute(&input, BlockTradeParams::default()).unwrap();
        let acc: Vec<_> = out.events.iter()
            .filter(|e| e.kind == BlockTradeEventKind::BlockTradeAccumulation).collect();
        assert_eq!(acc.len(), 1, "edge trigger at streak=3");
    }

    #[test]
    fn no_streak_below_threshold() {
        let core = BlockTradeCore::new();
        let mut points = Vec::new();
        for i in 0..5 {
            let d = format!("2025-01-{:02}", i + 1);
            points.push(pt(&d, 50_000_000, 0.4));  // 小於 100M threshold
        }
        let input = BlockTradeSeries { stock_id: "2330".to_string(), points };
        let out = core.compute(&input, BlockTradeParams::default()).unwrap();
        let acc: Vec<_> = out.events.iter()
            .filter(|e| matches!(e.kind, BlockTradeEventKind::BlockTradeAccumulation | BlockTradeEventKind::BlockTradeDistribution))
            .collect();
        assert_eq!(acc.len(), 0);
    }
}
