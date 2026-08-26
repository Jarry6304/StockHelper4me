// round_engine.rs — Compaction v2 tiling-round 引擎(G2.3:重評 / 真算 / 細分)
//
// 對齊 m3Spec/neely_compaction_v2.md:
//   §2.2 不變量 I1–I6 / §3.1 目標架構 / §4 try_all_neely 七階梯 / §4.4 W6 分岔 /
//   §5 round 引擎 / §6 Round 2 Reassessment / §9.3 shadow 比對 / §12 Q1/Q3 收案。
//
// G2.1(骨架)+ G2.2(全階梯)+ G2.3(本輪)已落地:
//   - 基礎設施:Rc 節點、base tiling(Neutral 合成葉橋接)、round 迴圈、dedup、
//     beam、`level_cap_hit`(A-8)
//   - 階梯:W1(I2 防衛)/ W2(I5 閉合表 + **A-9 Flat 七變體 / CombinationKind
//     細分,與 classifier 量值版核心同源**)/ W3(交替)/ W4(S&B)/
//     W5(Ch5 Validator 端點泛化,§4.3)/ W6(3-3-3-3-3 分岔判別,§4.4,
//     D-5 修復 — Terminal Impulse 以 Diagonal 表徵)/ W7(Fib² 全相鄰對)
//   - **§6.1 邊界波重評**(D-4):真鄰居 magnitude 三檔判定(Pass / Info /
//     Warning),不拒絕聚合;shadow 期計數觀測
//   - **§6.2 Complexity 真算 + Triplexity**、**§6.3 Degree ceiling 錨定對映**、
//     **A-10 anchors union vs 現行 overlap 近似**(收集 forest 統計)
//   - Q3 bars 反查為判準(2026-08-26 拍板;q3_* 保留為殘差觀測);Q1 雙軌定案
//     (符號鏈 = 首子波 → `synth_window` 之 `window[0].net_direction`;
//     幾何鏈 = 節點 net → W3)
//   - **shadow 雙軌**(§3.3):輸出僅寫 `NeelyDiagnostics.shadow_compaction`,
//     serving forest 完全不受影響;Gate v3 通過後才切換
//
// 工程注意:
//   - W5 端點介面:每視窗合成 ClassifiedMonowave(端點價 / duration_bars /
//     label 候選),餵既有 `validator::validate_candidate` — 同一套規則碼雙形態
//     輸入(§4.3「bar 級概念不上樓,價格結構規則全上樓」;規則本體不消費 ATR,
//     §4.3 的 R3 容差替換在此程式路徑無實體)。passed 清單與 Level-0 同源
//     (classifier::default_passed_rules 反推),beam 鍵 2 因此可比
//   - W7 位於便宜前濾(與 W3/W4 同組)— 與 spec 階梯末位語意等價(row 無關,
//     全拒/全過),省掉對必死視窗跑 W5
//   - round 內生成走 **兩階段**(G2.1 gate 實測修正):先枚舉全部視窗收
//     splice 候選(輕量 spec,不 materialize),再依 beam 鍵近似分數降序選
//     materialize 至 branch cap — 消除先枚舉視窗的時間軸偏置
//   - W5 / Q3 計數以「唯一視窗」為單位(memo 命中不重計);§6.1 邊界重評屬
//     tiling 語境(同節點跨 tiling 鄰居不同)→ 逐 materialize 計

use crate::candidates::WaveCandidate;
use crate::classifier;
use crate::config::NeelyEngineConfig;
use crate::monowave::{ClassifiedMonowave, ProportionMetrics};
use crate::output::{
    Certainty, Degree, DiagonalKind, MonowaveDirection, NeelyPatternType, OhlcvBar, PatternBound,
    RuleId, Scenario, ShadowCompactionDiagnostics, StructureLabel, StructureLabelCandidate,
    TriangleKind, WaveNode, ZigzagKind,
};
use crate::power_rating;
use crate::validator::{self, ValidationReport};
use chrono::NaiveDate;
use fact_schema::Timeframe;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Instant;

// ---------------------------------------------------------------------------
// 節點(§5.1;internal-only,不進 wire contract)
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum NodeKind {
    Leaf,
    Pattern(NeelyPatternType),
}

/// 節點的 W2 slot 標:聚合節點持唯一 `:3`/`:5`(I5);葉節點持 Stage 0
/// Pre-Constructive 候選集合(任一候選匹配即過,多義性延後 W5/W6 消解,§4.2)。
#[derive(Debug)]
pub enum NodeLabel {
    Fixed(StructureLabel),
    LeafCandidates(Vec<StructureLabel>),
}

#[derive(Debug)]
pub struct CompactionNode {
    pub kind: NodeKind,
    pub label: NodeLabel,
    /// 葉 = 0;I4:parent = max(children) + 1
    pub degree_level: usize,
    pub start_bar: usize,
    pub end_bar: usize,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub start_price: f64,
    pub end_price: f64,
    pub children: Vec<Rc<CompactionNode>>,
    /// W5 產出;G2.1 stub 恆 None
    pub validation: Option<ValidationReport>,
    pub net_direction: MonowaveDirection,
    /// §5.4 canonical_key,建構時預算(節點不可變,Rc 共享)
    pub canonical: String,
    /// **Q3 拍板(2026-08-26 六檔實測 flip 33.3% > 5% → 落 bars 反查)**:
    /// 節點範圍的真實 (low, high) — 葉於 base tiling 建構時掃 bars 一次,
    /// parent 取 children 聯集;bars 不可用 → None(判定退端點版)。
    /// 消費點:W6 分岔的回測 / Overlap / 觸線判定
    pub true_range: Option<(f64, f64)>,
}

