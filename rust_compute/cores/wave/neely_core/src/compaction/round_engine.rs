// round_engine.rs — Compaction v2 tiling-round 引擎(G2.1 shadow 骨架)
//
// 對齊 m3Spec/neely_compaction_v2.md:
//   §2.2 不變量 I1–I6 / §3.1 目標架構 / §5.1 CompactionNode 合約 / §5.2 主迴圈 /
//   §5.4 canonical_key / §5.5 beam 排序鍵 / §5.6 memoization / §9.3 shadow 比對。
//
// G2.1 範圍(v2 附錄 B):
//   - 基礎設施:Rc 節點共享、base tiling(Neutral 合成葉橋接)、round 迴圈、
//     per-round canonical_key dedup、round_beam_size beam、`level_cap_hit` 旗標(A-8)
//   - 接受階梯:W1(I2 防衛)/ W2(label 序列 I5 閉合表)/ W3(方向交替)/
//     W4(S&B)/ W7(Fib² 視窗內部比例)
//   - W5(Ch5 端點泛化)/ W6(分岔判別,D-5 修復)留 G2.2:本檔 stub —
//     validation 恆 None、Level-N rules 計數恆 0、3-3-3-3-3 暫僅出 Triangle
//   - **shadow 雙軌**(§3.3):輸出僅寫 `NeelyDiagnostics.shadow_compaction`,
//     serving forest 完全不受影響;Gate v3 通過後才切換
//
// 工程注意(G2.1 拍板,G2.2 可推翻):
//   - W4 time 維度沿用現行 `three_rounds::similarity_and_balance` 的日曆日基準,
//     shadow 比對同基準不引入額外差異;bars 基準留 G2.2 隨 Q3 實驗一併裁決
//   - W7 依 v2 §4.2「語意修正」檢查視窗內**全**相鄰對(舊引擎僅查首尾邊界對);
//     此為 spec 明訂語意變更,shadow 召回缺口若源於此,屬 Gate 報告需標注類別

