// OHLCV 共用型別(P1-1 自 neely_core/src/output.rs 原樣搬入)。
//
// 落點 rationale:43+ crates 已依賴 fact_schema 且本型別已用其 `Timeframe`,
// 下放至此解除 ohlcv_loader(共用層)→ neely_core(領域層)反向依賴;
// neely_core::output 以 `pub use` 維持回溯相容路徑。

use chrono::NaiveDate;
use serde::Serialize;

use crate::Timeframe;

/// 後復權 OHLC 序列。Silver `price_*_fwd` 表已處理漲跌停合併與後復權。
/// Volume 為選填,Volume Alignment 子規則(§9.1 `volume_alignment`)需要時用。
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "neely/"))]
pub struct OhlcvSeries {
    pub stock_id: String,
    #[cfg_attr(feature = "ts", ts(type = "\"Daily\" | \"Weekly\" | \"Monthly\" | \"Quarterly\""))]
    pub timeframe: Timeframe,
    pub bars: Vec<OhlcvBar>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export, export_to = "neely/"))]
pub struct OhlcvBar {
    pub date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: Option<i64>,
}