/// `:5` 族 label(W2 slot 匹配用;含 Neely extension 變體)。
fn is_impulsive_label(l: StructureLabel) -> bool {
    matches!(
        l,
        StructureLabel::Five
            | StructureLabel::F5
            | StructureLabel::L5
            | StructureLabel::UnknownFive
            | StructureLabel::S5
            | StructureLabel::SL5
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Three,
    Five,
}

impl CompactionNode {
    fn matches_slot(&self, slot: Slot) -> bool {
        match &self.label {
            NodeLabel::Fixed(l) => match slot {
                Slot::Five => is_impulsive_label(*l),
                Slot::Three => !is_impulsive_label(*l),
            },
            NodeLabel::LeafCandidates(cands) => cands.iter().any(|l| match slot {
                Slot::Five => is_impulsive_label(*l),
                Slot::Three => !is_impulsive_label(*l),
            }),
        }
    }

    fn price_magnitude(&self) -> f64 {
        (self.end_price - self.start_price).abs()
    }

    /// W4 time 維度:duration_bars(Q3 拍板連動 — bars 為判準後,time 同步
    /// 轉交易時間基準;端點日曆日的舊引擎 parity 理由已隨舊 Level-N 消亡失效)
    fn duration_bars(&self) -> f64 {
        (self.end_bar.saturating_sub(self.start_bar) + 1) as f64
    }

    /// 判定用範圍:真實 high/low(Q3 拍板)優先,無 bars 退端點
    fn judge_range(&self) -> (f64, f64) {
        self.true_range.unwrap_or_else(|| {
            (
                self.start_price.min(self.end_price),
                self.start_price.max(self.end_price),
            )
        })
    }
}

/// pattern_type 緊湊 tag(canonical_key 與 §9.3 shadow 比對共用;含變體避免誤併)。
pub fn pattern_tag(pt: &NeelyPatternType) -> String {
    match pt {
        NeelyPatternType::Impulse => "Impulse".to_string(),
        NeelyPatternType::Diagonal { sub_kind } => format!("Diagonal:{:?}", sub_kind),
        NeelyPatternType::Zigzag { sub_kind } => format!("Zigzag:{:?}", sub_kind),
        NeelyPatternType::Flat { sub_kind } => format!("Flat:{:?}", sub_kind),
        NeelyPatternType::Triangle { sub_kind } => format!("Triangle:{:?}", sub_kind),
        NeelyPatternType::Combination { sub_kinds } => {
            let kinds: Vec<String> = sub_kinds.iter().map(|k| format!("{:?}", k)).collect();
            format!("Combination:{}", kinds.join("+"))
        }
        NeelyPatternType::RunningCorrection => "RunningCorrection".to_string(),
    }
}

fn leaf_canonical(start_bar: usize, end_bar: usize) -> String {
    format!("L({},{})", start_bar, end_bar)
}

fn pattern_canonical(
    pt: &NeelyPatternType,
    base: StructureLabel,
    start_bar: usize,
    end_bar: usize,
    children: &[Rc<CompactionNode>],
) -> String {
    let child_keys: Vec<&str> = children.iter().map(|c| c.canonical.as_str()).collect();
    format!(
        "P({},{:?},{},{},[{}])",
        pattern_tag(pt),
        base,
        start_bar,
        end_bar,
        child_keys.join(",")
    )
}

// ---------------------------------------------------------------------------
// Base tiling(§5.2 步驟 1:非 Neutral + 合成葉橋接)
// ---------------------------------------------------------------------------

struct BaseTiling {
    nodes: Vec<Rc<CompactionNode>>,
    /// Neutral 段併入前一 directional 節點的次數
    bridged: usize,
    /// 開頭 Neutral 無前節點可併,直接排除的數量(記 diagnostics)
    leading_dropped: usize,
}

/// 葉範圍的真實 (low, high):掃 bars 一次(Q3 拍板成本,spec §12 已列);
/// bars 缺 / 範圍越界 → None(判定退端點)。
fn leaf_true_range(bars: &[OhlcvBar], start_bar: usize, end_bar: usize) -> Option<(f64, f64)> {
    if bars.is_empty() || start_bar > end_bar || end_bar >= bars.len() {
        return None;
    }
    let slice = &bars[start_bar..=end_bar];
    let hi = slice.iter().map(|b| b.high).fold(f64::MIN, f64::max);
    let lo = slice.iter().map(|b| b.low).fold(f64::MAX, f64::min);
    Some((lo, hi))
}

fn build_base_tiling(classified: &[ClassifiedMonowave], bars: &[OhlcvBar]) -> BaseTiling {
    // 兩段式:先聚出「葉 spec」(Neutral 併前節點延伸端點),再一次建 Rc 節點
    struct LeafSpec {
        start_bar: usize,
        end_bar: usize,
        start_date: NaiveDate,
        end_date: NaiveDate,
        start_price: f64,
        end_price: f64,
        direction: MonowaveDirection,
        candidates: Vec<StructureLabel>,
    }

    let mut specs: Vec<LeafSpec> = Vec::new();
    let mut bridged = 0usize;
    let mut leading_dropped = 0usize;

    for cm in classified {
        let mw = &cm.monowave;
        if mw.direction == MonowaveDirection::Neutral {
            match specs.last_mut() {
                Some(prev) => {
                    // 合成葉:時間範圍延伸至 Neutral 段 end,價格端點取前節點 start
                    // 至 Neutral 段 end(§5.2;現況「過濾 Neutral 後滑窗」的顯式化)
                    prev.end_bar = mw.bar_indices.1;
                    prev.end_date = mw.end_date;
                    prev.end_price = mw.end_price;
                    // 延伸後淨向依端點重算;端點打平時保留原方向
                    let delta = prev.end_price - prev.start_price;
                    if delta > 0.0 {
                        prev.direction = MonowaveDirection::Up;
                    } else if delta < 0.0 {
                        prev.direction = MonowaveDirection::Down;
                    }
                    bridged += 1;
                }
                None => leading_dropped += 1,
            }
        } else {
            specs.push(LeafSpec {
                start_bar: mw.bar_indices.0,
                end_bar: mw.bar_indices.1,
                start_date: mw.start_date,
                end_date: mw.end_date,
                start_price: mw.start_price,
                end_price: mw.end_price,
                direction: mw.direction,
                candidates: cm
                    .structure_label_candidates
                    .iter()
                    .map(|c| c.label)
                    .collect(),
            });
        }
    }

    let nodes = specs
        .into_iter()
        .map(|s| {
            Rc::new(CompactionNode {
                kind: NodeKind::Leaf,
                label: NodeLabel::LeafCandidates(s.candidates),
                degree_level: 0,
                canonical: leaf_canonical(s.start_bar, s.end_bar),
                true_range: leaf_true_range(bars, s.start_bar, s.end_bar),
                start_bar: s.start_bar,
                end_bar: s.end_bar,
                start_date: s.start_date,
                end_date: s.end_date,
                start_price: s.start_price,
                end_price: s.end_price,
                children: Vec::new(),
                validation: None,
                net_direction: s.direction,
            })
        })
        .collect();

    BaseTiling {
        nodes,
        bridged,
        leading_dropped,
    }
}

// ---------------------------------------------------------------------------
// 接受階梯 W1–W4 + W7(§4.2;W5/W6 stub 留 G2.2)
// ---------------------------------------------------------------------------

/// S&B 區間(rules line 1189-1197;與 three_rounds.rs 同值)
const SB_MIN_RATIO: f64 = 0.382;
const SB_MAX_RATIO: f64 = 2.618;
/// Fib² 極端區間(v4.8 G1.3 沿用;W7)
const FIB2_MIN: f64 = 0.236;
const FIB2_MAX: f64 = 4.236;
const PRICE_EPS: f64 = 1e-9;

#[derive(Debug, Clone)]
struct AcceptedKind {
    pattern: NeelyPatternType,
    base: StructureLabel,
}

fn ratio_in(a: f64, b: f64, min: f64, max: f64) -> bool {
    if a <= 0.0 || b <= 0.0 {
        return false;
    }
    let r = a / b;
    r >= min && r <= max
}

/// W1:I2 共享端點防衛。tiling 建構已保證,違反 = 引擎 bug —
/// debug panic / release 記 Engineering violation(caller 計數)。
fn w1_adjacency(window: &[Rc<CompactionNode>]) -> bool {
    for i in 1..window.len() {
        let a = &window[i - 1];
        let b = &window[i];
        if a.end_date != b.start_date || a.end_price != b.start_price {
            debug_assert!(
                false,
                "W1/I2 violation in tiling window: {} → {}",
                a.canonical, b.canonical
            );
            return false;
        }
    }
    true
}

/// W3:相鄰節點 net direction 嚴格交替(Neutral 不進聚合視窗)。
fn w3_alternating(window: &[Rc<CompactionNode>]) -> bool {
    for i in 0..window.len() {
        if window[i].net_direction == MonowaveDirection::Neutral {
            return false;
        }
        if i > 0 && window[i].net_direction == window[i - 1].net_direction {
            return false;
        }
    }
    true
}

/// W4:S&B — 每相鄰對 price 或 time 其一之比 ∈ [0.382, 2.618]。
/// price 取端點差絕對值(零幅度退 time 單維,沿用現行 Option 語意);
/// time 取 duration_bars(Q3 拍板連動,交易時間基準)。
fn w4_similarity_balance(window: &[Rc<CompactionNode>]) -> bool {
    for i in 1..window.len() {
        let (a, b) = (&window[i - 1], &window[i]);
        let (pa, pb) = (a.price_magnitude(), b.price_magnitude());
        let price_ok = pa > PRICE_EPS && pb > PRICE_EPS && ratio_in(pa, pb, SB_MIN_RATIO, SB_MAX_RATIO);
        let time_ok = ratio_in(a.duration_bars(), b.duration_bars(), SB_MIN_RATIO, SB_MAX_RATIO);
        if !(price_ok || time_ok) {
            return false;
        }
    }
    true
}

/// W7:視窗內部比例極端 — 全相鄰對 magnitude 比落 [0.236, 4.236] 外 → 拒絕。
/// 零幅度對(不可判)跳過,與舊引擎 Some/Some 才比對的語意一致。
fn w7_internal_extreme(window: &[Rc<CompactionNode>]) -> bool {
    for i in 1..window.len() {
        let (pa, pb) = (window[i - 1].price_magnitude(), window[i].price_magnitude());
        if pa > PRICE_EPS && pb > PRICE_EPS && !ratio_in(pb, pa, FIB2_MIN, FIB2_MAX) {
            return false;
        }
    }
    true
}

/// W2 row 匹配結果:Direct = 形態唯一;ThreesFive = 3-3-3-3-3,交 W6 分岔(§4.4)。
enum RowMatch {
    Direct(AcceptedKind),
    ThreesFive,
}

/// 3 節點 sub-segment(a-b-c)以量值版核心分類 — 與 monowave 級
/// `classifier::classify_3wave_mags` 同源(A-9 要求;「波」介面泛化為端點幅度)。
fn classify_3seg_nodes(seg: &[Rc<CompactionNode>]) -> NeelyPatternType {
    debug_assert_eq!(seg.len(), 3);
    classifier::classify_3wave_mags(
        seg[0].price_magnitude(),
        seg[1].price_magnitude(),
        seg[2].price_magnitude(),
    )
}

/// x-wave 相對兩側 sub-segment 是否為「大 x-wave」(Table B)—
/// 與 monowave 級 `x_wave_is_large` 同式:x magnitude ≥ 61.8% × min(兩側淨幅)。
fn x_node_is_large(
    x: &Rc<CompactionNode>,
    sub_a: &[Rc<CompactionNode>],
    sub_b: &[Rc<CompactionNode>],
) -> bool {
    let net_span = |seg: &[Rc<CompactionNode>]| -> f64 {
        (seg[seg.len() - 1].end_price - seg[0].start_price).abs()
    };
    x.price_magnitude() >= 0.618 * net_span(sub_a).min(net_span(sub_b))
}

/// W2:I5 閉合表(§4.2.1)。回傳匹配 rows(0..k,同窗多解各產分支)。
///
/// **G2.3(A-9)**:Flat 七變體與 CombinationKind 細分以 classifier 量值版核心
/// 同源判定,取代 Common / DoubleThree / TripleThree 佔位 —
/// 3-窗 [:3 :3 :5] 依 a/b/c 幅度細分 RunningCorrection / Flat{七變體}
/// (b/a < 61.8% 不符 Flat 最低要求 → 該 row 不成立);
/// 7/11-窗依 Ch8 Table A/B(x-wave 大小 + 構成段 kind)對映 11-variant,
/// 不可辨識組合 → row 不成立(不產 garbage,與 monowave 級行為一致)。
fn w2_label_rows(window: &[Rc<CompactionNode>]) -> Vec<RowMatch> {
    use Slot::{Five, Three};
    let mut out = Vec::new();

    let matches_seq = |slots: &[Slot]| -> bool {
        slots.len() == window.len()
            && window.iter().zip(slots).all(|(n, s)| n.matches_slot(*s))
    };

    match window.len() {
        3 => {
            if matches_seq(&[Five, Three, Five]) {
                out.push(RowMatch::Direct(AcceptedKind {
                    pattern: NeelyPatternType::Zigzag {
                        sub_kind: ZigzagKind::Single,
                    },
                    base: StructureLabel::Three,
                }));
            }
            if matches_seq(&[Three, Three, Five]) {
                // A-9:依幅度細分 Flat-family(Zigzag 判讀 = b/a 過小,
                // 與 [:3 :3 :5] row 的 Flat 語意矛盾 → row 不成立)
                match classify_3seg_nodes(window) {
                    NeelyPatternType::Zigzag { .. } => {}
                    pt => out.push(RowMatch::Direct(AcceptedKind {
                        pattern: pt,
                        base: StructureLabel::Three,
                    })),
                }
            }
        }
        5 => {
            if matches_seq(&[Five, Three, Five, Three, Five]) {
                out.push(RowMatch::Direct(AcceptedKind {
                    pattern: NeelyPatternType::Impulse,
                    base: StructureLabel::Five,
                }));
            }
            if matches_seq(&[Three, Three, Three, Three, Three]) {
                out.push(RowMatch::ThreesFive);
            }
        }
        7 => {
            if matches_seq(&[Three; 7]) {
                // A-9:3+x+3 → Double-* 細分(位置慣例:x = 第 4 節點)
                let kind_a = classify_3seg_nodes(&window[0..3]);
                let kind_b = classify_3seg_nodes(&window[4..7]);
                let large_x = x_node_is_large(&window[3], &window[0..3], &window[4..7]);
                if let Some(k) = classifier::map_double_combination(&kind_a, &kind_b, large_x) {
                    out.push(RowMatch::Direct(AcceptedKind {
                        pattern: NeelyPatternType::Combination {
                            sub_kinds: vec![k],
                        },
                        base: StructureLabel::Three,
                    }));
                }
            }
        }
        11 => {
            if matches_seq(&[Three; 11]) {
                // A-9:3+x+3+x+3 → Triple-* 細分(x = 第 4、8 節點)
                let kind_a = classify_3seg_nodes(&window[0..3]);
                let kind_b = classify_3seg_nodes(&window[4..7]);
                let kind_c = classify_3seg_nodes(&window[8..11]);
                let large_x = x_node_is_large(&window[3], &window[0..3], &window[4..7])
                    || x_node_is_large(&window[7], &window[4..7], &window[8..11]);
                if let Some(k) =
                    classifier::map_triple_combination(&kind_a, &kind_b, &kind_c, large_x)
                {
                    out.push(RowMatch::Direct(AcceptedKind {
                        pattern: NeelyPatternType::Combination {
                            sub_kinds: vec![k],
                        },
                        base: StructureLabel::Three,
                    }));
                }
            }
        }
        _ => {}
    }
    out
}

// ---------------------------------------------------------------------------
// W5:Ch5 Validator 端點泛化(§4.3)
// ---------------------------------------------------------------------------

/// 視窗 → 合成 (ClassifiedMonowave 序列, WaveCandidate),餵既有 validator。
/// 「波」介面泛化為端點結構:monowave 與聚合節點同構(§4.3);
/// ATR / 45° metrics 為 bar 級概念,不為 Level-N 虛構(規則本體不消費)。
fn synth_window(window: &[Rc<CompactionNode>]) -> (Vec<ClassifiedMonowave>, WaveCandidate) {
    let synth: Vec<ClassifiedMonowave> = window
        .iter()
        .map(|n| {
            let candidates: Vec<StructureLabelCandidate> = match &n.label {
                NodeLabel::Fixed(l) => vec![StructureLabelCandidate {
                    label: *l,
                    certainty: Certainty::Primary,
                }],
                NodeLabel::LeafCandidates(v) => v
                    .iter()
                    .map(|l| StructureLabelCandidate {
                        label: *l,
                        certainty: Certainty::Primary,
                    })
                    .collect(),
            };
            ClassifiedMonowave {
                monowave: crate::output::Monowave {
                    start_date: n.start_date,
                    end_date: n.end_date,
                    start_price: n.start_price,
                    end_price: n.end_price,
                    direction: n.net_direction,
                    bar_indices: (n.start_bar, n.end_bar),
                },
                atr_at_start: 0.0,
                metrics: ProportionMetrics {
                    magnitude: n.price_magnitude(),
                    duration_bars: n.end_bar.saturating_sub(n.start_bar) + 1,
                    atr_relative: 0.0,
                    slope_vs_45deg: 0.0,
                },
                structure_label_candidates: candidates,
                polywave_size: n.children.len(),
            }
        })
        .collect();
    let candidate = WaveCandidate {
        id: format!(
            "shadow-w{}-b{}-b{}",
            window.len(),
            window[0].start_bar,
            window[window.len() - 1].end_bar
        ),
        monowave_indices: (0..window.len()).collect(),
        wave_count: window.len(),
        initial_direction: window[0].net_direction,
    };
    (synth, candidate)
}

// ---------------------------------------------------------------------------
// W6:3-3-3-3-3 分岔判別(§4.4,D-5 修復)
// ---------------------------------------------------------------------------

/// 兩點式端點趨勢線在 x 的取值(x1 == x2 退化時回 y1)。
fn line_at(x1: f64, y1: f64, x2: f64, y2: f64, x: f64) -> f64 {
    if (x2 - x1).abs() < 1e-9 {
        y1
    } else {
        y1 + (y2 - y1) * (x - x1) / (x2 - x1)
    }
}

/// 修正波 corr 未完全回測前波 prev:corr 的判定極值(Q3 拍板 — 真實影線;
/// 無 bars 退端點)未越過 prev 起點。
fn not_fully_retraced(prev: &CompactionNode, corr: &CompactionNode) -> bool {
    let (corr_lo, corr_hi) = corr.judge_range();
    match prev.net_direction {
        MonowaveDirection::Up => corr_lo > prev.start_price,
        MonowaveDirection::Down => corr_hi < prev.start_price,
        MonowaveDirection::Neutral => false,
    }
}

fn endpoint_range(n: &CompactionNode) -> (f64, f64) {
    (
        n.start_price.min(n.end_price),
        n.start_price.max(n.end_price),
    )
}

/// §4.4:同序列雙候選,端點幾何判別,**兩者可同時接受**(各產分支,不選 primary)。
///
/// | 形態 | 端點幾何必要條件 | base |
/// |---|---|---|
/// | Contracting Triangle | a-c 線與 b-d 線收斂(視窗首尾兩線垂直間距遞減);e 不破 a-c 線(±5%,三檔容差之 Triangle 檔) | :3 |
/// | Expanding Triangle | 兩線發散;逐波擴大(±10% 一般容差鬆綁) | :3 |
/// | Terminal Impulse | W2/W4 不完全回測前波 + W3 非最短(R7)+ W1/W4 價格範圍重疊(Overlap_Terminal 反向作必要條件) | :5 |
///
/// Terminal Impulse 以 `Diagonal{Ending}` 表徵(classifier / ch11_terminal_impulse
/// 「Terminal ↔ Diagonal」既有慣例;I5:Diagonal → :5)。
/// 兩組皆不滿足 → 本 row 不產 kind(視窗拒絕)。
/// 趨勢線先用端點內建幾何;trendline_core 耦合留 P1(§4.4 / Q3)。
fn w6_fork_threes_five(window: &[Rc<CompactionNode>]) -> Vec<AcceptedKind> {
    debug_assert_eq!(window.len(), 5);
    let (a, b, c, d, e) = (&window[0], &window[1], &window[2], &window[3], &window[4]);
    let (ma, mc, me) = (a.price_magnitude(), c.price_magnitude(), e.price_magnitude());
    let mut out = Vec::new();

    let ac = |x: f64| line_at(a.end_bar as f64, a.end_price, c.end_bar as f64, c.end_price, x);
    let bd = |x: f64| line_at(b.end_bar as f64, b.end_price, d.end_bar as f64, d.end_price, x);
    let x_early = b.end_bar as f64;
    let x_late = e.end_bar as f64;
    let gap_early = (ac(x_early) - bd(x_early)).abs();
    let gap_late = (ac(x_late) - bd(x_late)).abs();

    // Contracting:收斂 + e 不破 a-c 線(觸線以判定極值檢查,Q3 拍板)
    let e_breach = {
        let line_val = ac(e.end_bar as f64);
        let tol = line_val.abs() * 0.05;
        let bd_side = bd(x_early) - ac(x_early); // b/d 所在側
        let (e_lo, e_hi) = e.judge_range();
        if bd_side < 0.0 {
            // b/d 在線下 → 破線 = e 極值越到線上超容差
            e_hi - line_val > tol
        } else if bd_side > 0.0 {
            line_val - e_lo > tol
        } else {
            false
        }
    };
    if gap_late < gap_early && !e_breach {
        out.push(AcceptedKind {
            pattern: NeelyPatternType::Triangle {
                sub_kind: TriangleKind::Contracting,
            },
            base: StructureLabel::Three,
        });
    }

    // Expanding:發散 + 逐波擴大
    if gap_late > gap_early && mc >= ma * 0.9 && me >= mc * 0.9 {
        out.push(AcceptedKind {
            pattern: NeelyPatternType::Triangle {
                sub_kind: TriangleKind::Expanding,
            },
            base: StructureLabel::Three,
        });
    }

    // Terminal Impulse(Overlap 以判定範圍檢查,Q3 拍板)
    let w3_not_shortest = !(mc < ma && mc < me);
    let (a_lo, a_hi) = a.judge_range();
    let (d_lo, d_hi) = d.judge_range();
    let w1_w4_overlap = a_lo <= d_hi && d_lo <= a_hi;
    if not_fully_retraced(a, b) && not_fully_retraced(c, d) && w3_not_shortest && w1_w4_overlap {
        out.push(AcceptedKind {
            pattern: NeelyPatternType::Diagonal {
                sub_kind: DiagonalKind::Ending,
            },
            base: StructureLabel::Five,
        });
    }

    out
}

// ---------------------------------------------------------------------------
// §6.1 邊界波重評(D-4 修復;G2.3)— 真鄰居 magnitude 三檔判定,不拒絕聚合
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoundaryTier {
    /// ∈ [0.382, 2.618] — 通過,無事
    Pass,
    /// ∈ [0.236, 0.382) ∪ (2.618, 4.236] — Advisory(Info):現行 mild 檔語意搬移
    Info,
    /// [0.236, 4.236] 外 — Advisory(Warning):該解讀在更大序列中的角色可疑;
    /// 形態內部合法性已由 W5/W7 保證,**不拒絕聚合**
    Warning,
}

/// 單側 magnitude 比三檔判定;任一側零幅度(不可比)→ None(caller 記 skipped)。
fn boundary_tier(a_mag: f64, b_mag: f64) -> Option<BoundaryTier> {
    if a_mag <= PRICE_EPS || b_mag <= PRICE_EPS {
        return None;
    }
    let r = a_mag / b_mag;
    if (SB_MIN_RATIO..=SB_MAX_RATIO).contains(&r) {
        Some(BoundaryTier::Pass)
    } else if (FIB2_MIN..=FIB2_MAX).contains(&r) {
        Some(BoundaryTier::Info)
    } else {
        Some(BoundaryTier::Warning)
    }
}

/// §6.1:聚合成功後,parent 於其 tiling 取真實前後鄰居 m(−1) / m(+1),
/// 對 (|m(−1)|, parent 首子波) 與 (parent 末子波, |m(+1)|) 各判一次。
/// shadow 期以計數觀測;凍結時之 AdvisoryFinding 掛載屬 tiling 語境
/// (同節點跨 tiling 鄰居不同),留 G2.4 收集階段處理。
fn boundary_reassess(
    prev: Option<&Rc<CompactionNode>>,
    parent: &Rc<CompactionNode>,
    next: Option<&Rc<CompactionNode>>,
    diag: &mut ShadowCompactionDiagnostics,
) {
    let sides = [
        (
            prev.map(|p| p.price_magnitude()),
            parent.children.first().map(|c| c.price_magnitude()),
        ),
        (
            parent.children.last().map(|c| c.price_magnitude()),
            next.map(|n| n.price_magnitude()),
        ),
    ];
    for (a, b) in sides {
        match (a, b) {
            (Some(a), Some(b)) => match boundary_tier(a, b) {
                Some(tier) => {
                    diag.boundary_pairs_checked += 1;
                    match tier {
                        BoundaryTier::Pass => {}
                        BoundaryTier::Info => diag.boundary_advisory_info += 1,
                        BoundaryTier::Warning => diag.boundary_advisory_warning += 1,
                    }
                }
                None => diag.boundary_sides_skipped += 1,
            },
            // tiling 首/末 parent 無對應鄰居 → 該側跳過(記 diagnostics,§6.1)
            _ => diag.boundary_sides_skipped += 1,
        }
    }
}

// ---------------------------------------------------------------------------
// §6.2 ComplexityLevel 真算 + Triplexity(G2.3;取代硬寫 Complex)
// ---------------------------------------------------------------------------

/// rules Ch7 Complexity Rule(Neely Extension)遞迴定義(§6.2 表):
/// 葉 = Level-0(Simple);degree-1 合法形態 = Level-1(Polywave);
/// 至少一個 `:5` 子節點自身為 pattern node(impulsive polywave)= Level-2
/// (Multiwave);一 `:5` 子節點為 Multiwave 且另一 `:5` 至少 Polywave =
/// Level-3(Macrowave,上限)。
fn node_complexity(n: &CompactionNode) -> usize {
    if matches!(n.kind, NodeKind::Leaf) {
        return 0;
    }
    let five_levels: Vec<usize> = n
        .children
        .iter()
        .filter(|c| {
            matches!(c.kind, NodeKind::Pattern(_))
                && matches!(&c.label, NodeLabel::Fixed(l) if is_impulsive_label(*l))
        })
        .map(|c| node_complexity(c))
        .collect();
    if five_levels.is_empty() {
        1
    } else if five_levels.len() >= 2 && five_levels.iter().copied().max().unwrap_or(0) >= 2 {
        3
    } else {
        2
    }
}

/// 形態內 `:5` slot 的 child index(Level-0 Impulse 段辨識用;
/// Triangle / Combination / Terminal(Diagonal)children 全 `:3`,無 slot)。
fn five_slot_indices(pt: &NeelyPatternType) -> &'static [usize] {
    match pt {
        NeelyPatternType::Impulse => &[0, 2, 4],
        NeelyPatternType::Zigzag { .. } => &[0, 2],
        NeelyPatternType::Flat { .. } | NeelyPatternType::RunningCorrection => &[2],
        _ => &[],
    }
}

