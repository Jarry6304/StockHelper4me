"""
verify_pr19c_silver.py
======================
PR #19c-1 5 個 market-level Silver builder round-trip 驗證(對應 v2.0 / v3.2 Bronze)。

驗證原則(對齊 verify_pr19b_silver 模式):
  - taiex_index / us_market_index:Bronze 與 Silver schema 1:1,逐欄等值比對
  - exchange_rate:PK 含 currency 維度,逐 (market, date, currency) 比對
  - market_margin:Bronze ratio 對齊 Silver ratio;PR #19c-1 暫不填的 2 衍生欄
                  (total_margin_purchase_balance / total_short_sale_balance)skip
  - business_indicator:Bronze (market, date) → Silver (market, '_market_', date)
                       sentinel 比對策略:Silver 的 stock_id 應全為 '_market_',
                       對 Bronze (market, date) 收集後展開 Silver 形狀

執行:
  python scripts/verify_pr19c_silver.py                  # 全部
  python scripts/verify_pr19c_silver.py --skip-build     # 假設 Silver 已寫,只比對

退出碼:0 = 5/5 OK,1 = 任一 FAIL
"""

from __future__ import annotations

import argparse
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "src"))

from _reverse_pivot_lib import _coerce_jsonb, _values_equal  # noqa: E402

from silver.builders import (  # noqa: E402
    business_indicator,
    exchange_rate,
    market_margin,
    taiex_index,
    us_market_index,
)
from db import create_writer  # noqa: E402


@dataclass(frozen=True)
class VerifySpec:
    name: str
    builder: Any
    silver_table: str
    bronze_table: str
    pk_cols: tuple[str, ...]
    bronze_pk_cols: tuple[str, ...]            # 用於 fetch+match Bronze
    compare_cols: tuple[str, ...]              # 逐欄等值
    detail_col: str | None = None              # JSONB 比對
    skip_silver_cols: tuple[str, ...] = ()     # PR #19c-1 暫不填的衍生欄
    silver_stock_id_const: str | None = None   # business_indicator 的 '_market_' sentinel


VERIFY_SPECS: list[VerifySpec] = [
    VerifySpec(
        name           = "taiex_index",
        builder        = taiex_index,
        silver_table   = "taiex_index_derived",
        bronze_table   = "market_ohlcv_tw",
        pk_cols        = ("market", "stock_id", "date"),
        bronze_pk_cols = ("market", "stock_id", "date"),
        compare_cols   = ("open", "high", "low", "close", "volume"),
        detail_col     = "detail",
    ),
    VerifySpec(
        name           = "us_market_index",
        builder        = us_market_index,
        silver_table   = "us_market_index_derived",
        bronze_table   = "market_index_us",
        pk_cols        = ("market", "stock_id", "date"),
        bronze_pk_cols = ("market", "stock_id", "date"),
        compare_cols   = ("open", "high", "low", "close", "volume"),
        detail_col     = "detail",
    ),
    VerifySpec(
        name           = "exchange_rate",
        builder        = exchange_rate,
        silver_table   = "exchange_rate_derived",
        bronze_table   = "exchange_rate",
        pk_cols        = ("market", "date", "currency"),
        bronze_pk_cols = ("market", "date", "currency"),
        compare_cols   = ("rate",),
        detail_col     = "detail",
    ),
    VerifySpec(
        name           = "market_margin",
        builder        = market_margin,
        silver_table   = "market_margin_maintenance_derived",
        bronze_table   = "market_margin_maintenance",
        pk_cols        = ("market", "date"),
        bronze_pk_cols = ("market", "date"),
        compare_cols   = ("ratio",),
        skip_silver_cols = ("total_margin_purchase_balance", "total_short_sale_balance"),
    ),
    VerifySpec(
        name           = "business_indicator",
        builder        = business_indicator,
        silver_table   = "business_indicator_derived",
        bronze_table   = "business_indicator_tw",
        pk_cols        = ("market", "stock_id", "date"),    # Silver PK
        bronze_pk_cols = ("market", "date"),                  # Bronze PK
        compare_cols   = ("leading_indicator", "coincident_indicator",
                          "lagging_indicator", "monitoring", "monitoring_color"),
        silver_stock_id_const = "_market_",
    ),
]


# =============================================================================
# 比對邏輯
# =============================================================================

def _select_all(
    db: Any, table: str, pk_cols: tuple[str, ...]
) -> dict[tuple, dict[str, Any]]:
    """SELECT * FROM table → {pk_tuple: row_dict}。strip control 欄(source/dirty)。"""
    rows = db.query(f"SELECT * FROM {table}")
    out: dict[tuple, dict[str, Any]] = {}
    for r in rows:
        clean = {k: v for k, v in r.items()
                 if k not in ("source", "is_dirty", "dirty_at")}
        out[tuple(r[c] for c in pk_cols)] = clean
    return out


