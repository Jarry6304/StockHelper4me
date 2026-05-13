// ma_core(P1)— 對齊 m2Spec/oldm2Spec/indicator_cores_momentum.md §七 r2
//
// Spec critical:
//   - Output 是 `MaOutput { series_by_spec: Vec<MaSeriesEntry> }`(非單一 series)
//   - MaKind: Sma / Ema / Wma / Dema / Tema / Hma(6 種)
//   - PriceSource: Close / Open / High / Low / Hl2 / Hlc3 / Ohlc4
//   - CrossPairPolicy: None / AllPairs / Pairs(Vec<(usize, usize)>)
//   - 跨均線交叉(SMA20 vs SMA60)單一實例內部偵測,對齊 §7.8 不違反零耦合
//
// Dema/Tema/Hma 演算法用 best-guess 標準公式,P0 後可校準。

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::OhlcvSeries;
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "ma_core", "0.1.0", core_registry::CoreKind::Indicator, "P1",
        "MA Core(SMA/EMA/WMA/Dema/Tema/Hma 同族 + cross detection)",
    )
}

/// Reference(2026-05-12 校準): Brock, Lakonishok & LeBaron (1992) JF 47(5):1731-1764
/// 使用單點 P_t ≥ m_t 加 1% band 過濾，無連續天數概念；Faber (2007) SSRN 962461
/// 採月底收盤快照，同樣無 streak 設計。
///
/// Production data 校準(2026-05-12)：MA20/SMA 以舊 constant=30 跑出 0.59次/股/年(🟢合理)。
/// 公式 `period * 3 / 2`，上限 30，下限 5，使 MA20 → 30（保持），MA5 → 7，MA10 → 15，
/// MA60+ → 30（上限）。避免前版 period/8 對 MA20 給出 3 天（過於頻繁）的問題。
fn above_ma_streak_min(period: usize) -> usize {
    (period * 3 / 2).min(30).max(5)
}

/// MaBullishCross / MaBearishCross / MaGoldenCross / MaDeathCross 最小間距。
/// Production data 校準(2026-05-12): BullishCross ~11.5/yr 🟠 → 目標 6–9/yr 🟢。
/// Verification: scripts/p2_calibration_data.sql §2 (ma_core / BullishCross|GoldenCross)。
/// 10-bar = 2 週;適用全部 6 種 MA(SMA/EMA/WMA/DEMA/TEMA/HMA)同一閾值。
const MIN_MA_CROSS_SPACING: usize = 10;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum MaKind { Sma, Ema, Wma, Dema, Tema, Hma }

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum PriceSource { Close, Open, High, Low, Hl2, Hlc3, Ohlc4 }

#[derive(Debug, Clone, Serialize)]
pub struct MaSpec { pub kind: MaKind, pub period: usize, pub source: PriceSource }

#[derive(Debug, Clone, Serialize)]
pub enum CrossPairPolicy {
    None,
    AllPairs,
    Pairs(Vec<(usize, usize)>),
}

