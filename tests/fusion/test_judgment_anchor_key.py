"""anchor_key golden tests(wave_judgment_loop §6)。

**格式即 PIT 身分**:golden 字串一旦寫入 wave_judgments 即不可漂移;
本檔斷言失敗 = 格式變更 = 需要全表遷移拍板,不是改 test。
"""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "src"))

from fusion.judgment import (  # noqa: E402
    anchor_key,
    forest_anchor_keys,
    is_strict_subtree,
    pattern_tag,
    scenario_anchor_key,
)


def _leaf(n: int, slot: str, start: str, end: str) -> dict:
    return {
        "label": f"W{n} :{slot}↑",
        "base_label": "Five" if slot == "5" else "Three",
        "start": start,
        "end": end,
        "children": [],
    }


def _zigzag_tree(start="2026-03-04", mid1="2026-05-02", mid2="2026-06-10", end="2026-08-21") -> dict:
    return {
        "label": "Zigzag:Single L1↑",
        "base_label": "Three",
        "start": start,
        "end": end,
        "degree_level": 1,
        "children": [
            _leaf(1, "5", start, mid1),
            _leaf(2, "3", mid1, mid2),
            _leaf(3, "5", mid2, end),
        ],
    }


def _zigzag_scenario(**kw) -> dict:
    return {
        "id": "cmp1-b0-b84-Zigzag:Single",
        "pattern_type": {"Zigzag": {"sub_kind": "Single"}},
        "wave_tree": _zigzag_tree(**kw),
    }


GOLDEN_ZIGZAG_KEY = (
    "Zigzag:Single|Three|2026-03-04|2026-08-21["
    "W1 :5↑|Five|2026-03-04|2026-05-02[],"
    "W2 :3↑|Three|2026-05-02|2026-06-10[],"
    "W3 :5↑|Five|2026-06-10|2026-08-21[]"
    "]"
)


class TestGolden:
    def test_zigzag_golden_string(self):
        assert scenario_anchor_key(_zigzag_scenario()) == GOLDEN_ZIGZAG_KEY

    def test_pattern_tag_variants(self):
        assert pattern_tag("Impulse") == "Impulse"
        assert pattern_tag("RunningCorrection") == "RunningCorrection"
        assert pattern_tag({"Zigzag": {"sub_kind": "Single"}}) == "Zigzag:Single"
        assert pattern_tag({"Diagonal": {"sub_kind": "Ending"}}) == "Diagonal:Ending"
        assert (
            pattern_tag({"Combination": {"sub_kinds": ["DoubleThree", "TripleThree"]}})
            == "Combination:DoubleThree+TripleThree"
        )
        assert pattern_tag(None) is None


class TestStability:
    def test_window_slide_keeps_key(self):
        """視窗滑動 1 bar:日期不變 → 鍵不變(engine canonical 的 bar index 才會漂)。"""
        assert scenario_anchor_key(_zigzag_scenario()) == scenario_anchor_key(_zigzag_scenario())

    def test_label_display_suffix_stripped_matches_pattern_tag(self):
        """頂層 pattern_tag 版與 label 剝尾碼版同鍵(absorbed 比對前提)。"""
        s = _zigzag_scenario()
        no_pt = dict(s)
        no_pt["pattern_type"] = None  # fallback 走 label 剝尾碼
        assert scenario_anchor_key(no_pt) == scenario_anchor_key(s)

    def test_different_end_different_key(self):
        assert scenario_anchor_key(_zigzag_scenario()) != scenario_anchor_key(
            _zigzag_scenario(end="2026-08-22")
        )


class TestSubtree:
    def test_standalone_key_found_as_strict_subtree_of_parent(self):
        """standalone 判讀過的 Zigzag 被更大 Impulse 收編為 child →
        absorbed 判定要能以同一把鍵命中(§J2 判定 3)。"""
        zz_key = scenario_anchor_key(_zigzag_scenario())
        parent = {
            "id": "cmp2",
            "pattern_type": "Impulse",
            "wave_tree": {
                "label": "Impulse L2↑",
                "base_label": "Five",
                "start": "2026-03-04",
                "end": "2026-12-01",
                "degree_level": 2,
                "children": [
                    _zigzag_tree(),
                    _leaf(2, "3", "2026-08-21", "2026-09-15"),
                    _leaf(3, "5", "2026-09-15", "2026-12-01"),
                ],
            },
        }
        assert is_strict_subtree(zz_key, parent)
        # 整棵樹本身不算(嚴格子樹)
        assert not is_strict_subtree(scenario_anchor_key(parent), parent)

    def test_forest_keys(self):
        forest = [_zigzag_scenario(), _zigzag_scenario(end="2026-08-22")]
        keys = forest_anchor_keys(forest)
        assert len(keys) == 2
        assert GOLDEN_ZIGZAG_KEY in keys


class TestLeafGrammar:
    def test_empty_children_bracket(self):
        leaf = _leaf(1, "5", "2026-01-01", "2026-02-01")
        assert anchor_key(leaf) == "W1 :5↑|Five|2026-01-01|2026-02-01[]"
