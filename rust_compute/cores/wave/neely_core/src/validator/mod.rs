// validator — Stage 4:Validator R1-R7 / F1-F2 / Z1-Z4 / T1-T10 / W1-W2
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 4 / §十(規則組)。
// 子模組(留後續 PR 實作):
//   - core_rules.rs   — R1-R7
//   - flat_rules.rs   — F1-F2
//   - zigzag_rules.rs — Z1-Z4
//   - triangle_rules.rs — T1-T10
//   - wave_rules.rs   — W1-W2
//
// 容差規範(§10.4):相對 ±4%(寫死)+ Waterfall Effect ±5% 例外(寫死)
// — 不可外部化(§4.4 / §6.6)

#![allow(dead_code)]

// M3 PR-1 skeleton:Stage 4 Pipeline 留後續 PR
