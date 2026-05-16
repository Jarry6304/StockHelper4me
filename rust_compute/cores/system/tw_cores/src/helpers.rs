// helpers.rs — parse_timeframe + extract_indicator_meta(從 main.rs v3.5 R4 C8 抽出)

use anyhow::Result;
use chrono::{NaiveDate, Utc};
use fact_schema::Timeframe;

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
