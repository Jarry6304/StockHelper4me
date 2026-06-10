"""Regression: cross-stock ranked 表 / 欄白名單擋 SQL identifier 注入。

fetch_cross_stock_ranked / fetch_is_top_30 用 f-string 拼 table / column 名(SQL
identifier 無法 bind 參數化)。source_table 可從 MCP 工具 dual_track_resonance
(cross_stock_table=...)由 LLM 控 → 非白名單值須在 query 前 raise ValueError,
且 conn 完全不被 touch。本 test 不打真 DB。
"""

from __future__ import annotations

from datetime import date

import pytest


class _BoomConn:
    """任何 .cursor() 都 assert fail — 驗證 reject 發生在 query 之前。"""

    def cursor(self):
        raise AssertionError("conn 不該被使用:identifier 應在 query 前就被擋下")


def test_ensure_accepts_whitelisted():
    from fusion.raw._db import ensure_safe_ranked_identifiers

    # 不該 raise
    ensure_safe_ranked_identifiers(
        source_table="magic_formula_ranked_derived",
        rank_col="combined_rank", is_top_col="is_top_n",
    )
    ensure_safe_ranked_identifiers(
        source_table="f_score_ranked_derived", rank_col="score_rank",
    )


def test_ensure_rejects_unknown_table():
    from fusion.raw._db import ensure_safe_ranked_identifiers

    with pytest.raises(ValueError):
        ensure_safe_ranked_identifiers(source_table="facts; DROP TABLE facts; --")


def test_ensure_rejects_bad_rank_col():
    from fusion.raw._db import ensure_safe_ranked_identifiers

    with pytest.raises(ValueError):
        ensure_safe_ranked_identifiers(
            source_table="magic_formula_ranked_derived", rank_col="combined_rank; --",
        )


def test_ensure_rejects_bad_flag_col():
    from fusion.raw._db import ensure_safe_ranked_identifiers

    with pytest.raises(ValueError):
        ensure_safe_ranked_identifiers(
            source_table="magic_formula_ranked_derived", is_top_col="is_top_n OR 1=1",
        )


def test_ensure_rejects_bad_extra_cols():
    from fusion.raw._db import ensure_safe_ranked_identifiers

    with pytest.raises(ValueError):
        ensure_safe_ranked_identifiers(
            source_table="magic_formula_ranked_derived", extra_cols=["ok_col", "bad col"],
        )


def test_fetch_cross_stock_ranked_rejects_before_touching_conn():
    from fusion.raw._db import fetch_cross_stock_ranked

    with pytest.raises(ValueError):
        fetch_cross_stock_ranked(
            _BoomConn(), source_table="x'; DROP TABLE facts; --", as_of=date(2026, 5, 30),
        )


def test_fetch_is_top_30_rejects_before_touching_conn():
    from fusion.dual_track.resonance import fetch_is_top_30

    with pytest.raises(ValueError):
        fetch_is_top_30(
            _BoomConn(), stock_id="2330", as_of=date(2026, 5, 30),
            source_table="magic_formula_ranked_derived'; DROP TABLE facts; --",
        )


def test_web_api_screens_allowlist_subset_of_library():
    """screens._ALLOWED 值須全在 library 權威白名單(import-time assert 的 test 版)。"""
    from fusion.raw._db import ALLOWED_RANKED_TABLES
    from web_api.routers.screens import _ALLOWED

    assert set(_ALLOWED.values()) <= ALLOWED_RANKED_TABLES
