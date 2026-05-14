"""Aggregation query.py 邏輯測試(mock PG row 輸入)。

不對真實 PG 跑 — 那部分留 integration test。
"""

from datetime import date

from agg.query import _indicator_key, _to_fact_row, _to_indicator_row, _to_structural_row


class TestRowConverters:
    def test_fact_row_full_fields(self):
        r = {
            "stock_id": "2330",
            "fact_date": date(2026, 5, 1),
            "timeframe": "daily",
            "source_core": "macd_core",
            "source_version": "0.4.0",
            "statement": "GoldenCross",
            "metadata": {"kind": "GoldenCross"},
            "params_hash": "abc123",
        }
        f = _to_fact_row(r)
        assert f.stock_id == "2330"
        assert f.fact_date == date(2026, 5, 1)
        assert f.metadata == {"kind": "GoldenCross"}
        assert f.params_hash == "abc123"

    def test_fact_row_handles_none_metadata(self):
        r = {
            "stock_id": "2330",
            "fact_date": date(2026, 5, 1),
            "timeframe": "daily",
            "source_core": "macd_core",
            "source_version": "0.4.0",
            "statement": "GoldenCross",
            "metadata": None,
            "params_hash": None,
        }
        f = _to_fact_row(r)
        assert f.metadata == {}
        assert f.params_hash is None

    def test_indicator_row_value_dict(self):
        r = {
            "stock_id": "2330",
            "value_date": date(2026, 5, 1),
            "timeframe": "daily",
            "source_core": "rsi_core",
            "source_version": "0.1.0",
            "value": {"series": [{"date": "2026-05-01", "rsi": 65.2}]},
            "params_hash": "xyz",
        }
        ind = _to_indicator_row(r)
        assert ind.value["series"][0]["rsi"] == 65.2

    def test_structural_row_optional_derived_from_core(self):
        r = {
            "stock_id": "2330",
            "snapshot_date": date(2026, 5, 1),
            "timeframe": "daily",
            "core_name": "neely_core",
            "source_version": "0.4.0",
            "snapshot": {"forest_size": 25},
            "params_hash": "",
            "derived_from_core": None,
        }
        s = _to_structural_row(r)
        assert s.derived_from_core is None
        assert s.snapshot["forest_size"] == 25


class TestIndicatorKey:
    def test_concat_format(self):
        assert _indicator_key("macd_core", "daily") == "macd_core@daily"

    def test_distinct_timeframes(self):
        assert _indicator_key("ma_core", "daily") != _indicator_key("ma_core", "weekly")
