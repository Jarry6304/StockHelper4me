// node.rs — v3 fractal 引擎內部節點 EngineNode(compaction 過程用),emit 時轉成凍結的
// output::WaveNode / TraditionalPatternType。internal-only,不進 wire contract。

use crate::mode::{Mode, PatternKind};
use crate::output::{
    DiagonalKind, DiagonalShape, DiagonalSub, Direction, TradRuleId, TraditionalPatternType, WaveNode,
};
use chrono::NaiveDate;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct EngineNode {
    pub kind: PatternKind,
    pub mode: Mode,
    pub direction: Direction,
    /// 0 = monowave(degree-0 葉);每 compaction round +1
    pub degree_level: usize,
    /// 原始 bar index 範圍(span / dedup / degree 用)
    pub start_bar: usize,
    pub end_bar: usize,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub start_price: f64,
    pub end_price: f64,
    /// 對角專屬(kind / shape / sub)
    pub diag: Option<(DiagonalKind, DiagonalShape, DiagonalSub)>,
    /// flat regular/expanded/running、triangle contracting/expanding 等顯示用子分類
    pub variant: Option<String>,
    /// v3 perf:子樹用 `Rc` 共享 — compaction 反覆 clone tiling 時只 bump 指標,
    /// 不深拷貝整棵樹(原 `Vec<EngineNode>` 深拷貝是 ~100s/股 + swap 的元兇)。
    pub children: Vec<Rc<EngineNode>>,
    pub passed_rules: Vec<TradRuleId>,
    pub deferred_rules: Vec<TradRuleId>,
}

impl EngineNode {
    /// degree-0 monowave 葉節點。
    pub fn monowave(
        start_bar: usize,
        end_bar: usize,
        start_date: NaiveDate,
        end_date: NaiveDate,
        start_price: f64,
        end_price: f64,
    ) -> Self {
        let direction = if end_price >= start_price {
            Direction::Up
        } else {
            Direction::Down
        };
        EngineNode {
            kind: PatternKind::Monowave,
            mode: Mode::Unknown,
            direction,
            degree_level: 0,
            start_bar,
            end_bar,
            start_date,
            end_date,
            start_price,
            end_price,
            diag: None,
            variant: None,
            children: Vec::new(),
            passed_rules: Vec::new(),
            deferred_rules: Vec::new(),
        }
    }

    /// orthodox 淨移動長度(價格絕對值)。
    pub fn amp(&self) -> f64 {
        (self.end_price - self.start_price).abs()
    }

    /// orthodox 淨移動百分比(對 start_price)。
    pub fn pct(&self) -> f64 {
        self.amp() / self.start_price.abs().max(1e-9)
    }

    /// 跨多少原始 bar(degree span)。
    pub fn span_bars(&self) -> usize {
        self.end_bar.saturating_sub(self.start_bar)
    }

    /// dedup canonical key:形態 + bar span + arity。
    pub fn canonical_key(&self) -> String {
        format!(
            "{:?}|{}|{}|{}",
            self.kind,
            self.start_bar,
            self.end_bar,
            self.children.len()
        )
    }

    /// 對映凍結的 output::TraditionalPatternType。
    pub fn pattern_type(&self) -> TraditionalPatternType {
        match self.kind {
            PatternKind::Impulse => TraditionalPatternType::Impulse,
            PatternKind::LeadingDiagonal => {
                let (k, s, sub) = self.diag.unwrap_or((
                    DiagonalKind::Leading,
                    DiagonalShape::Contracting,
                    DiagonalSub::AllThrees,
                ));
                TraditionalPatternType::Diagonal { kind: k, shape: s, sub }
            }
            PatternKind::EndingDiagonal => {
                let (k, s, sub) = self.diag.unwrap_or((
                    DiagonalKind::Ending,
                    DiagonalShape::Contracting,
                    DiagonalSub::AllThrees,
                ));
                TraditionalPatternType::Diagonal { kind: k, shape: s, sub }
            }
            PatternKind::Zigzag => TraditionalPatternType::Zigzag,
            PatternKind::Flat => TraditionalPatternType::Flat,
            PatternKind::Triangle => TraditionalPatternType::Triangle,
            PatternKind::Combination => TraditionalPatternType::Combination,
            // monowave 不會被當 top scenario emit(compaction 只收 degree_level>=1)
            PatternKind::Monowave => TraditionalPatternType::Zigzag,
        }
    }

    /// 遞迴轉成凍結 output::WaveNode(label 依 parent kind 的 child_labels)。
    pub fn to_wave_node(&self, label: String) -> WaveNode {
        let child_labels = self.kind.child_labels();
        let children = self
            .children
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let lab = child_labels
                    .get(i)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("{}", i + 1));
                c.to_wave_node(lab)
            })
            .collect();
        WaveNode {
            label,
            start: self.start_date,
            end: self.end_date,
            start_price: self.start_price,
            end_price: self.end_price,
            children,
        }
    }
}
