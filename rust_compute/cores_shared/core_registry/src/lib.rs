// core_registry — inventory CoreRegistration + CoreRegistry::discover
//
// 對齊 m3Spec/cores_overview.md §五(Monolithic Binary 部署模型)。
//
// 設計:
//   - 每個 Core 在自己的 lib 用 `inventory::submit!` 註冊一個 CoreRegistration
//   - tw_cores binary 啟動時呼叫 `CoreRegistry::discover()` 自動發現所有已編譯的 Core
//   - 新增 Core 不用改 Orchestrator 代碼,只要寫好 Core 並編譯進去,自動可用
//
// **M3 PR-8 階段**:
//   - CoreRegistration struct(name / version / kind / category)
//   - CoreRegistry::discover() 從 inventory iter 收集所有註冊
//   - inventory::collect!(CoreRegistration)
//   - 各 Core 留下 PR 自己加 inventory::submit!
//
// 為什麼 neely_core 暫不 submit:
//   - inventory::submit! 需要 const fn constructor(對 NeelyCore::new() OK)
//   - 但 trait object Box<dyn WaveCore> 不能直接 store(WaveCore 有 associated types)
//   - 完整 trait object dispatch 留 PR-8b(需 erase associated types,設計 ErasedWaveCore trait)
//
// PR-8 範圍:registration metadata 落地 + framework expose,
// 各 Core trait dispatch 留 PR-8b(可能需要 dyn-compatible trait wrapper)。

use std::sync::OnceLock;

/// Core 種類分類(對齊 cores_overview.md §8 所有 Core 清單)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreKind {
    Wave,
    Indicator,
    Chip,
    Fundamental,
    Environment,
    System,
}

/// Core 註冊 metadata。各 Core 用 inventory::submit! 註冊一筆。
#[derive(Debug, Clone)]
pub struct CoreRegistration {
    pub name: &'static str,
    pub version: &'static str,
    pub kind: CoreKind,
    pub priority: &'static str, // "P0" / "P1" / "P2" / "P3"
    pub description: &'static str,
}

impl CoreRegistration {
    pub const fn new(
        name: &'static str,
        version: &'static str,
        kind: CoreKind,
        priority: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            name,
            version,
            kind,
            priority,
            description,
        }
    }
}

inventory::collect!(CoreRegistration);

/// CoreRegistry — Orchestrator 啟動時呼叫 discover() 自動找全部 Core。
pub struct CoreRegistry {
    cores: Vec<CoreRegistration>,
}

impl CoreRegistry {
    /// 從 inventory 自動發現所有編譯期註冊的 Core。
    ///
    /// 注意:Rust inventory 在 binary linker 階段收集 — 必須由 binary crate
    /// 透過 dependency 引用所有要納入的 core crate,否則 dead-code 會被剃掉。
    /// tw_cores 已對 neely_core 等 dep,後續 chip/indicator core 一律走同 pattern。
    pub fn discover() -> &'static Self {
        static REGISTRY: OnceLock<CoreRegistry> = OnceLock::new();
        REGISTRY.get_or_init(|| CoreRegistry {
            cores: inventory::iter::<CoreRegistration>
                .into_iter()
                .cloned()
                .collect(),
        })
    }

    pub fn cores(&self) -> &[CoreRegistration] {
        &self.cores
    }

    pub fn find(&self, name: &str) -> Option<&CoreRegistration> {
        self.cores.iter().find(|c| c.name == name)
    }

    pub fn by_kind(&self, kind: CoreKind) -> Vec<&CoreRegistration> {
        self.cores.iter().filter(|c| c.kind == kind).collect()
    }

    pub fn by_priority(&self, priority: &str) -> Vec<&CoreRegistration> {
        self.cores.iter().filter(|c| c.priority == priority).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 在 test 註冊一筆 fake Core 驗 discover() 機制 work
    inventory::submit! {
        CoreRegistration::new(
            "_test_core_",
            "0.0.0-test",
            CoreKind::System,
            "P0",
            "Inventory smoke test fixture",
        )
    }

    #[test]
    fn registry_discovers_test_core() {
        let registry = CoreRegistry::discover();
        // 至少有 _test_core_ 一筆(testing 內 inventory::submit! 在編譯期已加)
        let test_core = registry.find("_test_core_").expect("應發現 _test_core_");
        assert_eq!(test_core.version, "0.0.0-test");
        assert_eq!(test_core.kind, CoreKind::System);
    }

    #[test]
    fn by_kind_filters_correctly() {
        let registry = CoreRegistry::discover();
        let system_cores = registry.by_kind(CoreKind::System);
        assert!(system_cores.iter().any(|c| c.name == "_test_core_"));
    }
}
