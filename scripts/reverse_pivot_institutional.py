"""
reverse_pivot_institutional.py
==============================
PR #18(blueprint v3.2 §六 #11):把 institutional_daily(v2.0 pivot 寬表)反推回
institutional_investors_tw(v3.2 Bronze raw,1 row per investor_type)。

預期 row 比:legacy N → Bronze 5N(每日最多 5 法人,實際依 FinMind 當日是否回該法人)。

前置:
  1. alembic upgrade head → 確保 institutional_investors_tw 已建
  2. .env 載入或 export DATABASE_URL

用法:
  python scripts/reverse_pivot_institutional.py --stocks 2330 --dry-run
  python scripts/reverse_pivot_institutional.py --stocks 2330
  python scripts/reverse_pivot_institutional.py            # 全市場

退出碼:
  0 = round-trip 100% 等值
  1 = round-trip 落差 / 任何錯誤
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
        "institutional",
        stock_ids = stock_ids,
        dry_run   = args.dry_run,
    )
    return 0 if result["round_trip"]["match"] else 1


if __name__ == "__main__":
    sys.exit(main())