/// §6.2 Triplexity:收集子樹內所有 Impulse 段的 Complexity Level —
/// impulsive pattern node 記自身 level;`:5` slot 上的葉記 Level-0。
fn collect_impulse_levels(n: &CompactionNode, out: &mut HashSet<usize>) {
    if let NodeKind::Pattern(pt) = &n.kind {
        if matches!(
            pt,
            NeelyPatternType::Impulse | NeelyPatternType::Diagonal { .. }
        ) {
            out.insert(node_complexity(n));
        }
        for idx in five_slot_indices(pt) {
            if let Some(c) = n.children.get(*idx) {
                if matches!(c.kind, NodeKind::Leaf) {
                    out.insert(0);
                }
            }
        }
        for c in &n.children {
            collect_impulse_levels(c, out);
        }
    }
}

/// 同一結構內出現 ≥ 3 種不同 Complexity Level 的 Impulse 段 → triplexity。
fn triplexity_detected(n: &CompactionNode) -> bool {
    let mut levels = HashSet::new();
    collect_impulse_levels(n, &mut levels);
    levels.len() >= 3
}

// ---------------------------------------------------------------------------
// §6.3 degree_level → Degree 對映(G2.3;ceiling 錨定,輸出展示用)
// ---------------------------------------------------------------------------

/// 11 級 Degree 體系升冪(architecture §13.3;index 0 = SubMicro 下界)。
const DEGREE_LADDER: [Degree; 11] = [
    Degree::SubMicro,
    Degree::Micro,
    Degree::SubMinuette,
    Degree::Minuette,
    Degree::Minute,
    Degree::Minor,
    Degree::Intermediate,
    Degree::Primary,
    Degree::Cycle,
    Degree::Supercycle,
    Degree::GrandSupercycle,
];

fn degree_ladder_index(d: &Degree) -> usize {
    match d {
        Degree::SubMicro => 0,
        Degree::Micro => 1,
        Degree::SubMinuette => 2,
        Degree::Minuette => 3,
        Degree::Minute => 4,
        Degree::Minor => 5,
        Degree::Intermediate => 6,
        Degree::Primary => 7,
        Degree::Cycle => 8,
        Degree::Supercycle => 9,
        Degree::GrandSupercycle => 10,
    }
}

/// §6.3 ceiling 錨定法:tiling 最高 degree_level 對映 ceiling 允許之最高
/// Degree,逐層向下遞減;超出 11 級下界夾至 SubMicro(計數記 diagnostics)。
/// 僅供輸出展示與 cross_timeframe_hints,**不**回饋任何接受條件。
fn degree_name_map(max_level: usize, ceiling: &Degree) -> (HashMap<String, String>, usize) {
    let ceil_idx = degree_ladder_index(ceiling);
    let mut map = HashMap::new();
    let mut clamped = 0usize;
    for level in 0..=max_level {
        let offset = max_level - level;
        let idx = match ceil_idx.checked_sub(offset) {
            Some(i) => i,
            None => {
                clamped += 1;
                0
            }
        };
        map.insert(level.to_string(), format!("{:?}", DEGREE_LADDER[idx]));
    }
    (map, clamped)
}

// ---------------------------------------------------------------------------
// Q3 儀表(§12;2026-08-26 六檔實測 flip 33.3% > 5% → bars 反查定案)
// ---------------------------------------------------------------------------

/// Q3 儀表(拍板後保留為**殘差觀測**):判準已為 bars 反查(w6_fork /
/// not_fully_retraced 消費 `judge_range`),此處量測端點版 vs bars 版的
/// Overlap / 回測判定殘餘分歧,供 Gate 報告與 spec r4 佐證。
/// 回 Some(是否分歧);節點無 bars 反查 → None(不計)。
fn q3_compare(window: &[Rc<CompactionNode>]) -> Option<bool> {
    debug_assert_eq!(window.len(), 5);
    let (a, b, c, d, _e) = (&window[0], &window[1], &window[2], &window[3], &window[4]);

    // 端點版
    let (a_lo, a_hi) = endpoint_range(a);
    let (d_lo, d_hi) = endpoint_range(d);
    let ep_overlap = a_lo <= d_hi && d_lo <= a_hi;
    let ep_b_full = match a.net_direction {
        MonowaveDirection::Up => b.end_price <= a.start_price,
        MonowaveDirection::Down => b.end_price >= a.start_price,
        MonowaveDirection::Neutral => false,
    };
    let ep_d_full = match c.net_direction {
        MonowaveDirection::Up => d.end_price <= c.start_price,
        MonowaveDirection::Down => d.end_price >= c.start_price,
        MonowaveDirection::Neutral => false,
    };

    // bars 反查版(判準)
    let (abl, abh) = a.true_range?;
    let (dbl, dbh) = d.true_range?;
    let (bbl, bbh) = b.true_range?;
    let bars_overlap = abl <= dbh && dbl <= abh;
    let bars_b_full = match a.net_direction {
        MonowaveDirection::Up => bbl <= a.start_price,
        MonowaveDirection::Down => bbh >= a.start_price,
        MonowaveDirection::Neutral => false,
    };
    let bars_d_full = match c.net_direction {
        MonowaveDirection::Up => dbl <= c.start_price,
        MonowaveDirection::Down => dbh >= c.start_price,
        MonowaveDirection::Neutral => false,
    };

    Some(ep_overlap != bars_overlap || ep_b_full != bars_b_full || ep_d_full != bars_d_full)
}

// ---------------------------------------------------------------------------
// 階梯組裝
// ---------------------------------------------------------------------------

/// 唯一視窗計數(memo 命中不重計;§4.1 失敗記錄的 shadow 版 — 完整 RuleRejection
/// 留 G2.4 切換時進 NeelyDiagnostics.rejections,shadow 期以計數觀測)。
#[derive(Debug, Default)]
struct LadderCounters {
    w1_violations: usize,
    w5_rejected_windows: usize,
    q3_windows: usize,
    q3_flips: usize,
}

struct LadderOutcome {
    kinds: Vec<AcceptedKind>,
    /// W5 報告(passed 已含 default_passed_rules 同源推導);視窗內各 kind 共享
    report: Option<Rc<ValidationReport>>,
}

impl LadderOutcome {
    fn empty() -> Self {
        LadderOutcome {
            kinds: Vec::new(),
            report: None,
        }
    }
}

