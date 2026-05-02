"""
reverse_pivot_foreign_holding.py
================================
PR #18:foreign_holding(v2.0)→ foreign_investor_share_tw(v3.2 Bronze)。

11 raw FinMind 欄位:2 stored(foreign_holding_shares/ratio)+ 9 detail JSONB key
(remaining/upper_limit/cn_upper_limit/total_issued/declare_date/intl_code/...)。
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
        "foreign_holding",
        stock_ids = stock_ids,
        dry_run   = args.dry_run,
    )
    return 0 if result["round_trip"]["match"] else 1


if __name__ == "__main__":
    sys.exit(main())
