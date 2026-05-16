#![allow(clippy::needless_range_loop)]
// support_resistance_core(P2)— 對齊 m3Spec/indicator_cores_pattern.md §四
// Params §4.2 / warmup §4.3 / Output §4.4 / Fact §4.5
//
// Reference:
//   Edwards & Magee (1948), "Technical Analysis of Stock Trends" Ch.13:撐壓位概念原作
//   Murphy (1999) p.55 — 撐壓位定義 + 撐壓互換規則
//   touch_count_min=3:Murphy「3 個觸碰才算有效撐壓」
//   price_cluster_tolerance=0.01:聚類容差 1%(慣例 — 個股一般單日波動 1-2%)
//
// 演算法:
//   1. 用 rolling local extreme(window=PIVOT_WINDOW)找 swing high / swing low
//   2. 把所有 swing 點按價位聚類(±tolerance%),≥ touch_count_min 為候選 SR
//   3. 過濾過於靠近的 levels(min_distance_between_levels)
//   4. 把候選分成 support(low pivot 為主)/ resistance(high pivot 為主)

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::{OhlcvBar, OhlcvSeries};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "support_resistance_core", "0.1.0", core_registry::CoreKind::Indicator, "P2",
        "Support/Resistance Core(靜態撐壓位識別)",
    )
}

/// pivot 偵測 window — 對齊 NEoWave 慣例(同 neely_core / mfi_core PIVOT_N=3)
const PIVOT_WINDOW: usize = 3;

/// 撐壓互換(level_flip)後續回測 N 棒(spec §4.6)
const FLIP_RETEST_WINDOW: usize = 30;

