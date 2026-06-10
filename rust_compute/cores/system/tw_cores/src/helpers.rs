// helpers.rs — parse_timeframe + extract_indicator_meta(從 main.rs v3.5 R4 C8 抽出)

use anyhow::Result;
use chrono::{NaiveDate, Utc};
use fact_schema::Timeframe;
use traditional_core::TraditionalEngineConfig;

/// 讀環境變數覆寫 traditional 引擎 4 個 P0-Gate 校準旋鈕(免重編即可 sweep epsilon 等):
///   `TRAD_MONOWAVE_EPSILON`(f64,反轉雜訊門檻)/ `TRAD_ROUND_BEAM_SIZE`(usize)/
///   `TRAD_MAX_DEGREE_LEVELS`(usize)/ `TRAD_FOREST_MAX_SIZE`(usize)。
/// 未設或 parse 失敗則沿用 cfg 既有值(Default)。run-all dispatch + traditional-debug 共用。
pub fn apply_traditional_env_overrides(cfg: &mut TraditionalEngineConfig) {
    if let Ok(v) = std::env::var("TRAD_MONOWAVE_EPSILON") {
        if let Ok(f) = v.trim().parse::<f64>() {
            cfg.monowave_epsilon = f;
        }
    }
    if let Ok(v) = std::env::var("TRAD_ROUND_BEAM_SIZE") {
        if let Ok(n) = v.trim().parse::<usize>() {
            cfg.round_beam_size = n;
        }
    }
    if let Ok(v) = std::env::var("TRAD_MAX_DEGREE_LEVELS") {
        if let Ok(n) = v.trim().parse::<usize>() {
            cfg.max_degree_levels = n;
        }
    }
    if let Ok(v) = std::env::var("TRAD_FOREST_MAX_SIZE") {
        if let Ok(n) = v.trim().parse::<usize>() {
            cfg.forest_max_size = n;
        }
    }
}

pub fn parse_timeframe(s: &str) -> Result<Timeframe> {
    match s.to_lowercase().as_str() {
        "daily" => Ok(Timeframe::Daily),
        "weekly" => Ok(Timeframe::Weekly),
        "monthly" => Ok(Timeframe::Monthly),
        other => anyhow::bail!("unknown timeframe '{}',expected daily/weekly/monthly", other),
    }
}

/// 從 Output JSON 抽 (stock_id, value_date, timeframe_str)。
/// 處理 ma_core series_by_spec / taiex_core series_by_index 例外:
/// fallback 從巢狀 series 結構拿最後 date。
pub fn extract_indicator_meta(
    output_json: &serde_json::Value,
) -> (String, NaiveDate, String) {
    let stock_id = output_json
        .get("stock_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let timeframe = output_json
        .get("timeframe")
        .and_then(|v| v.as_str())
        .unwrap_or("daily")
        .to_string();

    fn nested_last_date(output_json: &serde_json::Value, key: &str) -> Option<String> {
        output_json
            .get(key)
            .and_then(|v| v.as_array())
            .and_then(|outer| outer.iter().rev().find_map(|first| {
                first.get("series")
                    .and_then(|s| s.as_array())
                    .and_then(|arr| arr.last())
                    .and_then(|p| p.get("date"))
                    .and_then(|d| d.as_str())
                    .map(String::from)
            }))
    }

    let last_date_str = output_json
        .get("series")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.last())
        .and_then(|p| p.get("date"))
        .and_then(|d| d.as_str())
        .map(String::from)
        .or_else(|| nested_last_date(output_json, "series_by_spec"))    // ma_core
        .or_else(|| nested_last_date(output_json, "series_by_index"))   // taiex_core
        // P2 pattern cores 無 series array,但有 `generated_at: NaiveDate`
        .or_else(|| {
            output_json
                .get("generated_at")
                .and_then(|v| v.as_str())
                .map(String::from)
        });
    let last_date_str = last_date_str.as_deref();

    let last_date = last_date_str
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or_else(|| Utc::now().date_naive());

    (stock_id, last_date, timeframe)
}

/// 判斷 indicator output JSON 是否「無任何序列資料點」。
///
/// 空序列 output 不應寫進 `indicator_values`:對所有 consumer 無值,且
/// `extract_indicator_meta` 對無日期 output 會 fallback 今天 → 空 row 的 `value_date`
/// 變「今天」→ `fetch_indicator_latest`(`value_date DESC`)把空 row 排到真實資料 row
/// 前面,consumer 取到空 series 誤判 core「缺資料」(business_indicator /
/// commodity_macro 曾在 market_dashboard 消失即此因)。
pub fn indicator_output_is_empty(output_json: &serde_json::Value) -> bool {
    // 直接 series(多數 indicator + environment core)
    if let Some(arr) = output_json.get("series").and_then(|v| v.as_array()) {
        return arr.is_empty();
    }
    // 巢狀 series(ma_core: series_by_spec / taiex_core: series_by_index)
    for key in ["series_by_spec", "series_by_index"] {
        if let Some(outer) = output_json.get(key).and_then(|v| v.as_array()) {
            let has_data = outer.iter().any(|entry| {
                entry
                    .get("series")
                    .and_then(|s| s.as_array())
                    .map(|a| !a.is_empty())
                    .unwrap_or(false)
            });
            return !has_data;
        }
    }
    // 無任何序列鍵 → 非序列型 output,不在判定範圍,保守回 false(照常寫入)
    false
}

