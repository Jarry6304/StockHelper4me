#![allow(clippy::needless_range_loop)]
// candlestick_pattern_core(P2)— 對齊 m3Spec/indicator_cores_pattern.md §三
// Params §3.4 / warmup §3.5 / Output §3.6 / Fact §3.7
//
// Reference:
//   Nison, Steve (1991), "Japanese Candlestick Charting Techniques" — 西方 K 線型態原作
//   Bulkowski, Thomas N. (2008), "Encyclopedia of Candlestick Charts" — 量化規則化
//   trend_lookback=5:Bulkowski 慣例
//   doji_threshold=0.1 / tweezer_tolerance=0.005:Nison + Bulkowski 共識
//
// 收錄 16 種型態(spec §3.3):
//   單根:Doji / LongLeggedDoji / Hammer / InvertedHammer / HangingMan /
//          ShootingStar / MarubozuBullish / MarubozuBearish
//   雙根:BullishEngulfing / BearishEngulfing / TweezerTop / TweezerBottom
//   三根:MorningStar / EveningStar / ThreeWhiteSoldiers / ThreeBlackCrows
//
// 「嚴格規則式」(spec §3.2):純 OHLC 數學定義 / 無歧義 / 不依賴情緒

use anyhow::Result;
use chrono::NaiveDate;
use fact_schema::{Fact, IndicatorCore, Timeframe};
use ohlcv_loader::{OhlcvBar, OhlcvSeries};
use serde::Serialize;
use serde_json::json;

inventory::submit! {
    core_registry::CoreRegistration::new(
        "candlestick_pattern_core", "0.1.0", core_registry::CoreKind::Indicator, "P2",
        "Candlestick Pattern Core(16 種嚴格規則式 K 線型態)",
    )
}