#[derive(Debug, Clone, Serialize)]
pub struct SupportResistanceParams {
    pub lookback_bars: usize,
    pub touch_count_min: usize,
    pub price_cluster_tolerance: f64,
    pub min_distance_between_levels: f64,
    pub timeframe: Timeframe,
}
impl Default for SupportResistanceParams {
    fn default() -> Self {
        Self {
            lookback_bars: 120,
            touch_count_min: 3,
            price_cluster_tolerance: 0.01,
            min_distance_between_levels: 0.02,
            timeframe: Timeframe::Daily,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum SrKind {
    Support,
    Resistance,
}

#[derive(Debug, Clone, Serialize)]
pub struct SrStrength {
    pub recency_bars: usize,
    pub time_span_bars: usize,
    pub avg_volume_at_touches: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SrLevel {
    pub price: f64,
    pub level_kind: SrKind,
    pub touch_count: usize,
    pub touch_dates: Vec<NaiveDate>,
    pub first_seen: NaiveDate,
    pub last_seen: NaiveDate,
    pub strength_metric: SrStrength,
}

#[derive(Debug, Clone, Serialize)]
pub struct SrEvent {
    pub date: NaiveDate,
    pub kind: SrEventKind,
    pub price: f64,
    pub metadata: serde_json::Value,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum SrEventKind {
    SupportBreak,
    ResistanceBreak,
    LevelFlip,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupportResistanceOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub support_levels: Vec<SrLevel>,
    pub resistance_levels: Vec<SrLevel>,
    pub generated_at: NaiveDate,
    #[serde(skip)]
    pub events: Vec<SrEvent>,
}

pub struct SupportResistanceCore;
impl SupportResistanceCore {
    pub fn new() -> Self {
        SupportResistanceCore
    }
}
impl Default for SupportResistanceCore {
    fn default() -> Self {
        SupportResistanceCore::new()
    }
}

/// 找 swing point — value[i] 是 PIVOT_WINDOW 範圍內局部 max(high pivot)or min(low pivot)
fn find_pivots(bars: &[OhlcvBar], use_high: bool) -> Vec<usize> {
    let n = bars.len();
    let mut out = Vec::new();
    if n < 2 * PIVOT_WINDOW + 1 {
        return out;
    }
    for i in PIVOT_WINDOW..n - PIVOT_WINDOW {
        let cur = if use_high { bars[i].high } else { bars[i].low };
        let mut is_pivot = true;
        for j in 1..=PIVOT_WINDOW {
            let lhs = if use_high { bars[i - j].high } else { bars[i - j].low };
            let rhs = if use_high { bars[i + j].high } else { bars[i + j].low };
            if use_high {
                if lhs >= cur || rhs >= cur {
                    is_pivot = false;
                    break;
                }
            } else if lhs <= cur || rhs <= cur {
                is_pivot = false;
                break;
            }
        }
        if is_pivot {
            out.push(i);
        }
    }
    out
}

/// 把 pivots 按價位聚類(±tolerance%),回 (avg_price, indices) 列表
fn cluster_pivots(
    bars: &[OhlcvBar],
    pivots: &[usize],
    use_high: bool,
    tolerance: f64,
) -> Vec<(f64, Vec<usize>)> {
    let mut sorted: Vec<usize> = pivots.to_vec();
    sorted.sort_by(|&a, &b| {
        let pa = if use_high { bars[a].high } else { bars[a].low };
        let pb = if use_high { bars[b].high } else { bars[b].low };
        pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut clusters: Vec<(f64, Vec<usize>)> = Vec::new();
    for &idx in &sorted {
        let price = if use_high { bars[idx].high } else { bars[idx].low };
        let mut placed = false;
        for c in clusters.iter_mut() {
            let center = c.0;
            if (price - center).abs() / center <= tolerance {
                // update centroid weighted by count
                let count = c.1.len() as f64;
                c.0 = (center * count + price) / (count + 1.0);
                c.1.push(idx);
                placed = true;
                break;
            }
        }
        if !placed {
            clusters.push((price, vec![idx]));
        }
    }
    clusters
}

impl IndicatorCore for SupportResistanceCore {
    type Input = OhlcvSeries;
    type Params = SupportResistanceParams;
    type Output = SupportResistanceOutput;
    fn name(&self) -> &'static str {
        "support_resistance_core"
    }
    fn version(&self) -> &'static str {
        "0.1.0"
    }
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.lookback_bars + 10
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let n = input.bars.len();
        // 只看最近 lookback_bars 棒(對齊 spec §4.2 default 120)
        let start = n.saturating_sub(params.lookback_bars);
        let bars = &input.bars[start..];
        let nb = bars.len();

        let resistance_pivots = find_pivots(bars, true);
        let support_pivots = find_pivots(bars, false);

        let resistance_clusters = cluster_pivots(bars, &resistance_pivots, true, params.price_cluster_tolerance);
        let support_clusters = cluster_pivots(bars, &support_pivots, false, params.price_cluster_tolerance);

        // 過濾觸碰數 >= touch_count_min,build SrLevel
        let build_level = |cluster: &(f64, Vec<usize>), kind: SrKind| -> Option<SrLevel> {
            if cluster.1.len() < params.touch_count_min {
                return None;
            }
            let mut touch_dates: Vec<NaiveDate> = cluster.1.iter().map(|&i| bars[i].date).collect();
            touch_dates.sort();
            let first_seen = touch_dates[0];
            let last_seen = touch_dates[touch_dates.len() - 1];
            let avg_volume = if !cluster.1.is_empty() {
                cluster
                    .1
                    .iter()
                    .map(|&i| bars[i].volume.unwrap_or(0).max(0) as f64)
                    .sum::<f64>()
                    / cluster.1.len() as f64
            } else {
                0.0
            };
            let last_idx = *cluster.1.iter().max().unwrap_or(&0);
            let recency = nb.saturating_sub(last_idx + 1);
            let span_idx_min = *cluster.1.iter().min().unwrap_or(&0);
            let span_bars = last_idx.saturating_sub(span_idx_min);
            Some(SrLevel {
                price: cluster.0,
                level_kind: kind,
                touch_count: cluster.1.len(),
                touch_dates,
                first_seen,
                last_seen,
                strength_metric: SrStrength {
                    recency_bars: recency,
                    time_span_bars: span_bars,
                    avg_volume_at_touches: avg_volume,
                },
            })
        };
        let mut resistance_levels: Vec<SrLevel> = resistance_clusters
            .iter()
            .filter_map(|c| build_level(c, SrKind::Resistance))
            .collect();
        let mut support_levels: Vec<SrLevel> = support_clusters
            .iter()
            .filter_map(|c| build_level(c, SrKind::Support))
            .collect();

        // 過濾過於靠近的 levels(保留 touch_count 大的)
        let dedupe = |levels: Vec<SrLevel>| -> Vec<SrLevel> {
            let mut sorted = levels;
            sorted.sort_by(|a, b| b.touch_count.cmp(&a.touch_count));
            let mut kept: Vec<SrLevel> = Vec::new();
            for level in sorted {
                let too_close = kept.iter().any(|k| {
                    let center = (k.price + level.price) / 2.0;
                    if center > 0.0 {
                        (k.price - level.price).abs() / center < params.min_distance_between_levels
                    } else {
                        false
                    }
                });
                if !too_close {
                    kept.push(level);
                }
            }
            kept
        };
        resistance_levels = dedupe(resistance_levels);
        support_levels = dedupe(support_levels);

        // events:break + flip
        let mut events = Vec::new();
        for i in 1..nb {
            let prev_close = bars[i - 1].close;
            let cur_close = bars[i].close;
            for level in &support_levels {
                if prev_close >= level.price && cur_close < level.price * 0.98 {
                    events.push(SrEvent {
                        date: bars[i].date,
                        kind: SrEventKind::SupportBreak,
                        price: level.price,
                        metadata: json!({"event": "support_break", "broken_level": level.price, "close": cur_close}),
                    });
                }
            }
            for level in &resistance_levels {
                if prev_close <= level.price && cur_close > level.price * 1.02 {
                    events.push(SrEvent {
                        date: bars[i].date,
                        kind: SrEventKind::ResistanceBreak,
                        price: level.price,
                        metadata: json!({"event": "resistance_break", "broken_level": level.price, "close": cur_close}),
                    });
                }
            }
        }
        // level_flip:resistance break → 後續 30 棒回測 → 反彈
        for level in &resistance_levels {
            let break_idx = (1..nb).find(|&i| {
                bars[i - 1].close <= level.price && bars[i].close > level.price * 1.02
            });
            if let Some(bi) = break_idx {
                let end = (bi + FLIP_RETEST_WINDOW).min(nb);
                let retest_idx = (bi + 1..end).find(|&j| {
                    let low = bars[j].low;
                    (low - level.price).abs() / level.price < params.price_cluster_tolerance
                });
                if let Some(ri) = retest_idx {
                    let after_end = (ri + 5).min(nb);
                    let bounced = (ri..after_end).all(|k| bars[k].close >= level.price);
                    if bounced {
                        events.push(SrEvent {
                            date: bars[ri].date,
                            kind: SrEventKind::LevelFlip,
                            price: level.price,
                            metadata: json!({
                                "event": "level_flip",
                                "price": level.price,
                                "broken_at": bars[bi].date.to_string(),
                                "retested_at": bars[ri].date.to_string(),
                            }),
                        });
                    }
                }
            }
        }

        Ok(SupportResistanceOutput {
            stock_id: input.stock_id.clone(),
            timeframe: params.timeframe,
            support_levels,
            resistance_levels,
            generated_at: bars.last().map(|b| b.date).unwrap_or_default(),
            events,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        let mut facts: Vec<Fact> = Vec::new();
        for level in &output.support_levels {
            facts.push(Fact {
                stock_id: output.stock_id.clone(),
                fact_date: level.last_seen,
                timeframe: output.timeframe,
                source_core: "support_resistance_core".to_string(),
                source_version: "0.1.0".to_string(),
                params_hash: None,
                statement: format!(
                    "Support at {:.2} (touched {} times from {} to {})",
                    level.price, level.touch_count, level.first_seen, level.last_seen
                ),
                metadata: json!({
                    "event_kind": "Support",  // v3.4 r2 r5
                    "kind": "support",
                    "price": level.price,
                    "touch_count": level.touch_count,
                }),
            });
        }
        for level in &output.resistance_levels {
            facts.push(Fact {
                stock_id: output.stock_id.clone(),
                fact_date: level.last_seen,
                timeframe: output.timeframe,
                source_core: "support_resistance_core".to_string(),
                source_version: "0.1.0".to_string(),
                params_hash: None,
                statement: format!(
                    "Resistance at {:.2} (touched {} times from {} to {})",
                    level.price, level.touch_count, level.first_seen, level.last_seen
                ),
                metadata: json!({
                    "event_kind": "Resistance",  // v3.4 r2 r5
                    "kind": "resistance",
                    "price": level.price,
                    "touch_count": level.touch_count,
                }),
            });
        }
        for ev in &output.events {
            facts.push(Fact {
                stock_id: output.stock_id.clone(),
                fact_date: ev.date,
                timeframe: output.timeframe,
                source_core: "support_resistance_core".to_string(),
                source_version: "0.1.0".to_string(),
                params_hash: None,
                statement: format!("SR {:?} at {:.2} on {}", ev.kind, ev.price, ev.date),
                metadata: fact_schema::with_event_kind(ev.metadata.clone(), &ev.kind),
            });
        }
        facts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use fact_schema::Timeframe;

    fn make_series(bars: Vec<(f64, f64, f64)>) -> OhlcvSeries {
        let bars = bars
            .into_iter()
            .enumerate()
            .map(|(i, (h, l, c))| OhlcvBar {
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
                    + chrono::Duration::days(i as i64),
                open: c,
                high: h,
                low: l,
                close: c,
                volume: Some(1000),
            })
            .collect();
        OhlcvSeries {
            stock_id: "TEST".to_string(),
            timeframe: Timeframe::Daily,
            bars,
        }
    }

    #[test]
    fn name_and_warmup() {
        let core = SupportResistanceCore::new();
        assert_eq!(core.name(), "support_resistance_core");
        assert_eq!(core.warmup_periods(&SupportResistanceParams::default()), 130);
    }

    #[test]
    fn detects_repeated_resistance() {
        let core = SupportResistanceCore::new();
        // 構造 30 bars,期間 5 個 high 集中在 100 附近(觸碰 5 次)
        let mut bars: Vec<(f64, f64, f64)> = Vec::new();
        for i in 0..30 {
            let h = if i % 6 == 3 { 100.0 } else { 95.0 };
            let l = if i % 6 == 3 { 90.0 } else { 85.0 };
            let c = (h + l) / 2.0;
            bars.push((h, l, c));
        }
        let series = make_series(bars);
        let out = core.compute(&series, SupportResistanceParams::default()).unwrap();
        assert!(!out.resistance_levels.is_empty(), "should find at least one resistance");
        let level = &out.resistance_levels[0];
        assert!(level.touch_count >= 3);
        assert!((level.price - 100.0).abs() < 1.0);
    }
}
