"""as_of() input validation 測試 — 早 raise 比 silently 跑 SQL 安全。"""

from __future__ import annotations

from datetime import date

import pytest

from agg.query import as_of


class TestInputValidation:
    def test_empty_stock_id_raises(self):
        with pytest.raises(ValueError, match="stock_id 不可為空"):
            as_of("", date(2026, 5, 1))

    def test_whitespace_stock_id_raises(self):
        with pytest.raises(ValueError, match="stock_id 不可為空"):
            as_of("   ", date(2026, 5, 1))

    def test_negative_lookback_raises(self):
        with pytest.raises(ValueError, match="lookback_days 不可為負"):
            as_of("2330", date(2026, 5, 1), lookback_days=-1)

    def test_empty_cores_list_raises(self):
        # 明確空 list 容易誤殺;None 才走 all-cores
        with pytest.raises(ValueError, match="cores 不可為空 list"):
            as_of("2330", date(2026, 5, 1), cores=[])
