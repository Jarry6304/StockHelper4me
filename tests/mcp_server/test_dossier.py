"""neely_forecast(dossier)tool-level tests(wave_judgment_loop §4/§10;mock conn)。"""

from __future__ import annotations

import json
import sys
from datetime import date
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "src"))

from mcp_server.tools import data as data_tools  # noqa: E402

AS_OF = "2026-08-28"


class _DispatchCursor:
    """SQL 文字路由 canned rows(對齊 tests/web_api 的 _DispatchCursor 精神)。"""

    def __init__(self, routes: dict[str, list[dict]]):
        self._routes = routes
        self._rows: list[dict] = []

    def execute(self, sql: str, params=None):
        for needle, rows in self._routes.items():
            if needle in sql:
                self._rows = list(rows)
                return
        self._rows = []

    def fetchall(self):
        return list(self._rows)

    def fetchone(self):
        return self._rows[0] if self._rows else None

    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False


class DossierFakeConn:
    def __init__(self, routes: dict[str, list[dict]]):
        self._routes = routes
        self.closed = False

    def cursor(self):
        return _DispatchCursor(self._routes)

    def close(self):
        self.closed = True


def _deep_tree(depth: int, start: str, end: str) -> dict:
    node = {
        "label": "W1 :5↑", "base_label": "Five",
        "start": start, "end": end, "degree_level": 0, "children": [],
    }
    for lvl in range(1, depth + 1):
        node = {
            "label": f"Impulse L{lvl}↑", "base_label": "Five",
            "start": start, "end": end, "degree_level": lvl,
            "children": [node, node, node, node, node],
        }
    return node


def make_neely_routes(
    *,
    n_candidates: int = 12,
    tree_depth: int = 3,
    weekly_monthly_candidates: int | None = None,
) -> dict[str, list[dict]]:
    """深 wave_tree × 滿 candidates 的 worst-case routes(payload budget 用)。

    `weekly_monthly_candidates`:weekly/monthly 的候選數(None = 同 daily;
    現實 worst 給 1 — 資料窗小,候選常 0-1)。"""
    forest = []
    for i in range(n_candidates):
        forest.append({
            "id": f"cmp{i}",
            "pattern_type": "Impulse",
            "structure_label": f"Impulse L{tree_depth} [:5 :3 :5 :3 :5] #{i}",
            "initial_direction": "Up",
            "wave_tree": {
                **_deep_tree(tree_depth, "2026-03-04", "2026-08-28"),
                "degree_level": tree_depth,
            },
            "passed_rules": ["Ch5_Essential", "Ch6_Impulse_Stage1"],
            "deferred_rules": ["Ch9_TimeRule"],
            "rules_passed_count": 2,
            "advisory_findings": [
                {"rule_id": "Ch9_TimeRule", "severity": "Info", "message": "m" * 60},
            ],
            "complexity_level": "Simple",
            "triplexity_detected": False,
            "power_rating": "Bullish",
            "post_pattern_behavior": "Unconstrained",
            "max_retracement": 0.8,
            "invalidation_triggers": [
                {"trigger_type": {"PriceBreakBelow": 1052.0},
                 "on_trigger": "InvalidateScenario",
                 "rule_reference": "Ch5_Essential", "neely_page": "p.5-1"},
            ],
            "expected_fib_zones": [
                {"label": "W5 ext", "low": 1200.0, "high": 1300.0, "source_ratio": 1.618},
            ],
            "awaiting_l_label": False,
            "ch6_status": "Pending",
            "robust": True,
        })
    wm = n_candidates if weekly_monthly_candidates is None else weekly_monthly_candidates

    def _snapshot(count: int) -> dict:
        return dict(base_snapshot, scenario_forest=forest[:count])

    base_snapshot = {
        "scenario_forest": forest,
        "monowave_series": [
            {"start_date": "2026-01-05", "end_date": "2026-03-04",
             "start_price": 900.0, "end_price": 1000.0, "direction": "Up",
             "bar_indices": [0, 40]},
            {"start_date": "2026-03-04", "end_date": "2026-08-28",
             "start_price": 1000.0, "end_price": 1180.0, "direction": "Up",
             "bar_indices": [40, 140]},
        ],
        "data_range": {"start": "2026-01-05", "end": "2026-08-28"},
        "assumptions": [
            {"name": "REVERSAL_ATR_MULTIPLIER", "value": 0.5, "source": "Engineering"},
        ] * 8,
        "assumption_hash": "9f3a9f3a9f3a9f3a",
        "live_edge_ambiguity": {"count": n_candidates, "kinds": ["Impulse"], "degree_level": tree_depth},
    }
    neely_rows = [{
        "stock_id": "2330", "snapshot_date": date(2026, 8, 28), "timeframe": tf,
        "core_name": "neely_core", "source_version": "1.3.0",
        "snapshot": _snapshot(n_candidates if tf == "daily" else wm), "params_hash": "ph-1",
    } for tf in ("daily", "weekly", "monthly")]
    return {
        "FROM structural_snapshots": neely_rows,
        "FROM traditional_snapshots": [],
        "FROM wave_judgments": [],
    }


