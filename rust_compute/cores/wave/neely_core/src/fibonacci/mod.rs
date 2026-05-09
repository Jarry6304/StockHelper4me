// fibonacci — Stage 10b:Fibonacci 投影
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 10 / §十四。
// 子模組(留後續 PR 實作):
//   - ratios.rs     — 比率清單(38.2%、61.8%、100%、161.8% 等)寫死(§4.4)
//   - projection.rs — expected_fib_zones 計算
//
// 設計原則:
//   - **Fibonacci 不獨立成 Core**(§十,cores_overview §十)— 屬 Neely 內部子模組
//   - 比率清單與 ±4% 容差寫死,**不可外部化**(§4.4)

#![allow(dead_code)]

// M3 PR-1 skeleton:Stage 10 Pipeline 留後續 PR
