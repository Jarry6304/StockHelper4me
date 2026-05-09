// institutional_core(P2)— Chip Core
//
// 對齊 m3Spec/chip_cores.md §三 institutional_core(user 已寫定的最新 spec)。
//
// 定位(§3.1):法人買賣超(外資 / 投信 / 自營商)資料的事實萃取。
// 上游 Silver(§3.2):institutional_daily_derived
//
// **本 PR 範圍**(極限推進,部分功能 TODO):
//   - 完整 InstitutionalParams + InstitutionalOutput + 4 EventKind(對齊 §3.5)
//   - compute():逐筆組 series + foreign_cumulative_5d/20d
//   - detect_events:NetBuyStreak / NetSellStreak / DivergenceWithinInstitution 完整
//   - LargeTransaction(z-score)：**TODO** P0 後對齊 spec §3.3 lookback_for_z 校準
//   - produce_facts() 對齊 §3.7 範例
//
// TODO(後續討論):
//   - z-score 計算用全市場 mean / std vs 個股 lookback_for_z(目前用個股 60 天)
//   - LargeTransaction 是否分 institution(目前只對 foreign 偵測,trust/dealer 留 follow-up)
//   - DivergenceWithinInstitution metadata 完整 institution 三方判斷(目前只看 foreign vs dealer)

use anyhow::Result;
use chip_loader::{InstitutionalDailyRaw, InstitutionalDailySeries};
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "institutional_core",
        "0.1.0",
        core_registry::CoreKind::Chip,
        "P2",
        "Institutional Core(法人買賣超事實萃取)",
    )
}

// ---------------------------------------------------------------------------
// Params
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct InstitutionalParams {
    pub timeframe: Timeframe,
    /// 連續買賣超的最小天數,預設 3
    pub streak_min_days: usize,
    /// 大額異動 Z-score 閾值,預設 2.0
    pub large_transaction_z: f64,
    /// 計算 Z-score 的回看窗口,預設 60
    pub lookback_for_z: usize,
}

