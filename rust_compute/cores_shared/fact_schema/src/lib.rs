// fact_schema:Cores 層共用合約。
// 對齊 m2Spec/oldm2Spec/cores_overview.md §3(trait)+ §6.2(Fact schema)+ §13.2.1(命名規範)。
//
// 範圍(M3 PR-1 skeleton):
//   - `Timeframe` enum
//   - `Fact` struct(寫 facts 表的單筆事實)
//   - `IndicatorCore` trait(Indicator / Chip / Fundamental / Environment 共用)
//   - `WaveCore` trait(Neely / Traditional 共用,Output 為 Scenario Forest)
//   - `params_hash()` 工具(blake3 + canonical JSON,§7.4)
//
// 不包含(留後續 PR):
//   - loader trait(`shared/ohlcv_loader/`、`shared/chip_loader/` 等)
//   - inventory 註冊機制(`CoreRegistration` / `CoreRegistry::discover`)
//   - 寫入端(`indicator_values` / `structural_snapshots` / `facts` 三表)

use anyhow::Result;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Timeframe
// ---------------------------------------------------------------------------

/// 時間粒度。Daily 為主,Weekly / Monthly 由 Silver `price_*_fwd` 已聚合。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Timeframe {
    Daily,
    Weekly,
    Monthly,
}

impl Timeframe {
    pub fn as_str(&self) -> &'static str {
        match self {
            Timeframe::Daily => "daily",
            Timeframe::Weekly => "weekly",
            Timeframe::Monthly => "monthly",
        }
    }
}

// ---------------------------------------------------------------------------
// Fact
// ---------------------------------------------------------------------------

/// 統一 Fact schema。對齊 m2Spec/oldm2Spec/cores_overview.md §6.2。
///
/// 寫入 `facts` 表時,Unique constraint 為
/// `(stock_id, fact_date, timeframe, source_core, COALESCE(params_hash, ''), md5(statement))`
/// + `INSERT ON CONFLICT DO NOTHING`(§6.3)。
///
/// `stock_id` 保留字規範見 §6.2.1(`_market_` / `_global_` / `_index_taiex_`)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub stock_id: String,
    pub fact_date: NaiveDate,
    pub timeframe: Timeframe,
    pub source_core: String,
    pub source_version: String,
    /// blake3(canonical_json(params, sort_keys=ASC))[..16] (hex);詳見 §7.4
    pub params_hash: Option<String>,
    /// 機械式 Fact 文字(禁主觀詞彙,§6.1.1)
    pub statement: String,
    /// Core 特定的結構化補充資料
    pub metadata: serde_json::Value,
}

// ---------------------------------------------------------------------------
// IndicatorCore trait
// ---------------------------------------------------------------------------

/// Indicator / Chip / Fundamental / Environment 共用 trait。
/// 對齊 m2Spec/oldm2Spec/cores_overview.md §3。
///
/// `Input` 由各 Core 自行宣告(OHLCVSeries / InstitutionalDailySeries / ...),
/// 由對應 loader 提供(§3.4)。
pub trait IndicatorCore: Send + Sync {
    type Input: Send + Sync;
    type Params: Default + Clone + Serialize;
    type Output: Serialize;

    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output>;
    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact>;

    /// Core 自宣告所需暖機「輸入序列單位數」(K 棒數 / 月份數 / 季別數),
    /// 由對應 loader 與 Pipeline 解讀(§3.4 / §7.3.1)。
    fn warmup_periods(&self, params: &Self::Params) -> usize;
}

// ---------------------------------------------------------------------------
// WaveCore trait
// ---------------------------------------------------------------------------

/// Wave Cores(Neely / Traditional)專用 trait。
/// 對齊 m2Spec/oldm2Spec/cores_overview.md §3.3。
///
/// `Input` 限定為 OHLC 序列(讀 Silver `price_*_fwd`)。
/// `Output` 為 Scenario Forest 結構,實作 `Serialize` 寫入 `structural_snapshots`。
pub trait WaveCore: Send + Sync {
    type Input: Send + Sync;
    type Params: Default + Clone + Serialize;
    type Output: Serialize;

    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;

    fn compute(&self, input: &Self::Input, params: Self::Params) -> Result<Self::Output>;
    fn produce_facts(&self, output: &Self::Output) -> Vec<Fact>;
    fn warmup_periods(&self, params: &Self::Params) -> usize;
}

// ---------------------------------------------------------------------------
// params_hash 工具
// ---------------------------------------------------------------------------

/// blake3(canonical_json(params, sort_keys=ASC))[..16] (hex)
/// 對齊 m2Spec/oldm2Spec/cores_overview.md §7.4。
pub fn params_hash<P: Serialize>(params: &P) -> Result<String> {
    let raw = serde_json::to_value(params)?;
    let canonical = canonical_json(&raw);
    let hash = blake3::hash(canonical.as_bytes());
    let hex = hash.to_hex();
    Ok(hex[..16].to_string())
}

/// Canonical JSON:object key 升序排序 + 無多餘空白。
/// serde_json 預設不排序,自己走一輪。
fn canonical_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let inner: Vec<String> = keys
                .iter()
                .map(|k| format!("{}:{}", serde_json::to_string(k).unwrap(), canonical_json(&map[*k])))
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        serde_json::Value::Array(arr) => {
            let inner: Vec<String> = arr.iter().map(canonical_json).collect();
            format!("[{}]", inner.join(","))
        }
        // primitive 走 serde_json 預設 string 表示
        _ => serde_json::to_string(value).unwrap_or_else(|_| "null".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Default, Clone)]
    struct DummyParams {
        b: i32,
        a: i32,
    }

    #[test]
    fn params_hash_is_order_independent() {
        // 相同欄位、不同序的 struct 應產生相同 hash(canonical JSON 排序)
        let p = DummyParams { b: 2, a: 1 };
        let h1 = params_hash(&p).unwrap();
        let h2 = params_hash(&serde_json::json!({"a": 1, "b": 2})).unwrap();
        let h3 = params_hash(&serde_json::json!({"b": 2, "a": 1})).unwrap();
        assert_eq!(h1, h2);
        assert_eq!(h2, h3);
        assert_eq!(h1.len(), 16);
    }

    #[test]
    fn timeframe_as_str_round_trip() {
        assert_eq!(Timeframe::Daily.as_str(), "daily");
        let s = serde_json::to_string(&Timeframe::Weekly).unwrap();
        assert_eq!(s, "\"weekly\"");
    }
}
