"""Fusion Layer · Integration 端口共用 helper。

對齊 m3Spec/fusion_layer.md §5。純函式工具,無 ABC、無 orchestrator —
各 integration 模組各自獨立(spec §9 #2)。
"""

from __future__ import annotations

# facts.severity SMALLINT 編碼 — 對齊 fact_schema::Severity::as_i16
# (1=info / 2=notable / 3=warning / 4=critical)。
SEVERITY_RANK: dict[str, int] = {
    "info": 1,
    "notable": 2,
    "warning": 3,
    "critical": 4,
}
SEVERITY_LABEL: dict[int, str] = {v: k for k, v in SEVERITY_RANK.items()}

# Environment cores — market_dashboard / market_events 的資料來源
# (對齊 cores_overview §8.5,7 個)。
ENVIRONMENT_CORES: list[str] = [
    "taiex_core",
    "us_market_core",
    "exchange_rate_core",
    "fear_greed_core",
    "market_margin_core",
    "business_indicator_core",
    "commodity_macro_core",
]


def severity_to_int(severity: str | int) -> int:
    """severity 字串 → SMALLINT 編碼。未知值退化為 info(1)。"""
    if isinstance(severity, int):
        return severity if severity in SEVERITY_LABEL else 1
    return SEVERITY_RANK.get(str(severity).strip().lower(), 1)


def severity_to_label(value: int | None) -> str:
    """SMALLINT 編碼 → severity 字串。未知值退化為 info。"""
    try:
        return SEVERITY_LABEL.get(int(value), "info")  # type: ignore[arg-type]
    except (TypeError, ValueError):
        return "info"
