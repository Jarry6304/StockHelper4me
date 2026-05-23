"""Forecast scoring: pinball loss, sharpness, reliability.

Theory references (v0.3 spec §「理論依據」):
  - Gneiting, Balabdaoui & Raftery (2007) JRSS-B 69(2):243-268
      校準前提下最大化銳利度
  - Koenker & Bassett (1978) Econometrica 46(1):33-50  pinball check loss
  - Winkler (1977) JASA  interval score

This is a pure function over already-settled rows.  Does not touch the DB,
does not re-run any core.  See `settlement.resolve_pending` for the side that
fills pinball_loss + hit columns.
"""

from __future__ import annotations

from collections import defaultdict
from typing import Any, Iterable


def _coerce_float(v: Any) -> float | None:
    if v is None:
        return None
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


def _aggregate(forecasts: Iterable[dict[str, Any]]) -> dict[str, Any]:
    rows = [r for r in forecasts if r.get("pinball_loss") is not None]
    n = len(rows)
    if n == 0:
        return {
            "n": 0,
            "mean_pinball_loss": None,
            "sharpness": None,
            "reliability": [],
        }
    pinballs = [_coerce_float(r["pinball_loss"]) for r in rows]
    pinballs = [p for p in pinballs if p is not None]
    widths = []
    for r in rows:
        l = _coerce_float(r.get("lower"))
        u = _coerce_float(r.get("upper"))
        if l is not None and u is not None:
            widths.append(u - l)

    # reliability: for each unique confidence, empirical coverage
    cov_groups: dict[float, list[bool]] = defaultdict(list)
    for r in rows:
        c = _coerce_float(r.get("confidence"))
        hit = r.get("hit")
        if c is None or hit is None:
            continue
        cov_groups[c].append(bool(hit))
    reliability = []
    for c in sorted(cov_groups.keys()):
        hits = cov_groups[c]
        if hits:
            reliability.append((c, sum(hits) / len(hits)))

    return {
        "n": n,
        "mean_pinball_loss": sum(pinballs) / len(pinballs) if pinballs else None,
        "sharpness": sum(widths) / len(widths) if widths else None,
        "reliability": reliability,
    }


def score(
    forecasts: list[dict[str, Any]],
    group_by: str | None = None,
) -> dict[str, Any] | dict[Any, dict[str, Any]]:
    """Score a list of already-settled forecast rows.

    Args:
        forecasts: list of dicts; each must have at least pinball_loss + hit +
                   confidence + lower + upper for full metrics.
        group_by: None | "source_core" | "horizon_days" | "regime_tag".

    Returns:
        If group_by is None: {n, mean_pinball_loss, sharpness, reliability}.
        Otherwise: {group_key: {n, mean_pinball_loss, sharpness, reliability}, ...}.
        group_key=None is used for rows where the grouping column is NULL.
    """
    if group_by is None:
        return _aggregate(forecasts)

    valid_keys = {"source_core", "horizon_days", "regime_tag"}
    if group_by not in valid_keys:
        raise ValueError(f"group_by must be one of {valid_keys}, got {group_by!r}")

    grouped: dict[Any, list[dict[str, Any]]] = defaultdict(list)
    for r in forecasts:
        grouped[r.get(group_by)].append(r)
    return {k: _aggregate(v) for k, v in grouped.items()}


# ─── pinball helper (also used by settlement) ────────────────────────────────


def quantile_pinball(realized: float, quantile_value: float, tau: float) -> float:
    """Standard pinball / check loss.

    pinball(y, q, tau) = (y - q) * tau           if y >= q
                       = (q - y) * (1 - tau)     otherwise
    """
    diff = realized - quantile_value
    return diff * tau if diff >= 0 else -diff * (1.0 - tau)


def interval_pinball(
    realized: float,
    lower: float,
    upper: float,
    confidence: float,
) -> float:
    """Two-sided interval pinball: mean of lower-tail and upper-tail check losses.

    For a (1 - alpha) confidence interval:
        tau_lo = alpha / 2
        tau_hi = 1 - alpha / 2
    """
    alpha = 1.0 - confidence
    tau_lo = alpha / 2.0
    tau_hi = 1.0 - alpha / 2.0
    lo_loss = quantile_pinball(realized, lower, tau_lo)
    hi_loss = quantile_pinball(realized, upper, tau_hi)
    return 0.5 * (lo_loss + hi_loss)
