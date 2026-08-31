"""J2 錨定 diff 測試(m3Spec/wave_judgment_loop.md §6)。

全狀態矩陣:intact / invalidated(PriceBreakBelow / Above / TimeExceeds /
time_limit_bar)/ absorbed / vanished(engine_changed / engine_regression)
+ 多錨最差優先 + 狀態列內容拷貝。全 mock,不碰 PG。
"""

from __future__ import annotations

import sys
from datetime import date, timedelta
from pathlib import Path
from unittest.mock import patch

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for _p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if _p not in sys.path:
        sys.path.insert(0, _p)

from fusion.judgment.anchor_key import anchor_key, scenario_anchor_key  # noqa: E402
from fusion.judgment.diff import run_anchor_diff  # noqa: E402


AS_OF = date(2026, 8, 31)
JUDGED_AT = date(2026, 8, 20)


# ────────────────────────────────────────────────────────────
# Fixtures
# ────────────────────────────────────────────────────────────


def _tree(pattern="Impulse", start="2026-01-05", end="2026-08-14", children=None):
    return {
        "label": f"{pattern} L5↑",
        "base_label": ":5",
        "start": start,
        "end": end,
        "children": children or [],
    }


def _scenario(pattern="Impulse", start="2026-01-05", end="2026-08-14", children=None):
    return {
        "pattern_type": pattern,
        "wave_tree": _tree(pattern, start, end, children),
    }


def _judgment(
    anchors: list[str],
    *,
    jid=101,
    triggers_by_anchor: dict[str, list] | None = None,
    time_limit_bar: str | None = None,
    engine_version="1.3.0",
    assumption_hash="abcd1234abcd1234",
):
    invalidation: dict = {}
    if triggers_by_anchor:
        invalidation["recorded_triggers"] = [
            {"anchor_key": k, "triggers": v} for k, v in triggers_by_anchor.items()
        ]
    if time_limit_bar:
        invalidation["time_limit_bar"] = time_limit_bar
    return {
        "id": jid,
        "stock_id": "2330",
        "timeframe": "daily",
        "as_of": JUDGED_AT,
        "judged_by": "human:jarry",
        "snapshot_date": JUDGED_AT,
        "params_hash": "ph",
        "engine_version": engine_version,
        "assumption_hash": assumption_hash,
        "accepted": [
            {"role": "preferred" if i == 0 else "alternate", "anchor_key": a}
            for i, a in enumerate(anchors)
        ],
        "degree_read": None,
        "rationale": {"summary": "test"},
        "invalidation": invalidation,
        "confidence_class": "single",
        "status": "active",
        "supersedes_id": None,
        "diff_detail": None,
    }


def _snapshot_row(forest, *, source_version="1.3.0", assumption_hash="abcd1234abcd1234"):
    return {
        "timeframe": "daily",
        "source_version": source_version,
        "snapshot": {"scenario_forest": forest, "assumption_hash": assumption_hash},
    }


def _bars(*specs):
    """specs: (date, low, high)"""
    return [{"date": d, "low": lo, "high": hi} for d, lo, hi in specs]


def _run(judgments, row, bars=None, *, stock_ids=None):
    inserted: list[dict] = []
    with patch("fusion.judgment.diff.fetch_all_active_judgments",
               return_value=judgments), \
         patch("fusion.judgment.diff.fetch_structural_latest",
               return_value=[row] if row else []), \
         patch("fusion.judgment.diff._fetch_bars_since",
               return_value=bars or []), \
         patch("fusion.judgment.diff.insert_judgment",
               side_effect=lambda conn, r: inserted.append(r) or 999):
        summary = run_anchor_diff(None, stock_ids=stock_ids, as_of=AS_OF)
    return summary, inserted


# ────────────────────────────────────────────────────────────
# 判定 1:intact(命中不新增列)
# ────────────────────────────────────────────────────────────


class TestIntact:
    def test_anchor_hit_no_row(self):
        scn = _scenario()
        j = _judgment([scenario_anchor_key(scn)])
        summary, inserted = _run([j], _snapshot_row([scn]))
        assert summary == {
            "checked": 1, "intact": 1, "invalidated": 0,
            "absorbed": 0, "vanished": 0, "engine_regression": 0,
        }
        assert inserted == []

    def test_no_fit_judgment_is_noop(self):
        # accepted=[] 無錨 — J2 無事可比
        j = _judgment([])
        summary, inserted = _run([j], _snapshot_row([_scenario()]))
        assert summary["intact"] == 1
        assert inserted == []

    def test_stock_ids_filter(self):
        scn = _scenario()
        j_other = _judgment([scenario_anchor_key(scn)], jid=7)
        j_other["stock_id"] = "2317"
        summary, inserted = _run([j_other], _snapshot_row([scn]),
                                 stock_ids=["2330"])
        assert summary["checked"] == 0


