"""anchor_key — 判讀錨定的日期樹鍵(m3Spec/wave_judgment_loop.md §6)。

不用 engine canonical(含 start_bar/end_bar,1500-bar 視窗每日滑動即失效);
日期樹鍵對同一形態跨 run 恆等。**格式即 PIT 身分** — 已寫入 wave_judgments
的鍵不可因格式改動而失配,變更格式 = 全表遷移;golden test 凍結格式。

頭部標籤正規化:WaveNode.label 為顯示字(pattern 節點 `"{tag} L{degree}{↑↓·}"`、
葉 `"W{n} :{slot}{↑↓·}"`)— pattern 節點剝 ` L{degree}{arrow}` 尾碼取回
pattern_tag,使「同一子樹以 standalone scenario 出現」與「作為更大候選的
children 出現」產生**同一把鍵**(§J2 判定 3 absorbed 的「同 pattern_tag/dates」
比對依此成立);degree/方向可由樹形與端點導出,不進鍵。
"""

from __future__ import annotations

import hashlib
import re
from datetime import date
from typing import Any

__all__ = [
    "anchor_key",
    "scenario_anchor_key",
    "pattern_tag",
    "forest_anchor_keys",
    "is_strict_subtree",
]

# pattern 節點顯示字尾碼:` L{degree}{arrow}`(arrow ∈ ↑ ↓ ·)
_LABEL_SUFFIX = re.compile(r" L\d+[↑↓·]$")

# children 串超過此長度 → 決定性收斂為 `#<sha256 前 16 hex>`(深樹鍵尺寸護欄:
# 大型聚合的全遞迴鍵可達數十 KB,炸 dossier payload 與 judgments 儲存)。
# 同一函式供 dossier / 驗證 / J2 共用 → 收斂後等值比對、子樹比對全數一致;
# 淺樹(判讀常態)保持人可讀。閾值屬鍵格式(PIT),不可事後調整。
_CHILDREN_HASH_THRESHOLD = 2048


def pattern_tag(pattern_type: Any) -> str | None:
    """Scenario.pattern_type(serde 外部標記 JSON)→ 緊湊 tag,鏡射 Rust
    `round_engine::pattern_tag`("Impulse" / "Diagonal:Ending" /
    "Combination:DoubleThree+…" / "RunningCorrection")。"""
    if isinstance(pattern_type, str):
        return pattern_type
    if isinstance(pattern_type, dict) and pattern_type:
        kind, payload = next(iter(pattern_type.items()))
        if not isinstance(payload, dict):
            return str(kind)
        if kind == "Combination":
            sub_kinds = payload.get("sub_kinds") or []
            return f"Combination:{'+'.join(str(k) for k in sub_kinds)}"
        sub = payload.get("sub_kind")
        return f"{kind}:{sub}" if sub is not None else str(kind)
    return None


def _iso(d: Any) -> str:
    if isinstance(d, date):
        return d.isoformat()
    return str(d)


def _head_tag(node: dict) -> str:
    """節點頭部標籤:pattern 節點剝顯示尾碼(= pattern_tag);葉照 label。"""
    label = str(node.get("label") or "")
    return _LABEL_SUFFIX.sub("", label)


def anchor_key(node: dict, pattern_tag_override: str | None = None) -> str:
    """遞迴日期樹鍵:`{tag}|{base_label}|{start}|{end}[{child},{child}…]`
    (children 串過長 → `[#<hash16>]`,見 _CHILDREN_HASH_THRESHOLD)。"""
    head = "|".join([
        str(pattern_tag_override) if pattern_tag_override else _head_tag(node),
        str(node.get("base_label")),
        _iso(node.get("start")),
        _iso(node.get("end")),
    ])
    kids = ",".join(anchor_key(c) for c in node.get("children") or [])
    if len(kids) > _CHILDREN_HASH_THRESHOLD:
        kids = "#" + hashlib.sha256(kids.encode("utf-8")).hexdigest()[:16]
    return f"{head}[{kids}]"


def scenario_anchor_key(scenario: dict) -> str:
    """Scenario dict → anchor_key(頂層優先用 pattern_type 推 tag;
    與 label 剝尾碼結果同值,pattern_type 缺失時 fallback label)。"""
    tree = scenario.get("wave_tree") or {}
    return anchor_key(tree, pattern_tag(scenario.get("pattern_type")))


def forest_anchor_keys(forest: list[dict]) -> set[str]:
    """forest 全體 scenario 的 anchor_key 集合(J2 命中判定用)。"""
    return {scenario_anchor_key(s) for s in forest if isinstance(s, dict)}


def _collect_subtree_keys(node: dict, out: set[str]) -> None:
    for c in node.get("children") or []:
        out.add(anchor_key(c))
        _collect_subtree_keys(c, out)


def is_strict_subtree(key: str, candidate_scenario: dict) -> bool:
    """`key` 是否為候選 wave_tree 的**嚴格**子樹(不含整棵樹本身)。

    結構遞迴收集全部子樹鍵後比對(不靠字串包含);整棵樹的鍵不入集合,
    嚴格語意自動成立。
    """
    tree = candidate_scenario.get("wave_tree") or {}
    subs: set[str] = set()
    _collect_subtree_keys(tree, subs)
    return key in subs
