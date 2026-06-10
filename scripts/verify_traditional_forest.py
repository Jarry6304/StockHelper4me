"""P0-Gate 驗收:traditional_snapshots forest size 分布 + 覆蓋率(免 psql)。

用法(repo root):
    python scripts/verify_traditional_forest.py

走 repo get_connection(自動讀 .env DATABASE_URL),印每 timeframe 的
stocks / p50 / p95 / max_n + distinct stock 覆蓋。驗收線:max_n ≤ forest_max_size
(200 cap)、p95 不過碎、每 tf 覆蓋接近 universe。
"""

from __future__ import annotations

import sys
from pathlib import Path

_REPO = Path(__file__).resolve().parent.parent
for p in (str(_REPO / "src"), str(_REPO)):
    if p not in sys.path:
        sys.path.insert(0, p)

from fusion.raw import get_connection  # noqa: E402

_DIST_SQL = """
SELECT timeframe,
       COUNT(*) AS stocks,
       PERCENTILE_CONT(0.50) WITHIN GROUP (
           ORDER BY jsonb_array_length(forest->'scenario_forest')) AS p50,
       PERCENTILE_CONT(0.95) WITHIN GROUP (
           ORDER BY jsonb_array_length(forest->'scenario_forest')) AS p95,
       MAX(jsonb_array_length(forest->'scenario_forest')) AS max_n
  FROM traditional_snapshots
 GROUP BY timeframe
 ORDER BY timeframe
"""

_COV_SQL = """
SELECT timeframe, COUNT(DISTINCT stock_id) AS n_stocks
  FROM traditional_snapshots
 GROUP BY timeframe
 ORDER BY timeframe
"""


def main() -> int:
    conn = get_connection()
    with conn.cursor() as cur:
        print("== forest size 分布 ==")
        print(f"{'timeframe':<10} {'stocks':>7} {'p50':>6} {'p95':>6} {'max_n':>6}")
        cur.execute(_DIST_SQL)
        rows = cur.fetchall()
        for r in rows:
            tf, stocks, p50, p95, max_n = (
                r["timeframe"], r["stocks"], r["p50"], r["p95"], r["max_n"]
            )
            flag = "  <-- max>200!" if (max_n or 0) > 200 else ""
            print(f"{tf:<10} {stocks:>7} {float(p50 or 0):>6.1f} "
                  f"{float(p95 or 0):>6.1f} {int(max_n or 0):>6}{flag}")

        print("\n== 覆蓋率(distinct stock / timeframe)==")
        cur.execute(_COV_SQL)
        for r in cur.fetchall():
            print(f"{r['timeframe']:<10} {r['n_stocks']:>7} stocks")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