# ────────────────────────────────────────────────────────────
# 判定 2:invalidated(記錄的 triggers + time_limit_bar)
# ────────────────────────────────────────────────────────────


class TestInvalidated:
    def test_price_break_below(self):
        scn = _scenario()
        anchor = scenario_anchor_key(scn)
        j = _judgment(
            [anchor],
            triggers_by_anchor={anchor: [
                {"trigger_type": {"PriceBreakBelow": 100.0},
                 "rule_reference": "R2:wave2-origin"},
            ]},
        )
        bars = _bars((date(2026, 8, 25), 99.5, 103.0))
        summary, inserted = _run([j], _snapshot_row([]), bars)
        assert summary["invalidated"] == 1
        assert len(inserted) == 1
        row = inserted[0]
        assert row["status"] == "invalidated"
        assert row["supersedes_id"] == 101
        assert row["diff_detail"] == {
            "anchor_key": anchor, "rule": "R2:wave2-origin",
            "bar": "2026-08-25", "price": 99.5,
        }

    def test_price_break_above(self):
        scn = _scenario()
        anchor = scenario_anchor_key(scn)
        j = _judgment(
            [anchor],
            triggers_by_anchor={anchor: [
                {"trigger_type": {"PriceBreakAbove": 110.0},
                 "rule_reference": "R7:overlap"},
            ]},
        )
        bars = _bars((date(2026, 8, 26), 105.0, 112.0))
        summary, inserted = _run([j], _snapshot_row([]), bars)
        assert summary["invalidated"] == 1
        assert inserted[0]["diff_detail"]["price"] == 112.0

    def test_no_breach_falls_through_to_vanished(self):
        scn = _scenario()
        anchor = scenario_anchor_key(scn)
        j = _judgment(
            [anchor],
            triggers_by_anchor={anchor: [
                {"trigger_type": {"PriceBreakBelow": 100.0}, "rule_reference": "R2"},
            ]},
        )
        bars = _bars((date(2026, 8, 25), 101.0, 103.0))  # 未破線
        summary, inserted = _run([j], _snapshot_row([]), bars)
        assert summary["invalidated"] == 0
        assert summary["vanished"] == 1

    def test_time_exceeds_trigger(self):
        scn = _scenario()
        anchor = scenario_anchor_key(scn)
        j = _judgment(
            [anchor],
            triggers_by_anchor={anchor: [
                {"trigger_type": {"TimeExceeds": "2026-08-28"},
                 "rule_reference": "T1:duration"},
            ]},
        )
        summary, inserted = _run([j], _snapshot_row([]), [])
        assert summary["invalidated"] == 1
        assert inserted[0]["diff_detail"]["bar"] == "2026-08-28"
        assert inserted[0]["diff_detail"]["price"] is None

    def test_judgment_time_limit_bar(self):
        scn = _scenario()
        anchor = scenario_anchor_key(scn)
        j = _judgment([anchor], time_limit_bar="2026-08-25")
        summary, inserted = _run([j], _snapshot_row([]), [])
        assert summary["invalidated"] == 1
        assert inserted[0]["diff_detail"]["rule"] == "judgment.time_limit_bar"

    def test_time_limit_not_yet_reached(self):
        scn = _scenario()
        anchor = scenario_anchor_key(scn)
        j = _judgment([anchor], time_limit_bar="2026-09-30")
        summary, _ = _run([j], _snapshot_row([]), [])
        assert summary["invalidated"] == 0
        assert summary["vanished"] == 1


# ────────────────────────────────────────────────────────────
# 判定 3:absorbed(嚴格子樹,結構遞迴非字串包含)
# ────────────────────────────────────────────────────────────


class TestAbsorbed:
    def test_anchor_is_strict_subtree(self):
        child = _tree("Impulse", "2026-01-05", "2026-04-10")
        parent = _scenario(
            "Diagonal", "2026-01-05", "2026-08-14",
            children=[child, _tree("Flat", "2026-04-10", "2026-08-14")],
        )
        anchor = anchor_key(child)
        j = _judgment([anchor])
        summary, inserted = _run([j], _snapshot_row([parent]), [])
        assert summary["absorbed"] == 1
        assert inserted[0]["status"] == "absorbed"
        assert inserted[0]["diff_detail"] == {
            "anchor_key": anchor,
            "parent_anchor_key": scenario_anchor_key(parent),
        }

    def test_whole_tree_key_is_not_absorbed(self):
        # 整棵樹的鍵不算子樹(嚴格語意)→ vanished
        scn = _scenario()
        j = _judgment([scenario_anchor_key(scn) + "x"])  # 不等值也非子樹
        summary, _ = _run([j], _snapshot_row([scn]), [])
        assert summary["absorbed"] == 0
        assert summary["vanished"] == 1


