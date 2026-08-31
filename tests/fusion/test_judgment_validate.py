"""判讀驗證器 tests(wave_judgment_loop §2 階段 4 / §10 判讀驗證)。"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "src"))

from fusion.judgment import JudgmentValidationError, validate_judgment  # noqa: E402

KEY_A = "Impulse|Five|2026-03-04|2026-08-28[]"
KEY_B = "Zigzag:Single|Three|2026-06-10|2026-08-28[]"
KEY_FRAGILE = "Flat:Common|Three|2026-07-01|2026-08-28[]"

_TRIGGER = {
    "trigger_type": {"PriceBreakBelow": 1052.0},
    "on_trigger": "InvalidateScenario",
    "rule_reference": "Ch5_Essential",
    "neely_page": "p.5",
}


def _dossier() -> dict:
    def cand(key: str, robust) -> dict:
        return {
            "anchor_key": key,
            "evidence": {"robust": robust, "ch6_status": "Pending"},
            "forward": {"invalidation_triggers": [_TRIGGER]},
        }

    return {
        "stock_id": "2330",
        "engine": {"neely": "1.3.0", "assumption_hash": "9f3a"},
        "timeframes": {
            "daily": {
                "snapshot_ref": {"snapshot_date": "2026-08-28", "params_hash": "ph-1"},
                "candidates": [cand(KEY_A, True), cand(KEY_B, True), cand(KEY_FRAGILE, False)],
            },
            "weekly": {"snapshot_ref": None, "candidates": []},
            "monthly": {"snapshot_ref": None, "candidates": []},
        },
    }


def _judgment(**over) -> dict:
    j = {
        "stock_id": "2330",
        "timeframe": "daily",
        "as_of": "2026-08-28",
        "judged_by": "llm:test-model",
        "accepted": [{"anchor_key": KEY_A, "role": "preferred"}],
        "degree_read": "Minor",
        "rationale": {"rule_refs": ["Ch5 pass", "Ch6:Pending"], "notes": "n"},
        "invalidation": {
            "price_levels": [{"level": 1052.0, "meaning": "W4 低點"}],
            "time_limit_bar": "2026-10-15",
        },
        "confidence_class": "single",
    }
    j.update(over)
    return j


class TestAccept:
    def test_single_happy_path_fills_pit_anchoring(self):
        row = validate_judgment(_judgment(), _dossier())
        assert row["snapshot_date"].isoformat() == "2026-08-28"
        assert row["params_hash"] == "ph-1"
        assert row["engine_version"] == "1.3.0"
        assert row["assumption_hash"] == "9f3a"
        assert row["status"] == "active"
        # J2 用記錄的 triggers
        rec = row["invalidation"]["recorded_triggers"]
        assert rec == [{"anchor_key": KEY_A, "triggers": [_TRIGGER]}]

    def test_contested(self):
        j = _judgment(
            accepted=[
                {"anchor_key": KEY_A, "role": "preferred"},
                {"anchor_key": KEY_B, "role": "alternate"},
            ],
            confidence_class="contested",
        )
        row = validate_judgment(j, _dossier())
        assert row["confidence_class"] == "contested"

    def test_no_fit_stores_reason_in_rationale(self):
        j = _judgment(
            accepted=[], confidence_class="no_fit",
            no_fit_reason="Neutrality Aspect-2 未實作,端點切法不成立",
            invalidation={"price_levels": []},
            rationale={"rule_refs": [], "notes": "n"},
        )
        row = validate_judgment(j, _dossier())
        # 缺口表 = confidence_class='no_fit' 查詢;reason 落 rationale
        assert row["rationale"]["no_fit_reason"].startswith("Neutrality")


class TestReject:
    def test_foreign_anchor_key_lists_legal_keys(self):
        j = _judgment(accepted=[{"anchor_key": "Made|Up|2026-01-01|2026-02-01[]", "role": "preferred"}])
        with pytest.raises(JudgmentValidationError) as ei:
            validate_judgment(j, _dossier())
        assert "不在 dossier 候選集內" in str(ei.value)
        assert ei.value.legal_keys == sorted([KEY_A, KEY_B, KEY_FRAGILE])

    def test_single_with_multiple_accepted(self):
        j = _judgment(accepted=[
            {"anchor_key": KEY_A, "role": "preferred"},
            {"anchor_key": KEY_B, "role": "alternate"},
        ])
        with pytest.raises(JudgmentValidationError, match="single 要求 accepted 僅 1 筆"):
            validate_judgment(j, _dossier())

    def test_single_with_fragile_preferred(self):
        j = _judgment(accepted=[{"anchor_key": KEY_FRAGILE, "role": "preferred"}])
        with pytest.raises(JudgmentValidationError, match="robust=false"):
            validate_judgment(j, _dossier())

    def test_no_fit_requires_empty_accepted_and_reason(self):
        with pytest.raises(JudgmentValidationError, match="accepted = \\[\\]"):
            validate_judgment(_judgment(confidence_class="no_fit"), _dossier())
        j = _judgment(accepted=[], confidence_class="no_fit",
                      invalidation={"price_levels": []},
                      rationale={"rule_refs": [], "notes": "n"})
        with pytest.raises(JudgmentValidationError, match="no_fit_reason"):
            validate_judgment(j, _dossier())

    def test_as_of_after_snapshot_rejected(self):
        with pytest.raises(JudgmentValidationError, match="晚於最新 snapshot"):
            validate_judgment(_judgment(as_of="2026-08-29"), _dossier())

    def test_missing_invalidation(self):
        j = _judgment(invalidation={"price_levels": []})
        with pytest.raises(JudgmentValidationError, match="invalidation 不可為空"):
            validate_judgment(j, _dossier())

    def test_empty_rule_refs(self):
        j = _judgment(rationale={"rule_refs": [], "notes": "n"})
        with pytest.raises(JudgmentValidationError, match="rule_refs"):
            validate_judgment(j, _dossier())

    def test_bad_judged_by(self):
        with pytest.raises(JudgmentValidationError, match="judged_by"):
            validate_judgment(_judgment(judged_by="gpt"), _dossier())

    def test_contested_needs_alternate(self):
        j = _judgment(confidence_class="contested")
        with pytest.raises(JudgmentValidationError, match="alternate"):
            validate_judgment(j, _dossier())


class TestSchemaInterlock:
    """skill output-schema.json 與驗證器互鎖(必填集合 / enum 同步)。"""

    def _schema(self) -> dict:
        p = (
            Path(__file__).resolve().parents[2]
            / ".claude/skills/neely-judgment/references/output-schema.json"
        )
        return json.loads(p.read_text(encoding="utf-8"))

    def test_required_and_enums_match_validator(self):
        schema = self._schema()
        assert set(schema["required"]) == {
            "stock_id", "timeframe", "as_of", "judged_by",
            "accepted", "rationale", "invalidation", "confidence_class",
        }
        assert schema["properties"]["timeframe"]["enum"] == ["daily", "weekly", "monthly"]
        assert schema["properties"]["confidence_class"]["enum"] == ["single", "contested", "no_fit"]
        role_enum = schema["properties"]["accepted"]["items"]["properties"]["role"]["enum"]
        assert role_enum == ["preferred", "alternate"]

    def test_happy_fixture_satisfies_schema_requireds(self):
        schema = self._schema()
        j = _judgment()
        for field in schema["required"]:
            assert field in j, field
