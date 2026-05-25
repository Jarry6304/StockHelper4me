"""Tests for v3.32 10 new cross_cores builders。

Smoke + 邏輯測:
- 每 builder 跑 empty DB → graceful empty result
- shared helpers(assign_ranks / compute_std / fetch_universe_filter)正確
- rank 排序方向(reverse=True/False)正確
- excluded_reason 紀錄完整

對齊既有 test_magic_formula.py mock 風格(MagicMock db.query)。
"""

from __future__ import annotations

import sys
from datetime import date
from pathlib import Path
from unittest.mock import MagicMock

import pytest

_REPO_ROOT = Path(__file__).resolve().parent.parent.parent
_SRC_ROOT = _REPO_ROOT / "src"
for p in (str(_SRC_ROOT), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)


# ════════════════════════════════════════════════════════════
# Shared helpers
# ════════════════════════════════════════════════════════════


class TestSharedHelpers:

    def test_assign_ranks_reverse_true(self):
        """reverse=True:高值 rank 1。"""
        from cross_cores._shared import assign_ranks

        rows = [
            {"stock_id": "A", "metric": 10.0},
            {"stock_id": "B", "metric": 30.0},
            {"stock_id": "C", "metric": 20.0},
        ]
        assign_ranks(rows, rank_col="rk", metric_col="metric",
                     reverse=True, top_n=2)
        ranks = {r["stock_id"]: r.get("rk") for r in rows}
        assert ranks["B"] == 1  # 最高
        assert ranks["C"] == 2
        assert ranks["A"] == 3
        # top_n=2:B, C is_top_n;A 不 in top(helper 只 set True 不 set False,
        # builder 自己 init False;測:top 2 stocks 都 True / A 非 True)
        tops = {r["stock_id"]: r.get("is_top_n") for r in rows}
        assert tops["B"] is True
        assert tops["C"] is True
        assert tops["A"] is not True

    def test_assign_ranks_reverse_false(self):
        """reverse=False:低值 rank 1(low vol use case)。"""
        from cross_cores._shared import assign_ranks

        rows = [
            {"stock_id": "A", "std": 0.05},
            {"stock_id": "B", "std": 0.02},
            {"stock_id": "C", "std": 0.08},
        ]
        assign_ranks(rows, rank_col="vol_rank", metric_col="std",
                     reverse=False, top_n=2)
        ranks = {r["stock_id"]: r.get("vol_rank") for r in rows}
        assert ranks["B"] == 1  # 最低
        assert ranks["A"] == 2
        assert ranks["C"] == 3

    def test_assign_ranks_excludes_none(self):
        """metric=None 的 row 不入 rank。"""
        from cross_cores._shared import assign_ranks

        rows = [
            {"stock_id": "A", "m": 10.0},
            {"stock_id": "B", "m": None},   # excluded
            {"stock_id": "C", "m": 20.0},
        ]
        assign_ranks(rows, rank_col="r", metric_col="m", reverse=True, top_n=10)
        ranks = {r["stock_id"]: r.get("r") for r in rows}
        assert ranks["A"] == 2
        assert ranks["C"] == 1
        assert ranks["B"] is None
        # universe_size 只算 eligible(2 stocks)
        assert rows[0]["universe_size"] == 2

    def test_compute_std(self):
        from cross_cores._shared import compute_std

        # std([1, 2, 3]) = 1.0(sample N-1)
        assert abs(compute_std([1.0, 2.0, 3.0]) - 1.0) < 1e-9
        # < 2 values → None
        assert compute_std([1.0]) is None
        assert compute_std([]) is None

    def test_compute_returns_from_closes(self):
        from cross_cores._shared import compute_returns_from_closes

        # closes [100, 110, 99] → returns [0.1, -0.1]
        result = compute_returns_from_closes([100.0, 110.0, 99.0])
        assert abs(result[0] - 0.1) < 1e-9
        assert abs(result[1] - (-0.1)) < 1e-9

    def test_universe_filter_excludes(self):
        from cross_cores._shared import fetch_universe_filter

        db = MagicMock()
        db.query.return_value = [
            {"stock_id": "2330", "industry_category": "半導體業", "delisting_date": None},
            {"stock_id": "2880", "industry_category": "金融業", "delisting_date": None},
            {"stock_id": "9907", "industry_category": "電力供應業", "delisting_date": None},
            {"stock_id": "1101", "industry_category": "水泥工業", "delisting_date": None},
            {"stock_id": "9999", "industry_category": "半導體業", "delisting_date": date(2020, 1, 1)},
        ]
        result = fetch_universe_filter(db)
        assert result["2330"] is None
        assert result["2880"] == "financial"
        assert result["9907"] == "utility"
        assert result["1101"] is None     # 水泥不應被誤排
        assert result["9999"] == "delisted"   # 已下市


