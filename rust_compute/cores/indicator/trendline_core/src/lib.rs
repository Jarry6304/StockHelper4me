#![allow(clippy::needless_range_loop)]
// trendline_core(P2)— 對齊 m3Spec/indicator_cores_pattern.md §五
// Params §5.3 / warmup §5.4 / Output §5.5 / Fact §5.6 / 耦合管控 §5.7-5.8
//
// Reference:
//   Edwards & Magee (1948), "Technical Analysis of Stock Trends" Ch.4-5:trendline 原作
//   Schwager (1996), "Schwager on Futures: Technical Analysis" Ch.4:量化 trendline 規則
//   min_pivots=3:Schwager 三點確認原則
//   touch_tolerance=0.005:0.5% 對齊 candlestick_pattern_core tweezer
//   min_slope_bars=10:Schwager「太短的 trendline 沒有意義」下限
//
// **唯一耦合例外**(spec §5.2 + cores_overview §12):
//   trendline_core 是全 Core 系統中唯一允許消費另一個 Core 輸出的 Core,
//   讀取 neely_core 的 monowave_series 作為 swing 來源。
//   不讀 scenario forest / structural_facts 等其他 Neely 輸出。

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use neely_core::output::{Monowave, MonowaveDirection, OhlcvSeries};
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "trendline_core", "0.1.0", core_registry::CoreKind::Indicator, "P2",
        "Trendline Core(趨勢線偵測,唯一允許消費 neely_core 的耦合例外)",
    )
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum SwingSource {
    NeelyMonowave,
    SharedSwingDetector,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendlineParams {
    pub timeframe: Timeframe,
    pub swing_source: SwingSource,
    pub min_pivots: usize,
    pub touch_tolerance: f64,
    pub min_slope_bars: usize,
    pub max_lookback_bars: usize,
}
impl Default for TrendlineParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            swing_source: SwingSource::NeelyMonowave,
            // v1.34 Round 5 production calibration:全市場 737/yr/stock 嚴重噪音(60× spec 目標)
            // 三管齊下:min_pivots 3→5 + min_slope_bars 10→30 + MAX_TRENDLINES_PER_STOCK cap
            min_pivots: 5,
            touch_tolerance: 0.005,
            min_slope_bars: 30,
            max_lookback_bars: 250,
        }
    }
}

