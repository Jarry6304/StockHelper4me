"""Tests for SilverOrchestrator builder filter(v4.28+ --builder flag)。

對齊 cross_cores/orchestrator.py 既有 `--builder` pattern。
"""

from __future__ import annotations

import asyncio
import sys
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

from silver.orchestrator import PHASE_GROUPS, SilverOrchestrator  # noqa: E402


class _FakeModule:
    """Minimal silver builder module mock with .run()。"""
    def __init__(self, name):
        self.name = name
        self.call_count = 0

    def run(self, db, stock_ids=None, full_rebuild=False):
        self.call_count += 1
        return {
            "name": self.name, "rows_read": 0, "rows_written": 0, "elapsed_ms": 1,
        }


def _make_fake_builders(*names) -> dict[str, _FakeModule]:
    return {n: _FakeModule(n) for n in names}


def _run(orch, **kwargs):
    return asyncio.run(orch.run(phases=["7a"], **kwargs))


# ─── _filter_builders ────────────────────────────────────────────────────────


def test_filter_builders_none_returns_all_phase_builders():
    """builders=None → 該 phase 全 builder。"""
    out = SilverOrchestrator._filter_builders("7a", None)
    assert out == PHASE_GROUPS["7a"]


def test_filter_builders_empty_returns_all():
    """builders=[] (empty list) → fall back to all(same as None)。"""
    out = SilverOrchestrator._filter_builders("7a", [])
    assert out == PHASE_GROUPS["7a"]


def test_filter_builders_subset_preserves_phase_order():
    """指定 subset → 保留 PHASE_GROUPS 順序。"""
    # PHASE_GROUPS["7a"] 順序:institutional, margin, foreign_holding, ...,
    # monthly_revenue 在中間。指定亂序 → 仍依 PHASE_GROUPS 順序回。
    out = SilverOrchestrator._filter_builders(
        "7a", ["monthly_revenue", "institutional"]
    )
    assert out == ["institutional", "monthly_revenue"]


def test_filter_builders_single_target():
    out = SilverOrchestrator._filter_builders("7a", ["monthly_revenue"])
    assert out == ["monthly_revenue"]


def test_filter_builders_unknown_raises_valueerror():
    with pytest.raises(ValueError, match="未知 silver builder.*nonexistent"):
        SilverOrchestrator._filter_builders("7a", ["nonexistent"])


def test_filter_builders_partial_unknown_still_raises():
    """有效 + 無效混搭 → 仍 raise(防 user typo 部分被靜默吃)。"""
    with pytest.raises(ValueError, match="未知 silver builder"):
        SilverOrchestrator._filter_builders(
            "7a", ["monthly_revenue", "typo_builder"]
        )


def test_filter_builders_7b_phase():
    """7b phase 自己 subset 校驗(避免 cross-phase 誤套)。"""
    out = SilverOrchestrator._filter_builders("7b", None)
    assert out == PHASE_GROUPS["7b"]
    # 7a 的 builder 名不該被 7b filter 接受
    with pytest.raises(ValueError, match="未知 silver builder"):
        SilverOrchestrator._filter_builders("7b", ["institutional"])


# ─── Orchestrator.run() integration with --builder filter ───────────────────


def test_run_with_builder_filter_only_invokes_selected(monkeypatch):
    """run(builders=['monthly_revenue']) → 只有 monthly_revenue 跑,其餘 skip。"""
    fakes = _make_fake_builders(*PHASE_GROUPS["7a"])
    monkeypatch.setattr(
        "silver.orchestrator.BUILDERS", fakes,
    )

    orch = SilverOrchestrator(db=MagicMock())
    result = _run(orch, full_rebuild=True, builders=["monthly_revenue"])

    # monthly_revenue called once
    assert fakes["monthly_revenue"].call_count == 1
    # 其餘 builder 0 call
    for name, mod in fakes.items():
        if name == "monthly_revenue":
            continue
        assert mod.call_count == 0, f"{name} 不該被 call"

    # results dict 只含 monthly_revenue
    phase_result = result["results"]["7a"]
    assert set(phase_result.keys()) == {"monthly_revenue"}


def test_run_without_filter_invokes_all(monkeypatch):
    """run(builders=None) → 全 builder 跑(預設行為,backward compat)。"""
    fakes = _make_fake_builders(*PHASE_GROUPS["7a"])
    monkeypatch.setattr("silver.orchestrator.BUILDERS", fakes)

    orch = SilverOrchestrator(db=MagicMock())
    result = _run(orch, full_rebuild=True)  # builders 不傳

    for name, mod in fakes.items():
        assert mod.call_count == 1, f"{name} 應被 call 一次"

    phase_result = result["results"]["7a"]
    assert set(phase_result.keys()) == set(PHASE_GROUPS["7a"])


def test_run_with_unknown_builder_raises_before_db_work(monkeypatch):
    """Unknown builder → raise ValueError,db 不被觸發。"""
    fakes = _make_fake_builders(*PHASE_GROUPS["7a"])
    monkeypatch.setattr("silver.orchestrator.BUILDERS", fakes)

    orch = SilverOrchestrator(db=MagicMock())
    with pytest.raises(ValueError, match="未知 silver builder"):
        _run(orch, full_rebuild=True, builders=["typo_name"])

    # 0 builder 被 call(早 raise)
    for mod in fakes.values():
        assert mod.call_count == 0


def test_run_filter_applies_in_incremental_path(monkeypatch):
    """v4.15 incremental 路徑(full_rebuild=False)也套 --builder filter。"""
    fakes = _make_fake_builders(*PHASE_GROUPS["7a"])
    monkeypatch.setattr("silver.orchestrator.BUILDERS", fakes)

    # incremental 路徑會 call set_incremental_window / clear_incremental_window
    with patch("silver.orchestrator.set_incremental_window"), \
         patch("silver.orchestrator.clear_incremental_window"):
        orch = SilverOrchestrator(db=MagicMock())
        _run(orch, full_rebuild=False, builders=["monthly_revenue"])

    assert fakes["monthly_revenue"].call_count == 1
    for name, mod in fakes.items():
        if name == "monthly_revenue":
            continue
        assert mod.call_count == 0
