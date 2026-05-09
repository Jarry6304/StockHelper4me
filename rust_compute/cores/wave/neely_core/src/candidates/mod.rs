// candidates — Stage 3:Bottom-up Candidate Generator
//
// 對齊 m2Spec/oldm2Spec/neely_core.md §三 / §七 Stage 3。
// 子模組:
//   - generator.rs:把 monowave 序列窮舉成所有可能的「波浪結構候選」
//
// Stage 3 範圍:
//   - 輸入:Stage 2 ClassifiedMonowave list(已套 Rule of Neutrality + Proportion)
//   - 輸出:Vec<WaveCandidate> — 所有可能 wave structure 視窗
//   - **不**判定 pattern_type(Impulse / Zigzag / ...)— 那是 Stage 5 Classifier
//   - **不**檢查 R1-R7 Neely 規則 — 那是 Stage 4 Validator

pub mod generator;

pub use generator::{generate_candidates, WaveCandidate};