# ════════════════════════════════════════════════════════════
# Builder smoke tests:empty DB → return empty result gracefully
# ════════════════════════════════════════════════════════════


def _empty_db():
    """空 DB:所有 query 回 []。"""
    db = MagicMock()
    db.query.return_value = []
    db.upsert.return_value = 0
    return db


class TestBuildersEmptyDB:
    """空 DB → 每 builder 都 return 標準 dict shape(不 raise)。"""

    @pytest.mark.parametrize("builder_name", [
        "persistent_momentum", "revenue_momentum", "institutional_concert",
        "f_score", "low_volatility", "industry_adj_gp",
        "long_term_low_vol", "dividend_yield", "mom_12_1",
        "monthly_trigger",
    ])
    def test_empty_db_returns_zero_rows(self, builder_name):
        from importlib import import_module
        mod = import_module(f"cross_cores.{builder_name}")
        result = mod.run(_empty_db())
        assert result["name"] == builder_name
        assert result["rows_written"] == 0
        assert "elapsed_ms" in result


class TestOrchestratorRegistration:

    def test_all_builders_registered(self):
        """orchestrator BUILDERS dict 應有 magic_formula + 10 v3.32 + wave_impulse_screen = 12 個。"""
        from cross_cores.orchestrator import BUILDERS

        expected = {
            "magic_formula",
            "persistent_momentum", "revenue_momentum", "institutional_concert",
            "f_score", "low_volatility", "industry_adj_gp",
            "long_term_low_vol", "dividend_yield", "mom_12_1",
            "monthly_trigger",
            "wave_impulse_screen",   # plan wave-impulse-cross-stock-virtual-papert.md
        }
        assert set(BUILDERS.keys()) == expected
        for name, mod in BUILDERS.items():
            assert hasattr(mod, "run"), f"builder {name} 缺 run()"
            assert hasattr(mod, "NAME"), f"builder {name} 缺 NAME"
            assert hasattr(mod, "OUTPUT_TABLE"), f"builder {name} 缺 OUTPUT_TABLE"


# ════════════════════════════════════════════════════════════
# Targeted logic test:f_score 計算
# ════════════════════════════════════════════════════════════


class TestFScoreComputation:

    def test_compute_f_score_full_9(self):
        """全 9 條件命中 → score = 9。"""
        from cross_cores.f_score import _compute_f_score

        # latest 季 vs 前 1 季:全部改善
        fins = {
            "income": [
                {"detail": {"本期淨利": 1000, "營業收入": 10000, "營業成本": 5000}},
                {"detail": {"本期淨利": 500, "營業收入": 9000, "營業成本": 5500}},
            ],
            "balance": [
                {"detail": {"資產總額": 20000, "流動資產": 8000, "流動負債": 3000,
                            "長期借款": 1000, "股本": 1000}},
                {"detail": {"資產總額": 22000, "流動資產": 7000, "流動負債": 4000,
                            "長期借款": 2000, "股本": 1000}},
            ],
            "cashflow": [
                {"detail": {"營業活動之現金流量": 1500}},  # CFO > 0 + > NI
            ],
        }
        score, breakdown = _compute_f_score(fins)
        # 9 個條件:roa>0, cfo>0, roa_yoy>0, cfo>ni,
        #          ltd_yoy<, current_yoy>, shares_yoy<=,
        #          gm_yoy>, at_yoy>
        # 全綠 → 9
        assert score == 9
        assert breakdown["profitability"] == 4
        assert breakdown["leverage"] == 3
        assert breakdown["efficiency"] == 2

    def test_compute_f_score_insufficient_data(self):
        """關鍵 key 缺 → 回 (None, {})。"""
        from cross_cores.f_score import _compute_f_score

        # 只 1 季 income → 無法算 YoY
        fins = {
            "income":   [{"detail": {"本期淨利": 1000}}],
            "balance":  [{"detail": {"資產總額": 20000}}],
            "cashflow": [],
        }
        score, breakdown = _compute_f_score(fins)
        assert score is None
        assert breakdown == {}