def _compare(silver_by: dict, bronze_by: dict, spec: VerifySpec) -> dict:
    """逐 PK 比對 silver vs bronze。對 business_indicator 處理 stock_id sentinel。"""

    # business_indicator 特殊:Bronze (market, date) → Silver (market, '_market_', date)
    # 把 Bronze key 加上 '_market_' 後與 Silver PK 對齊
    if spec.silver_stock_id_const:
        bronze_by_pk = {}
        for (market, date), row in bronze_by.items():
            silver_pk = (market, spec.silver_stock_id_const, date)
            bronze_by_pk[silver_pk] = row
    else:
        bronze_by_pk = bronze_by

    silver_pks = set(silver_by.keys())
    bronze_pks = set(bronze_by_pk.keys())

    missing  = sorted(bronze_pks - silver_pks)
    extra    = sorted(silver_pks - bronze_pks)
    diffs: list[dict] = []

    for pk in sorted(bronze_pks & silver_pks):
        s = silver_by[pk]
        b = bronze_by_pk[pk]

        for col in spec.compare_cols:
            if not _values_equal(s.get(col), b.get(col)):
                diffs.append({"pk": pk, "col": col, "silver": s.get(col), "bronze": b.get(col)})

        if spec.detail_col:
            sd = _coerce_jsonb(s.get(spec.detail_col))
            bd = _coerce_jsonb(b.get(spec.detail_col))
            if not _values_equal(sd, bd):
                diffs.append({"pk": pk, "col": spec.detail_col, "silver": sd, "bronze": bd})

    return {
        "match":              not (missing or extra or diffs),
        "silver_count":       len(silver_by),
        "bronze_count":       len(bronze_by_pk),
        "missing_in_silver":  missing,
        "extra_in_silver":    extra,
        "value_diffs":        diffs,
    }


# =============================================================================
# main
# =============================================================================

def main() -> int:
    p = argparse.ArgumentParser(description="PR #19c-1 5 個 market-level Silver builder 驗證")
    p.add_argument("--skip-build", action="store_true",
                    help="跳過 builder run(假設 Silver 已有資料,只比對)")
    args = p.parse_args()

    db = create_writer()
    try:
        # 0. Bronze 空表 sanity(對齊 verify_pr19b 同一 trap)
        if not args.skip_build:
            empty: list[str] = []
            for spec in VERIFY_SPECS:
                row = db.query_one(f"SELECT COUNT(*) AS cnt FROM {spec.bronze_table}")
                if row and row["cnt"] == 0:
                    empty.append(spec.bronze_table)
            if empty:
                print()
                print("=" * 80)
                print("Bronze 表是空的,builder 會讀 0 寫 0 → 全 FAIL。")
                print("=" * 80)
                for t in empty:
                    print(f"  - {t}(空)")
                print()
                print("修法(看是哪張表空的):")
                print("  - market_ohlcv_tw:python src/main.py backfill --phases 1   # B-1/B-2")
                print("  - market_index_us / exchange_rate / market_margin_maintenance:")
                print("      python src/main.py backfill --phases 6                 # macro")
                print("  - business_indicator_tw:python src/main.py backfill --phases 6")
                print()
                return 1

        # 1. 跑 5 個 builder
        if not args.skip_build:
            print("=" * 80)
            print("Phase 1:跑 5 個 Silver builder(market-level,full_rebuild)")
            print("=" * 80)
            for spec in VERIFY_SPECS:
                result = spec.builder.run(db, full_rebuild=True)
                print(f"  [{result['name']:20}] read={result['rows_read']:>8} → "
                      f"wrote={result['rows_written']:>8} ({result['elapsed_ms']}ms)")
            print()

        # 2. 比對
        print("=" * 80)
        print("Phase 2:Silver vs Bronze 等值比對")
        print("=" * 80)
        results: list[tuple[VerifySpec, dict]] = []
        for spec in VERIFY_SPECS:
            silver_by = _select_all(db, spec.silver_table, spec.pk_cols)
            bronze_by = _select_all(db, spec.bronze_table, spec.bronze_pk_cols)
            report = _compare(silver_by, bronze_by, spec)
            results.append((spec, report))

        # 3. status table
        print()
        print(f"{'builder':<22} {'silver':>8} {'bronze':>8}  {'status':>6}")
        print("-" * 54)
        all_ok = True
        for spec, report in results:
            ok = report["match"]
            all_ok = all_ok and ok
            status = "OK" if ok else "FAIL"
            print(f"{spec.name:<22} "
                  f"{report['silver_count']:>8} "
                  f"{report['bronze_count']:>8}  "
                  f"{status:>6}")
        print("-" * 54)
        pass_count = sum(1 for _, r in results if r["match"])
        print(f"{'TOTAL':<22} {pass_count}/{len(results):>15}  {'OK' if all_ok else 'FAIL':>6}")
        print()

        # 4. 失敗 diff
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
                        print(f"        bronze={d['bronze']!r}")

        return 0 if all_ok else 1
    finally:
        db.close()


if __name__ == "__main__":
    sys.exit(main())
