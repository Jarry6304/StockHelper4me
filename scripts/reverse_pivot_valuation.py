"""
reverse_pivot_valuation.py
==========================
PR #18:valuation_daily(v2.0)→ valuation_per_tw(v3.2 Bronze)。

最簡單的一張:3 stored cols(per/pbr/dividend_yield),無 detail JSONB。
row 比:legacy N → Bronze N(1:1)。

前置同 institutional 版本(alembic upgrade head + DATABASE_URL)。
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _reverse_pivot_lib import run_reverse_pivot  # noqa: E402


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__.split("\n")[1])
    p.add_argument("--stocks", help="逗號分隔股票清單(預設全市場)")
    p.add_argument("--dry-run", action="store_true", help="不寫 Bronze,只跑反推 + 比對")
    args = p.parse_args()

    stock_ids = [s.strip() for s in args.stocks.split(",")] if args.stocks else None

    result = run_reverse_pivot(
        "valuation",
        stock_ids = stock_ids,
        dry_run   = args.dry_run,
    )
    return 0 if result["round_trip"]["match"] else 1


if __name__ == "__main__":
    sys.exit(main())
