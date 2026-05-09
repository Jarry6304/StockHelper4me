// compaction — Stage 8:Compaction(窮舉 Forest)+ Forest 上限保護
//
// 對齊 m3Spec/neely_core.md §三 / §七 Stage 8 / §十一 / §十二。
// 子模組(留後續 PR 實作):
//   - exhaustive.rs   — 窮舉模式(預設)
//   - beam_search.rs  — Forest 上限保護的 fallback(§12)
//
// 關鍵設計:
//   - 純結構壓縮,**不**選最優,不附 primary(§9.3)
//   - 重寫 v1.1 的「貪心選分數」(§4.2)— 多種解讀路徑窮舉成 Forest

#![allow(dead_code)]

// M3 PR-1 skeleton:Stage 8 Pipeline 留後續 PR
