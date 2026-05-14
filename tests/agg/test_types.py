"""Aggregation Layer dataclass serialization tests。"""

from datetime import date

from agg._types import (
    AsOfSnapshot,
    FactRow,
    IndicatorRow,
    QueryMetadata,
    StructuralRow,
)


class TestFactRow:
    def test_to_dict_date_serialized(self):
        f = FactRow(
            stock_id="2330",
            fact_date=date(2026, 5, 1),
            timeframe="daily",
            source_core="macd_core",
            source_version="0.4.0",
            statement="GoldenCross",
            metadata={"kind": "GoldenCross", "value": 12.5},
        )
        d = f.to_dict()
        assert d["fact_date"] == "2026-05-01"
        assert d["stock_id"] == "2330"
        assert d["metadata"]["kind"] == "GoldenCross"


class TestSnapshot:
    def test_to_dict_empty(self):
        snap = AsOfSnapshot(stock_id="2330", as_of=date(2026, 5, 1))
        d = snap.to_dict()
        assert d["stock_id"] == "2330"
        assert d["as_of"] == "2026-05-01"
        assert d["facts"] == []
        assert d["indicator_latest"] == {}
        assert d["market"] == {}
        assert d["metadata"] is None

    def test_to_dict_with_facts(self):
        snap = AsOfSnapshot(
            stock_id="2330",
            as_of=date(2026, 5, 1),
            facts=[
                FactRow(
                    stock_id="2330",
                    fact_date=date(2026, 4, 30),
                    timeframe="daily",
                    source_core="rsi_core",
                    source_version="0.1.0",
                    statement="RsiOversold",
                )
            ],
            metadata=QueryMetadata(
                stock_id="2330",
                as_of=date(2026, 5, 1),
                lookback_days=90,
                cores=None,
                include_market=True,
                timeframes=None,
            ),
        )
        d = snap.to_dict()
        assert len(d["facts"]) == 1
        assert d["facts"][0]["statement"] == "RsiOversold"
        assert d["metadata"]["as_of"] == "2026-05-01"

    def test_facts_df(self):
        try:
            import pandas as pd  # noqa: F401
        except ImportError:
            import pytest
            pytest.skip("pandas not installed")

        snap = AsOfSnapshot(
            stock_id="2330",
            as_of=date(2026, 5, 1),
            facts=[
                FactRow(
                    stock_id="2330",
                    fact_date=date(2026, 4, 30),
                    timeframe="daily",
                    source_core="rsi_core",
                    source_version="0.1.0",
                    statement="RsiOversold",
                    metadata={"kind": "RsiOversold", "value": 25.5},
                ),
                FactRow(
                    stock_id="2330",
                    fact_date=date(2026, 4, 29),
                    timeframe="daily",
                    source_core="macd_core",
                    source_version="0.4.0",
                    statement="GoldenCross",
                    metadata={"kind": "GoldenCross"},
                ),
            ],
        )
        df = snap.facts_df()
        assert len(df) == 2
        assert "fact_date" in df.columns
        assert "source_core" in df.columns
        assert df.iloc[0]["source_core"] == "rsi_core"
        assert df.iloc[0]["kind"] == "RsiOversold"


class TestStructuralRow:
    def test_to_dict_optional_field(self):
        r = StructuralRow(
            stock_id="2330",
            snapshot_date=date(2026, 5, 1),
            timeframe="daily",
            core_name="neely_core",
            source_version="0.4.0",
            snapshot={"forest_size": 25},
            derived_from_core=None,
        )
        d = r.to_dict()
        assert d["snapshot_date"] == "2026-05-01"
        assert d["derived_from_core"] is None
        assert d["snapshot"]["forest_size"] == 25