/// 同一 PatternKind 連續 N 個交易日內只觸發一次(避免噪音放大)。
///
/// **v1.34 Round 5 production calibration**:全市場 1263 stocks 跑出
/// 115.5 facts/yr/stock(10× 嚴重噪音)。Doji 在動盪期 / Bullish/Bearish Engulfing
/// 在震盪區頻繁觸發 → spec §3.7「強訊號型態」失去信號價值。
///
/// 加 MIN_PATTERN_GAP_BARS = 10 — 同 pattern_kind 至少 10 個 bar 才能再次觸發,
/// 對齊 v1.32 ma_core / kd_core MIN_X_CROSS_SPACING=10 慣例。
/// 預期 115.5 → ~40-50/yr/stock(2.5× 降量),仍 > 12 但對 16 patterns 加總合理。
const MIN_PATTERN_GAP_BARS: usize = 10;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Hash)]
pub enum PatternKind {
    Doji,
    LongLeggedDoji,
    Hammer,
    InvertedHammer,
    HangingMan,
    ShootingStar,
    MarubozuBullish,
    MarubozuBearish,
    BullishEngulfing,
    BearishEngulfing,
    TweezerTop,
    TweezerBottom,
    MorningStar,
    EveningStar,
    ThreeWhiteSoldiers,
    ThreeBlackCrows,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum TrendContext {
    Uptrend,
    Downtrend,
    Sideways,
}

#[derive(Debug, Clone, Serialize)]
pub struct CandlestickPatternParams {
    pub timeframe: Timeframe,
    pub trend_lookback: usize,
    pub doji_threshold: f64,
    pub tweezer_tolerance: f64,
    pub enabled_patterns: Vec<PatternKind>,
}
impl Default for CandlestickPatternParams {
    fn default() -> Self {
        Self {
            timeframe: Timeframe::Daily,
            trend_lookback: 5,
            doji_threshold: 0.1,
            tweezer_tolerance: 0.005,
            enabled_patterns: vec![
                PatternKind::Doji,
                PatternKind::LongLeggedDoji,
                PatternKind::Hammer,
                PatternKind::InvertedHammer,
                PatternKind::HangingMan,
                PatternKind::ShootingStar,
                PatternKind::MarubozuBullish,
                PatternKind::MarubozuBearish,
                PatternKind::BullishEngulfing,
                PatternKind::BearishEngulfing,
                PatternKind::TweezerTop,
                PatternKind::TweezerBottom,
                PatternKind::MorningStar,
                PatternKind::EveningStar,
                PatternKind::ThreeWhiteSoldiers,
                PatternKind::ThreeBlackCrows,
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DetectedPattern {
    pub date: NaiveDate,
    pub pattern_kind: PatternKind,
    pub bar_count: usize,
    pub trend_context: TrendContext,
    pub strength_metric: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CandlestickPatternOutput {
    pub stock_id: String,
    pub timeframe: Timeframe,
    pub patterns: Vec<DetectedPattern>,
}

pub struct CandlestickPatternCore;
impl CandlestickPatternCore {
    pub fn new() -> Self {
        CandlestickPatternCore
    }
}
impl Default for CandlestickPatternCore {
    fn default() -> Self {
        CandlestickPatternCore::new()
    }
}

fn body(bar: &OhlcvBar) -> f64 {
    (bar.close - bar.open).abs()
}
fn range(bar: &OhlcvBar) -> f64 {
    bar.high - bar.low
}
fn upper_shadow(bar: &OhlcvBar) -> f64 {
    bar.high - bar.open.max(bar.close)
}
fn lower_shadow(bar: &OhlcvBar) -> f64 {
    bar.open.min(bar.close) - bar.low
}
fn is_bullish(bar: &OhlcvBar) -> bool {
    bar.close > bar.open
}
fn is_bearish(bar: &OhlcvBar) -> bool {
    bar.close < bar.open
}

/// 判斷 i 處的趨勢上下文 — 用過去 trend_lookback 棒 close 是否單調
fn classify_trend(bars: &[OhlcvBar], i: usize, lookback: usize) -> TrendContext {
    if i < lookback {
        return TrendContext::Sideways;
    }
    let start = i - lookback;
    let mut up = 0i32;
    let mut dn = 0i32;
    for j in start..i {
        let cur = bars[j + 1].close;
        let prev = bars[j].close;
        if cur > prev {
            up += 1;
        } else if cur < prev {
            dn += 1;
        }
    }
    if up >= dn * 2 && up >= (lookback as i32 * 3 / 5).max(1) {
        TrendContext::Uptrend
    } else if dn >= up * 2 && dn >= (lookback as i32 * 3 / 5).max(1) {
        TrendContext::Downtrend
    } else {
        TrendContext::Sideways
    }
}

impl IndicatorCore for CandlestickPatternCore {
    type Input = OhlcvSeries;
    type Params = CandlestickPatternParams;
    type Output = CandlestickPatternOutput;
    fn name(&self) -> &'static str {
        "candlestick_pattern_core"
    }
    fn version(&self) -> &'static str {
        "0.1.0"
    }
    fn warmup_periods(&self, params: &Self::Params) -> usize {
        params.trend_lookback + 5
    }

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output> {
        let bars = &input.bars;
        let n = bars.len();
        let enabled: std::collections::HashSet<PatternKind> =
            params.enabled_patterns.iter().copied().collect();
        let mut patterns: Vec<DetectedPattern> = Vec::new();

        // 14 (ATR 期間)給 Long-legged Doji
        // 簡化:用 range 平均(過去 14 棒)代替 ATR(本 core 不依 atr_core)
        let avg_range = |i: usize| -> f64 {
            let p = 14usize;
            if i + 1 < p {
                return range(&bars[i]);
            }
            let start = i + 1 - p;
            bars[start..=i].iter().map(range).sum::<f64>() / p as f64
        };

        for i in 0..n {
            let bar = &bars[i];
            let b = body(bar);
            let r = range(bar);
            let trend = classify_trend(bars, i, params.trend_lookback);

            // 1. Doji
            if r > 0.0 && b / r < params.doji_threshold && enabled.contains(&PatternKind::Doji) {
                let strength = b / r;
                patterns.push(DetectedPattern {
                    date: bar.date,
                    pattern_kind: PatternKind::Doji,
                    bar_count: 1,
                    trend_context: trend,
                    strength_metric: strength,
                });
                // Long-legged Doji:Doji + range > 1.5 × avg_range(14)
                if r > 1.5 * avg_range(i) && enabled.contains(&PatternKind::LongLeggedDoji) {
                    patterns.push(DetectedPattern {
                        date: bar.date,
                        pattern_kind: PatternKind::LongLeggedDoji,
                        bar_count: 1,
                        trend_context: trend,
                        strength_metric: r / avg_range(i),
                    });
                }
            }
            // 2. Hammer / Hanging Man:下影線 ≥ 2 倍實體 + 上影線 ≤ 0.5 倍實體
            if b > 0.0
                && lower_shadow(bar) >= 2.0 * b
                && upper_shadow(bar) <= 0.5 * b
            {
                let strength = lower_shadow(bar) / b;
                let kind = match trend {
                    TrendContext::Downtrend => PatternKind::Hammer,
                    TrendContext::Uptrend => PatternKind::HangingMan,
                    TrendContext::Sideways => PatternKind::Hammer, // 預設 Hammer(無趨勢時也可能反轉)
                };
                if enabled.contains(&kind) {
                    patterns.push(DetectedPattern {
                        date: bar.date,
                        pattern_kind: kind,
                        bar_count: 1,
                        trend_context: trend,
                        strength_metric: strength,
                    });
                }
            }
            // 3. Inverted Hammer / Shooting Star:上影線 ≥ 2 倍實體 + 下影線 ≤ 0.5 倍實體
            if b > 0.0 && upper_shadow(bar) >= 2.0 * b && lower_shadow(bar) <= 0.5 * b {
                let strength = upper_shadow(bar) / b;
                let kind = match trend {
                    TrendContext::Downtrend => PatternKind::InvertedHammer,
                    TrendContext::Uptrend => PatternKind::ShootingStar,
                    TrendContext::Sideways => PatternKind::ShootingStar,
                };
                if enabled.contains(&kind) {
                    patterns.push(DetectedPattern {
                        date: bar.date,
                        pattern_kind: kind,
                        bar_count: 1,
                        trend_context: trend,
                        strength_metric: strength,
                    });
                }
            }
            // 4. Marubozu:上下影線 < 實體 5%
            if b > 0.0 && upper_shadow(bar) < 0.05 * b && lower_shadow(bar) < 0.05 * b {
                let kind = if is_bullish(bar) {
                    PatternKind::MarubozuBullish
                } else {
                    PatternKind::MarubozuBearish
                };
                if enabled.contains(&kind) {
                    patterns.push(DetectedPattern {
                        date: bar.date,
                        pattern_kind: kind,
                        bar_count: 1,
                        trend_context: trend,
                        strength_metric: 1.0 - (upper_shadow(bar) + lower_shadow(bar)) / b,
                    });
                }
            }

            // ---- 雙根 K 線(需 i >= 1)----
            if i < 1 {
                continue;
            }
            let prev = &bars[i - 1];

            // 5. Bullish Engulfing
            if is_bearish(prev)
                && is_bullish(bar)
                && bar.open < prev.close
                && bar.close > prev.open
                && enabled.contains(&PatternKind::BullishEngulfing)
            {
                let strength = body(bar) / body(prev).max(1e-9);
                patterns.push(DetectedPattern {
                    date: bar.date,
                    pattern_kind: PatternKind::BullishEngulfing,
                    bar_count: 2,
                    trend_context: trend,
                    strength_metric: strength,
                });
            }
            // 6. Bearish Engulfing
            if is_bullish(prev)
                && is_bearish(bar)
                && bar.open > prev.close
                && bar.close < prev.open
                && enabled.contains(&PatternKind::BearishEngulfing)
            {
                let strength = body(bar) / body(prev).max(1e-9);
                patterns.push(DetectedPattern {
                    date: bar.date,
                    pattern_kind: PatternKind::BearishEngulfing,
                    bar_count: 2,
                    trend_context: trend,
                    strength_metric: strength,
                });
            }
            // 7. Tweezer Top — 兩高點 ±0.5% 容差 + 上漲趨勢
            if trend == TrendContext::Uptrend
                && prev.high > 0.0
                && (prev.high - bar.high).abs() / prev.high < params.tweezer_tolerance
                && enabled.contains(&PatternKind::TweezerTop)
            {
                patterns.push(DetectedPattern {
                    date: bar.date,
                    pattern_kind: PatternKind::TweezerTop,
                    bar_count: 2,
                    trend_context: trend,
                    strength_metric: 1.0 - (prev.high - bar.high).abs() / prev.high,
                });
            }
            // 8. Tweezer Bottom — 兩低點 ±0.5% + 下跌趨勢
            if trend == TrendContext::Downtrend
                && prev.low > 0.0
                && (prev.low - bar.low).abs() / prev.low < params.tweezer_tolerance
                && enabled.contains(&PatternKind::TweezerBottom)
            {
                patterns.push(DetectedPattern {
                    date: bar.date,
                    pattern_kind: PatternKind::TweezerBottom,
                    bar_count: 2,
                    trend_context: trend,
                    strength_metric: 1.0 - (prev.low - bar.low).abs() / prev.low,
                });
            }

            // ---- 三根 K 線(需 i >= 2)----
            if i < 2 {
                continue;
            }
            let b1 = &bars[i - 2];
            let b2 = &bars[i - 1];
            let b3 = bar;

            // 9. Morning Star — 大黑 K + 小實體跳空 + 大紅 K 收復一半
            if is_bearish(b1)
                && body(b2) < 0.3 * body(b1)
                && is_bullish(b3)
                && b3.close > (b1.open + b1.close) / 2.0
                && b2.open.max(b2.close) < b1.close
                && enabled.contains(&PatternKind::MorningStar)
            {
                patterns.push(DetectedPattern {
                    date: bar.date,
                    pattern_kind: PatternKind::MorningStar,
                    bar_count: 3,
                    trend_context: trend,
                    strength_metric: body(b3) / body(b1).max(1e-9),
                });
            }
            // 10. Evening Star
            if is_bullish(b1)
                && body(b2) < 0.3 * body(b1)
                && is_bearish(b3)
                && b3.close < (b1.open + b1.close) / 2.0
                && b2.open.min(b2.close) > b1.close
                && enabled.contains(&PatternKind::EveningStar)
            {
                patterns.push(DetectedPattern {
                    date: bar.date,
                    pattern_kind: PatternKind::EveningStar,
                    bar_count: 3,
                    trend_context: trend,
                    strength_metric: body(b3) / body(b1).max(1e-9),
                });
            }
            // 11. Three White Soldiers — 三連紅 K + 開盤在前根實體內 + 收高於前根
            if is_bullish(b1)
                && is_bullish(b2)
                && is_bullish(b3)
                && b2.open >= b1.open
                && b2.open <= b1.close
                && b3.open >= b2.open
                && b3.open <= b2.close
                && b2.close > b1.close
                && b3.close > b2.close
                && enabled.contains(&PatternKind::ThreeWhiteSoldiers)
            {
                patterns.push(DetectedPattern {
                    date: bar.date,
                    pattern_kind: PatternKind::ThreeWhiteSoldiers,
                    bar_count: 3,
                    trend_context: trend,
                    strength_metric: (b3.close - b1.open) / b1.open.max(1e-9),
                });
            }
            // 12. Three Black Crows
            if is_bearish(b1)
                && is_bearish(b2)
                && is_bearish(b3)
                && b2.open <= b1.open
                && b2.open >= b1.close
                && b3.open <= b2.open
                && b3.open >= b2.close
                && b2.close < b1.close
                && b3.close < b2.close
                && enabled.contains(&PatternKind::ThreeBlackCrows)
            {
                patterns.push(DetectedPattern {
                    date: bar.date,
                    pattern_kind: PatternKind::ThreeBlackCrows,
                    bar_count: 3,
                    trend_context: trend,
                    strength_metric: (b1.open - b3.close) / b1.open.max(1e-9),
                });
            }
        }

        // v1.34 Round 5:同 PatternKind MIN_PATTERN_GAP_BARS 後處理
        // 對映 bar idx,後過濾(patterns Vec 已 date 順序 push,O(n) 即可)
        let mut filtered: Vec<DetectedPattern> = Vec::with_capacity(patterns.len());
        let mut last_idx_by_kind: std::collections::HashMap<PatternKind, usize> =
            std::collections::HashMap::new();
        let date_to_idx: std::collections::HashMap<NaiveDate, usize> = bars
            .iter()
            .enumerate()
            .map(|(i, b)| (b.date, i))
            .collect();
        for p in patterns {
            let cur_idx = match date_to_idx.get(&p.date) {
                Some(&v) => v,
                None => continue,
            };
            let too_close = matches!(
                last_idx_by_kind.get(&p.pattern_kind),
                Some(&last) if cur_idx < last + MIN_PATTERN_GAP_BARS
            );
            if !too_close {
                last_idx_by_kind.insert(p.pattern_kind, cur_idx);
                filtered.push(p);
            }
        }

        Ok(CandlestickPatternOutput {
            stock_id: input.stock_id.clone(),
            timeframe: params.timeframe,
            patterns: filtered,
        })
    }

    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact> {
        output
            .patterns
            .iter()
            .map(|p| Fact {
                stock_id: output.stock_id.clone(),
                fact_date: p.date,
                timeframe: output.timeframe,
                source_core: "candlestick_pattern_core".to_string(),
                source_version: "0.1.0".to_string(),
                params_hash: None,
                statement: format!(
                    "{:?} at {} in {:?} (strength={:.3})",
                    p.pattern_kind, p.date, p.trend_context, p.strength_metric
                ),
                metadata: json!({
                    "pattern": format!("{:?}", p.pattern_kind),
                    "bar_count": p.bar_count,
                    "trend_context": format!("{:?}", p.trend_context),
                    "strength_metric": p.strength_metric,
                }),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use fact_schema::Timeframe;

    fn make_bars(items: Vec<(f64, f64, f64, f64)>) -> OhlcvSeries {
        let bars = items
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
        let core = CandlestickPatternCore::new();
        assert_eq!(core.name(), "candlestick_pattern_core");
        assert_eq!(
            core.warmup_periods(&CandlestickPatternParams::default()),
            10
        );
    }

    #[test]
    fn detects_doji() {
        let core = CandlestickPatternCore::new();
        // 20 個正常 bar + 1 個 doji(open == close,range 較大)
        let mut bars: Vec<(f64, f64, f64, f64)> = (0..20)
            .map(|i| {
                let base = 100.0 + i as f64;
                (base, base + 1.0, base - 1.0, base + 0.5)
            })
            .collect();
        // doji:open=close=120, high=125, low=115 → body=0 < threshold × 10
        bars.push((120.0, 125.0, 115.0, 120.0));
        let series = make_bars(bars);
        let out = core
            .compute(&series, CandlestickPatternParams::default())
            .unwrap();
        let dojis = out
            .patterns
            .iter()
            .filter(|p| p.pattern_kind == PatternKind::Doji)
            .count();
        assert!(dojis >= 1, "expected at least 1 Doji");
    }

    #[test]
    fn detects_bullish_engulfing() {
        let core = CandlestickPatternCore::new();
        // 構造下跌趨勢 + 黑 K + 大紅 K 吞噬
        let mut bars: Vec<(f64, f64, f64, f64)> = (0..10)
            .map(|i| {
                let base = 110.0 - i as f64; // 下跌
                (base, base + 1.0, base - 1.0, base - 1.0)
            })
            .collect();
        // 黑 K:o=101 h=102 l=99 c=99
        bars.push((101.0, 102.0, 99.0, 99.0));
        // 大紅 K 吞噬:o=98 c=103 high=104 low=97
        bars.push((98.0, 104.0, 97.0, 103.0));
        let series = make_bars(bars);
        let out = core
            .compute(&series, CandlestickPatternParams::default())
            .unwrap();
        let engulfings = out
            .patterns
            .iter()
            .filter(|p| p.pattern_kind == PatternKind::BullishEngulfing)
            .count();
        assert!(engulfings >= 1, "expected at least 1 BullishEngulfing");
    }
}
