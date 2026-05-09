// Rule of Neutrality:標 monowave 為 Neutral 當 magnitude 過小
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 2 / §10.4.1。
//
// 個股(spec 描述「|單日漲跌幅| < 預設 ATR 比例 → Neutral」):
//   monowave-level 套用為:|magnitude| < ATR(at start) * 1.0 → Neutral
//
// 加權指數(`stock_id == "_index_taiex_"`,對齊 cores_overview §6.2.1 保留字):
//   |magnitude| / start_price * 100 < neutral_threshold_taiex(預設 0.5%)→ Neutral
//
// 設計意圖(§10.4.1):
//   加權指數波動天然較個股小,沿用個股閾值會誤判多數交易日為趨勢日,
//   使 monowave 切割過密、Validator 拒絕率異常。

use crate::output::{Monowave, MonowaveDirection};

/// 加權指數保留字(對齊 cores_overview.md §6.2.1)
pub const TAIEX_RESERVED_STOCK_ID: &str = "_index_taiex_";

/// 個股 Neutrality 閾值倍數:|magnitude| < ATR * STOCK_NEUTRAL_ATR_MULTIPLIER → Neutral
/// 寫死 1.0(對齊 §4.4 Neely 規則寫死原則)
const STOCK_NEUTRAL_ATR_MULTIPLIER: f64 = 1.0;

/// 套用 Rule of Neutrality 回傳 monowave 應有的 direction。
///
/// - 個股:|magnitude| < ATR(at start) * 1.0 → Neutral,否則回原 direction
/// - 加權指數:|magnitude| / start_price * 100 < neutral_threshold_taiex → Neutral
///
/// `atr_at_start = 0.0`(暖機不足或 0 ATR)時對個股 fallback 為 always Neutral
/// (對齊「資料不足無法判定」精神;Pipeline 後續 stage 會根據 insufficient_data
/// 標記降級處理)。
pub fn classify_neutrality(
    monowave: &Monowave,
    atr_at_start: f64,
    stock_id: &str,
    neutral_threshold_taiex: f64,
) -> MonowaveDirection {
    let magnitude = (monowave.end_price - monowave.start_price).abs();

    let is_neutral = if stock_id == TAIEX_RESERVED_STOCK_ID {
        // 加權指數:絕對 % 閾值
        if monowave.start_price.abs() < f64::EPSILON {
            // start_price 為 0(theoretical impossible for TAIEX)→ 視為 Neutral
            true
        } else {
            let pct = magnitude / monowave.start_price.abs() * 100.0;
            pct < neutral_threshold_taiex
        }
    } else {
        // 個股:ATR 比例
        if atr_at_start <= 0.0 {
            true
        } else {
            magnitude < atr_at_start * STOCK_NEUTRAL_ATR_MULTIPLIER
        }
    };

    if is_neutral {
        MonowaveDirection::Neutral
    } else {
        monowave.direction
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn mw(start: f64, end: f64, dir: MonowaveDirection) -> Monowave {
        Monowave {
            start_date: NaiveDate::parse_from_str("2026-01-01", "%Y-%m-%d").unwrap(),
            end_date: NaiveDate::parse_from_str("2026-01-05", "%Y-%m-%d").unwrap(),
            start_price: start,
            end_price: end,
            direction: dir,
        }
    }

    // -----------------------------------------------------------------
    // 個股(stock_id 為真實代號)
    // -----------------------------------------------------------------

    #[test]
    fn stock_magnitude_below_atr_marks_neutral() {
        // magnitude 0.5 < ATR 1.0 → Neutral
        let m = mw(100.0, 100.5, MonowaveDirection::Up);
        let result = classify_neutrality(&m, 1.0, "2330", 0.5);
        assert!(matches!(result, MonowaveDirection::Neutral));
    }

    #[test]
    fn stock_magnitude_at_atr_keeps_direction() {
        // magnitude 1.0 == ATR 1.0(嚴格 < 才 Neutral)→ 保持 Up
        let m = mw(100.0, 101.0, MonowaveDirection::Up);
        let result = classify_neutrality(&m, 1.0, "2330", 0.5);
        assert!(matches!(result, MonowaveDirection::Up));
    }

    #[test]
    fn stock_magnitude_above_atr_keeps_direction() {
        let m = mw(100.0, 102.5, MonowaveDirection::Up);
        let result = classify_neutrality(&m, 1.0, "2330", 0.5);
        assert!(matches!(result, MonowaveDirection::Up));
    }

    #[test]
    fn stock_zero_atr_falls_back_to_neutral() {
        let m = mw(100.0, 105.0, MonowaveDirection::Up);
        let result = classify_neutrality(&m, 0.0, "2330", 0.5);
        assert!(matches!(result, MonowaveDirection::Neutral));
    }

    // -----------------------------------------------------------------
    // 加權指數(stock_id == "_index_taiex_")
    // -----------------------------------------------------------------

    #[test]
    fn taiex_magnitude_below_threshold_marks_neutral() {
        // 起點 22000,終點 22080(+0.36% < 0.5%)→ Neutral,即使 ATR 比例上不會被視為 Neutral
        let m = mw(22000.0, 22080.0, MonowaveDirection::Up);
        let result = classify_neutrality(&m, 1.0, TAIEX_RESERVED_STOCK_ID, 0.5);
        assert!(matches!(result, MonowaveDirection::Neutral));
    }

    #[test]
    fn taiex_magnitude_above_threshold_keeps_direction() {
        // 起點 22000,終點 22220(+1.0% > 0.5%)→ 保持 Up
        let m = mw(22000.0, 22220.0, MonowaveDirection::Up);
        let result = classify_neutrality(&m, 1.0, TAIEX_RESERVED_STOCK_ID, 0.5);
        assert!(matches!(result, MonowaveDirection::Up));
    }

    #[test]
    fn taiex_uses_pct_not_atr() {
        // ATR 巨大但變動 % 小 → 仍應 Neutral(taiex 不看 ATR)
        let m = mw(22000.0, 22050.0, MonowaveDirection::Down);
        let result = classify_neutrality(&m, 999.0, TAIEX_RESERVED_STOCK_ID, 0.5);
        // pct = 50/22000*100 = 0.227% < 0.5% → Neutral
        assert!(matches!(result, MonowaveDirection::Neutral));
    }
}