# ────────────────────────────────────────────────────────────
# 判定 4:vanished(engine_changed / engine_regression)
# ────────────────────────────────────────────────────────────


class TestVanished:
    def test_engine_changed_by_version(self):
        j = _judgment(["Impulse|:5|2026-01-05|2026-08-14[]"],
                      engine_version="1.2.0")
        summary, inserted = _run([j], _snapshot_row([], source_version="1.3.0"), [])
        assert summary["vanished"] == 1
        assert summary["engine_regression"] == 0
        assert inserted[0]["diff_detail"]["cause"] == "engine_changed"

    def test_engine_changed_by_assumption_hash(self):
        j = _judgment(["Impulse|:5|2026-01-05|2026-08-14[]"],
                      assumption_hash="oldhash0oldhash0")
        summary, inserted = _run(
            [j], _snapshot_row([], assumption_hash="newhash0newhash0"), [])
        assert inserted[0]["diff_detail"]["cause"] == "engine_changed"

    def test_engine_regression_alerts(self):
        # 同 engine_version + 同 assumption_hash 下 anchor 消失 = 引擎 bug
        j = _judgment(["Impulse|:5|2026-01-05|2026-08-14[]"])
        summary, inserted = _run([j], _snapshot_row([]), [])
        assert summary["vanished"] == 1
        assert summary["engine_regression"] == 1
        assert inserted[0]["diff_detail"]["cause"] == "engine_regression"

    def test_missing_snapshot_row_is_engine_changed(self):
        # 無最新 snapshot(source_version=None ≠ 判讀時)→ engine_changed
        j = _judgment(["Impulse|:5|2026-01-05|2026-08-14[]"])
        summary, inserted = _run([j], None, [])
        assert summary["vanished"] == 1
        assert inserted[0]["diff_detail"]["cause"] == "engine_changed"


# ────────────────────────────────────────────────────────────
# 判定 5:多錨最差優先(invalidated > vanished > absorbed > intact)
# ────────────────────────────────────────────────────────────


class TestMultiAnchorWorstWins:
    def test_intact_plus_invalidated(self):
        hit = _scenario()
        anchor_hit = scenario_anchor_key(hit)
        anchor_miss = "Flat|:3|2026-02-02|2026-05-05[]"
        j = _judgment(
            [anchor_hit, anchor_miss],
            triggers_by_anchor={anchor_miss: [
                {"trigger_type": {"PriceBreakBelow": 100.0}, "rule_reference": "R2"},
            ]},
        )
        bars = _bars((date(2026, 8, 25), 99.0, 103.0))
        summary, inserted = _run([j], _snapshot_row([hit]), bars)
        assert summary["invalidated"] == 1
        assert summary["intact"] == 0
        assert inserted[0]["status"] == "invalidated"

    def test_absorbed_plus_vanished_takes_vanished(self):
        child = _tree("Impulse", "2026-01-05", "2026-04-10")
        parent = _scenario("Diagonal", "2026-01-05", "2026-08-14", children=[child])
        j = _judgment([anchor_key(child), "Flat|:3|2026-02-02|2026-05-05[]"])
        summary, inserted = _run([j], _snapshot_row([parent]), [])
        assert summary["vanished"] == 1
        assert summary["absorbed"] == 0
        assert inserted[0]["status"] == "vanished"

    def test_all_intact_multi_anchor(self):
        s1, s2 = _scenario(), _scenario("Flat", "2026-02-02", "2026-05-05")
        j = _judgment([scenario_anchor_key(s1), scenario_anchor_key(s2)])
        summary, inserted = _run([j], _snapshot_row([s1, s2]), [])
        assert summary["intact"] == 1
        assert inserted == []


# ────────────────────────────────────────────────────────────
# 狀態列內容(拷貝原列;judged_by 沿用 — J2 非判讀者)
# ────────────────────────────────────────────────────────────


class TestStatusRowContent:
    def test_copies_content_fields_and_links_chain(self):
        j = _judgment(["Impulse|:5|2026-01-05|2026-08-14[]"], jid=55)
        _, inserted = _run([j], _snapshot_row([]), [])
        row = inserted[0]
        assert row["supersedes_id"] == 55
        assert row["judged_by"] == "human:jarry"  # 沿用原列
        assert row["stock_id"] == "2330"
        assert row["timeframe"] == "daily"
        assert row["as_of"] == JUDGED_AT
        assert row["accepted"] == j["accepted"]
        assert row["rationale"] == {"summary": "test"}
        assert row["confidence_class"] == "single"
        assert row["engine_version"] == "1.3.0"
        # id / created_at 不拷貝(DB 產生)
        assert "id" not in row
