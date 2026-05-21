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