/// try_all_neely(§4)G2.2 全階梯:W1 → (W3/W4/W7 便宜前濾)→ W2 rows →
/// W6 row 展開(含 3-3-3-3-3 分岔)→ W5(Ch5 端點重驗)按 I5 族別閘門。
///
/// **W5 閘門按族別適用**(§4.3 適用表的實作詮釋):Ch5 Essential R1–R7 是
/// 衝動建構規則 — 對 `:5` 族 kind(Impulse / Terminal)以 `overall_pass` 硬閘;
/// `:3` 族(Zigzag/Flat/Triangle/Combination)essentials 天生不成立(Triangle
/// 的 W3 短於 W2 即 R4 fail),一律硬閘會使 W6 分岔成死碼、D-5 不可修 —
/// 對齊 validator 自身「變體規則 Fail 是資訊性,交 Classifier」語意,
/// `:3` 族不受衝動 essentials 閘,ValidationReport 仍附掛節點供下游參考。
fn try_ladder(window: &[Rc<CompactionNode>], counters: &mut LadderCounters) -> LadderOutcome {
    if !w1_adjacency(window) {
        counters.w1_violations += 1;
        return LadderOutcome::empty();
    }
    if !w3_alternating(window) || !w4_similarity_balance(window) || !w7_internal_extreme(window) {
        return LadderOutcome::empty();
    }
    let rows = w2_label_rows(window);
    if rows.is_empty() {
        return LadderOutcome::empty();
    }

    // W6:row 展開(3-3-3-3-3 → 分岔判別)
    let mut proposed: Vec<AcceptedKind> = Vec::new();
    for row in rows {
        match row {
            RowMatch::Direct(k) => proposed.push(k),
            RowMatch::ThreesFive => proposed.extend(w6_fork_threes_five(window)),
        }
    }
    if proposed.is_empty() {
        return LadderOutcome::empty();
    }

    // W5:Ch5 Validator 端點重驗(每視窗一次;passed 與 Level-0 同源推導)
    let (synth, candidate) = synth_window(window);
    let mut report = validator::validate_candidate(&candidate, &synth);
    let derived: Vec<RuleId> = report
        .passed
        .iter()
        .cloned()
        .chain(classifier::default_passed_rules(&candidate, &report))
        .collect();
    report.passed = derived;

    // Q3 殘差觀測(5-窗且節點有 bars 反查;量測不受閘門結果影響)
    if window.len() == 5 {
        if let Some(flipped) = q3_compare(window) {
            counters.q3_windows += 1;
            if flipped {
                counters.q3_flips += 1;
            }
        }
    }

    // 族別閘門::5 族要求 overall_pass
    let kinds: Vec<AcceptedKind> = proposed
        .into_iter()
        .filter(|k| k.base != StructureLabel::Five || report.overall_pass)
        .collect();
    if kinds.is_empty() {
        // 視窗曾有候選但全數被 W5 擋下 → 唯一視窗拒絕計數(§4.1 shadow 版)
        counters.w5_rejected_windows += 1;
        return LadderOutcome::empty();
    }
    LadderOutcome {
        kinds,
        report: Some(Rc::new(report)),
    }
}

/// 視窗淨向(與 make_parent 同式;splice 候選評分共用)。
fn window_net_direction(window: &[Rc<CompactionNode>]) -> MonowaveDirection {
    let first = window.first().expect("non-empty window");
    let last = window.last().expect("non-empty window");
    let delta = last.end_price - first.start_price;
    if delta > 0.0 {
        MonowaveDirection::Up
    } else if delta < 0.0 {
        MonowaveDirection::Down
    } else {
        first.net_direction
    }
}

/// `canonical` 由 caller 預算(`pattern_canonical` 只需 window slice)—
/// 讓 round 迴圈能以 spliced key 先查 seen,命中即免 materialize。
/// `validation` = W5 報告(G2.2 起真值;beam 鍵 2 讀 passed.len())。
fn make_parent(
    window: &[Rc<CompactionNode>],
    kind: &AcceptedKind,
    canonical: String,
    validation: Option<ValidationReport>,
) -> Rc<CompactionNode> {
    let first = window.first().expect("non-empty window");
    let last = window.last().expect("non-empty window");
    let degree = window.iter().map(|n| n.degree_level).max().unwrap_or(0) + 1;
    let net = window_net_direction(window);
    // parent 真實範圍 = children 聯集(任一 child 無 bars 反查 → None,判定退端點)
    let true_range = window
        .iter()
        .try_fold((f64::MAX, f64::MIN), |(lo, hi), n| {
            n.true_range.map(|(l, h)| (lo.min(l), hi.max(h)))
        });
    let children: Vec<Rc<CompactionNode>> = window.to_vec();
    Rc::new(CompactionNode {
        kind: NodeKind::Pattern(kind.pattern.clone()),
        label: NodeLabel::Fixed(kind.base),
        degree_level: degree,
        canonical,
        true_range,
        start_bar: first.start_bar,
        end_bar: last.end_bar,
        start_date: first.start_date,
        end_date: last.end_date,
        start_price: first.start_price,
        end_price: last.end_price,
        children,
        validation,
        net_direction: net,
    })
}

// ---------------------------------------------------------------------------
// Tiling / beam(§5.5)
// ---------------------------------------------------------------------------

struct Tiling {
    nodes: Vec<Rc<CompactionNode>>,
}

fn tiling_key(nodes: &[Rc<CompactionNode>]) -> String {
    let keys: Vec<&str> = nodes.iter().map(|n| n.canonical.as_str()).collect();
    keys.join(";")
}

/// 「視窗 [s, s+w) 換成 parent」後的 tiling key — 不 materialize 節點 Vec 就能
/// 查 seen(#4 護欄配套:重複分支只花一次字串組裝,不花 Rc clone)。
fn spliced_tiling_key(
    nodes: &[Rc<CompactionNode>],
    s: usize,
    w: usize,
    parent_key: &str,
) -> String {
    let mut parts: Vec<&str> = Vec::with_capacity(nodes.len() - w + 1);
    parts.extend(nodes[..s].iter().map(|n| n.canonical.as_str()));
    parts.push(parent_key);
    parts.extend(nodes[s + w..].iter().map(|n| n.canonical.as_str()));
    parts.join(";")
}

/// beam 排序鍵(§5.5,降序):
/// 1. tiling 內節點 PowerRating 最強級別(聚合節點查表;葉 = 0)
/// 2. Σ rules_passed_count(G2.1 W5 stub 下恆 0,G2.2 起為真值)
/// 3. Σ degree_level(偏好深樹)
fn tiling_beam_score(t: &Tiling) -> (i32, usize, usize) {
    let mut max_power = 0i32;
    let mut sum_rules = 0usize;
    let mut sum_degree = 0usize;
    for n in &t.nodes {
        if let NodeKind::Pattern(pt) = &n.kind {
            let rating =
                power_rating::table::lookup_power_rating(pt, n.net_direction, false);
            let mag = super::power_rating_magnitude(rating).abs();
            max_power = max_power.max(mag);
        }
        if let Some(v) = &n.validation {
            sum_rules += v.passed.len();
        }
        sum_degree += n.degree_level;
    }
    (max_power, sum_rules, sum_degree)
}

// ---------------------------------------------------------------------------
// 不變量檢查 I1–I6(§2.2;violation = 引擎 bug,Gate 紅燈)
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct InvariantCounters {
    pub i1: usize,
    pub i2: usize,
    pub i3: usize,
    pub i4: usize,
    pub i5: usize,
    pub i6: usize,
}

impl InvariantCounters {
    pub fn total(&self) -> usize {
        self.i1 + self.i2 + self.i3 + self.i4 + self.i5 + self.i6
    }

    fn to_map(&self) -> HashMap<String, usize> {
        [
            ("I1", self.i1),
            ("I2", self.i2),
            ("I3", self.i3),
            ("I4", self.i4),
            ("I5", self.i5),
            ("I6", self.i6),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
    }
}

/// I5:pattern kind → 唯一 base label(rules Ch7 表;G2.1 產出的 kinds 皆在表內)。
fn expected_base_label(pt: &NeelyPatternType) -> StructureLabel {
    match pt {
        // Trending Impulse / Diagonal(:5 族;Terminal Impulse 於 G2.2 併入)
        NeelyPatternType::Impulse | NeelyPatternType::Diagonal { .. } => StructureLabel::Five,
        _ => StructureLabel::Three,
    }
}

fn check_node_recursive<'a>(
    node: &'a CompactionNode,
    c: &mut InvariantCounters,
    visited: &mut HashSet<&'a str>,
) {
    // Rc 子樹可被多個 tiling 共享 — canonical 相同即同一子樹,只檢一次,
    // 避免違反計數隨 beam 排序 / 共享度膨脹(Gate 報告需可重現的統計)
    if !visited.insert(node.canonical.as_str()) {
        return;
    }
    // I1:節點自身時間正序(巢狀節點同樣適用)
    if node.start_date > node.end_date {
        c.i1 += 1;
    }
    match &node.kind {
        NodeKind::Leaf => {
            if node.degree_level != 0 || !node.children.is_empty() {
                c.i4 += 1;
            }
        }
        NodeKind::Pattern(pt) => {
            // I4:層級單調
            let max_child = node.children.iter().map(|ch| ch.degree_level).max();
            if max_child.map(|m| m + 1) != Some(node.degree_level) {
                c.i4 += 1;
            }
            // I5:label 閉合
            if let NodeLabel::Fixed(l) = &node.label {
                if *l != expected_base_label(pt) {
                    c.i5 += 1;
                }
            } else {
                c.i5 += 1; // 聚合節點必為 Fixed
            }
            // I3:children 構成自身範圍的 tiling(遞迴 I1/I2)
            if let (Some(first), Some(last)) = (node.children.first(), node.children.last()) {
                if first.start_date != node.start_date
                    || last.end_date != node.end_date
                    || first.start_price != node.start_price
                    || last.end_price != node.end_price
                    || first.start_bar != node.start_bar
                    || last.end_bar != node.end_bar
                {
                    c.i3 += 1;
                }
                for i in 1..node.children.len() {
                    let (a, b) = (&node.children[i - 1], &node.children[i]);
                    if a.end_date != b.start_date || a.end_price != b.start_price {
                        c.i3 += 1;
                    }
                }
            } else {
                c.i3 += 1; // pattern 無 children
            }
            for ch in &node.children {
                check_node_recursive(ch, c, visited);
            }
        }
    }
}

/// 對 tiling 集合檢查 I1–I6(I6 在凍結收集時另行驗證去重)。
///
/// `base_range` = base tiling 的 [t₀, t_N]:I1「聯集覆蓋」條款 — 每個 tiling
/// 首節點 start / 末節點 end 必須等於 base 範圍(視窗替換不增減覆蓋;首尾節點
/// 被吞掉的 splice bug 唯有此檢查抓得到)。None = 無 base 範圍可對(單元測試用)。
pub fn check_invariants(
    tilings: &[&[Rc<CompactionNode>]],
    base_range: Option<(NaiveDate, NaiveDate)>,
) -> InvariantCounters {
    let mut c = InvariantCounters::default();
    let mut visited: HashSet<&str> = HashSet::new();
    for nodes in tilings {
        // I1:聯集覆蓋 [t₀, t_N]
        if let (Some((t0, tn)), Some(first), Some(last)) =
            (base_range, nodes.first(), nodes.last())
        {
            if first.start_date != t0 || last.end_date != tn {
                c.i1 += 1;
            }
        }
        for i in 0..nodes.len() {
            let n = &nodes[i];
            // I2:共享端點(日期精確相等 + 價格相等,不設容差)
            if i > 0 {
                let prev = &nodes[i - 1];
                if prev.end_date != n.start_date || prev.end_price != n.start_price {
                    c.i2 += 1;
                }
            }
            check_node_recursive(n, &mut c, &mut visited);
        }
    }
    c
}

// ---------------------------------------------------------------------------
// Shadow 主入口(§5.2 主迴圈 + §9.3 比對計數)
// ---------------------------------------------------------------------------

/// round 內 splice 候選(輕量 spec;兩階段生成的第一階段產物,不 materialize)。
struct SpliceSpec {
    tiling_idx: usize,
    s: usize,
    w: usize,
    kind: AcceptedKind,
    report: Option<Rc<ValidationReport>>,
    /// beam 鍵近似分數(parent |power| / W5 passed 數 / parent degree)—
    /// 供 materialize 排序,消除先枚舉視窗的時間軸偏置(G2.1 gate 實測修正)
    score: (i32, usize, usize),
}

fn splice_score(
    kind: &AcceptedKind,
    report: &Option<Rc<ValidationReport>>,
    window: &[Rc<CompactionNode>],
) -> (i32, usize, usize) {
    let net = window_net_direction(window);
    let rating = power_rating::table::lookup_power_rating(&kind.pattern, net, false);
    let power = super::power_rating_magnitude(rating).abs();
    let passed = report.as_ref().map(|r| r.passed.len()).unwrap_or(0);
    let degree = window.iter().map(|n| n.degree_level).max().unwrap_or(0) + 1;
    (power, passed, degree)
}