#[derive(Debug, Clone, Serialize)]
pub struct MaParams {
    pub specs: Vec<MaSpec>,
    pub timeframe: Timeframe,
    pub detect_cross_pairs: CrossPairPolicy,
}
impl Default for MaParams {
    fn default() -> Self {
        Self {
            specs: vec![MaSpec { kind: MaKind::Sma, period: 20, source: PriceSource::Close }],
            timeframe: Timeframe::Daily,
            detect_cross_pairs: CrossPairPolicy::None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MaOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series_by_spec: Vec<MaSeriesEntry>,
    #[serde(skip)]
    pub events: Vec<MaEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaSeriesEntry { pub spec: MaSpec, pub series: Vec<MaPoint> }
#[derive(Debug, Clone, Serialize)]
pub struct MaPoint { pub date: NaiveDate, pub value: f64 }
#[derive(Debug, Clone, Serialize)]
pub struct MaEvent { pub date: NaiveDate, pub kind: MaEventKind, pub value: f64, pub metadata: serde_json::Value }
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum MaEventKind { MaBullishCross, MaBearishCross, MaGoldenCross, MaDeathCross, AboveMaStreak }

pub struct MaCore;
impl MaCore { pub fn new() -> Self { MaCore } }
impl Default for MaCore { fn default() -> Self { MaCore::new() } }

fn sma(v: &[f64], p: usize) -> Vec<f64> {
    let mut out = vec![0.0; v.len()];
    if v.is_empty() || p == 0 { return out; }
    let mut s = 0.0;
    for i in 0..v.len() { s += v[i]; if i >= p { s -= v[i - p]; } out[i] = s / (i + 1).min(p) as f64; }
    out
}
fn ema(v: &[f64], p: usize) -> Vec<f64> {
    let mut out = vec![0.0; v.len()];
    if v.is_empty() || p == 0 { return out; }
    let a = 2.0 / (p as f64 + 1.0);
    out[0] = v[0];
    for i in 1..v.len() { out[i] = a * v[i] + (1.0 - a) * out[i - 1]; }
    out
}
fn wma(v: &[f64], p: usize) -> Vec<f64> {
    let mut out = vec![0.0; v.len()];
    if v.is_empty() || p == 0 { return out; }
    for i in 0..v.len() {
        let pp = (i + 1).min(p);
        let mut num = 0.0; let mut den = 0.0;
        for k in 0..pp { let w = (k + 1) as f64; num += w * v[i - (pp - 1 - k)]; den += w; }
        out[i] = if den > 0.0 { num / den } else { 0.0 };
    }
    out
}
fn dema(v: &[f64], p: usize) -> Vec<f64> {
    let e1 = ema(v, p); let e2 = ema(&e1, p);
    e1.iter().zip(e2.iter()).map(|(a, b)| 2.0 * a - b).collect()
}
fn tema(v: &[f64], p: usize) -> Vec<f64> {
    let e1 = ema(v, p); let e2 = ema(&e1, p); let e3 = ema(&e2, p);
    (0..v.len()).map(|i| 3.0 * e1[i] - 3.0 * e2[i] + e3[i]).collect()
}
fn hma(v: &[f64], p: usize) -> Vec<f64> {
    // Hull MA = WMA(2*WMA(p/2) - WMA(p), sqrt(p))
    let half = p / 2;
    let w_half = wma(v, half.max(1));
    let w_full = wma(v, p);
    let raw: Vec<f64> = w_half.iter().zip(w_full.iter()).map(|(a, b)| 2.0 * a - b).collect();
    let sqrt_p = (p as f64).sqrt().round() as usize;
    wma(&raw, sqrt_p.max(1))
}

fn compute_ma(values: &[f64], spec: &MaSpec) -> Vec<f64> {
    match spec.kind {
        MaKind::Sma => sma(values, spec.period),
        MaKind::Ema => ema(values, spec.period),
        MaKind::Wma => wma(values, spec.period),
        MaKind::Dema => dema(values, spec.period),
        MaKind::Tema => tema(values, spec.period),
        MaKind::Hma => hma(values, spec.period),
    }
}

fn pick_source(bars: &[ohlcv_loader::OhlcvBar], src: PriceSource) -> Vec<f64> {
    bars.iter().map(|b| match src {
        PriceSource::Close => b.close,
        PriceSource::Open => b.open,
        PriceSource::High => b.high,
        PriceSource::Low => b.low,
        PriceSource::Hl2 => (b.high + b.low) / 2.0,
        PriceSource::Hlc3 => (b.high + b.low + b.close) / 3.0,
        PriceSource::Ohlc4 => (b.open + b.high + b.low + b.close) / 4.0,
    }).collect()
}

impl IndicatorCore for MaCore {
    type Input = OhlcvSeries;
    type Params = MaParams;
    type Output = MaOutput;
    fn name(&self) -> &'static str { "ma_core" }
    fn version(&self) -> &'static str { "0.1.0" }
    /// §7.4:各 kind 倍數,取 max + 5 緩衝
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.specs.iter().map(|s| match s.kind {
            MaKind::Sma | MaKind::Wma => s.period,
            MaKind::Ema => s.period * 4,
            MaKind::Dema => s.period * 6,
            MaKind::Tema => s.period * 8,
            MaKind::Hma => s.period * 2,
        }).max().unwrap_or(0) + 5
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let closes_for_streak: Vec<f64> = input.bars.iter().map(|b| b.close).collect();

        // 對每個 spec 算 series
        let mut series_by_spec: Vec<MaSeriesEntry> = Vec::with_capacity(params.specs.len());
        for spec in &params.specs {
            let src = pick_source(&input.bars, spec.source);
            let ma = compute_ma(&src, spec);
            let series: Vec<MaPoint> = (0..ma.len()).map(|i| MaPoint { date: input.bars[i].date, value: ma[i] }).collect();
            series_by_spec.push(MaSeriesEntry { spec: spec.clone(), series });
        }

        let mut events = Vec::new();
        // Price cross MA + AboveMaStreak
        for entry in &series_by_spec {
            let mut streak: usize = 0;
            let mut last_bullish_i: Option<usize> = None;
            let mut last_bearish_i: Option<usize> = None;
            for i in 1..entry.series.len() {
                let prev_above = closes_for_streak[i - 1] > entry.series[i - 1].value;
                let cur_above = closes_for_streak[i] > entry.series[i].value;
                if !prev_above && cur_above {
                    if last_bullish_i.map_or(true, |li| i - li >= MIN_MA_CROSS_SPACING) {
                        events.push(MaEvent { date: entry.series[i].date, kind: MaEventKind::MaBullishCross, value: entry.series[i].value,
                            metadata: json!({"event": "ma_bullish_cross", "ma_kind": format!("{:?}", entry.spec.kind), "period": entry.spec.period}) });
                        last_bullish_i = Some(i);
                    }
                } else if prev_above && !cur_above {
                    if last_bearish_i.map_or(true, |li| i - li >= MIN_MA_CROSS_SPACING) {
                        events.push(MaEvent { date: entry.series[i].date, kind: MaEventKind::MaBearishCross, value: entry.series[i].value,
                            metadata: json!({"event": "ma_bearish_cross", "ma_kind": format!("{:?}", entry.spec.kind), "period": entry.spec.period}) });
                        last_bearish_i = Some(i);
                    }
                }
                if cur_above { streak += 1; } else {
                    if streak >= above_ma_streak_min(entry.spec.period) {
                        events.push(MaEvent { date: entry.series[i - 1].date, kind: MaEventKind::AboveMaStreak, value: streak as f64,
                            metadata: json!({"event": "above_ma_streak", "ma_kind": format!("{:?}", entry.spec.kind), "period": entry.spec.period, "days": streak}) });
                    }
                    streak = 0;
                }
            }
            if streak >= above_ma_streak_min(entry.spec.period) {
                events.push(MaEvent { date: entry.series.last().unwrap().date, kind: MaEventKind::AboveMaStreak, value: streak as f64,
                    metadata: json!({"event": "above_ma_streak", "ma_kind": format!("{:?}", entry.spec.kind), "period": entry.spec.period, "days": streak}) });
            }
        }
        // Cross pairs(short period crossing long period)
        let pairs: Vec<(usize, usize)> = match &params.detect_cross_pairs {
            CrossPairPolicy::None => Vec::new(),
            CrossPairPolicy::AllPairs => {
                let mut v = Vec::new();
                for i in 0..series_by_spec.len() {
                    for j in 0..series_by_spec.len() {
                        if series_by_spec[i].spec.period < series_by_spec[j].spec.period { v.push((i, j)); }
                    }
                }
                v
            }
            CrossPairPolicy::Pairs(spec_pairs) => {
                let mut v = Vec::new();
                for (sp, lp) in spec_pairs {
                    let i = series_by_spec.iter().position(|e| e.spec.period == *sp);
                    let j = series_by_spec.iter().position(|e| e.spec.period == *lp);
                    if let (Some(i), Some(j)) = (i, j) { v.push((i, j)); }
                }
                v
            }
        };
        for (si, li) in pairs {
            let s = &series_by_spec[si]; let l = &series_by_spec[li];
            let n = s.series.len().min(l.series.len());
            let mut last_golden_i: Option<usize> = None;
            let mut last_death_i: Option<usize> = None;
            for i in 1..n {
                let prev_above = s.series[i - 1].value > l.series[i - 1].value;
                let cur_above = s.series[i].value > l.series[i].value;
                if !prev_above && cur_above {
                    if last_golden_i.map_or(true, |li| i - li >= MIN_MA_CROSS_SPACING) {
                        events.push(MaEvent { date: s.series[i].date, kind: MaEventKind::MaGoldenCross, value: s.series[i].value,
                            metadata: json!({"event": "ma_golden_cross",
                                "short": {"kind": format!("{:?}", s.spec.kind), "period": s.spec.period},
                                "long": {"kind": format!("{:?}", l.spec.kind), "period": l.spec.period}}) });
                        last_golden_i = Some(i);
                    }
                } else if prev_above && !cur_above {
                    if last_death_i.map_or(true, |li| i - li >= MIN_MA_CROSS_SPACING) {
                        events.push(MaEvent { date: s.series[i].date, kind: MaEventKind::MaDeathCross, value: s.series[i].value,
                            metadata: json!({"event": "ma_death_cross",
                                "short": {"kind": format!("{:?}", s.spec.kind), "period": s.spec.period},
                                "long": {"kind": format!("{:?}", l.spec.kind), "period": l.spec.period}}) });
                        last_death_i = Some(i);
                    }
                }
            }
        }

        Ok(MaOutput { stock_id: input.stock_id.clone(), timeframe: params.timeframe, series_by_spec, events })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| Fact {
            stock_id: output.stock_id.clone(), fact_date: e.date, timeframe: output.timeframe,
            source_core: "ma_core".to_string(), source_version: "0.1.0".to_string(),
            params_hash: None, statement: format!("MA {:?} on {}", e.kind, e.date),
            metadata: e.metadata.clone(),
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn name_warmup_default() {
        let core = MaCore::new();
        assert_eq!(core.name(), "ma_core");
        assert_eq!(core.warmup_periods(&MaParams::default()), 25); // 20 + 5
    }
    #[test]
    fn ma_cross_spacing_constant_is_10() {
        assert_eq!(MIN_MA_CROSS_SPACING, 10);
    }

    #[test]
    fn series_by_spec_count_matches_specs() {
        let params = MaParams {
            specs: vec![
                MaSpec { kind: MaKind::Sma, period: 5, source: PriceSource::Close },
                MaSpec { kind: MaKind::Ema, period: 20, source: PriceSource::Close },
            ],
            timeframe: Timeframe::Daily,
            detect_cross_pairs: CrossPairPolicy::None,
        };
        let core = MaCore::new();
        let input = OhlcvSeries { stock_id: "t".to_string(), timeframe: Timeframe::Daily, bars: vec![] };
        let out = core.compute(&input, params).unwrap();
        assert_eq!(out.series_by_spec.len(), 2);
    }
}
