"""
verify_pr19c2_silver.py
=======================
PR #19c-2 3 個依賴 PR #18.5 Bronze 的 Silver builder round-trip 驗證。

驗證模式對齊 verify_pr19b_silver.py(對 v2.0 legacy_v2 表逐 PK 比對),因為 3 張表
v2.0 路徑與 v3.2 Silver 都做相同 pack/rename,輸出應該等值(±1% SLO 內)。

3 個 builder:
  - holding_shares_per:Bronze N rows/level → Silver 1 row/(stock,date) + detail
  - monthly_revenue:Bronze raw FinMind 欄名 → Silver rename + detail pack
  - financial_statement:Bronze N rows/origin_name → Silver 1 row/(stock,date,type)

由於 PR #18.5 user 只 smoke test 了 3 stocks(1101 / 2317 / 2330),verifier 預設
過濾這 3 支(對齊 PR #18.5 已 backfill 的範圍)。

⚠️ 邊緣日期 1-row delta 屬正常 dual-write 時間錯位
==================================================
v3 path(主名 Bronze)和 v2 path(*_legacy_v2)是兩條 incremental segment,
各自跑的時間點不同 → 最新一兩天可能出現一邊有、另一邊還沒抓的 1-row delta
(e.g. holding_shares_per: silver=1135 vs legacy_v2=1134,2330 / 2026-04-30)。

plan §7.2 觀察期 SLO:`3 張 _legacy_v2 row count 與主名表 ±1%`。
1/1135 = 0.088%、1/265 = 0.377%,均落在 SLO 內,不算 R5 觀察期失敗。

連續跑兩輪 incremental 兩條 path 通常會收斂(各自抓到對方那天即可)。

執行:
  python scripts/verify_pr19c2_silver.py                  # 預設 1101,2317,2330
  python scripts/verify_pr19c2_silver.py --stocks 2330    # 單股驗
  python scripts/verify_pr19c2_silver.py --skip-build     # 假設 Silver 已寫,只比對

退出碼:0 = 3/3 OK,1 = 任一 FAIL
"""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "src"))

from _reverse_pivot_lib import _coerce_jsonb, _values_equal  # noqa: E402

from silver.builders import (  # noqa: E402
    financial_statement,
    holding_shares_per,
    monthly_revenue,
)
from db import create_writer  # noqa: E402


# 預設 PR #18.5 smoke test 3 stocks(對齊 user 已 backfill 的範圍)
DEFAULT_STOCKS = ["1101", "2317", "2330"]


@dataclass(frozen=True)
class VerifySpec:
    name: str
    builder: Any
    silver_table: str
    legacy_table: str          # v2.0 對照表
    pk_cols: tuple[str, ...]
    compare_cols: tuple[str, ...]   # 逐欄 stored 比對
    detail_col: str | None = None


VERIFY_SPECS: list[VerifySpec] = [
    VerifySpec(
        name         = "holding_shares_per",
        builder      = holding_shares_per,
        silver_table = "holding_shares_per_derived",
        legacy_table = "holding_shares_per_legacy_v2",   # PR #R2 rename
        pk_cols      = ("market", "stock_id", "date"),
        compare_cols = (),                # 全在 detail JSONB
        detail_col   = "detail",
    ),
    VerifySpec(
        name         = "monthly_revenue",
        builder      = monthly_revenue,
        silver_table = "monthly_revenue_derived",
        legacy_table = "monthly_revenue_legacy_v2",      # PR #R2 rename
        pk_cols      = ("market", "stock_id", "date"),
        compare_cols = ("revenue", "revenue_yoy", "revenue_mom"),
        detail_col   = "detail",
    ),
    VerifySpec(
        name         = "financial_statement",
        builder      = financial_statement,
        silver_table = "financial_statement_derived",
        legacy_table = "financial_statement_legacy_v2",  # PR #R2 rename
        pk_cols      = ("market", "stock_id", "date", "type"),
        compare_cols = (),                # 全在 detail JSONB
        detail_col   = "detail",
    ),
]


# =============================================================================
# 比對邏輯(對齊 verify_pr19b_silver._compare 模式)
# =============================================================================

def _select_all(
    db: Any, table: str, pk_cols: tuple[str, ...],
    stock_ids: list[str] | None,
) -> dict[tuple, dict[str, Any]]:
    sql = f"SELECT * FROM {table}"
    params: list[Any] = []
    if stock_ids:
        placeholders = ",".join(["%s"] * len(stock_ids))
        sql += f" WHERE stock_id IN ({placeholders})"
        params = list(stock_ids)
    rows = db.query(sql, params if params else None)
    out: dict[tuple, dict[str, Any]] = {}
    for r in rows:
        clean = {k: v for k, v in r.items()
                 if k not in ("source", "is_dirty", "dirty_at")}
        out[tuple(r[c] for c in pk_cols)] = clean
    return out