/// 跑 tiling-round 引擎(shadow):產 diagnostics,不動 serving forest。
/// `old_forest` 供 §9.3 召回計數;`bars` 供 Q3 bars 反查判準(空 = 退端點);
/// `pattern_bounds` 供 A-10 anchors union 觀測;`timeframe` 供 §6.3 Degree
/// ceiling 錨定(與 Stage 11 同式推導)。
pub fn run_shadow(
    classified: &[ClassifiedMonowave],
    old_forest: &[Scenario],
    bars: &[OhlcvBar],
    pattern_bounds: &[PatternBound],
    timeframe: Timeframe,
    cfg: &NeelyEngineConfig,
) -> ShadowCompactionDiagnostics {
    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(cfg.compaction_timeout_secs);

    let base = build_base_tiling(classified, bars);
    let mut diag = ShadowCompactionDiagnostics {
        engine: "tiling-round-g2.4".to_string(),
        base_tiling_len: base.nodes.len(),
        neutral_bridged: base.bridged,
        leading_neutral_dropped: base.leading_dropped,
        rounds_run: 0,
        tiling_count: 0,
        level_cap_hit: false,
        timed_out: false,
        w1_violations: 0,
        round_branch_cap_hits: 0,
        w5_rejected_windows: 0,
        q3_windows: 0,
        q3_flips: 0,
        node_count_by_level: HashMap::new(),
        node_count_by_pattern: HashMap::new(),
        invariant_violations: InvariantCounters::default().to_map(),
        old_forest_scenarios: old_forest.len(),
        old_forest_matched: 0,
        boundary_pairs_checked: 0,
        boundary_advisory_info: 0,
        boundary_advisory_warning: 0,
        boundary_sides_skipped: 0,
        complexity_count_by_level: HashMap::new(),
        triplexity_nodes: 0,
        degree_map: HashMap::new(),
        degree_clamped_levels: 0,
        degree1_node_keys: Vec::new(),
        anchors_union_total: 0,
        anchors_overlap_total: 0,
        elapsed_us: 0,
    };

    if base.nodes.len() < 3 {
        diag.tiling_count = usize::from(!base.nodes.is_empty());
        diag.elapsed_us = start.elapsed().as_micros() as u64;
        return diag;
    }

    // I1 覆蓋條款依據:base tiling 的 [t₀, t_N](beam 可能淘汰 base tiling,先存)
    let base_range = base
        .nodes
        .first()
        .zip(base.nodes.last())
        .map(|(f, l)| (f.start_date, l.end_date));
    let base_key = tiling_key(&base.nodes);
    let mut pool: Vec<Tiling> = vec![Tiling { nodes: base.nodes }];
    let mut seen: HashSet<String> = HashSet::from([base_key]);
    // §5.6 memo:視窗 children canonical 串 → 階梯結果(分支間大量共享視窗;
    // W5/Q3 計數在 memo miss 時累計 = 唯一視窗語意)
    let mut memo: HashMap<String, Rc<LadderOutcome>> = HashMap::new();
    let mut counters = LadderCounters::default();
    // 凍結收集(I6「全 tilings」,G2.4 修正):materialize 時累積,
    // canonical 去重、Rc 共享;beam / branch cap 不限縮收集
    let mut forest_nodes: HashMap<String, Rc<CompactionNode>> = HashMap::new();

    // 工程護欄:round 內 materialize 的新分支上限(beam 只保 round_beam_size,
    // 超額 materialize 無意義;無上限實測 600-monowave 檔 3.1GB)。
    // G2.2 起兩階段生成:先枚舉全視窗收輕量 splice 候選,再依 beam 鍵近似分數
    // 降序 materialize 至上限 — 截斷不再偏向時間軸前段(G2.1 gate 實測修正)。
    let branch_cap = cfg.round_beam_size.saturating_mul(8).max(8);

    'rounds: for round in 1..=cfg.max_compaction_levels {
        // ── 階段 1:枚舉 + 階梯(輕量 spec,不 materialize)
        let mut specs: Vec<SpliceSpec> = Vec::new();
        for (tiling_idx, t) in pool.iter().enumerate() {
            for w in [3usize, 5, 7, 11] {
                if t.nodes.len() < w {
                    continue;
                }
                for s in 0..=t.nodes.len() - w {
                    // 逾時硬保險走視窗粒度:單一 tiling 可含數百視窗
                    if start.elapsed() > timeout {
                        diag.timed_out = true;
                        break 'rounds;
                    }
                    let window = &t.nodes[s..s + w];
                    let memo_key = tiling_key(window);
                    let outcome = match memo.get(&memo_key) {
                        Some(o) => Rc::clone(o),
                        None => {
                            let o = Rc::new(try_ladder(window, &mut counters));
                            memo.insert(memo_key, Rc::clone(&o));
                            o
                        }
                    };
                    for kind in outcome.kinds.iter() {
                        let score = splice_score(kind, &outcome.report, window);
                        specs.push(SpliceSpec {
                            tiling_idx,
                            s,
                            w,
                            kind: kind.clone(),
                            report: outcome.report.clone(),
                            score,
                        });
                    }
                }
            }
        }

        if specs.is_empty() {
            // Round 3 暫停:本輪零聚合(§5.2.d)
            break;
        }

        // ── 階段 2:依分數降序 materialize(seen dedup;至多 branch_cap)
        //
        // **G2.4 收集語意修正(Gate v3 第一輪揭露)**:§7.1 I6 收集的是
        // 「全 tilings」— 每個階梯接受的視窗解讀在概念上都屬某條 tiling,
        // branch cap 與 beam 只是限制**深化探索**的工程護欄,不得限縮收集。
        // 原實作僅從最終 beam pool 收集 → 全市場召回率 10.7%(分子被 beam
        // 截斷)。改為 materialize 時累積收集(canonical 去重、Rc 共享),
        // cap 之外的接受解讀照收、僅不展開 tiling 分支;凍結側護欄
        // (forest_max_size 200 / BeamSearchFallback)依 §7.1 步驟 4 於
        // 切換時把關,shadow 期全量觀測。
        specs.sort_by(|a, b| b.score.cmp(&a.score));
        let mut new_tilings: Vec<Tiling> = Vec::new();
        let mut branch_capped = false;
        for spec in &specs {
            let t = &pool[spec.tiling_idx];
            let window = &t.nodes[spec.s..spec.s + spec.w];
            let parent_key = pattern_canonical(
                &spec.kind.pattern,
                spec.kind.base,
                window[0].start_bar,
                window[spec.w - 1].end_bar,
                window,
            );
            // 收集(I6;同 canonical 跨 tiling 共享同一 Rc 節點)
            let parent = match forest_nodes.get(&parent_key) {
                Some(p) => Rc::clone(p),
                None => {
                    let p = make_parent(
                        window,
                        &spec.kind,
                        parent_key.clone(),
                        spec.report.as_ref().map(|r| (**r).clone()),
                    );
                    collect_pattern_nodes(&p, &mut forest_nodes);
                    p
                }
            };
            if new_tilings.len() >= branch_cap {
                branch_capped = true;
                continue;
            }
            let key = spliced_tiling_key(&t.nodes, spec.s, spec.w, &parent_key);
            if !seen.insert(key) {
                continue;
            }
            // §6.1 邊界波重評(G2.3):parent 對其 tiling 中真實前後鄰居判定
            // (advisory 三檔,不拒絕;同節點跨 tiling 鄰居不同 → 逐 splice 計)
            let prev = spec.s.checked_sub(1).map(|i| &t.nodes[i]);
            let next = t.nodes.get(spec.s + spec.w);
            boundary_reassess(prev, &parent, next, &mut diag);
            // 新 tiling:視窗換 parent,其餘保留(Rc 指標串,不深拷貝)
            let mut nodes: Vec<Rc<CompactionNode>> = Vec::with_capacity(t.nodes.len() - spec.w + 1);
            nodes.extend(t.nodes[..spec.s].iter().cloned());
            nodes.push(parent);
            nodes.extend(t.nodes[spec.s + spec.w..].iter().cloned());
            new_tilings.push(Tiling { nodes });
        }

        if new_tilings.is_empty() {
            // 全數 dedup:本輪無新解讀 → 收斂
            break;
        }
        diag.rounds_run = round;
        if branch_capped {
            diag.round_branch_cap_hits += 1;
        }
        pool.extend(new_tilings);

        // beam(§5.5):round 內 top-N 保留;原 tiling 同場競爭
        pool.sort_by(|a, b| tiling_beam_score(b).cmp(&tiling_beam_score(a)));
        pool.truncate(cfg.round_beam_size);

        if round == cfg.max_compaction_levels {
            // A-8:觸輪數上限跳出(非零聚合收斂)→ 缺漏可觀察
            diag.level_cap_hit = true;
        }
    }

    diag.tiling_count = pool.len();
    diag.w1_violations = counters.w1_violations;
    diag.w5_rejected_windows = counters.w5_rejected_windows;
    diag.q3_windows = counters.q3_windows;
    diag.q3_flips = counters.q3_flips;

    // 不變量檢查(I1–I5;I6 = 收集去重,materialize 時以 canonical HashMap 保證)
    let tiling_slices: Vec<&[Rc<CompactionNode>]> =
        pool.iter().map(|t| t.nodes.as_slice()).collect();
    let counters = check_invariants(&tiling_slices, base_range);
    diag.invariant_violations = counters.to_map();

    // 凍結收集(I6)已於 materialize 累積(全 tilings 語意,G2.4 修正);
    // 此處僅統計 + 產 §9.3 diff 用鍵
    for n in forest_nodes.values() {
        *diag
            .node_count_by_level
            .entry(n.degree_level.to_string())
            .or_insert(0) += 1;
        // G2.4 Gate v3:pattern 分布(Terminal 存在性門檻 §9.2 的資料源)
        if let NodeKind::Pattern(pt) = &n.kind {
            *diag
                .node_count_by_pattern
                .entry(pattern_tag(pt))
                .or_insert(0) += 1;
        }
        // §6.2 Complexity 真算 + Triplexity(G2.3;canonical 去重後逐節點)
        *diag
            .complexity_count_by_level
            .entry(node_complexity(n).to_string())
            .or_insert(0) += 1;
        if triplexity_detected(n) {
            diag.triplexity_nodes += 1;
        }
    }

    // §6.3 Degree 對映(G2.3):最高 degree_level 錨定 Stage 11 同式 ceiling
    let max_level = forest_nodes
        .values()
        .map(|n| n.degree_level)
        .max()
        .unwrap_or(0);
    let ceiling = crate::degree::compute_ceiling(bars, timeframe);
    let (degree_map, clamped) = degree_name_map(max_level, &ceiling.max_reachable_degree);
    diag.degree_map = degree_map;
    diag.degree_clamped_levels = clamped;

    // A-10(G2.3):anchors 語意收緊觀測 — union(PatternBound 完整含於節點
    // 覆蓋 monowave 範圍)vs 現行「日期範圍重疊即算」近似,收緊幅度供 Gate。
    if !pattern_bounds.is_empty() {
        let mw_bar_ranges: Vec<(usize, usize)> =
            classified.iter().map(|c| c.monowave.bar_indices).collect();
        for n in forest_nodes.values() {
            for pb in pattern_bounds {
                let (Some(pb_s), Some(pb_e)) =
                    (mw_bar_ranges.get(pb.start_idx), mw_bar_ranges.get(pb.end_idx))
                else {
                    continue;
                };
                if pb_s.0 >= n.start_bar && pb_e.1 <= n.end_bar {
                    diag.anchors_union_total += 1;
                }
                if pb_s.0 <= n.end_bar && n.start_bar <= pb_e.1 {
                    diag.anchors_overlap_total += 1;
                }
            }
        }
    }

    // §9.3 shadow 召回計數:舊 forest scenario 依 (start_bar, end_bar, pattern_tag)
    // 對新引擎 **degree_level = 1** 節點匹配(spec §9.3 投影定義,精確 = 1 非 ≥ 1)。
    // 舊 forest 內 Level ≥ 2 聚合結構上不可能被 degree-1 投影命中,屬 Gate 缺口
    // diff 報告的預期類別。門檻在 Gate v3 全階梯後才適用,此處僅計數。
    let (start_map, end_map) = build_date_bar_maps(classified);
    let new_keys: HashSet<(usize, usize, String)> = forest_nodes
        .values()
        .filter(|n| n.degree_level == 1)
        .map(|n| {
            let tag = match &n.kind {
                NodeKind::Pattern(pt) => pattern_tag(pt),
                NodeKind::Leaf => "Leaf".to_string(),
            };
            (n.start_bar, n.end_bar, tag)
        })
        .collect();
    // §9.3 逐檔 diff 資料源(G2.4):degree-1 鍵序列化進 diagnostics
    let mut key_strs: Vec<String> = new_keys
        .iter()
        .map(|(s, e, tag)| format!("{}-{}:{}", s, e, tag))
        .collect();
    key_strs.sort();
    diag.degree1_node_keys = key_strs;
    for sc in old_forest {
        if let Some(key) = old_scenario_key(sc, &start_map, &end_map) {
            if new_keys.contains(&key) {
                diag.old_forest_matched += 1;
            }
        }
    }

    diag.elapsed_us = start.elapsed().as_micros() as u64;
    diag
}

fn collect_pattern_nodes(
    node: &Rc<CompactionNode>,
    out: &mut HashMap<String, Rc<CompactionNode>>,
) {
    if node.degree_level >= 1 {
        out.entry(node.canonical.clone())
            .or_insert_with(|| Rc::clone(node));
        for ch in &node.children {
            collect_pattern_nodes(ch, out);
        }
    }
}

fn build_date_bar_maps(
    classified: &[ClassifiedMonowave],
) -> (HashMap<NaiveDate, usize>, HashMap<NaiveDate, usize>) {
    let mut start_map = HashMap::new();
    let mut end_map = HashMap::new();
    for cm in classified {
        let mw = &cm.monowave;
        start_map.entry(mw.start_date).or_insert(mw.bar_indices.0);
        end_map.entry(mw.end_date).or_insert(mw.bar_indices.1);
    }
    (start_map, end_map)
}

