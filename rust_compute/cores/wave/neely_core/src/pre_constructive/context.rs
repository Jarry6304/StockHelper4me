// pre_constructive/context.rs — m(-3)..m5 上下文構造
//
// 對齊 m3Spec/neely_rules.md §Rules of Observation(263-267 行):
//   m1 = 目前正在分析的 monowave
//   m0 = m1 前一個 monowave(index i-1)
//   m(-1) = m0 之前(i-2);m(-2) = i-3;m(-3) = i-4
//   m2 = m1 後一個(i+1);m3 = i+2;m4 = i+3;m5 = i+4
//
// **v4.6(2026-05-19)Group 3.1**:加 `bars: &'a [OhlcvBar]` 給 predicates 走
// intraday OHLC reference(e.g. m1_endpoint_broken_by_m2 用 m2.bar_indices
// 在 bars slice 找 high/low extrema)。

use crate::monowave::ClassifiedMonowave;
use crate::output::OhlcvBar;

/// 對 classified[i](= m1)的 9-frame 上下文 reference。
///
/// 邊界:m(-3)..m5 任一個若超出 slice 邊界 → None。
pub struct MonowaveContext<'a> {
    pub i: usize,
    pub classified: &'a [ClassifiedMonowave],
    /// v4.6:原始 OHLC bars slice(monowave 來源資料);predicate 走 intraday extrema 用
    pub bars: &'a [OhlcvBar],

    pub m_minus_3: Option<&'a ClassifiedMonowave>, // i-4
    pub m_minus_2: Option<&'a ClassifiedMonowave>, // i-3
    pub m_minus_1: Option<&'a ClassifiedMonowave>, // i-2
    pub m0: Option<&'a ClassifiedMonowave>,        // i-1
    pub m1: &'a ClassifiedMonowave,                // i (current)
    pub m2: Option<&'a ClassifiedMonowave>,        // i+1
    pub m3: Option<&'a ClassifiedMonowave>,        // i+2
    pub m4: Option<&'a ClassifiedMonowave>,        // i+3
    pub m5: Option<&'a ClassifiedMonowave>,        // i+4
}

impl<'a> MonowaveContext<'a> {
    /// 為 classified[i](當 m1)構造 9-frame 上下文。
    ///
    /// `bars` 為 monowave 原始 OHLC bars,供 intraday-aware predicate 用
    /// (e.g. m1_endpoint_broken_by_m2)。
    pub fn build(
        classified: &'a [ClassifiedMonowave],
        bars: &'a [OhlcvBar],
        i: usize,
    ) -> Option<Self> {
        if i >= classified.len() {
            return None;
        }
        Some(Self {
            i,
            classified,
            bars,
            m_minus_3: i.checked_sub(4).and_then(|j| classified.get(j)),
            m_minus_2: i.checked_sub(3).and_then(|j| classified.get(j)),
            m_minus_1: i.checked_sub(2).and_then(|j| classified.get(j)),
            m0: i.checked_sub(1).and_then(|j| classified.get(j)),
            m1: &classified[i],
            m2: classified.get(i + 1),
            m3: classified.get(i + 2),
            m4: classified.get(i + 3),
            m5: classified.get(i + 4),
        })
    }
}
