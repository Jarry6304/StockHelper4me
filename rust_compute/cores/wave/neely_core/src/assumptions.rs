// assumptions.rs — E1:引擎假設清單 + assumption_hash
//
// 對齊 m3Spec/wave_judgment_loop.md §3(E1)。判讀者(人/LLM)判讀前先讀本清單:
// 引擎的計數受哪些工程常數 / 詮釋容差影響。**值不在此重打** — 全部引用各模組
// 既有常數(升 pub(crate)),防兩處數值漂移;常數本身仍寫死(「不外部化 Neely
// 常數」invariant 不變,此處僅回報)。

use sha2::{Digest, Sha256};

use crate::output::{Assumption, AssumptionSource};

/// 引擎假設清單(排序穩定:依 name 升冪;spec §3 列的 8 個常數)。
pub fn collect() -> Vec<Assumption> {
    let mut list = vec![
        Assumption {
            name: "REVERSAL_ATR_MULTIPLIER".to_string(),
            value: crate::monowave::pure_close::REVERSAL_ATR_MULTIPLIER,
            source: AssumptionSource::Engineering,
        },
        Assumption {
            name: "NEUTRAL_ATR_MULTIPLIER".to_string(),
            value: crate::monowave::neutrality::STOCK_NEUTRAL_ATR_MULTIPLIER,
            source: AssumptionSource::Engineering,
        },
        Assumption {
            // architecture §4.2 第 1 檔:一般近似詞 ±10%(精華版翻譯表)
            name: "APPROX_TOLERANCE".to_string(),
            value: crate::validator::wave_rules::APPROX_TOL,
            source: AssumptionSource::Interpretation,
        },
        Assumption {
            // architecture §4.2 第 2 檔:具體 Fibonacci 比率 ±4%
            name: "FIB_TOLERANCE".to_string(),
            value: crate::fibonacci::ratios::FIB_TOLERANCE_PCT / 100.0,
            source: AssumptionSource::Interpretation,
        },
        Assumption {
            // Ch9 Exception Rule「差距不大」的 10% 量化(原書質性)
            name: "CH9_EXCEPTION_GAP".to_string(),
            value: crate::validator::CH9_EXCEPTION_GAP_PCT / 100.0,
            source: AssumptionSource::Interpretation,
        },
        Assumption {
            // S&B 區間下界(rules 1189-1197 原書比例)
            name: "SB_MIN_RATIO".to_string(),
            value: crate::compaction::round_engine::SB_MIN_RATIO,
            source: AssumptionSource::Canon,
        },
        Assumption {
            // Ch9 Trendline Touchpoints「觸及」±2% 量化
            name: "TRENDLINE_TOUCH_TOLERANCE".to_string(),
            value: crate::advanced_rules::ch9::CH9_TOUCH_TOLERANCE_PCT,
            source: AssumptionSource::Interpretation,
        },
        Assumption {
            // polywave 門檻(> 3 sub-monowaves;spec 1042-1062 定義)
            name: "POLYWAVE_THRESHOLD".to_string(),
            value: crate::pre_constructive::predicates::POLYWAVE_THRESHOLD as f64,
            source: AssumptionSource::Canon,
        },
    ];
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

/// `name=value` 換行串接(collect 已排序)→ sha256 → 前 16 hex。
/// 跨 run 恆等;任一常數變動即變(J2 `engine_changed` 判別的機械依據)。
pub fn hash(assumptions: &[Assumption]) -> String {
    let joined: String = assumptions
        .iter()
        .map(|a| format!("{}={}", a.name, a.value))
        .collect::<Vec<_>>()
        .join("\n");
    let digest = Sha256::digest(joined.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
    hex[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_covers_spec_section_3_constants() {
        let list = collect();
        assert_eq!(list.len(), 8);
        let names: Vec<&str> = list.iter().map(|a| a.name.as_str()).collect();
        for expected in [
            "APPROX_TOLERANCE",
            "CH9_EXCEPTION_GAP",
            "FIB_TOLERANCE",
            "NEUTRAL_ATR_MULTIPLIER",
            "POLYWAVE_THRESHOLD",
            "REVERSAL_ATR_MULTIPLIER",
            "SB_MIN_RATIO",
            "TRENDLINE_TOUCH_TOLERANCE",
        ] {
            assert!(names.contains(&expected), "缺 {}", expected);
        }
        // name 升冪(hash 決定性依此)
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
        // 現值抽查(值變動 = 有意識決策,同步 spec)
        let by_name = |n: &str| list.iter().find(|a| a.name == n).unwrap();
        assert_eq!(by_name("REVERSAL_ATR_MULTIPLIER").value, 0.5);
        assert_eq!(by_name("NEUTRAL_ATR_MULTIPLIER").value, 1.0);
        assert_eq!(by_name("APPROX_TOLERANCE").value, 0.10);
        assert_eq!(by_name("FIB_TOLERANCE").value, 0.04);
        assert_eq!(by_name("CH9_EXCEPTION_GAP").value, 0.10);
        assert_eq!(by_name("SB_MIN_RATIO").value, 0.382);
        assert_eq!(by_name("TRENDLINE_TOUCH_TOLERANCE").value, 0.02);
        assert_eq!(by_name("POLYWAVE_THRESHOLD").value, 3.0);
        assert_eq!(
            by_name("REVERSAL_ATR_MULTIPLIER").source,
            crate::output::AssumptionSource::Engineering
        );
        assert_eq!(
            by_name("SB_MIN_RATIO").source,
            crate::output::AssumptionSource::Canon
        );
    }

    #[test]
    fn hash_is_stable_across_runs_and_sensitive_to_values() {
        let list = collect();
        let h1 = hash(&list);
        let h2 = hash(&collect());
        assert_eq!(h1, h2, "同常數集跨 run 恆等");
        assert_eq!(h1.len(), 16);
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));

        let mut mutated = collect();
        mutated[0].value += 0.01;
        assert_ne!(hash(&mutated), h1, "值變動必須反映到 hash");
    }
}