/// 依 timeframe 自動載入足量 OHLCV(NeelyCore 用)。
///
/// P1-1 自 ohlcv_loader 原樣搬入:lookback 策略綁 NeelyCore.warmup_periods,
/// 屬 Neely 領域邏輯;共用 loader 不得 dep 領域 crate,故住 dispatcher 端
/// (本 crate 本就同時 dep neely_core + ohlcv_loader,兩個 caller 皆在本 crate)。
///
/// **v3.38(2026-05-18)user 拍版 per-forecast-horizon spec**:
///   支援 1m / 3m / 6m forecast 三 horizon(drop 1y),統一資料窗口拉取:
///   - Daily   = 1,500 bars(~6 yr,覆蓋 6m forecast `daily_bars_required=1500`)
///   - Weekly  = 300 bars(~6 yr,覆蓋 6m forecast `weekly_bars_required=300`)
///   - Monthly = 60 bars(~5 yr,對齊 user 拍版「年級評估不期待精準,monthly 只給
///     long-anchor reference」+ 6m forecast `monthly_bars_required=60`)
///   - Quarterly = warmup_buffered.max(72)(spec 外保留 floor)
///
/// **背景**:v3.36 hotfix 把 daily 推到 6 yr 是為了長 history 股(3030)長 degree
/// scenarios,但 user audit + spec(neely_core_architecture §13.3)揭露 Daily 1-3 yr
/// 對應 Minute degree(剛好是 1m-6m horizon);**長 degree anchor 走 weekly/monthly Neely
/// 而非過度延伸 daily**(對齊 v3.37 multi-timeframe 設計 + NEoWave 原書「各 timeframe
/// 負責自己 degree」哲學)。
///
/// MCP layer 用 `daily_bars` / `weekly_bars` / `monthly_bars` actual count 走 degradation
/// logic(per-forecast-horizon `degree_uncertain` / `no_6m` / `insufficient_history`)。
///
/// 對齊 cores_overview §3.4 / §7.3 + m3Spec/neely_core_architecture.md §5.4 §8.6 §13.3。
pub async fn load_for_neely(
    pool: &sqlx::postgres::PgPool,
    stock_id: &str,
    params: &neely_core::NeelyCoreParams,
) -> Result<fact_schema::OhlcvSeries> {
    use fact_schema::WaveCore;
    let core = neely_core::NeelyCore::new();
    let warmup = core.warmup_periods(params);
    // 1.2x 緩衝(對齊 §7.3 原規格,Quarterly fallback 用)
    let warmup_buffered = (warmup as f64 * 1.2).ceil() as i32;

    // v3.38 user 拍版 fixed table(對齊 per-forecast-horizon spec 完整 6m 需求)
    let lookback = match params.timeframe {
        Timeframe::Daily     => 1500,                       // ~6 yr,6m forecast daily_bars_required
        Timeframe::Weekly    => 300,                        // ~6 yr,6m forecast weekly_bars_required
        Timeframe::Monthly   => 60,                         // ~5 yr,6m forecast monthly_bars_required
        Timeframe::Quarterly => warmup_buffered.max(72),    // Quarterly 不在 user spec,保留 6 yr floor
    };

    match params.timeframe {
        Timeframe::Daily => ohlcv_loader::load_daily(pool, stock_id, lookback).await,
        Timeframe::Weekly => ohlcv_loader::load_weekly(pool, stock_id, lookback).await,
        Timeframe::Monthly => ohlcv_loader::load_monthly(pool, stock_id, lookback).await,
        Timeframe::Quarterly => Err(anyhow::anyhow!(
            "load_for_neely: Timeframe::Quarterly 不適用 OHLCV(Quarterly 為 financial_statement \
             季頻財報專用,沒對應 price_*_fwd 表)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_when_series_array_empty() {
        assert!(indicator_output_is_empty(&json!({"series": []})));
    }

    #[test]
    fn not_empty_when_series_has_points() {
        assert!(!indicator_output_is_empty(
            &json!({"series": [{"date": "2026-05-15"}]})
        ));
    }

    #[test]
    fn empty_when_series_by_index_all_empty() {
        assert!(indicator_output_is_empty(
            &json!({"series_by_index": [{"index_code": "Taiex", "series": []}]})
        ));
    }

    #[test]
    fn not_empty_when_series_by_index_has_data() {
        assert!(!indicator_output_is_empty(&json!({
            "series_by_index": [{"index_code": "Taiex", "series": [{"date": "2026-05-15"}]}]
        })));
    }

    #[test]
    fn not_empty_when_one_series_by_spec_has_data() {
        assert!(!indicator_output_is_empty(&json!({
            "series_by_spec": [{"series": []}, {"series": [{"date": "2026-05-15"}]}]
        })));
    }

    #[test]
    fn not_empty_when_no_series_key() {
        // 非序列型 output(e.g. P2 pattern core 的 generated_at)— 保守不擋
        assert!(!indicator_output_is_empty(&json!({"generated_at": "2026-05-15"})));
    }
}
