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


def cluster_price_levels(
    points: list[dict],
    *,
    bucket_pct: float = 0.01,
) -> list[dict]:
    """把帶 `price` 的點 cluster 成價位區。對齊 m3Spec/fusion_layer.md §8.1。

    依 price 升序排序,greedy 收同一 bucket(與該 cluster 首個成員相對距離
    < `bucket_pct`)。`strength` = cluster 內 distinct `source` 數(被越多來源
    確認的價位越強)。

    Args:
        points: [{price: float, source: str, ...}]
        bucket_pct: 同一價位的相對容差(預設 1%)。

    Returns:
        [{price, low, high, sources, strength, member_count}],依 price 升序。
    """
    valid = [
        p for p in points
        if isinstance(p.get("price"), (int, float))
        and not isinstance(p.get("price"), bool)
        and p["price"] > 0
    ]
    valid.sort(key=lambda p: p["price"])

    clusters: list[list[dict]] = []
    cur: list[dict] = []
    for p in valid:
        if not cur:
            cur = [p]
            continue
        anchor = cur[0]["price"]
        if abs(p["price"] - anchor) / anchor < bucket_pct:
            cur.append(p)
        else:
            clusters.append(cur)
            cur = [p]
    if cur:
        clusters.append(cur)

    out: list[dict] = []
    for c in clusters:
        prices = [m["price"] for m in c]
        sources = sorted({str(m.get("source")) for m in c})
        out.append({
            "price": round(sum(prices) / len(prices), 4),
            "low": round(min(prices), 4),
            "high": round(max(prices), 4),
            "sources": sources,
            "strength": len(sources),
            "member_count": len(c),
        })
    return out
