// facts.rs — Fact 產出規則
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §十五。
// 從 NeelyCoreOutput 萃取機械式 Fact(禁主觀詞彙,m2Spec/oldm2Spec/cores_overview §6.1.1):
//   - 結構性事實:每個 Scenario 的 pattern_type / power_rating
//   - 規則拒絕原因:RuleRejection 列表
//   - Fibonacci 對齊比率:expected_fib_zones
//   - Forest 規模 / 複雜度等等

#![allow(dead_code)]

// M3 PR-1 skeleton:Fact 產出規則留後續 PR;
// 寫好之後本檔 `expose pub fn produce(output: &NeelyCoreOutput) -> Vec<Fact>`,
// `lib.rs::NeelyCore::produce_facts` 直接 delegate。