def _patch_tool(monkeypatch, routes: dict[str, list[dict]]):
    # patch 於 _db 層(fusion.raw 為 PEP-562 轉發;setattr 到 façade 會物化
    # 真屬性、之後蓋掉轉發 — 污染他檔 test)
    import fusion.raw._db as raw_db
    from mcp_server import _price

    monkeypatch.setattr(raw_db, "get_connection", lambda *a, **k: DossierFakeConn(routes))
    monkeypatch.setattr(
        _price, "fetch_latest_close_for_tool",
        lambda *a, **k: {"close": 1180.0, "date": AS_OF},
    )


class TestDossierContract:
    def test_deleted_keys_absent_and_dossier_keys_present(self, monkeypatch):
        _patch_tool(monkeypatch, make_neely_routes(n_candidates=3, tree_depth=1))
        out = data_tools.neely_forecast("2330", AS_OF)
        for k in ("primary_scenario", "scenario_count", "scenario_staleness"):
            assert k not in out, k
        for k in ("engine", "assumptions", "timeframes", "cross_timeframe",
                  "active_judgment", "quality_caveat"):
            assert k in out, k
        daily = out["timeframes"]["daily"]
        assert len(daily["candidates"]) == 3
        c = daily["candidates"][0]
        assert set(c) >= {"id", "anchor_key", "pattern_type", "span", "age_bars",
                          "wave_tree", "evidence", "forward", "is_invalidated"}
        assert out["current_price"] == 1180.0

    def test_conn_closed(self, monkeypatch):
        import fusion.raw._db as raw_db
        from mcp_server import _price

        conn = DossierFakeConn(make_neely_routes(n_candidates=1, tree_depth=1))
        monkeypatch.setattr(raw_db, "get_connection", lambda *a, **k: conn)
        monkeypatch.setattr(
            _price, "fetch_latest_close_for_tool", lambda *a, **k: None,
        )
        data_tools.neely_forecast("2330", AS_OF)
        assert conn.closed


class TestDossierPayload:
    """payload 政策對齊 verify_mcp_toolkit(soft 50KB / hard 1MB);
    舊 5K-token 釘屬已退役的 compact 回應,不適用 dossier。"""

    def test_realistic_worst_under_soft_50kb(self, monkeypatch):
        """p95 級現實 worst(daily 6 候選 × depth-2 樹;weekly/monthly 各 1)。"""
        _patch_tool(monkeypatch, make_neely_routes(
            n_candidates=6, tree_depth=2, weekly_monthly_candidates=1,
        ))
        out = data_tools.neely_forecast("2330", AS_OF)
        kb = len(json.dumps(out, default=str)) / 1024
        # fixture 全 5-ary 全 max-degree,比 p95 production 更兇 → 釘 64KB;
        # production soft 50KB 由 verify_mcp_toolkit 對真資料把關
        assert kb < 64, f"neely_forecast dossier realistic payload {kb:.0f}KB > 64KB"

    def test_pathological_worst_under_hard_1mb(self, monkeypatch):
        """cap 滿載(12 候選 × depth-4 樹 × 3 tf):TREE_DEPTH_CAP 收斂下
        必須離 hard 1MB 有量級餘裕(深樹以 children_omitted 計數收斂)。"""
        _patch_tool(monkeypatch, make_neely_routes(n_candidates=12, tree_depth=4))
        out = data_tools.neely_forecast("2330", AS_OF)
        kb = len(json.dumps(out, default=str)) / 1024
        assert kb < 512, f"neely_forecast dossier pathological payload {kb:.0f}KB"
        # 深樹確實被收斂(cap 之下出現 children_omitted)
        c0 = out["timeframes"]["daily"]["candidates"][0]

        def _has_omitted(node: dict) -> bool:
            if node.get("children_omitted"):
                return True
            return any(_has_omitted(c) for c in node.get("children") or [])

        assert _has_omitted(c0["wave_tree"])
