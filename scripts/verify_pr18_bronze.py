"""
verify_pr18_bronze.py
=====================
PR #18 5 張 Bronze 反推聚合驗證器。對每張表跑 round-trip,印 status table。

判定:
  - 各 table 從 legacy 反推產出 Bronze rows(in-memory),re-pivot 回 legacy form,
    與原 legacy 比對。任何 row count / value diff → status FAIL。
  - 不寫 Bronze(--write 才寫)。預設 dry-run 只驗 lib 邏輯。

用法:
  python scripts/verify_pr18_bronze.py                  # dry-run all 5
  python scripts/verify_pr18_bronze.py --stocks 2330    # 單股驗
  python scripts/verify_pr18_bronze.py --write          # 順便 UPSERT 進 Bronze

退出碼:
  0 = 5/5 OK
  1 = 任一 table FAIL
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _reverse_pivot_lib import SPECS, run_reverse_pivot  # noqa: E402


# 順序 = 易→難(plan §Phase 3)
ORDER = ["institutional", "valuation", "day_trading", "margin", "foreign_holding"]


def main() -> int:
    p = argparse.ArgumentParser(description="PR #18 5 張 Bronze 反推聚合驗證")
    p.add_argument("--stocks", help="逗號分隔股票清單(預設全市場)")
    p.add_argument("--write", action="store_true", help="UPSERT 寫入 Bronze(預設 dry-run)")
    args = p.parse_args()

    stock_ids = [s.strip() for s in args.stocks.split(",")] if args.stocks else None

    results: list[dict] = []
    for spec_name in ORDER:
        try:
            r = run_reverse_pivot(
                spec_name,
                stock_ids = stock_ids,
                dry_run   = not args.write,
            )
            results.append({"spec_name": spec_name, **r})
        except Exception as exc:
            print(f"\n[FATAL] {spec_name}: {exc}\n")
            results.append({
                "spec_name": spec_name, "exception": str(exc),
                "round_trip": {"match": False},
                "legacy_count": 0, "bronze_count": 0, "repivot_count": 0,
            })

    # ───── 印 status table ─────
    print()
    print("=" * 80)
    print("PR #18 Bronze reverse-pivot 聚合驗證結果")
    print("=" * 80)
    print(f"{'table':<32} {'legacy':>8} {'bronze':>8} {'repivot':>8}  {'status':>6}")
    print("-" * 80)
    all_ok = True
    for r in results:
        spec = SPECS[r["spec_name"]]
        ok = r["round_trip"].get("match", False)
        all_ok = all_ok and ok
        status = "OK" if ok else "FAIL"
        print(f"{spec.bronze_table:<32} "
              f"{r['legacy_count']:>8} "
              f"{r['bronze_count']:>8} "
              f"{r['repivot_count']:>8}  "
              f"{status:>6}")
    print("-" * 80)
    pass_count = sum(1 for r in results if r["round_trip"].get("match"))
    summary = f"{pass_count}/{len(results)}"
    final_status = "OK" if all_ok else "FAIL"
    print(f"{'TOTAL':<32} {summary:>26}  {final_status:>6}")
    print()

    # 失敗時印 diff(每張表只印前 5 筆)
    if not all_ok:
        print("─" * 80)
        print("FAIL diff 細節(每張表前 5 筆):")
        for r in results:
            rt = r["round_trip"]
            if rt.get("match"):
                continue
            print(f"\n  ◇ {r['spec_name']}:")
            if "exception" in r:
                print(f"    exception: {r['exception']}")
                continue
            if rt.get("missing_in_repivot"):
                print(f"    missing_in_repivot: {len(rt['missing_in_repivot'])} 筆")
                for pk in rt["missing_in_repivot"][:5]:
                    print(f"      pk={pk}")
            if rt.get("extra_in_repivot"):
                print(f"    extra_in_repivot: {len(rt['extra_in_repivot'])} 筆")
                for pk in rt["extra_in_repivot"][:5]:
                    print(f"      pk={pk}")
            if rt.get("value_diffs"):
                print(f"    value_diffs: {len(rt['value_diffs'])} 筆")
                for d in rt["value_diffs"][:5]:
                    print(f"      pk={d['pk']} col={d['col']} "
                          f"legacy={d['legacy']!r} != repivot={d['repivot']!r}")
        print()

    return 0 if all_ok else 1


if __name__ == "__main__":
    sys.exit(main())