def _compare(silver_by, legacy_by, spec: VerifySpec) -> dict:
    silver_pks = set(silver_by.keys())
    legacy_pks = set(legacy_by.keys())
    missing  = sorted(legacy_pks - silver_pks)
    extra    = sorted(silver_pks - legacy_pks)
    diffs: list[dict] = []

    for pk in sorted(legacy_pks & silver_pks):
        s = silver_by[pk]
        l = legacy_by[pk]

        for col in spec.compare_cols:
            if not _values_equal(s.get(col), l.get(col)):
                diffs.append({"pk": pk, "col": col, "silver": s.get(col), "legacy": l.get(col)})

        if spec.detail_col:
            sd = _coerce_jsonb(s.get(spec.detail_col))
            ld = _coerce_jsonb(l.get(spec.detail_col))
            if not _values_equal(sd, ld):
                diffs.append({"pk": pk, "col": spec.detail_col, "silver": sd, "legacy": ld})

    return {
        "match":             not (missing or extra or diffs),
        "silver_count":      len(silver_by),
        "legacy_count":      len(legacy_by),
        "missing_in_silver": missing,
        "extra_in_silver":   extra,
        "value_diffs":       diffs,
    }


# =============================================================================
# main
# =============================================================================

def main() -> int:
    p = argparse.ArgumentParser(description="PR #19c-2 3 個 PR #18.5 依賴 builder round-trip 驗證")
    p.add_argument("--stocks", help=f"逗號分隔股票清單(預設:{','.join(DEFAULT_STOCKS)})")
    p.add_argument("--skip-build", action="store_true",
                    help="跳過 builder run(假設 Silver 已有資料,只比對)")
    args = p.parse_args()

    stock_ids = [s.strip() for s in args.stocks.split(",")] if args.stocks else DEFAULT_STOCKS

    db = create_writer()
    try:
        # 0. Bronze 空表 sanity(PR #18.5 三張)
        if not args.skip_build:
            empty: list[str] = []
            for spec in VERIFY_SPECS:
                bt = spec.builder.BRONZE_TABLES[0]
                # 用 = ANY(%s) 對 list,psycopg3 自動展 array;原 IN %s 會被 psycopg3
                # 翻成 IN $1(single placeholder)PG 不收
                row = db.query_one(
                    f"SELECT COUNT(*) AS cnt FROM {bt} WHERE stock_id = ANY(%s)",
                    [stock_ids],
                )
                if row and row["cnt"] == 0:
                    empty.append(bt)
            if empty:
                print()
                print("=" * 80)
                print(f"Bronze 表對 stocks {stock_ids} 沒資料,builder 會讀 0 寫 0 → 全 FAIL。")
                print("=" * 80)
                for t in empty:
                    print(f"  - {t}(對指定 stocks 無資料)")
                print()
                print("修法:跑 PR #18.5 backfill 補資料:")
                print(f"  python src/main.py backfill --phases 5 --stocks {','.join(stock_ids)}")
                print()
                return 1

        # 1. 跑 3 個 builder
        if not args.skip_build:
            print("=" * 80)
            print(f"Phase 1:跑 3 個 PR #19c-2 Silver builder(stocks={stock_ids})")
            print("=" * 80)
            for spec in VERIFY_SPECS:
                result = spec.builder.run(db, stock_ids=stock_ids, full_rebuild=True)
                print(f"  [{result['name']:22}] read={result['rows_read']:>8} → "
                      f"wrote={result['rows_written']:>8} ({result['elapsed_ms']}ms)")
            print()

        # 2. 比對
        print("=" * 80)
        print("Phase 2:Silver vs v2.0 legacy 等值比對")
        print("=" * 80)
        results: list[tuple[VerifySpec, dict]] = []
        for spec in VERIFY_SPECS:
            silver_by = _select_all(db, spec.silver_table, spec.pk_cols, stock_ids)
            legacy_by = _select_all(db, spec.legacy_table, spec.pk_cols, stock_ids)
            report = _compare(silver_by, legacy_by, spec)
            results.append((spec, report))

        # 3. status
        print()
        print(f"{'builder':<24} {'silver':>8} {'legacy':>8}  {'status':>6}")
        print("-" * 56)
        all_ok = True
        for spec, report in results:
            ok = report["match"]
            all_ok = all_ok and ok
            status = "OK" if ok else "FAIL"
            print(f"{spec.name:<24} "
                  f"{report['silver_count']:>8} "
                  f"{report['legacy_count']:>8}  "
                  f"{status:>6}")
        print("-" * 56)
        pass_count = sum(1 for _, r in results if r["match"])
        print(f"{'TOTAL':<24} {pass_count}/{len(results):>16}  {'OK' if all_ok else 'FAIL':>6}")
        print()

        # 4. fail diff
        if not all_ok:
            print("─" * 80)
            print("FAIL diff(每張表前 5 筆):")
            for spec, report in results:
                if report["match"]:
                    continue
                print(f"\n  ◇ {spec.name}:")
                if report["missing_in_silver"]:
                    print(f"    missing_in_silver: {len(report['missing_in_silver'])}")
                    for pk in report["missing_in_silver"][:5]:
                        print(f"      pk={pk}")
                if report["extra_in_silver"]:
                    print(f"    extra_in_silver: {len(report['extra_in_silver'])}")
                    for pk in report["extra_in_silver"][:5]:
                        print(f"      pk={pk}")
                if report["value_diffs"]:
                    print(f"    value_diffs: {len(report['value_diffs'])}")
                    for d in report["value_diffs"][:5]:
                        print(f"      pk={d['pk']} col={d['col']}")
                        print(f"        silver={d['silver']!r}")
                        print(f"        legacy={d['legacy']!r}")

        return 0 if all_ok else 1
    finally:
        db.close()


if __name__ == "__main__":
    sys.exit(main())
