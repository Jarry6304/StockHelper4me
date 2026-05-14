//! Workflow toml dispatch(PR-9b)
//!
//! 從 `workflows/*.toml` 讀「跑哪些 Cores」宣告,過濾 run-all dispatch。
//! 對齊 m3Spec/cores_overview.md §13.1 Workflow vs Orchestrator 切分。
//!
//! Design:
//! - 不引入 ErasedCore trait wrapper(對齊 §四 / §十四 禁止抽象)
//! - 23 個 hardcoded match arm 維持不變,每 arm 前加 `if filter.is_enabled("xxx") {}`
//! - `--workflow` 未指定 → `CoreFilter::all_enabled()` → 全 23 cores 跑(對齊原 PR-9a 行為)

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Workflow toml 最小子集 — 只解析 dispatch 需要的欄位
/// 其他欄位(`[workflow]` / `[targets]` / `[output]` / `[schedule]`)目前忽略
#[derive(Debug, Deserialize)]
struct WorkflowConfig {
    cores: Vec<CoreConfig>,
}

#[derive(Debug, Deserialize)]
struct CoreConfig {
    name: String,
    enabled: bool,
    // priority / params 等其他欄位忽略
}

/// Core 過濾器:run-all 每次 dispatch 前查詢
pub struct CoreFilter {
    /// None → 全部 enabled(--workflow 未指定)
    /// Some(set) → 只跑 set 內的 cores
    enabled: Option<HashSet<String>>,
}

impl CoreFilter {
    /// 全 34 cores enabled(預設 / `--workflow` 未指定時)
    /// 對應 P0 1 + P1 indicator 8 + P3 indicator 8 + P2 pattern 3 + P2 chip 5
    /// + P2 fundamental 3 + P2 environment 6 = 34
    pub fn all_enabled() -> Self {
        Self { enabled: None }
    }

    /// 從 workflow toml 載入。
    ///
    /// 相對路徑會 walk-up cwd 樹找(對齊 dotenvy 模式),user 從 repo 子目錄(如
    /// `rust_compute/`)跑 binary 也能找到 `workflows/tw_stock_standard.toml`。
    pub fn from_workflow_toml(path: &Path) -> Result<Self> {
        let resolved = resolve_workflow_path(path);
        let content = std::fs::read_to_string(&resolved)
            .with_context(|| format!("read workflow toml failed: {}", resolved.display()))?;
        Self::from_toml_str(&content)
            .with_context(|| format!("parse workflow toml failed: {}", resolved.display()))
    }

    /// 從 toml str 直接解析(unit test + flexibility)
    pub fn from_toml_str(s: &str) -> Result<Self> {
        let cfg: WorkflowConfig = toml::from_str(s).context("toml parse failed")?;
        let enabled: HashSet<String> = cfg
            .cores
            .iter()
            .filter(|c| c.enabled)
            .map(|c| c.name.clone())
            .collect();
        Ok(Self {
            enabled: Some(enabled),
        })
    }

    /// 查詢某 core 是否在 enabled 列表內
    pub fn is_enabled(&self, core_name: &str) -> bool {
        match &self.enabled {
            None => true,
            Some(set) => set.contains(core_name),
        }
    }

    /// enabled cores 數量(僅供 log;None → 34 全跑)
    pub fn count_summary(&self) -> String {
        match &self.enabled {
            None => "all 34 cores enabled (no workflow toml)".to_string(),
            Some(set) => format!("{} cores enabled via workflow toml", set.len()),
        }
    }
}

/// 把相對 workflow 路徑 walk-up cwd 樹解到實際存在的檔案。
/// 對齊 `dotenvy` 行為:user 從 `repo_root/rust_compute/` 跑 binary,仍可解到
/// `repo_root/workflows/tw_stock_standard.toml`。
///
/// 絕對路徑 / cwd 直接存在的相對路徑 → 不動;否則最多往上 6 層找。找不到回原路徑
/// (讓上層 read_to_string 報原本的「系統找不到指定的路徑」)。
fn resolve_workflow_path(path: &Path) -> PathBuf {
    if path.is_absolute() || path.exists() {
        return path.to_path_buf();
    }
    let mut cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(_) => return path.to_path_buf(),
    };
    for _ in 0..6 {
        let candidate = cwd.join(path);
        if candidate.exists() {
            return candidate;
        }
        if !cwd.pop() {
            break;
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_enabled_returns_true_for_any_name() {
        let filter = CoreFilter::all_enabled();
        assert!(filter.is_enabled("macd_core"));
        assert!(filter.is_enabled("unknown_core"));
    }

    #[test]
    fn workflow_toml_filters_correctly() {
        let toml_str = r#"
[workflow]
name = "test"

[[cores]]
name = "macd_core"
enabled = true

[[cores]]
name = "rsi_core"
enabled = false

[[cores]]
name = "neely_core"
enabled = true
"#;
        let filter = CoreFilter::from_toml_str(toml_str).unwrap();
        assert!(filter.is_enabled("macd_core"));
        assert!(!filter.is_enabled("rsi_core"));
        assert!(filter.is_enabled("neely_core"));
        assert!(!filter.is_enabled("absent_core"));
    }

    #[test]
    fn invalid_path_returns_error() {
        let result = CoreFilter::from_workflow_toml(Path::new("/non/existent/path.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn malformed_toml_returns_error() {
        let result = CoreFilter::from_toml_str("not a valid toml :::");
        assert!(result.is_err());
    }

    #[test]
    fn resolve_path_absolute_returns_unchanged() {
        // 絕對路徑(即使不存在)也直接回原值,讓 read_to_string 報原本的錯
        let abs = Path::new("/non/existent/abs.toml");
        let resolved = resolve_workflow_path(abs);
        assert_eq!(resolved, abs);
    }

    #[test]
    fn resolve_path_existing_relative_returns_unchanged() {
        // 構造一個臨時檔案在 cwd,relative 路徑直接 hit
        let tmp = std::env::temp_dir();
        let probe = tmp.join("__test_workflow_resolve_existing.toml");
        std::fs::write(&probe, "").unwrap();
        let resolved = resolve_workflow_path(&probe);
        assert_eq!(resolved, probe);
        let _ = std::fs::remove_file(&probe);
    }

    #[test]
    fn all_cores_disabled_returns_empty_filter() {
        let toml_str = r#"
[[cores]]
name = "macd_core"
enabled = false

[[cores]]
name = "rsi_core"
enabled = false
"#;
        let filter = CoreFilter::from_toml_str(toml_str).unwrap();
        assert!(!filter.is_enabled("macd_core"));
        assert!(!filter.is_enabled("rsi_core"));
    }
}
