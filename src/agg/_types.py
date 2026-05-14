"""Aggregation Layer dataclasses.

對齊 m3Spec/aggregation_layer.md §八 Output 結構。

設計:
- @dataclass(slots=True) — 記憶體友善 + 跑得快
- to_dict() — JSON 序列化
- 不引入 pydantic dependency(FastAPI 之後 wrap 再 pydantic 化)
"""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from datetime import date
from typing import Any


@dataclass(slots=True)
class FactRow:
    """一個 fact 事件。對齊 facts 表 schema。"""

    stock_id: str
    fact_date: date
    timeframe: str
    source_core: str
    source_version: str
    statement: str
    metadata: dict[str, Any] = field(default_factory=dict)
    params_hash: str | None = None

    def to_dict(self) -> dict[str, Any]:
        d = asdict(self)
        d["fact_date"] = self.fact_date.isoformat()
        return d


@dataclass(slots=True)
class IndicatorRow:
    """一個 indicator core 的最新 indicator_values row。"""

    stock_id: str
    value_date: date
    timeframe: str
    source_core: str
    source_version: str
    value: dict[str, Any]  # JSONB 內容
    params_hash: str = ""

    def to_dict(self) -> dict[str, Any]:
        d = asdict(self)
        d["value_date"] = self.value_date.isoformat()
        return d


@dataclass(slots=True)
class StructuralRow:
    """一個 structural_snapshots row(neely scenario_forest 等)。"""

    stock_id: str
    snapshot_date: date
    timeframe: str
    core_name: str
    source_version: str
    snapshot: dict[str, Any]  # JSONB 內容
    params_hash: str = ""
    derived_from_core: str | None = None

    def to_dict(self) -> dict[str, Any]:
        d = asdict(self)
        d["snapshot_date"] = self.snapshot_date.isoformat()
        return d


@dataclass(slots=True)
class QueryMetadata:
    """query 參數記錄(供 debug + reproducibility)。"""

    stock_id: str
    as_of: date
    lookback_days: int
    cores: list[str] | None  # None = all
    include_market: bool
    timeframes: list[str] | None  # None = all

    def to_dict(self) -> dict[str, Any]:
        d = asdict(self)
        d["as_of"] = self.as_of.isoformat()
        return d


@dataclass(slots=True)
class AsOfSnapshot:
    """as_of(stock_id, date) 主回傳結構。"""

    stock_id: str
    as_of: date

    facts: list[FactRow] = field(default_factory=list)
    indicator_latest: dict[str, IndicatorRow] = field(default_factory=dict)
    structural: dict[str, StructuralRow] = field(default_factory=dict)
    market: dict[str, list[FactRow]] = field(default_factory=dict)
    metadata: QueryMetadata | None = None

    def to_dict(self) -> dict[str, Any]:
        return {
            "stock_id": self.stock_id,
            "as_of": self.as_of.isoformat(),
            "facts": [f.to_dict() for f in self.facts],
            "indicator_latest": {k: v.to_dict() for k, v in self.indicator_latest.items()},
            "structural": {k: v.to_dict() for k, v in self.structural.items()},
            "market": {k: [f.to_dict() for f in v] for k, v in self.market.items()},
            "metadata": self.metadata.to_dict() if self.metadata else None,
        }

    def facts_df(self):
        """flatten facts to pandas DataFrame(若有 pandas)。"""
        try:
            import pandas as pd
        except ImportError:
            raise RuntimeError("pandas not installed; pip install pandas")
        rows = []
        for f in self.facts:
            rows.append({
                "fact_date": f.fact_date,
                "source_core": f.source_core,
                "statement": f.statement,
                "timeframe": f.timeframe,
                "kind": f.metadata.get("kind"),
                "value": f.metadata.get("value"),
            })
        return pd.DataFrame(rows)
