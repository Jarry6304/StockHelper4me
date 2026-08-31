"""POST /judgments tests(首個寫端點;TestClient + mock,不打真 PG)。

append-only trigger 本體屬 DB 側 → 本機 runbook probe;此處驗 API 契約:
201 寫入 / 422 拒絕(含 legal_anchor_keys)。
"""

from __future__ import annotations

import sys
from pathlib import Path
from unittest.mock import MagicMock

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "src"))

from fastapi.testclient import TestClient  # noqa: E402

from web_api.app import app  # noqa: E402
from web_api.pool import db_conn  # noqa: E402

KEY_A = "Impulse|Five|2026-03-04|2026-08-28[]"


def _dossier() -> dict:
    return {
        "stock_id": "2330",
        "engine": {"neely": "1.3.0", "assumption_hash": "9f3a"},
        "timeframes": {
            "daily": {
                "snapshot_ref": {"snapshot_date": "2026-08-28", "params_hash": "ph-1"},
                "candidates": [{
                    "anchor_key": KEY_A,
                    "evidence": {"robust": True},
                    "forward": {"invalidation_triggers": []},
                }],
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
        "judged_by": "human",
        "accepted": [{"anchor_key": KEY_A, "role": "preferred"}],
        "rationale": {"rule_refs": ["Ch5 pass"], "notes": "n"},
        "invalidation": {"price_levels": [{"level": 1052.0, "meaning": "W4 低點"}]},
        "confidence_class": "single",
    }
    j.update(over)
    return j


def _client(monkeypatch, *, insert_id: int = 42) -> TestClient:
    import fusion.judgment as judgment_pkg
    import web_api.routers.judgments as router_mod

    # router 於函式內 from fusion.judgment import … → patch 套件屬性
    monkeypatch.setattr(judgment_pkg, "build_dossier", lambda *a, **k: _dossier())
    monkeypatch.setattr(judgment_pkg, "insert_judgment", lambda conn, row: insert_id)
    _ = router_mod  # 明確化 patch 對象關係
    app.dependency_overrides[db_conn] = lambda: iter([MagicMock()])
    return TestClient(app)


def teardown_function():
    app.dependency_overrides.clear()


def test_post_valid_judgment_201(monkeypatch):
    c = _client(monkeypatch)
    r = c.post("/judgments", json=_judgment())
    assert r.status_code == 201, r.text
    body = r.json()
    assert body["id"] == 42
    assert body["confidence_class"] == "single"


def test_post_foreign_key_422_with_legal_keys(monkeypatch):
    c = _client(monkeypatch)
    bad = _judgment(accepted=[{"anchor_key": "Made|Up|x|y[]", "role": "preferred"}])
    r = c.post("/judgments", json=bad)
    assert r.status_code == 422
    detail = r.json()["detail"]
    assert "不在 dossier 候選集內" in detail["error"]
    assert detail["legal_anchor_keys"] == [KEY_A]


def test_post_bad_as_of_422(monkeypatch):
    c = _client(monkeypatch)
    r = c.post("/judgments", json=_judgment(as_of="not-a-date"))
    assert r.status_code == 422