/// 舊 forest scenario → (start_bar, end_bar, pattern_tag);wave_tree 日期經
/// monowave 端點反查 bar(同 three_rounds `find_price_at_date` 的雙向偏好)。
fn old_scenario_key(
    sc: &Scenario,
    start_map: &HashMap<NaiveDate, usize>,
    end_map: &HashMap<NaiveDate, usize>,
) -> Option<(usize, usize, String)> {
    let tree: &WaveNode = &sc.wave_tree;
    let start_bar = start_map
        .get(&tree.start)
        .or_else(|| end_map.get(&tree.start))?;
    let end_bar = end_map.get(&tree.end).or_else(|| start_map.get(&tree.end))?;
    Some((*start_bar, *end_bar, pattern_tag(&sc.pattern_type)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monowave::ProportionMetrics;
    use crate::output::{Certainty, Monowave, StructureLabelCandidate};

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// 建 ClassifiedMonowave:日期鏈以 base + bar*1 天(bar 即日 index),
    /// 價格端點鏈由 caller 保證共享。
    fn cm(
        start_bar: usize,
        end_bar: usize,
        sp: f64,
        ep: f64,
        dir: MonowaveDirection,
        labels: &[StructureLabel],
    ) -> ClassifiedMonowave {
        let base = date("2026-01-01");
        ClassifiedMonowave {
            monowave: Monowave {
                start_date: base + chrono::Duration::days(start_bar as i64),
                end_date: base + chrono::Duration::days(end_bar as i64),
                start_price: sp,
                end_price: ep,
                direction: dir,
                bar_indices: (start_bar, end_bar),
            },
            atr_at_start: 1.0,
            metrics: ProportionMetrics {
                magnitude: (ep - sp).abs(),
                duration_bars: end_bar - start_bar + 1,
                atr_relative: 0.0,
                slope_vs_45deg: 0.0,
            },
            structure_label_candidates: labels
                .iter()
                .map(|l| StructureLabelCandidate {
                    label: *l,
                    certainty: Certainty::Primary,
                })
                .collect(),
            polywave_size: 0,
        }
    }

    /// 5 段 :5/:3 交替鏈(等長 5 bar、幅度近似)→ 恰可聚合 Impulse + 2 Zigzag
    fn impulse_chain() -> Vec<ClassifiedMonowave> {
        use MonowaveDirection::{Down, Up};
        use StructureLabel::{Five, Three};
        vec![
            cm(0, 5, 100.0, 110.0, Up, &[Five]),
            cm(5, 10, 110.0, 104.0, Down, &[Three]),
            cm(10, 15, 104.0, 116.0, Up, &[Five]),
            cm(15, 20, 116.0, 109.0, Down, &[Three]),
            cm(20, 25, 109.0, 121.0, Up, &[Five]),
        ]
    }

    fn cfg() -> NeelyEngineConfig {
        NeelyEngineConfig::default()
    }

    #[test]
    fn base_tiling_bridges_neutral_into_previous_leaf() {
        use MonowaveDirection::{Down, Neutral, Up};
        use StructureLabel::{Five, Three};
        let classified = vec![
            cm(0, 5, 100.0, 110.0, Up, &[Five]),
            cm(5, 7, 110.0, 111.0, Neutral, &[Three]),
            cm(7, 12, 111.0, 104.0, Down, &[Three]),
        ];
        let base = build_base_tiling(&classified, &[]);
        assert_eq!(base.nodes.len(), 2, "Neutral 併入前節點");
        assert_eq!(base.bridged, 1);
        assert_eq!(base.leading_dropped, 0);
        // 合成葉端點延伸至 Neutral 段 end;與下一節點共享端點(I2)
        assert_eq!(base.nodes[0].end_date, date("2026-01-08"));
        assert_eq!(base.nodes[0].end_price, 111.0);
        assert_eq!(base.nodes[0].end_price, base.nodes[1].start_price);
        assert_eq!(base.nodes[0].end_date, base.nodes[1].start_date);
    }

    #[test]
    fn base_tiling_drops_leading_neutral() {
        use MonowaveDirection::{Neutral, Up};
        use StructureLabel::{Five, Three};
        let classified = vec![
            cm(0, 3, 100.0, 100.5, Neutral, &[Three]),
            cm(3, 8, 100.5, 110.0, Up, &[Five]),
        ];
        let base = build_base_tiling(&classified, &[]);
        assert_eq!(base.nodes.len(), 1);
        assert_eq!(base.leading_dropped, 1);
        assert_eq!(base.bridged, 0);
    }

    #[test]
    fn shadow_impulse_chain_aggregates_with_zero_invariant_violations() {
        let classified = impulse_chain();
        let diag = run_shadow(&classified, &[], &[], &[], Timeframe::Daily, &cfg());
        assert_eq!(diag.base_tiling_len, 5);
        assert!(diag.rounds_run >= 1, "應至少發生一輪聚合");
        assert!(!diag.timed_out);
        assert_eq!(diag.w1_violations, 0);
        // I1–I6 全零(G2.1 gate 準則)
        let total: usize = diag.invariant_violations.values().sum();
        assert_eq!(total, 0, "不變量違反必須為 0:{:?}", diag.invariant_violations);
        // degree-1 節點:Impulse(全窗)+ Zigzag [0..3] + Zigzag [2..5]
        assert_eq!(diag.node_count_by_level.get("1"), Some(&3));
    }

    #[test]
    fn shadow_round2_aggregates_impulse_then_zigzag() {
        use MonowaveDirection::{Down, Up};
        use StructureLabel::{Five, Three};
        // 7 葉:前 5 段聚 Impulse(:5)後,[P, l5, l6] 可於 round 2 聚 Zigzag
        // P mag 21(100→121)/ l5 mag 12 → 21/12 = 1.75 ∈ S&B;l6 mag 30 → 12/30 = 0.4
        let mut classified = impulse_chain();
        classified.push(cm(25, 30, 121.0, 109.0, Down, &[Three]));
        classified.push(cm(30, 55, 109.0, 139.0, Up, &[Five]));
        let diag = run_shadow(&classified, &[], &[], &[], Timeframe::Daily, &cfg());
        let total: usize = diag.invariant_violations.values().sum();
        assert_eq!(total, 0);
        assert!(diag.rounds_run >= 2, "round 2 應發生(rounds_run = {})", diag.rounds_run);
        assert!(
            diag.node_count_by_level.get("2").is_some(),
            "應出現 degree-2 節點:{:?}",
            diag.node_count_by_level
        );
    }

    #[test]
    fn shadow_level_cap_hit_when_max_levels_one() {
        use MonowaveDirection::{Down, Up};
        use StructureLabel::{Five, Three};
        let mut classified = impulse_chain();
        classified.push(cm(25, 30, 121.0, 109.0, Down, &[Three]));
        classified.push(cm(30, 55, 109.0, 139.0, Up, &[Five]));
        let mut c = cfg();
        c.max_compaction_levels = 1;
        let diag = run_shadow(&classified, &[], &[], &[], Timeframe::Daily, &c);
        assert_eq!(diag.rounds_run, 1);
        assert!(diag.level_cap_hit, "round 1 仍有聚合但被上限截斷 → level_cap_hit");
    }

    #[test]
    fn shadow_converges_without_cap_hit_when_no_more_aggregation() {
        let classified = impulse_chain();
        let diag = run_shadow(&classified, &[], &[], &[], Timeframe::Daily, &cfg());
        assert!(
            !diag.level_cap_hit,
            "零聚合收斂(Round 3 暫停)不得標 level_cap_hit"
        );
    }

    #[test]
    fn shadow_beam_caps_tiling_pool() {
        let classified = impulse_chain();
        let mut c = cfg();
        c.round_beam_size = 2;
        let diag = run_shadow(&classified, &[], &[], &[], Timeframe::Daily, &c);
        assert!(diag.tiling_count <= 2, "pool 超過 round_beam_size 應被 beam 截斷");
        let total: usize = diag.invariant_violations.values().sum();
        assert_eq!(total, 0);
    }

    #[test]
    fn shadow_too_few_nodes_returns_early() {
        use MonowaveDirection::Up;
        use StructureLabel::Five;
        let classified = vec![cm(0, 5, 100.0, 110.0, Up, &[Five])];
        let diag = run_shadow(&classified, &[], &[], &[], Timeframe::Daily, &cfg());
        assert_eq!(diag.base_tiling_len, 1);
        assert_eq!(diag.rounds_run, 0);
        assert!(diag.node_count_by_level.is_empty());
    }

    #[test]
    fn w2_ambiguous_leaf_candidates_yield_multiple_kinds() {
        use MonowaveDirection::{Down, Up};
        use StructureLabel::{Five, Three};
        // 5 葉全帶 :5 與 :3 雙候選 → 同窗同時匹配 Impulse 與 Triangle(0..k 多解)
        let classified: Vec<ClassifiedMonowave> = vec![
            cm(0, 5, 100.0, 110.0, Up, &[Five, Three]),
            cm(5, 10, 110.0, 104.0, Down, &[Five, Three]),
            cm(10, 15, 104.0, 116.0, Up, &[Five, Three]),
            cm(15, 20, 116.0, 109.0, Down, &[Five, Three]),
            cm(20, 25, 109.0, 121.0, Up, &[Five, Three]),
        ];
        let diag = run_shadow(&classified, &[], &[], &[], Timeframe::Daily, &cfg());
        // degree-1 節點應同時含 Impulse 路徑與 Triangle 路徑(+ Zigzag/Flat 子窗)
        let level1 = diag.node_count_by_level.get("1").copied().unwrap_or(0);
        assert!(level1 >= 4, "多解視窗應產多個 degree-1 節點,got {}", level1);
    }

    #[test]
    fn invariant_checker_flags_broken_chain_and_wrong_degree() {
        use MonowaveDirection::{Down, Up};
        use StructureLabel::{Five, Three};
        // 手工構造壞 tiling:間隙(I2)+ 錯 degree(I4)
        let a = Rc::new(CompactionNode {
            kind: NodeKind::Leaf,
            label: NodeLabel::LeafCandidates(vec![Five]),
            degree_level: 0,
            canonical: leaf_canonical(0, 5),
            true_range: None,
            start_bar: 0,
            end_bar: 5,
            start_date: date("2026-01-01"),
            end_date: date("2026-01-06"),
            start_price: 100.0,
            end_price: 110.0,
            children: Vec::new(),
            validation: None,
            net_direction: Up,
        });
        // 間隙:start_date 不接 a.end_date、價格也不接
        let b = Rc::new(CompactionNode {
            kind: NodeKind::Leaf,
            label: NodeLabel::LeafCandidates(vec![Three]),
            degree_level: 3, // 葉 degree 應為 0 → I4
            canonical: leaf_canonical(9, 12),
            true_range: None,
            start_bar: 9,
            end_bar: 12,
            start_date: date("2026-01-10"),
            end_date: date("2026-01-13"),
            start_price: 108.0,
            end_price: 104.0,
            children: Vec::new(),
            validation: None,
            net_direction: Down,
        });
        let tiling: Vec<Rc<CompactionNode>> = vec![a, b];
        let c = check_invariants(&[tiling.as_slice()], None);
        assert!(c.i2 >= 1, "間隙應計 I2");
        assert!(c.i4 >= 1, "葉 degree != 0 應計 I4");
    }

    #[test]
    fn invariant_checker_flags_missing_tail_coverage() {
        use MonowaveDirection::Up;
        use StructureLabel::Five;
        // I1 聯集覆蓋條款:tiling 鏈完整但末節點未達 base 範圍 t_N(尾節點被
        // splice 吞掉的 bug 類別)→ I1
        let a = Rc::new(CompactionNode {
            kind: NodeKind::Leaf,
            label: NodeLabel::LeafCandidates(vec![Five]),
            degree_level: 0,
            canonical: leaf_canonical(0, 5),
            true_range: None,
            start_bar: 0,
            end_bar: 5,
            start_date: date("2026-01-01"),
            end_date: date("2026-01-06"),
            start_price: 100.0,
            end_price: 110.0,
            children: Vec::new(),
            validation: None,
            net_direction: Up,
        });
        let tiling: Vec<Rc<CompactionNode>> = vec![a];
        let c = check_invariants(
            &[tiling.as_slice()],
            Some((date("2026-01-01"), date("2026-01-20"))),
        );
        assert_eq!(c.i1, 1, "末節點 end 2026-01-06 ≠ base t_N 2026-01-20 → I1");

        let c_ok = check_invariants(
            &[tiling.as_slice()],
            Some((date("2026-01-01"), date("2026-01-06"))),
        );
        assert_eq!(c_ok.total(), 0, "覆蓋完整 → 零違反");
    }

    // G2.2:W5 端點泛化 / W6 分岔 / Q3 雙軌 tests ---------------------------

    /// 對整條 base tiling 當單一視窗直測階梯(單元級,不經 round 迴圈)
    fn ladder_on(
        classified: &[ClassifiedMonowave],
        bars: &[crate::output::OhlcvBar],
    ) -> (Vec<AcceptedKind>, LadderCounters) {
        let base = build_base_tiling(classified, bars);
        let mut counters = LadderCounters::default();
        let out = try_ladder(&base.nodes, &mut counters);
        (out.kinds, counters)
    }

    #[test]
    fn w5_rejects_essential_violation_impulse_window() {
        use MonowaveDirection::{Down, Up};
        use StructureLabel::{Five, Three};
        // R4 違反:W3 mag(6)< W2 mag(7)→ Ch5 essential fail →
        // :5 族(Impulse)被 W5 閘下,整窗無 kind → 唯一視窗拒絕計數
        let classified = vec![
            cm(0, 5, 100.0, 110.0, Up, &[Five]),
            cm(5, 10, 110.0, 103.0, Down, &[Three]),
            cm(10, 15, 103.0, 109.0, Up, &[Five]),
            cm(15, 20, 109.0, 105.0, Down, &[Three]),
            cm(20, 25, 105.0, 113.0, Up, &[Five]),
        ];
        let (kinds, counters) = ladder_on(&classified, &[]);
        assert!(kinds.is_empty(), "essential 違反 → 無 kind:{:?}", kinds);
        assert_eq!(counters.w5_rejected_windows, 1);
    }

    #[test]
    fn w5_passes_clean_impulse_and_attaches_true_rule_counts() {
        use StructureLabel::Five;
        // 乾淨 Impulse 鏈 → overall_pass;passed 經 default_passed_rules 同源
        // 推導非空(D-3 修復:Level-N rules 欄真值,beam 鍵 2 可比)
        let base = build_base_tiling(&impulse_chain(), &[]);
        let mut counters = LadderCounters::default();
        let out = try_ladder(&base.nodes, &mut counters);
        assert!(out
            .kinds
            .iter()
            .any(|k| matches!(k.pattern, NeelyPatternType::Impulse) && k.base == Five));
        let report = out.report.expect("接受視窗必附 ValidationReport");
        assert!(report.overall_pass);
        assert!(
            !report.passed.is_empty(),
            "passed 反推不應為空(5-wave essentials 通過)"
        );
        assert_eq!(counters.w5_rejected_windows, 0);
    }

    #[test]
    fn w6_terminal_impulse_accepted_for_overlapping_threes() {
        use MonowaveDirection::{Down, Up};
        use StructureLabel::{Five, Three};
        // D-5 修復存在性:全 :3、W1/W4 範圍重疊、W2/W4 不完全回測、W3 非最短
        // → Terminal Impulse(:5,Diagonal 表徵);兩線發散但 e 未逐波擴大
        // → Triangle 不接受
        let classified = vec![
            cm(0, 5, 100.0, 110.0, Up, &[Three]),
            cm(5, 10, 110.0, 103.0, Down, &[Three]),
            cm(10, 15, 103.0, 114.0, Up, &[Three]),
            cm(15, 20, 114.0, 106.0, Down, &[Three]),
            cm(20, 25, 106.0, 115.0, Up, &[Three]),
        ];
        let (kinds, counters) = ladder_on(&classified, &[]);
        assert_eq!(counters.w5_rejected_windows, 0);
        assert!(
            kinds.iter().any(|k| matches!(k.pattern, NeelyPatternType::Diagonal { .. })
                && k.base == Five),
            "Terminal Impulse(:5)應被接受:{:?}",
            kinds
        );
        assert!(
            !kinds
                .iter()
                .any(|k| matches!(k.pattern, NeelyPatternType::Triangle { .. })),
            "發散且未逐波擴大 → 非 Triangle:{:?}",
            kinds
        );
    }

    #[test]
    fn w6_contracting_triangle_survives_w5_family_gate() {
        use MonowaveDirection::{Down, Up};
        use StructureLabel::Three;
        // 收斂震盪(W3 < W2 → R4 fail → overall_pass = false):
        // :3 族 Triangle 不受衝動 essentials 閘 → Contracting 存活;
        // 同窗 Terminal 端點條件雖成立,:5 族被 W5 閘下 → 族別閘門的直接驗證
        let classified = vec![
            cm(0, 5, 100.0, 110.0, Up, &[Three]),
            cm(5, 10, 110.0, 102.0, Down, &[Three]),
            cm(10, 15, 102.0, 108.0, Up, &[Three]),
            cm(15, 20, 108.0, 103.0, Down, &[Three]),
            cm(20, 25, 103.0, 105.5, Up, &[Three]),
        ];
        let (kinds, counters) = ladder_on(&classified, &[]);
        assert!(
            kinds.iter().any(|k| matches!(
                k.pattern,
                NeelyPatternType::Triangle {
                    sub_kind: TriangleKind::Contracting
                }
            )),
            "收斂 + e 不破線 → Contracting Triangle:{:?}",
            kinds
        );
        assert!(
            !kinds
                .iter()
                .any(|k| matches!(k.pattern, NeelyPatternType::Diagonal { .. })),
            ":5 族 Terminal 應被 W5(R4 fail)閘下:{:?}",
            kinds
        );
        assert_eq!(counters.w5_rejected_windows, 0, "視窗仍有 :3 kind → 非整窗拒絕");
    }

    #[test]
    fn bars_wick_suppresses_terminal_when_b_fully_retraces_intraday() {
        use chrono::Duration;
        use MonowaveDirection::{Down, Up};
        use StructureLabel::Three;
        // Q3 拍板判準測試:端點版 b 未完全回測(103 > 100)→ Terminal 成立;
        // bars 版 b 段影線 99 < a.start(100)→ 盤中已完全回測 → Terminal 不成立
        let classified = vec![
            cm(0, 5, 100.0, 110.0, Up, &[Three]),
            cm(5, 10, 110.0, 103.0, Down, &[Three]),
            cm(10, 15, 103.0, 114.0, Up, &[Three]),
            cm(15, 20, 114.0, 106.0, Down, &[Three]),
            cm(20, 25, 106.0, 115.0, Up, &[Three]),
        ];
        let base_date = date("2026-01-01");
        let path = |i: usize| -> f64 {
            let segs: [(usize, usize, f64, f64); 5] = [
                (0, 5, 100.0, 110.0),
                (5, 10, 110.0, 103.0),
                (10, 15, 103.0, 114.0),
                (15, 20, 114.0, 106.0),
                (20, 25, 106.0, 115.0),
            ];
            for (s, e, ps, pe) in segs {
                if i >= s && i <= e {
                    return ps + (pe - ps) * ((i - s) as f64) / ((e - s) as f64);
                }
            }
            115.0
        };
        let bars: Vec<crate::output::OhlcvBar> = (0..26)
            .map(|i| {
                let p = path(i);
                let low = if i == 8 { 99.0 } else { p };
                crate::output::OhlcvBar {
                    date: base_date + Duration::days(i as i64),
                    open: p,
                    high: p,
                    low,
                    close: p,
                    volume: None,
                }
            })
            .collect();
        let (kinds, _) = ladder_on(&classified, &bars);
        assert!(
            !kinds
                .iter()
                .any(|k| matches!(k.pattern, NeelyPatternType::Diagonal { .. })),
            "bars 判準下 b 盤中完全回測 → Terminal 不成立:{:?}",
            kinds
        );
    }

    #[test]
    fn q3_bars_wick_flips_retracement_verdict() {
        use chrono::Duration;
        // impulse_chain 的線性路徑 bars(高低 = 收盤路徑,零影線),唯 bar 8 加
        // 下影線 98 < a.start(100)→ bars 版 b 完全回測 true vs 端點版 false → flip
        let classified = impulse_chain();
        let base_date = date("2026-01-01");
        let path = |i: usize| -> f64 {
            let segs: [(usize, usize, f64, f64); 5] = [
                (0, 5, 100.0, 110.0),
                (5, 10, 110.0, 104.0),
                (10, 15, 104.0, 116.0),
                (15, 20, 116.0, 109.0),
                (20, 25, 109.0, 121.0),
            ];
            for (s, e, ps, pe) in segs {
                if i >= s && i <= e {
                    return ps + (pe - ps) * ((i - s) as f64) / ((e - s) as f64);
                }
            }
            121.0
        };
        let bars: Vec<crate::output::OhlcvBar> = (0..26)
            .map(|i| {
                let p = path(i);
                let low = if i == 8 { 98.0 } else { p };
                crate::output::OhlcvBar {
                    date: base_date + Duration::days(i as i64),
                    open: p,
                    high: p,
                    low,
                    close: p,
                    volume: None,
                }
            })
            .collect();
        let (_, counters) = ladder_on(&classified, &bars);
        assert_eq!(counters.q3_windows, 1, "5-窗應完成一次雙軌比對");
        assert_eq!(counters.q3_flips, 1, "影線使回測判定翻轉");

        // 對照組:無影線 → 比對進行但無翻轉
        let bars_clean: Vec<crate::output::OhlcvBar> = (0..26)
            .map(|i| {
                let p = path(i);
                crate::output::OhlcvBar {
                    date: base_date + Duration::days(i as i64),
                    open: p,
                    high: p,
                    low: p,
                    close: p,
                    volume: None,
                }
            })
            .collect();
        let (_, counters_clean) = ladder_on(&classified, &bars_clean);
        assert_eq!(counters_clean.q3_windows, 1);
        assert_eq!(counters_clean.q3_flips, 0, "零影線 → 端點版與 bars 版一致");
    }

    #[test]
    fn dense_ambiguous_chain_hits_branch_cap_and_stays_bounded() {
        use MonowaveDirection::{Down, Up};
        use StructureLabel::{Five, Three};
        // 60 段等長多義葉(:5/:3 雙候選)→ G2.1 階梯接受率極高;
        // 護欄:round 內 materialize 上限 = round_beam_size × 8,
        // pool 收斂 ≤ round_beam_size,cap 命中可觀察
        let mut classified = Vec::new();
        let mut price = 100.0;
        for i in 0..60usize {
            let (dir, next) = if i % 2 == 0 {
                (Up, price + 10.0)
            } else {
                (Down, price - 6.0)
            };
            classified.push(cm(i * 5, (i + 1) * 5, price, next, dir, &[Five, Three]));
            price = next;
        }
        let c = cfg();
        let started = std::time::Instant::now();
        let diag = run_shadow(&classified, &[], &[], &[], Timeframe::Daily, &c);
        assert!(
            started.elapsed().as_secs() < 10,
            "60 段稠密鏈必須在秒級完成(護欄失效 = 分支爆炸)"
        );
        assert!(diag.tiling_count <= c.round_beam_size);
        assert!(
            diag.round_branch_cap_hits >= 1,
            "稠密鏈應觸發 branch cap(可觀察):{:?}",
            diag.round_branch_cap_hits
        );
        let total: usize = diag.invariant_violations.values().sum();
        assert_eq!(total, 0, "護欄截斷不得產生不變量違反");
        assert_eq!(
            diag.degree1_node_keys.len(),
            *diag.node_count_by_level.get("1").unwrap_or(&0),
            "degree-1 diff 鍵數應等於 degree-1 收集數"
        );
        // G2.4 收集語意修正:branch cap / beam 不得限縮收集(I6 全 tilings)。
        // 縮 beam 至 1(cap = 8):materialize 每輪僅 8 分支、pool 收斂 1 條,
        // 但 round-1 接受的唯一解讀遠超 8 — 收集數必須大於 cap
        let mut c_small = cfg();
        c_small.round_beam_size = 1;
        let diag_small = run_shadow(&classified, &[], &[], &[], Timeframe::Daily, &c_small);
        let collected_small: usize = diag_small.node_count_by_level.values().sum();
        assert!(
            diag_small.round_branch_cap_hits >= 1,
            "beam=1 稠密鏈必觸 cap"
        );
        assert!(
            collected_small > 8,
            "cap=8 之外的接受解讀仍應被收集:collected={}",
            collected_small
        );
    }

    #[test]
    fn old_forest_recall_matches_by_bar_range_and_pattern() {
        use crate::output::{
            ComplexityLevel, PostBehavior, PowerRating, RoundState, StructuralFacts,
        };
        let classified = impulse_chain();
        // 造一個舊 forest scenario:日期範圍 = 新引擎 Zigzag [0..3](bar 0..15)
        let old = Scenario {
            wave_count: 0,
            id: "old-zz".to_string(),
            wave_tree: WaveNode {
                degree_level: 0,
                base_label: crate::output::StructureLabel::Three,
                label: "zz".to_string(),
                start: date("2026-01-01"),
                end: date("2026-01-16"),
                children: Vec::new(),
            },
            pattern_type: NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            initial_direction: MonowaveDirection::Up,
            compacted_base_label: StructureLabel::Three,
            structure_label: "zz".to_string(),
            complexity_level: ComplexityLevel::Simple,
            power_rating: PowerRating::Neutral,
            max_retracement: None,
            post_pattern_behavior: PostBehavior::Unconstrained,
            passed_rules: Vec::new(),
            deferred_rules: Vec::new(),
            rules_passed_count: 0,
            deferred_rules_count: 0,
            invalidation_triggers: Vec::new(),
            expected_fib_zones: Vec::new(),
            structural_facts: StructuralFacts::default(),
            advisory_findings: Vec::new(),
            in_triangle_context: false,
            awaiting_l_label: false,
            monowave_structure_labels: Vec::new(),
            round_state: RoundState::Round1,
            pattern_isolation_anchors: Vec::new(),
            triplexity_detected: false,
        };
        let diag = run_shadow(&classified, &[old], &[], &[], Timeframe::Daily, &cfg());
        assert_eq!(diag.old_forest_scenarios, 1);
        assert_eq!(
            diag.old_forest_matched, 1,
            "同 (bar 範圍, pattern) 的舊 scenario 應被召回;levels={:?}",
            diag.node_count_by_level
        );
    }

    #[test]
    fn dedup_same_aggregation_reached_twice_counted_once() {
        // impulse_chain 的 Zigzag [0..3] 在多輪間可被重複生成;seen dedup 應保唯一
        let classified = impulse_chain();
        let diag = run_shadow(&classified, &[], &[], &[], Timeframe::Daily, &cfg());
        // node_count_by_level 以 canonical 去重;Impulse+2 Zigzag = 3(不因分支重複膨脹)
        assert_eq!(diag.node_count_by_level.get("1"), Some(&3));
    }

    // ── G2.3:A-9 W2 細分(與 classifier 量值版核心同源)──────────────────

    /// 3-窗 [:3 :3 :5] 依幅度細分 Flat 變體(b/a=0.85、c/b≥1 → Common)
    #[test]
    fn a9_three_window_flat_refined_to_common() {
        use MonowaveDirection::{Down, Up};
        use StructureLabel::{Five, Three};
        let classified = vec![
            cm(0, 5, 1000.0, 900.0, Down, &[Three]),
            cm(5, 10, 900.0, 985.0, Up, &[Three]),
            cm(10, 15, 985.0, 895.0, Down, &[Five]),
        ];
        let base = build_base_tiling(&classified, &[]);
        let rows = w2_label_rows(&base.nodes);
        assert_eq!(rows.len(), 1);
        match &rows[0] {
            RowMatch::Direct(k) => assert!(
                matches!(
                    k.pattern,
                    NeelyPatternType::Flat {
                        sub_kind: crate::output::FlatKind::Common
                    }
                ),
                "應細分為 Flat Common:{:?}",
                k.pattern
            ),
            RowMatch::ThreesFive => panic!("非 3-3-3-3-3 分岔"),
        }
    }

    /// 3-窗 b>a 且 c<a → RunningCorrection(量值版核心同源)
    #[test]
    fn a9_three_window_running_correction() {
        use MonowaveDirection::{Down, Up};
        use StructureLabel::{Five, Three};
        let classified = vec![
            cm(0, 5, 1000.0, 900.0, Down, &[Three]),
            cm(5, 10, 900.0, 1020.0, Up, &[Three]),
            cm(10, 15, 1020.0, 940.0, Down, &[Five]),
        ];
        let base = build_base_tiling(&classified, &[]);
        let rows = w2_label_rows(&base.nodes);
        assert_eq!(rows.len(), 1);
        match &rows[0] {
            RowMatch::Direct(k) => assert!(
                matches!(k.pattern, NeelyPatternType::RunningCorrection),
                "{:?}",
                k.pattern
            ),
            RowMatch::ThreesFive => panic!(),
        }
    }

    /// 3-窗 b/a < 61.8% 不符 Flat 最低要求 → row 不成立(G2.2 的 Common 佔位廢除)
    #[test]
    fn a9_three_window_weak_b_drops_flat_row() {
        use MonowaveDirection::{Down, Up};
        use StructureLabel::{Five, Three};
        let classified = vec![
            cm(0, 5, 1000.0, 900.0, Down, &[Three]),
            cm(5, 10, 900.0, 950.0, Up, &[Three]),
            cm(10, 15, 950.0, 870.0, Down, &[Five]),
        ];
        let base = build_base_tiling(&classified, &[]);
        assert!(w2_label_rows(&base.nodes).is_empty());
    }

    /// 7-窗小 x-wave + 兩側 Zigzag 構成段 → DoubleZigzag(Table A)
    #[test]
    fn a9_seven_window_small_x_double_zigzag() {
        use MonowaveDirection::{Down, Up};
        use StructureLabel::Three;
        let classified = vec![
            cm(0, 5, 1000.0, 900.0, Down, &[Three]),
            cm(5, 10, 900.0, 950.0, Up, &[Three]),
            cm(10, 15, 950.0, 860.0, Down, &[Three]),
            cm(15, 20, 860.0, 890.0, Up, &[Three]), // x mag 30 < 0.618×140
            cm(20, 25, 890.0, 790.0, Down, &[Three]),
            cm(25, 30, 790.0, 840.0, Up, &[Three]),
            cm(30, 35, 840.0, 750.0, Down, &[Three]),
        ];
        let base = build_base_tiling(&classified, &[]);
        let rows = w2_label_rows(&base.nodes);
        assert_eq!(rows.len(), 1);
        match &rows[0] {
            RowMatch::Direct(k) => match &k.pattern {
                NeelyPatternType::Combination { sub_kinds } => assert_eq!(
                    sub_kinds,
                    &vec![crate::output::CombinationKind::DoubleZigzag]
                ),
                other => panic!("應為 Combination:{:?}", other),
            },
            RowMatch::ThreesFive => panic!(),
        }
    }

    /// 7-窗大 x-wave + Zigzag 構成段 → Table B 不可辨識 → row 不成立
    #[test]
    fn a9_seven_window_large_x_with_zigzag_drops_row() {
        use MonowaveDirection::{Down, Up};
        use StructureLabel::Three;
        let classified = vec![
            cm(0, 5, 1000.0, 900.0, Down, &[Three]),
            cm(5, 10, 900.0, 950.0, Up, &[Three]),
            cm(10, 15, 950.0, 860.0, Down, &[Three]),
            cm(15, 20, 860.0, 1060.0, Up, &[Three]), // x mag 200 ≥ 0.618×140 → 大 x
            cm(20, 25, 1060.0, 960.0, Down, &[Three]),
            cm(25, 30, 960.0, 1010.0, Up, &[Three]),
            cm(30, 35, 1010.0, 920.0, Down, &[Three]),
        ];
        let base = build_base_tiling(&classified, &[]);
        assert!(w2_label_rows(&base.nodes).is_empty());
    }

    /// 7-窗大 x-wave + 兩側 Flat 構成段 → DoubleThree(Table B)
    #[test]
    fn a9_seven_window_large_x_flats_double_three() {
        use MonowaveDirection::{Down, Up};
        use StructureLabel::Three;
        let classified = vec![
            cm(0, 5, 1000.0, 900.0, Down, &[Three]),
            cm(5, 10, 900.0, 985.0, Up, &[Three]),
            cm(10, 15, 985.0, 895.0, Down, &[Three]),
            cm(15, 20, 895.0, 965.0, Up, &[Three]), // x mag 70 ≥ 0.618×105 → 大 x
            cm(20, 25, 965.0, 865.0, Down, &[Three]),
            cm(25, 30, 865.0, 950.0, Up, &[Three]),
            cm(30, 35, 950.0, 860.0, Down, &[Three]),
        ];
        let base = build_base_tiling(&classified, &[]);
        let rows = w2_label_rows(&base.nodes);
        assert_eq!(rows.len(), 1);
        match &rows[0] {
            RowMatch::Direct(k) => match &k.pattern {
                NeelyPatternType::Combination { sub_kinds } => assert_eq!(
                    sub_kinds,
                    &vec![crate::output::CombinationKind::DoubleThree]
                ),
                other => panic!("應為 Combination:{:?}", other),
            },
            RowMatch::ThreesFive => panic!(),
        }
    }

    // ── G2.3:§6.1 邊界波重評三檔 ────────────────────────────────────────

    #[test]
    fn boundary_tier_three_bands() {
        assert_eq!(boundary_tier(100.0, 100.0), Some(BoundaryTier::Pass));
        assert_eq!(boundary_tier(100.0, 300.0), Some(BoundaryTier::Info)); // 0.333
        assert_eq!(boundary_tier(300.0, 100.0), Some(BoundaryTier::Info)); // 3.0
        assert_eq!(boundary_tier(100.0, 500.0), Some(BoundaryTier::Warning)); // 0.2
        assert_eq!(boundary_tier(500.0, 100.0), Some(BoundaryTier::Warning)); // 5.0
        assert_eq!(boundary_tier(0.0, 100.0), None); // 零幅度不可比
    }

    /// impulse_chain 三個 splice 的真鄰居計數:全窗 Impulse 兩側無鄰居(skip 2)、
    /// Zigzag [0..3] 右鄰 Pass + 左 skip、Zigzag [2..5] 左鄰 Pass + 右 skip;
    /// round 2 之 [P_z1, l3, l4] Flat row 因 A-9(b/a=0.44)不再成立,無新 splice。
    #[test]
    fn boundary_reassess_counts_on_impulse_chain() {
        let classified = impulse_chain();
        let diag = run_shadow(&classified, &[], &[], &[], Timeframe::Daily, &cfg());
        assert_eq!(diag.boundary_pairs_checked, 2);
        assert_eq!(diag.boundary_sides_skipped, 4);
        assert_eq!(diag.boundary_advisory_info, 0);
        assert_eq!(diag.boundary_advisory_warning, 0);
    }

    // ── G2.3:§6.2 Complexity 真算 + Triplexity ──────────────────────────

    fn leaf_node(labels: &[StructureLabel]) -> Rc<CompactionNode> {
        Rc::new(CompactionNode {
            kind: NodeKind::Leaf,
            label: NodeLabel::LeafCandidates(labels.to_vec()),
            degree_level: 0,
            start_bar: 0,
            end_bar: 1,
            start_date: date("2026-01-01"),
            end_date: date("2026-01-02"),
            start_price: 100.0,
            end_price: 110.0,
            children: Vec::new(),
            validation: None,
            net_direction: MonowaveDirection::Up,
            canonical: String::new(),
            true_range: None,
        })
    }

    fn pattern_node(
        pt: NeelyPatternType,
        base: StructureLabel,
        degree: usize,
        children: Vec<Rc<CompactionNode>>,
    ) -> Rc<CompactionNode> {
        Rc::new(CompactionNode {
            kind: NodeKind::Pattern(pt),
            label: NodeLabel::Fixed(base),
            degree_level: degree,
            start_bar: 0,
            end_bar: 10,
            start_date: date("2026-01-01"),
            end_date: date("2026-03-01"),
            start_price: 100.0,
            end_price: 120.0,
            children,
            validation: None,
            net_direction: MonowaveDirection::Up,
            canonical: String::new(),
            true_range: None,
        })
    }

    #[test]
    fn complexity_levels_per_spec_table() {
        use StructureLabel::{Five, Three};
        let leaf5 = || leaf_node(&[Five]);
        let leaf3 = || leaf_node(&[Three]);
        // Level-0:葉
        assert_eq!(node_complexity(&leaf5()), 0);
        // Level-1:degree-1 Polywave(children 全葉,無 :5 pattern 子節點)
        let poly_impulse = pattern_node(
            NeelyPatternType::Impulse,
            Five,
            1,
            vec![leaf5(), leaf3(), leaf5(), leaf3(), leaf5()],
        );
        assert_eq!(node_complexity(&poly_impulse), 1);
        // Level-2:Multiwave — 至少一個 :5 子節點自身為 impulsive polywave
        let multiwave = pattern_node(
            NeelyPatternType::Impulse,
            Five,
            2,
            vec![
                Rc::clone(&poly_impulse),
                leaf3(),
                leaf5(),
                leaf3(),
                leaf5(),
            ],
        );
        assert_eq!(node_complexity(&multiwave), 2);
        // Multiwave 修正:Zigzag 之 :5 子節點為 impulsive polywave → 同為 Level-2
        let multiwave_zz = pattern_node(
            NeelyPatternType::Zigzag {
                sub_kind: ZigzagKind::Single,
            },
            Three,
            2,
            vec![Rc::clone(&poly_impulse), leaf3(), leaf5()],
        );
        assert_eq!(node_complexity(&multiwave_zz), 2);
        // Level-3:Macrowave — 一 :5 為 Multiwave 且另一 :5 至少 Polywave
        let macrowave = pattern_node(
            NeelyPatternType::Impulse,
            Five,
            3,
            vec![
                Rc::clone(&multiwave),
                leaf3(),
                Rc::clone(&poly_impulse),
                leaf3(),
                leaf5(),
            ],
        );
        assert_eq!(node_complexity(&macrowave), 3);
        // Triangle(children 全 :3)→ 恆 Level-1
        let triangle = pattern_node(
            NeelyPatternType::Triangle {
                sub_kind: TriangleKind::Contracting,
            },
            Three,
            2,
            vec![leaf3(), leaf3(), leaf3(), leaf3(), leaf3()],
        );
        assert_eq!(node_complexity(&triangle), 1);
    }

    #[test]
    fn triplexity_requires_three_distinct_impulse_levels() {
        use StructureLabel::{Five, Three};
        let leaf5 = || leaf_node(&[Five]);
        let leaf3 = || leaf_node(&[Three]);
        let poly = pattern_node(
            NeelyPatternType::Impulse,
            Five,
            1,
            vec![leaf5(), leaf3(), leaf5(), leaf3(), leaf5()],
        );
        // Multiwave:Impulse 段 levels = {2(自身), 1(poly), 0(:5 slot 葉)} → triplexity
        let multiwave = pattern_node(
            NeelyPatternType::Impulse,
            Five,
            2,
            vec![Rc::clone(&poly), leaf3(), leaf5(), leaf3(), leaf5()],
        );
        assert!(triplexity_detected(&multiwave));
        // Polywave 單獨:levels = {1(自身), 0(:5 slot 葉)} → 不足 3
        assert!(!triplexity_detected(&poly));
    }

    // ── G2.3:§6.3 Degree ceiling 錨定對映 ──────────────────────────────

    #[test]
    fn degree_map_anchors_max_level_to_ceiling() {
        let (map, clamped) = degree_name_map(2, &Degree::Minor);
        assert_eq!(map.get("2").map(String::as_str), Some("Minor"));
        assert_eq!(map.get("1").map(String::as_str), Some("Minute"));
        assert_eq!(map.get("0").map(String::as_str), Some("Minuette"));
        assert_eq!(clamped, 0);
    }

    #[test]
    fn degree_map_clamps_below_submicro() {
        // ceiling SubMinuette(idx 2)、max_level 3 → level 0 超下界夾 SubMicro
        let (map, clamped) = degree_name_map(3, &Degree::SubMinuette);
        assert_eq!(map.get("3").map(String::as_str), Some("SubMinuette"));
        assert_eq!(map.get("2").map(String::as_str), Some("Micro"));
        assert_eq!(map.get("1").map(String::as_str), Some("SubMicro"));
        assert_eq!(map.get("0").map(String::as_str), Some("SubMicro"));
        assert_eq!(clamped, 1);
    }

    // ── G2.3:A-10 anchors union vs 現行 overlap 近似 ────────────────────

    #[test]
    fn a10_anchors_union_tighter_than_overlap() {
        use StructureLabel::Three;
        let pb = |s: usize, e: usize| PatternBound {
            start_idx: s,
            end_idx: e,
            start_label: Three,
            end_label: Three,
            validated: false,
            forced_corrective: false,
        };
        // forest = Impulse [bars 0..25] + Zigzag [0..15] + Zigzag [10..25]
        // pb(0,2) = bars 0..15:union 含於 Impulse + Z1(2);overlap 加 Z2(3)
        // pb(3,4) = bars 15..25:union 含於 Impulse + Z2(2);overlap 加 Z1(3)
        let classified = impulse_chain();
        let bounds = [pb(0, 2), pb(3, 4)];
        let diag = run_shadow(&classified, &[], &[], &bounds, Timeframe::Daily, &cfg());
        assert_eq!(diag.engine, "tiling-round-g2.4");
        assert_eq!(diag.anchors_union_total, 4);
        assert_eq!(diag.anchors_overlap_total, 6);
        // §6.3:空 bars → ceiling "no data" = SubMicro;max_level 1 錨定 →
        // level 1 = SubMicro、level 0 超下界夾 SubMicro(clamped 1)
        assert_eq!(diag.degree_map.get("1").map(String::as_str), Some("SubMicro"));
        assert_eq!(diag.degree_clamped_levels, 1);
        // §6.2:degree-1 節點(children 全葉)complexity 全 1
        assert_eq!(diag.complexity_count_by_level.get("1"), Some(&3));
        assert_eq!(diag.triplexity_nodes, 0);
    }
}
