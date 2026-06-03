// mode.rs — v3 fractal 引擎:Mode + PatternKind + L9 parent→slot 子浪 mode 表
//
// 對齊 traditional_rules.md L9(actionary corrective)+ 附錄 B(形態→子浪細分)。
// **忠實骨幹**:actionary ≠ 永遠是 5;子浪該是 motive(5)還是 corrective(3)由
// 「在 parent 的哪個 slot」決定(例:Ending Diagonal 的浪 1/3/5 雖 actionary,細分卻是 3)。

/// 一個波浪節點的「模式」:推動(計為 5)/ 修正(計為 3)/ 未知(degree-0 monowave,線內不可見)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Motive,
    Corrective,
    Unknown,
}

/// 引擎內部 pattern 種類(對映 output::TraditionalPatternType,但 Leading/Ending 拆開更好處理)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PatternKind {
    Monowave, // degree-0 葉(一條線,無內部結構)
    Impulse,
    LeadingDiagonal,
    EndingDiagonal,
    Zigzag,
    Flat,
    Triangle,
    Combination,
}

impl PatternKind {
    /// 此形態作為「上一級子浪」時的模式(monowave = Unknown)。
    pub fn mode(self) -> Mode {
        use PatternKind::*;
        match self {
            Monowave => Mode::Unknown,
            Impulse | LeadingDiagonal | EndingDiagonal => Mode::Motive,
            Zigzag | Flat | Triangle | Combination => Mode::Corrective,
        }
    }

    /// 此形態的 child 數(Zigzag/Flat=3;Impulse/Diagonal/Triangle=5;其他不固定)。
    pub fn child_count(self) -> usize {
        use PatternKind::*;
        match self {
            Zigzag | Flat => 3,
            Impulse | LeadingDiagonal | EndingDiagonal | Triangle => 5,
            _ => 0,
        }
    }

    /// child 標籤(對映 WaveNode.label)。
    pub fn child_labels(self) -> &'static [&'static str] {
        use PatternKind::*;
        match self {
            Impulse | LeadingDiagonal | EndingDiagonal => &["1", "2", "3", "4", "5"],
            Zigzag | Flat => &["A", "B", "C"],
            Triangle => &["a", "b", "c", "d", "e"],
            Combination => &["W", "X", "Y", "X", "Z"],
            Monowave => &[],
        }
    }
}

/// L9 parent→slot **必需子浪模式**(slot 為 0-based child index)。
///
/// 注意:LeadingDiagonal 因有 5-3-5-3-5 / 3-3-3-3-3 兩 sub,模式依 sub 而定 → 回 Unknown,
/// 由 diagonal grouper 自行依 sub 處理(不走此表)。
pub fn required_child_mode(parent: PatternKind, slot: usize) -> Mode {
    use PatternKind::*;
    match parent {
        // 1/3/5(idx 0/2/4)= Motive;2/4(idx 1/3)= Corrective。slot-3(idx 2)另須是 Impulse(grouper 查)
        Impulse => {
            if slot % 2 == 0 {
                Mode::Motive
            } else {
                Mode::Corrective
            }
        }
        // 結束對角全 3-3-3-3-3
        EndingDiagonal => Mode::Corrective,
        // 引導對角:依 sub,grouper 處理
        LeadingDiagonal => Mode::Unknown,
        // 鋸齒 5-3-5:A(0)Motive、B(1)Corrective、C(2)Motive
        Zigzag => {
            if slot == 1 {
                Mode::Corrective
            } else {
                Mode::Motive
            }
        }
        // 平台 3-3-5:A(0)Corrective、B(1)Corrective、C(2)Motive
        Flat => {
            if slot == 2 {
                Mode::Motive
            } else {
                Mode::Corrective
            }
        }
        // 三角形 / 組合:全 Corrective
        Triangle | Combination => Mode::Corrective,
        Monowave => Mode::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_slot_modes_5_3_5_3_5() {
        // idx 0,2,4 = Motive(5);idx 1,3 = Corrective(3)
        assert_eq!(required_child_mode(PatternKind::Impulse, 0), Mode::Motive);
        assert_eq!(required_child_mode(PatternKind::Impulse, 1), Mode::Corrective);
        assert_eq!(required_child_mode(PatternKind::Impulse, 2), Mode::Motive);
        assert_eq!(required_child_mode(PatternKind::Impulse, 4), Mode::Motive);
    }

    #[test]
    fn ending_diagonal_all_corrective_even_though_actionary() {
        // L9 核心:Ending 浪 1/3/5 雖 actionary,細分是 3(Corrective)
        for slot in 0..5 {
            assert_eq!(required_child_mode(PatternKind::EndingDiagonal, slot), Mode::Corrective);
        }
    }

    #[test]
    fn zigzag_5_3_5_flat_3_3_5() {
        assert_eq!(required_child_mode(PatternKind::Zigzag, 0), Mode::Motive); // A=5
        assert_eq!(required_child_mode(PatternKind::Flat, 0), Mode::Corrective); // A=3
        assert_eq!(required_child_mode(PatternKind::Flat, 2), Mode::Motive); // C=5
    }

    #[test]
    fn kind_mode_mapping() {
        assert_eq!(PatternKind::Impulse.mode(), Mode::Motive);
        assert_eq!(PatternKind::Zigzag.mode(), Mode::Corrective);
        assert_eq!(PatternKind::Monowave.mode(), Mode::Unknown);
    }
}
