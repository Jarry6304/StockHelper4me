// power_rating — Stage 10a:Power Rating 查表
//
// 對齊 m3Spec/neely_core.md §三 / §七 Stage 10 / §十三。
// 子模組(留後續 PR 實作):
//   - table.rs — Neely 書裡的 power rating 表(寫死,不可外部化 §6.6)
//
// 設計原則:
//   - PowerRating enum 取代 v1.1 i8(§9.4 防無效值)
//   - 截斷哲學:Neely 規則邊界外的 case 截斷不外推(§十三 power_rating 截斷哲學論證)

#![allow(dead_code)]

// M3 PR-1 skeleton:查表 PR-2 補,table.rs 包 Neely 書原始查表

pub mod table;