use crate::config::NeelyEngineConfig;
use crate::monowave::ClassifiedMonowave;
use crate::output::{
    MonowaveDirection, NeelyPatternType, Scenario, ShadowCompactionDiagnostics, StructureLabel,
    TriangleKind, WaveNode, ZigzagKind,
};
use crate::power_rating;
use crate::validator::ValidationReport;
use chrono::NaiveDate;
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

    fn time_days(&self) -> f64 {
        (self.end_date - self.start_date).num_days() as f64
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

fn build_base_tiling(classified: &[ClassifiedMonowave]) -> BaseTiling {
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
/// price 取端點差絕對值(零幅度退 time 單維,沿用現行 Option 語意)。
fn w4_similarity_balance(window: &[Rc<CompactionNode>]) -> bool {
    for i in 1..window.len() {
        let (a, b) = (&window[i - 1], &window[i]);
        let (pa, pb) = (a.price_magnitude(), b.price_magnitude());
        let price_ok = pa > PRICE_EPS && pb > PRICE_EPS && ratio_in(pa, pb, SB_MIN_RATIO, SB_MAX_RATIO);
        let time_ok = ratio_in(a.time_days(), b.time_days(), SB_MIN_RATIO, SB_MAX_RATIO);
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

/// W2:I5 閉合表(§4.2.1)。回傳匹配的候選形態(0..k,同窗多解各產分支)。
/// G2.1 註記:3-3-3-3-3 暫僅出 Triangle{Contracting}(Terminal 分岔 = W6,G2.2);
/// Flat 暫以 Common 佔位(七變體交 G2.2 W6);Combination 沿用通用 kind(A-9,G2.3)。
fn w2_label_rows(window: &[Rc<CompactionNode>]) -> Vec<AcceptedKind> {
    use Slot::{Five, Three};
    let mut out = Vec::new();

    let matches_seq = |slots: &[Slot]| -> bool {
        slots.len() == window.len()
            && window.iter().zip(slots).all(|(n, s)| n.matches_slot(*s))
    };

    match window.len() {
        3 => {
            if matches_seq(&[Five, Three, Five]) {
                out.push(AcceptedKind {
                    pattern: NeelyPatternType::Zigzag {
                        sub_kind: ZigzagKind::Single,
                    },
                    base: StructureLabel::Three,
                });
            }
            if matches_seq(&[Three, Three, Five]) {
                out.push(AcceptedKind {
                    pattern: NeelyPatternType::Flat {
                        sub_kind: crate::output::FlatKind::Common,
                    },
                    base: StructureLabel::Three,
                });
            }
        }
        5 => {
            if matches_seq(&[Five, Three, Five, Three, Five]) {
                out.push(AcceptedKind {
                    pattern: NeelyPatternType::Impulse,
                    base: StructureLabel::Five,
                });
            }
            if matches_seq(&[Three, Three, Three, Three, Three]) {
                out.push(AcceptedKind {
                    pattern: NeelyPatternType::Triangle {
                        sub_kind: TriangleKind::Contracting,
                    },
                    base: StructureLabel::Three,
                });
            }
        }
        7 => {
            if matches_seq(&[Three; 7]) {
                out.push(AcceptedKind {
                    pattern: NeelyPatternType::Combination {
                        sub_kinds: vec![crate::output::CombinationKind::DoubleThree],
                    },
                    base: StructureLabel::Three,
                });
            }
        }
        11 => {
            if matches_seq(&[Three; 11]) {
                out.push(AcceptedKind {
                    pattern: NeelyPatternType::Combination {
                        sub_kinds: vec![crate::output::CombinationKind::TripleThree],
                    },
                    base: StructureLabel::Three,
                });
            }
        }
        _ => {}
    }
    out
}

/// try_all_neely 的 G2.1 部分階梯(W1→W3→W4→W7 為 row 無關前濾,再展 W2 rows;
/// 短路語意與 §4.2 cheap→expensive 排序等價)。W5/W6 stub:接受即定案,無重驗。
fn try_ladder_g21(
    window: &[Rc<CompactionNode>],
    w1_violations: &mut usize,
) -> Vec<AcceptedKind> {
    if !w1_adjacency(window) {
        *w1_violations += 1;
        return Vec::new();
    }
    if !w3_alternating(window) || !w4_similarity_balance(window) || !w7_internal_extreme(window) {
        return Vec::new();
    }
    w2_label_rows(window)
}

/// `canonical` 由 caller 預算(`pattern_canonical` 只需 window slice)—
/// 讓 round 迴圈能以 spliced key 先查 seen,命中即免 materialize(#4 護欄配套)。
fn make_parent(
    window: &[Rc<CompactionNode>],
    kind: &AcceptedKind,
    canonical: String,
) -> Rc<CompactionNode> {
    let first = window.first().expect("non-empty window");
    let last = window.last().expect("non-empty window");
    let degree = window.iter().map(|n| n.degree_level).max().unwrap_or(0) + 1;
    let delta = last.end_price - first.start_price;
    let net = if delta > 0.0 {
        MonowaveDirection::Up
    } else if delta < 0.0 {
        MonowaveDirection::Down
    } else {
        first.net_direction
    };
    let children: Vec<Rc<CompactionNode>> = window.to_vec();
    Rc::new(CompactionNode {
        kind: NodeKind::Pattern(kind.pattern.clone()),
        label: NodeLabel::Fixed(kind.base),
        degree_level: degree,
        canonical,
        start_bar: first.start_bar,
        end_bar: last.end_bar,
        start_date: first.start_date,
        end_date: last.end_date,
        start_price: first.start_price,
        end_price: last.end_price,
        children,
        validation: None,
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

/// 跑 tiling-round 引擎(shadow):產 diagnostics,不動 serving forest。
/// `old_forest` 供 §9.3 召回計數((start_bar, end_bar, pattern_tag) 匹配)。
pub fn run_shadow(
    classified: &[ClassifiedMonowave],
    old_forest: &[Scenario],
    cfg: &NeelyEngineConfig,
) -> ShadowCompactionDiagnostics {
    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(cfg.compaction_timeout_secs);

    let base = build_base_tiling(classified);
    let mut diag = ShadowCompactionDiagnostics {
        engine: "tiling-round-g2.1".to_string(),
        base_tiling_len: base.nodes.len(),
        neutral_bridged: base.bridged,
        leading_neutral_dropped: base.leading_dropped,
        rounds_run: 0,
        tiling_count: 0,
        level_cap_hit: false,
        timed_out: false,
        w1_violations: 0,
        round_branch_cap_hits: 0,
        node_count_by_level: HashMap::new(),
        invariant_violations: InvariantCounters::default().to_map(),
        old_forest_scenarios: old_forest.len(),
        old_forest_matched: 0,
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
    // §5.6 memo:視窗 children canonical 串 → 接受形態(分支間大量共享視窗)
    let mut memo: HashMap<String, Rc<Vec<AcceptedKind>>> = HashMap::new();
    let mut w1_violations = 0usize;

    // 工程護欄(G2.1 review #4):round 內 materialize 的新分支上限。
    // beam 只保 round_beam_size,一輪 materialize 超過 8× beam 的分支不可能
    // 全數存活;G2.1 階梯無 W5/W6,稠密行情下接受率高,無上限會在 timeout
    // 觸發前吃掉數 GB(600-monowave 實測 3.1GB)。截斷偏向先枚舉的視窗,
    // 截斷輪數記 `round_branch_cap_hits` 供 Gate 觀測(A-8 精神)。
    let branch_cap = cfg.round_beam_size.saturating_mul(8).max(8);

    'rounds: for round in 1..=cfg.max_compaction_levels {
        let mut new_tilings: Vec<Tiling> = Vec::new();
        let mut branch_capped = false;

        'gen: for t in &pool {
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
                    let kinds = match memo.get(&memo_key) {
                        Some(k) => Rc::clone(k),
                        None => {
                            let k = Rc::new(try_ladder_g21(window, &mut w1_violations));
                            memo.insert(memo_key, Rc::clone(&k));
                            k
                        }
                    };
                    for kind in kinds.iter() {
                        // 先組 spliced key 查 seen,命中即免 materialize
                        let parent_key = pattern_canonical(
                            &kind.pattern,
                            kind.base,
                            window[0].start_bar,
                            window[w - 1].end_bar,
                            window,
                        );
                        let key = spliced_tiling_key(&t.nodes, s, w, &parent_key);
                        if !seen.insert(key) {
                            continue;
                        }
                        // 新 tiling:視窗換 parent,其餘保留(Rc 指標串,不深拷貝)
                        let parent = make_parent(window, kind, parent_key);
                        let mut nodes: Vec<Rc<CompactionNode>> =
                            Vec::with_capacity(t.nodes.len() - w + 1);
                        nodes.extend(t.nodes[..s].iter().cloned());
                        nodes.push(parent);
                        nodes.extend(t.nodes[s + w..].iter().cloned());
                        new_tilings.push(Tiling { nodes });
                        if new_tilings.len() >= branch_cap {
                            branch_capped = true;
                            break 'gen;
                        }
                    }
                }
            }
        }

        if new_tilings.is_empty() {
            // Round 3 暫停:本輪零聚合(§5.2.d)
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
    diag.w1_violations = w1_violations;

    // 不變量檢查(I1–I5;I6 = 收集去重,下方以 HashSet 保證)
    let tiling_slices: Vec<&[Rc<CompactionNode>]> =
        pool.iter().map(|t| t.nodes.as_slice()).collect();
    let counters = check_invariants(&tiling_slices, base_range);
    diag.invariant_violations = counters.to_map();

    // 凍結收集(I6):全 tilings 中 degree ≥ 1 節點,canonical 去重
    let mut forest_nodes: HashMap<String, Rc<CompactionNode>> = HashMap::new();
    for t in &pool {
        for n in &t.nodes {
            collect_pattern_nodes(n, &mut forest_nodes);
        }
    }
    for n in forest_nodes.values() {
        *diag
            .node_count_by_level
            .entry(n.degree_level.to_string())
            .or_insert(0) += 1;
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
        let base = build_base_tiling(&classified);
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
        let base = build_base_tiling(&classified);
        assert_eq!(base.nodes.len(), 1);
        assert_eq!(base.leading_dropped, 1);
        assert_eq!(base.bridged, 0);
    }

    #[test]
    fn shadow_impulse_chain_aggregates_with_zero_invariant_violations() {
        let classified = impulse_chain();
        let diag = run_shadow(&classified, &[], &cfg());
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
        let diag = run_shadow(&classified, &[], &cfg());
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
        let diag = run_shadow(&classified, &[], &c);
        assert_eq!(diag.rounds_run, 1);
        assert!(diag.level_cap_hit, "round 1 仍有聚合但被上限截斷 → level_cap_hit");
    }

    #[test]
    fn shadow_converges_without_cap_hit_when_no_more_aggregation() {
        let classified = impulse_chain();
        let diag = run_shadow(&classified, &[], &cfg());
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
        let diag = run_shadow(&classified, &[], &c);
        assert!(diag.tiling_count <= 2, "pool 超過 round_beam_size 應被 beam 截斷");
        let total: usize = diag.invariant_violations.values().sum();
        assert_eq!(total, 0);
    }

    #[test]
    fn shadow_too_few_nodes_returns_early() {
        use MonowaveDirection::Up;
        use StructureLabel::Five;
        let classified = vec![cm(0, 5, 100.0, 110.0, Up, &[Five])];
        let diag = run_shadow(&classified, &[], &cfg());
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
        let diag = run_shadow(&classified, &[], &cfg());
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
        let diag = run_shadow(&classified, &[], &c);
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
    }

    #[test]
    fn old_forest_recall_matches_by_bar_range_and_pattern() {
        use crate::output::{
            ComplexityLevel, PostBehavior, PowerRating, RoundState, StructuralFacts,
        };
        let classified = impulse_chain();
        // 造一個舊 forest scenario:日期範圍 = 新引擎 Zigzag [0..3](bar 0..15)
        let old = Scenario {
            id: "old-zz".to_string(),
            wave_tree: WaveNode {
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
        let diag = run_shadow(&classified, &[old], &cfg());
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
        let diag = run_shadow(&classified, &[], &cfg());
        // node_count_by_level 以 canonical 去重;Impulse+2 Zigzag = 3(不因分支重複膨脹)
        assert_eq!(diag.node_count_by_level.get("1"), Some(&3));
    }
}