impl Default for InstitutionalParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            streak_min_days: 3,
            large_transaction_z: 2.0,
            lookback_for_z: 60,
        }
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct InstitutionalOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub series: Vec<InstitutionalPoint>,
    pub events: Vec<InstitutionalEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstitutionalPoint {
    pub date: NaiveDate,
    pub foreign_net: i64,
    pub trust_net: i64,
    pub dealer_net: i64,
    pub total_net: i64,
    pub foreign_cumulative_5d: i64,
    pub foreign_cumulative_20d: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstitutionalEvent {
    pub date: NaiveDate,
    pub kind: InstitutionalEventKind,
    pub value: f64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum InstitutionalEventKind {
    NetBuyStreak,
    NetSellStreak,
    LargeTransaction,
    DivergenceWithinInstitution,
}

// ---------------------------------------------------------------------------
// Core
// ---------------------------------------------------------------------------

pub struct InstitutionalCore;

impl InstitutionalCore {
    pub fn new() -> Self {
        InstitutionalCore
    }
}

impl Default for InstitutionalCore {
    fn default() -> Self {
        InstitutionalCore::new()
    }
}

impl IndicatorCore for InstitutionalCore {
    type Input = InstitutionalDailySeries;
    type Params = InstitutionalParams;
    type Output = InstitutionalOutput;

    fn name(&self) -> &'static str {
        "institutional_core"
    }
    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let series: Vec<InstitutionalPoint> = (0..input.points.len())
            .map(|i| {
                let p = &input.points[i];
                let foreign_net = p.foreign_net();
                let cum_5d = cumulative_foreign_net(&input.points, i, 5);
                let cum_20d = cumulative_foreign_net(&input.points, i, 20);
                InstitutionalPoint {
                    date: p.date,
                    foreign_net,
                    trust_net: p.trust_net(),
                    dealer_net: p.dealer_net(),
                    total_net: p.total_net(),
                    foreign_cumulative_5d: cum_5d,
                    foreign_cumulative_20d: cum_20d,
                }
            })
            .collect();

        let events = detect_events(&input.points, &series, &params);

        Ok(InstitutionalOutput {
            stock_id: input.stock_id.clone(),
            timeframe: params.timeframe,
            series,
            events,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output.events.iter().map(|e| event_to_fact(output, e)).collect()
    }

    fn warmup_periods(&self, params: &Self::Params) -> usize {
        // §3.4:lookback_for_z + 10
        params.lookback_for_z + 10
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn cumulative_foreign_net(points: &[InstitutionalDailyRaw], end_idx: usize, n: usize) -> i64 {
    let start = end_idx.saturating_sub(n - 1);
    points[start..=end_idx].iter().map(|p| p.foreign_net()).sum()
}

fn detect_events(
    raw: &[InstitutionalDailyRaw],
    series: &[InstitutionalPoint],
    params: &InstitutionalParams,
) -> Vec<InstitutionalEvent> {
    let mut events = Vec::new();

    // NetBuyStreak / NetSellStreak — 只對 foreign(spec §3.7 範例 foreign 為主)
    streak_detect(
        series,
        params.streak_min_days,
        |p| p.foreign_net > 0,
        InstitutionalEventKind::NetBuyStreak,
        "foreign",
        &mut events,
    );
    streak_detect(
        series,
        params.streak_min_days,
        |p| p.foreign_net < 0,
        InstitutionalEventKind::NetSellStreak,
        "foreign",
        &mut events,
    );

    // LargeTransaction — z-score 對 foreign_net
    if raw.len() > params.lookback_for_z {
        for i in params.lookback_for_z..raw.len() {
            let window: Vec<i64> = raw[i - params.lookback_for_z..i]
                .iter()
                .map(|p| p.foreign_net())
                .collect();
            let (mean, std) = mean_std(&window);
            if std > 0.0 {
                let z = (raw[i].foreign_net() as f64 - mean) / std;
                if z.abs() >= params.large_transaction_z {
                    events.push(InstitutionalEvent {
                        date: raw[i].date,
                        kind: InstitutionalEventKind::LargeTransaction,
                        value: raw[i].foreign_net() as f64,
                        metadata: json!({
                            "institution": "foreign",
                            "z_score": z,
                            "lookback": params.lookback_for_z,
                        }),
                    });
                }
            }
        }
    }

    // DivergenceWithinInstitution — foreign vs dealer 反向
    for p in series {
        let f_dir = p.foreign_net.signum();
        let d_dir = p.dealer_net.signum();
        if f_dir != 0 && d_dir != 0 && f_dir != d_dir {
            events.push(InstitutionalEvent {
                date: p.date,
                kind: InstitutionalEventKind::DivergenceWithinInstitution,
                value: 0.0,
                metadata: json!({
                    "foreign_direction": if f_dir > 0 { "buy" } else { "sell" },
                    "dealer_direction":  if d_dir > 0 { "buy" } else { "sell" },
                }),
            });
        }
    }

    events
}

fn streak_detect(
    series: &[InstitutionalPoint],
    min_days: usize,
    predicate: impl Fn(&InstitutionalPoint) -> bool,
    kind: InstitutionalEventKind,
    institution: &str,
    out: &mut Vec<InstitutionalEvent>,
) {
    let mut start: Option<usize> = None;
    let mut cumulative: i64 = 0;
    for (i, p) in series.iter().enumerate() {
        if predicate(p) {
            if start.is_none() {
                start = Some(i);
                cumulative = 0;
            }
            cumulative += p.foreign_net;
        } else if let Some(s) = start.take() {
            let days = i - s;
            if days >= min_days {
                emit_streak(series, s, i - 1, days, cumulative, kind, institution, out);
            }
            cumulative = 0;
        }
    }
    if let Some(s) = start {
        let days = series.len() - s;
        if days >= min_days {
            emit_streak(series, s, series.len() - 1, days, cumulative, kind, institution, out);
        }
    }
}

fn emit_streak(
    series: &[InstitutionalPoint],
    start: usize,
    end: usize,
    days: usize,
    cumulative: i64,
    kind: InstitutionalEventKind,
    institution: &str,
    out: &mut Vec<InstitutionalEvent>,
) {
    out.push(InstitutionalEvent {
        date: series[end].date,
        kind,
        value: cumulative as f64,
        metadata: json!({
            "institution": institution,
            "start_date": series[start].date,
            "end_date": series[end].date,
            "days": days,
        }),
    });
}

fn mean_std(window: &[i64]) -> (f64, f64) {
    if window.is_empty() {
        return (0.0, 0.0);
    }
    let n = window.len() as f64;
    let mean: f64 = window.iter().map(|&x| x as f64).sum::<f64>() / n;
    let var: f64 = window
        .iter()
        .map(|&x| {
            let d = x as f64 - mean;
            d * d
        })
        .sum::<f64>()
        / n;
    (mean, var.sqrt())
}

fn event_to_fact(output: &InstitutionalOutput, event: &InstitutionalEvent) -> Fact {
    let statement = match event.kind {
        InstitutionalEventKind::NetBuyStreak => format!(
            "Foreign net buy {} consecutive days ending on {}, total {} lots",
            event.metadata["days"], event.date, event.value as i64
        ),
        InstitutionalEventKind::NetSellStreak => format!(
            "Foreign net sell {} consecutive days ending on {}, total {} lots",
            event.metadata["days"], event.date, event.value as i64
        ),
        InstitutionalEventKind::LargeTransaction => format!(
            "Foreign single-day large transaction: {} lots on {}(z={:.2})",
            event.value as i64,
            event.date,
            event.metadata["z_score"].as_f64().unwrap_or(0.0)
        ),
        InstitutionalEventKind::DivergenceWithinInstitution => format!(
            "Foreign and dealer diverge on {}(foreign {}, dealer {})",
            event.date,
            event.metadata["foreign_direction"].as_str().unwrap_or("?"),
            event.metadata["dealer_direction"].as_str().unwrap_or("?")
        ),
    };

    Fact {
        stock_id: output.stock_id.clone(),
        fact_date: event.date,
        timeframe: output.timeframe,
        source_core: "institutional_core".to_string(),
        source_version: "0.1.0".to_string(),
        params_hash: None,
        statement,
        metadata: event.metadata.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(d: &str, f_buy: i64, f_sell: i64, t_buy: i64, t_sell: i64, dl_buy: i64, dl_sell: i64) -> InstitutionalDailyRaw {
        InstitutionalDailyRaw {
            date: NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap(),
            foreign_buy: Some(f_buy),
            foreign_sell: Some(f_sell),
            investment_trust_buy: Some(t_buy),
            investment_trust_sell: Some(t_sell),
            dealer_buy: Some(dl_buy),
            dealer_sell: Some(dl_sell),
            dealer_hedging_buy: Some(0),
            dealer_hedging_sell: Some(0),
            gov_bank_net: None,
        }
    }

    #[test]
    fn streak_detection() {
        // 5 連續 foreign 淨買超(buy 1000 sell 500 = +500/day)
        let series = InstitutionalDailySeries {
            stock_id: "2330".to_string(),
            points: (1..=6)
                .map(|i| raw(&format!("2026-04-{:02}", 20 + i), 1000, 500, 0, 0, 0, 0))
                .collect(),
        };
        let core = InstitutionalCore::new();
        let out = core.compute(&series, InstitutionalParams::default()).unwrap();
        let streaks: Vec<_> = out.events.iter().filter(|e| e.kind == InstitutionalEventKind::NetBuyStreak).collect();
        assert_eq!(streaks.len(), 1);
        assert!((streaks[0].value - 6.0 * 500.0).abs() < 1e-9, "cumulative 應為 6×500");
    }

    #[test]
    fn divergence_detection() {
        // foreign buy(net +500), dealer sell(net -200)
        let series = InstitutionalDailySeries {
            stock_id: "2330".to_string(),
            points: vec![raw("2026-04-22", 1000, 500, 0, 0, 100, 300)],
        };
        let core = InstitutionalCore::new();
        let out = core.compute(&series, InstitutionalParams::default()).unwrap();
        let div: Vec<_> = out
            .events
            .iter()
            .filter(|e| e.kind == InstitutionalEventKind::DivergenceWithinInstitution)
            .collect();
        assert_eq!(div.len(), 1);
    }

    #[test]
    fn warmup_uses_lookback_plus_10() {
        let core = InstitutionalCore::new();
        assert_eq!(core.warmup_periods(&InstitutionalParams::default()), 70);
    }

    #[test]
    fn name_version_stable() {
        let core = InstitutionalCore::new();
        assert_eq!(core.name(), "institutional_core");
        assert_eq!(core.version(), "0.1.0");
    }
}
