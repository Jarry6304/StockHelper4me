"""Tests for src/silver/builders/magic_formula_ranked.py(Greenblatt 2005)。

對齊 plan §Phase A Silver 動工(2026-05-15)。
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


class TestUniverseFilter:
    """Greenblatt 2005 §六:排除金融保險 + 公用事業。"""

    def test_excludes_financial_industries(self):
        from silver.builders.magic_formula_ranked import _fetch_universe_filter

        db = MagicMock()
        db.query.return_value = [
            {"stock_id": "2330", "industry_category": "半導體業"},
            {"stock_id": "2880", "industry_category": "金融業"},
            {"stock_id": "2882", "industry_category": "金融保險業"},
            {"stock_id": "2823", "industry_category": "壽險業"},
            {"stock_id": "8112", "industry_category": "證券業"},
            {"stock_id": "9907", "industry_category": "電力供應業"},
            {"stock_id": "1611", "industry_category": "電器電纜業"},   # 不是公用
            {"stock_id": "1102", "industry_category": "水泥工業"},     # 不是「水」工業 — 水泥不該被排除
            {"stock_id": "9908", "industry_category": "燃氣業"},
        ]
        result = _fetch_universe_filter(db)
        # 注意:filter 用 substring match;水泥工業含「水」keyword,被歸 utility
        # 是 false positive。spec 中 keyword 是 "自來水" 而非 "水",已收緊。
        assert result["2330"] is None
        assert result["2880"] == "financial"
        assert result["2882"] == "financial"
        assert result["2823"] == "financial"
        assert result["8112"] == "financial"
        assert result["9907"] == "utility"
        assert result["1611"] is None
        assert result["1102"] is None      # 水泥工業 ✓ 不被誤排
        assert result["9908"] == "utility"

    def test_empty_industry_not_excluded(self):
        """industry_category 為 NULL / 空字串 → 不過濾(留 in universe)。"""
        from silver.builders.magic_formula_ranked import _fetch_universe_filter

        db = MagicMock()
        db.query.return_value = [
            {"stock_id": "2330", "industry_category": None},
            {"stock_id": "2317", "industry_category": ""},
        ]
        result = _fetch_universe_filter(db)
        assert result["2330"] is None
        assert result["2317"] is None


class TestDetailGet:
    """detail JSONB fallback chain 對齊 financial_statement_core 中文 IFRS key。"""

    def test_full_width_paren_key_chain(self):
        from silver.builders.magic_formula_ranked import _detail_get, EBIT_KEYS

        # 全形括號 key(實際 user production data 命名)
        detail = {"營業利益（損失）": "1234567"}
        assert _detail_get(detail, EBIT_KEYS) == 1234567.0

    def test_half_width_paren_fallback(self):
        from silver.builders.magic_formula_ranked import _detail_get, EBIT_KEYS

        detail = {"營業利益(損失)": 999}
        assert _detail_get(detail, EBIT_KEYS) == 999.0

    def test_english_fallback(self):
        from silver.builders.magic_formula_ranked import _detail_get, EBIT_KEYS

        detail = {"OperatingProfit": 555}
        assert _detail_get(detail, EBIT_KEYS) == 555.0

    def test_missing_key_returns_none(self):
        from silver.builders.magic_formula_ranked import _detail_get, EBIT_KEYS

        assert _detail_get({}, EBIT_KEYS) is None
        assert _detail_get({"foo": 1}, EBIT_KEYS) is None


class TestBuildRankRowsForDate:
    """單一 date 的跨股 ranking 邏輯。"""

    def test_top_30_selected_correctly(self):
        """排前 30 的 stocks 標 is_top_30=True。"""
        from silver.builders.magic_formula_ranked import _build_rank_rows_for_date

        # 40 個 eligible stocks,各自不同 EY / ROIC,top 30 應被標
        universe_filter = {f"S{i:04d}": None for i in range(1, 41)}
        financials = {}
        market_caps = {}
        for i in range(1, 41):
            sid = f"S{i:04d}"
            # i=1 最佳(EBIT 高 + 資產低 → EY/ROIC 高)→ rank 1
            financials[sid] = {
                "ebit_ttm":     1000.0 * (50 - i),       # 高 ebit
                "total_assets": 100.0 * i,
                "total_liab":   50.0 * i,
                "cash":         10.0,
            }
            market_caps[sid] = 500.0 * i

        rows = _build_rank_rows_for_date(
            date(2026, 5, 15), universe_filter, financials, market_caps
        )
        # 40 rows
        assert len(rows) == 40
        # 排序按 combined_rank,前 30 應 is_top_30=True
        eligible = [r for r in rows if r["excluded_reason"] is None]
        assert len(eligible) == 40
        eligible.sort(key=lambda r: r["combined_rank"])
        for r in eligible[:30]:
            assert r["is_top_30"] is True
        for r in eligible[30:]:
            assert r["is_top_30"] is False
        # universe_size 都應該是 40
        assert all(r["universe_size"] == 40 for r in eligible)

    def test_excluded_industry_no_rank(self):
        """金融股 row 寫入但 rank / metrics 都 NULL,is_top_30=False。"""
        from silver.builders.magic_formula_ranked import _build_rank_rows_for_date

        universe_filter = {
            "2330": None,            # 半導體
            "2880": "financial",     # 金融
        }
        financials = {
            "2330": {"ebit_ttm": 1e9, "total_assets": 2e9, "total_liab": 5e8, "cash": 3e8},
            "2880": {"ebit_ttm": 1e9, "total_assets": 2e9, "total_liab": 5e8, "cash": 3e8},
        }
        market_caps = {"2330": 1e10, "2880": 1e10}
        rows = _build_rank_rows_for_date(
            date(2026, 5, 15), universe_filter, financials, market_caps
        )
        by_sid = {r["stock_id"]: r for r in rows}
        assert by_sid["2880"]["excluded_reason"] == "financial"
        assert by_sid["2880"]["ey_rank"] is None
        assert by_sid["2880"]["is_top_30"] is False
        assert by_sid["2330"]["excluded_reason"] is None
        assert by_sid["2330"]["ey_rank"] == 1   # 唯一 eligible
        assert by_sid["2330"]["is_top_30"] is True
        assert by_sid["2330"]["universe_size"] == 1

    def test_negative_ebit_disqualified(self):
        """虧損股(EBIT < 0)不進 rank。"""
        from silver.builders.magic_formula_ranked import _build_rank_rows_for_date

        universe_filter = {"S1": None, "S2": None}
        financials = {
            "S1": {"ebit_ttm": -1000, "total_assets": 5000, "total_liab": 2000, "cash": 100},
            "S2": {"ebit_ttm":  1000, "total_assets": 5000, "total_liab": 2000, "cash": 100},
        }
        market_caps = {"S1": 10000, "S2": 10000}
        rows = _build_rank_rows_for_date(
            date(2026, 5, 15), universe_filter, financials, market_caps
        )
        by_sid = {r["stock_id"]: r for r in rows}
        assert by_sid["S1"]["excluded_reason"] == "negative_ebit_or_ev"
        assert by_sid["S1"]["ey_rank"] is None
        assert by_sid["S2"]["excluded_reason"] is None
        assert by_sid["S2"]["ey_rank"] == 1

    def test_no_market_cap_excluded(self):
        """缺市值的 stock 標 no_market_cap。"""
        from silver.builders.magic_formula_ranked import _build_rank_rows_for_date

        universe_filter = {"S1": None}
        financials = {"S1": {"ebit_ttm": 1000, "total_assets": 5000, "total_liab": 2000, "cash": 100}}
        market_caps = {}  # 空
        rows = _build_rank_rows_for_date(date(2026, 5, 15), universe_filter, financials, market_caps)
        assert len(rows) == 1
        assert rows[0]["excluded_reason"] == "no_market_cap"

    def test_no_ebit_data_propagated(self):
        """財報資料不足(financials dict 帶 excluded_reason)→ row excluded。"""
        from silver.builders.magic_formula_ranked import _build_rank_rows_for_date

        universe_filter = {"S1": None}
        financials = {"S1": {"excluded_reason": "no_ebit_data"}}
        rows = _build_rank_rows_for_date(date(2026, 5, 15), universe_filter, {}, {})
        # 直接無 financials → 不 emit row(eligible 集合空,跳過 ranking)
        # 注意:無 financials 也無 row;測試 financials 含 excluded_reason 的 case
        rows = _build_rank_rows_for_date(date(2026, 5, 15), universe_filter, financials, {"S1": 1e9})
        assert len(rows) == 1
        assert rows[0]["excluded_reason"] == "no_ebit_data"