/// Trendline 全市場每股上限(top-K by total touch_count)。
///
/// **v1.34 Round 5 production calibration**:全市場 1263 stocks 跑出
/// 4423 facts/stock(737.2/yr,60× 嚴重噪音)。原 2-point combination O(n²)
/// 候選暴增 + min_slope_bars=10 過短 → trendlines 過多。
///
/// 加 MAX_TRENDLINES_PER_STOCK 上限保留 touch_count 最多的 50 條(對齊 Schwager 1996
/// 「真正有效的 trendlines 通常 < 20 條 per stock」放寬至 50 為 cap),預期
/// 737 → ~50/yr/stock(15× 降量)。
const MAX_TRENDLINES_PER_STOCK: usize = 50;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum TrendDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum TrendlineKind {
    Support,
    Resistance,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum TrendlineStatus {
    Active,
    Broken,
    Reclaimed,
}

#[derive(Debug, Clone, Serialize)]
pub struct PivotRef {
    pub date: NaiveDate,
    pub price: f64,
    pub neely_monowave_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Trendline {
    pub id: String,
    pub direction: TrendDirection,
    pub kind: TrendlineKind,
    pub anchor_pivots: Vec<PivotRef>,
    pub additional_touches: Vec<NaiveDate>,
    pub start_date: NaiveDate,
    pub last_valid_date: NaiveDate,
    pub slope: f64,
    pub status: TrendlineStatus,
    pub broken_at: Option<NaiveDate>,
    pub source_core: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendlineEvent {
    pub date: NaiveDate,
    pub kind: TrendlineEventKind,
    pub trendline_id: String,
    pub metadata: serde_json::Value,
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum TrendlineEventKind {
    Break,
    Reclaim,
    Retest,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrendlineOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub trendlines: Vec<Trendline>,
    pub generated_at: NaiveDate,
    #[serde(skip)]
    pub events: Vec<TrendlineEvent>,
}

pub struct TrendlineCore;
impl TrendlineCore {
    pub fn new() -> Self {
        TrendlineCore
    }
}
impl Default for TrendlineCore {
    fn default() -> Self {
        TrendlineCore::new()
    }
}

/// Input 是 (OhlcvSeries + Vec<Monowave>) 的 wrap struct,讓 IndicatorCore::Input 為單一型別
pub struct TrendlineInput {
    pub ohlcv: OhlcvSeries,
    pub monowaves: Vec<Monowave>,
}

/// 把 Monowave 轉成 swing pivot(start + end date / price)
fn monowaves_to_pivots(monowaves: &[Monowave]) -> Vec<(NaiveDate, f64, MonowaveDirection)> {
    // 對相鄰 monowave:end 點是 pivot(方向翻轉處)
    let mut pivots: Vec<(NaiveDate, f64, MonowaveDirection)> = Vec::new();
    for m in monowaves {
        pivots.push((m.end_date, m.end_price, m.direction));
    }
    pivots
}

/// 從 swing pivots 中找出共線(至少 min_pivots 個)的趨勢線
///
/// 演算法(對齊 Schwager 1996 三點確認原則):
///   - 對每兩 swing 點作直線(2-point trendline)
///   - 檢查後續 swing 點是否在線上(±tolerance)
///   - ≥ min_pivots 才算有效
fn find_trendlines(
    pivots: &[(NaiveDate, f64, MonowaveDirection)],
    bars: &[ohlcv_loader::OhlcvBar],
    params: &TrendlineParams,
) -> Vec<Trendline> {
    let mut trendlines: Vec<Trendline> = Vec::new();
    if pivots.len() < params.min_pivots {
        return trendlines;
    }
    // 把 pivot 分成 high(direction=Up 結束=峰) / low(direction=Down 結束=谷)
    let highs: Vec<(NaiveDate, f64)> = pivots
        .iter()
        .filter(|p| matches!(p.2, MonowaveDirection::Up))
        .map(|p| (p.0, p.1))
        .collect();
    let lows: Vec<(NaiveDate, f64)> = pivots
        .iter()
        .filter(|p| matches!(p.2, MonowaveDirection::Down))
        .map(|p| (p.0, p.1))
        .collect();

    // 把日期轉成「相對 bar 索引」(O(1) lookup;v3.11 Round 7 把 O(n) linear scan
    // 改 HashMap pre-build,find_trendlines 整體 O(n³) → O(n²),預期 1266 stocks
    // wall time 2734s → 數百秒級別)
    let date_to_idx_map: HashMap<NaiveDate, usize> = bars
        .iter()
        .enumerate()
        .map(|(i, b)| (b.date, i))
        .collect();
    let date_to_idx = |d: NaiveDate| -> Option<usize> { date_to_idx_map.get(&d).copied() };

    let extract_trendlines = |pts: &[(NaiveDate, f64)], kind: TrendlineKind| -> Vec<Trendline> {
        let mut out: Vec<Trendline> = Vec::new();
        let n = pts.len();
        if n < params.min_pivots {
            return out;
        }
        // 對所有 2-point combination:i < j,連線 → 檢查後續 j+1..n 是否在線上
        for i in 0..n - 1 {
            for j in (i + 1)..n {
                let (d1, p1) = pts[i];
                let (d2, p2) = pts[j];
                let idx1 = date_to_idx(d1);
                let idx2 = date_to_idx(d2);
                if idx1.is_none() || idx2.is_none() {
                    continue;
                }
                let idx1 = idx1.unwrap();
                let idx2 = idx2.unwrap();
                if idx2 <= idx1 {
                    continue;
                }
                let bar_span = idx2 - idx1;
                if bar_span < params.min_slope_bars {
                    continue;
                }
                let slope = (p2 - p1) / bar_span as f64;
                // 對 i+1..n(除 j 之外)點檢查是否在線上
                let mut anchor: Vec<PivotRef> = vec![
                    PivotRef {
                        date: d1,
                        price: p1,
                        neely_monowave_id: None,
                    },
                    PivotRef {
                        date: d2,
                        price: p2,
                        neely_monowave_id: None,
                    },
                ];
                let mut additional: Vec<NaiveDate> = Vec::new();
                for k in (i + 1)..n {
                    if k == j {
                        continue;
                    }
                    let (dk, pk) = pts[k];
                    let idxk = match date_to_idx(dk) {
                        Some(v) => v,
                        None => continue,
                    };
                    let expected = p1 + slope * (idxk as f64 - idx1 as f64);
                    if expected > 0.0 && (pk - expected).abs() / expected < params.touch_tolerance
                    {
                        if k < j {
                            anchor.insert(anchor.len() - 1, PivotRef {
                                date: dk,
                                price: pk,
                                neely_monowave_id: None,
                            });
                        } else {
                            additional.push(dk);
                        }
                    }
                }
                let total_pivots = anchor.len() + additional.len();
                if total_pivots < params.min_pivots {
                    continue;
                }
                let direction = if slope > 0.0 {
                    TrendDirection::Ascending
                } else {
                    TrendDirection::Descending
                };
                let last_date = additional.last().copied().unwrap_or(d2);
                out.push(Trendline {
                    id: format!(
                        "{}_{}_{}_{}",
                        match kind {
                            TrendlineKind::Support => "sup",
                            TrendlineKind::Resistance => "res",
                        },
                        d1,
                        d2,
                        total_pivots
                    ),
                    direction,
                    kind,
                    anchor_pivots: anchor,
                    additional_touches: additional,
                    start_date: d1,
                    last_valid_date: last_date,
                    slope,
                    status: TrendlineStatus::Active,
                    broken_at: None,
                    source_core: Some("neely_core".to_string()),
                });
            }
        }
        out
    };

    trendlines.extend(extract_trendlines(&highs, TrendlineKind::Resistance));
    trendlines.extend(extract_trendlines(&lows, TrendlineKind::Support));

    // 過濾:對重複的 trendlines(slope + 起點接近)只保留 pivot 數最多的
    trendlines.sort_by(|a, b| {
        b.anchor_pivots
            .len()
            .cmp(&a.anchor_pivots.len())
            .then(a.start_date.cmp(&b.start_date))
    });
    let mut kept: Vec<Trendline> = Vec::new();
    for tl in trendlines {
        let dup = kept.iter().any(|k| {
            (k.slope - tl.slope).abs() / k.slope.abs().max(1e-9) < 0.05
                && k.kind == tl.kind
                && (k.start_date - tl.start_date).num_days().abs() < 5
        });
        if !dup {
            kept.push(tl);
        }
    }
    // v1.34 Round 5:max cap top-K by total touch_count(對齊 Schwager 1996)
    if kept.len() > MAX_TRENDLINES_PER_STOCK {
        kept.sort_by(|a, b| {
            let total_a = a.anchor_pivots.len() + a.additional_touches.len();
            let total_b = b.anchor_pivots.len() + b.additional_touches.len();
            total_b.cmp(&total_a)
        });
        kept.truncate(MAX_TRENDLINES_PER_STOCK);
    }
    kept
}

impl IndicatorCore for TrendlineCore {
    type Input = TrendlineInput;
    type Params = TrendlineParams;
    type Output = TrendlineOutput;
    fn name(&self) -> &'static str {
        "trendline_core"
    }
    fn version(&self) -> &'static str {
        "0.1.0"
    }
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.max_lookback_bars + 10
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let pivots = monowaves_to_pivots(&input.monowaves);
        let mut trendlines = find_trendlines(&pivots, &input.ohlcv.bars, &params);

        // 偵測 trendline break / retest
        let mut events: Vec<TrendlineEvent> = Vec::new();
        for tl in trendlines.iter_mut() {
            let idx_start = input
                .ohlcv
                .bars
                .iter()
                .position(|b| b.date == tl.start_date);
            if idx_start.is_none() {
                continue;
            }
            let idx_start = idx_start.unwrap();
            for i in (idx_start + 1)..input.ohlcv.bars.len() {
                let bar = &input.ohlcv.bars[i];
                let expected = tl.anchor_pivots[0].price
                    + tl.slope * (i as f64 - idx_start as f64);
                if expected <= 0.0 {
                    continue;
                }
                let close = bar.close;
                let break_threshold = match tl.kind {
                    TrendlineKind::Support => expected * (1.0 - 0.02),
                    TrendlineKind::Resistance => expected * (1.0 + 0.02),
                };
                let broken = match tl.kind {
                    TrendlineKind::Support => close < break_threshold,
                    TrendlineKind::Resistance => close > break_threshold,
                };
                if broken && tl.status == TrendlineStatus::Active {
                    tl.status = TrendlineStatus::Broken;
                    tl.broken_at = Some(bar.date);
                    events.push(TrendlineEvent {
                        date: bar.date,
                        kind: TrendlineEventKind::Break,
                        trendline_id: tl.id.clone(),
                        metadata: json!({
                            "event": "trendline_break",
                            "direction": format!("{:?}", tl.direction),
                            "kind": format!("{:?}", tl.kind),
                            "close": close,
                            "expected": expected,
                        }),
                    });
                    break;
                }
            }
        }

        Ok(TrendlineOutput {
            stock_id: input.ohlcv.stock_id.clone(),
            timeframe: params.timeframe,
            trendlines,
            generated_at: input
                .ohlcv
                .bars
                .last()
                .map(|b| b.date)
                .unwrap_or_default(),
            events,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        let mut facts: Vec<Fact> = Vec::new();
        for tl in &output.trendlines {
            let touch_total = tl.anchor_pivots.len() + tl.additional_touches.len();
            facts.push(Fact {
                stock_id: output.stock_id.clone(),
                fact_date: tl.last_valid_date,
                timeframe: output.timeframe,
                source_core: "trendline_core".to_string(),
                source_version: "0.1.0".to_string(),
                params_hash: None,
                statement: format!(
                    "{:?} trendline from {} to {}, status={:?}, {} pivots",
                    tl.direction, tl.start_date, tl.last_valid_date, tl.status, touch_total
                ),
                metadata: json!({
                    // v3.4 r2 r5:per-EventKind 統計用 event_kind 欄(對齊 SQL 查詢)
                    "event_kind": format!("Trendline{:?}", tl.kind),
                    "direction": format!("{:?}", tl.direction),
                    "kind": format!("{:?}", tl.kind),
                    "status": format!("{:?}", tl.status),
                    "touch_count": touch_total,
                    "slope": tl.slope,
                    "source_core": tl.source_core,
                }),
            });
        }
        for ev in &output.events {
            facts.push(Fact {
                stock_id: output.stock_id.clone(),
                fact_date: ev.date,
                timeframe: output.timeframe,
                source_core: "trendline_core".to_string(),
                source_version: "0.1.0".to_string(),
                params_hash: None,
                statement: format!("Trendline {:?} ({}) on {}", ev.kind, ev.trendline_id, ev.date),
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
    use neely_core::output::{Monowave, MonowaveDirection, OhlcvBar, OhlcvSeries};

    fn make_bars(closes: Vec<(f64, f64, f64, f64)>) -> OhlcvSeries {
        let bars = closes
            .into_iter()
            .enumerate()
            .map(|(i, (o, h, l, c))| OhlcvBar {
                date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
                    + chrono::Duration::days(i as i64),
                open: o,
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
        let core = TrendlineCore::new();
        assert_eq!(core.name(), "trendline_core");
        assert_eq!(core.warmup_periods(&TrendlineParams::default()), 260);
    }

    #[test]
    fn empty_monowaves_yields_empty_output() {
        let core = TrendlineCore::new();
        let series = make_bars((0..50).map(|i| (100.0, 101.0, 99.0, 100.0 + i as f64 * 0.1)).collect());
        let input = TrendlineInput {
            ohlcv: series,
            monowaves: vec![],
        };
        let out = core.compute(&input, TrendlineParams::default()).unwrap();
        assert!(out.trendlines.is_empty());
    }

    #[test]
    fn five_collinear_lows_yields_support_trendline() {
        // v1.34 Round 5:默認 min_pivots=5,構造 5 個 ascending support pivots
        let core = TrendlineCore::new();
        // 100 bar,5 個 pivot lows at i=10/30/50/70/90,間距 20(>= min_slope_bars=30 跨度)
        // 上升斜率:price = 99.5 + 0.5 × (idx - 10) / 20,5 點共線
        let idx_lows = [10usize, 30, 50, 70, 90];
        let prices = [99.5f64, 100.0, 100.5, 101.0, 101.5];
        let mut bars: Vec<(f64, f64, f64, f64)> = Vec::new();
        for i in 0..100 {
            let base = 100.0 + (i as f64 * 0.1);
            let pivot_idx = idx_lows.iter().position(|&p| p == i);
            let bar = if let Some(k) = pivot_idx {
                let p = prices[k];
                (p + 2.0, p + 2.5, p - 0.5, p)
            } else {
                (base, base + 1.0, base - 1.0, base)
            };
            bars.push(bar);
        }
        let series = make_bars(bars);
        // 構造 5 個 Monowave:Down direction,end_price 在 pivot lows
        let monowaves: Vec<Monowave> = idx_lows
            .iter()
            .enumerate()
            .map(|(k, &i)| Monowave {
                start_date: series.bars[if k == 0 { 0 } else { idx_lows[k - 1] }].date,
                end_date: series.bars[i].date,
                start_price: if k == 0 { 100.0 } else { prices[k - 1] },
                end_price: prices[k],
                direction: MonowaveDirection::Down,
            })
            .collect();
        let input = TrendlineInput {
            ohlcv: series,
            monowaves,
        };
        let out = core.compute(&input, TrendlineParams::default()).unwrap();
        // min_pivots=5 default + min_slope_bars=30 → 5 個 lows 滿足 2-point base 至少跨 30 bars
        assert!(
            out.trendlines.iter().any(|t| matches!(t.kind, TrendlineKind::Support)),
            "should detect support trendline from 5 ascending lows"
        );
    }
}
