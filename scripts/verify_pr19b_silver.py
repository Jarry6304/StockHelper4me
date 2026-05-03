"""
verify_pr19b_silver.py
======================
PR #19b 5 個 Silver builder 跑完後對 v2.0 legacy 表做 round-trip 驗證。

驗證原則(per PR #19b plan):
  - Silver `*_derived` 跑 builder 寫入後,核心欄位應與 v2.0 legacy 表逐欄等值
    (institutional 10 buy/sell,margin 6 stored + detail JSONB,foreign 2 stored
    + detail,day_trading 2 stored + detail,valuation 3 stored)
  - PR #19b 暫不填的欄(institutional.gov_bank_net / valuation.market_value_weight /
    margin SBL 6 欄)兩邊都 NULL,不比對
  - dirty 欄位(is_dirty / dirty_at)是 Silver 專屬,不對 legacy

執行流程:
  1. import 5 個 builder + 跑 run(db, full_rebuild=True)→ 寫 Silver
  2. SELECT * 從 Silver 與 legacy 兩邊
  3. 用 _reverse_pivot_lib._values_equal(數值容差 + dict normalize)逐 PK 比
  4. 印 status table

用法:
  python scripts/verify_pr19b_silver.py                  # 全市場
  python scripts/verify_pr19b_silver.py --stocks 2330    # 單股驗

退出碼:
  0 = 5/5 OK
  1 = 任一 table FAIL
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
    day_trading,
    foreign_holding,
    institutional,
    margin,
    valuation,
)
from db import create_writer  # noqa: E402


# =============================================================================
# 驗證 spec 表 — 每個 builder 對應一張 v2.0 legacy 表 + 比對欄位清單
# =============================================================================

@dataclass(frozen=True)
class VerifySpec:
    name: str
    builder: Any                # silver/builders/<name> module
    silver_table: str
    legacy_table: str
    pk_cols: tuple[str, ...]
    compare_cols: tuple[str, ...]    # stored cols(逐欄等值)
    detail_col: str | None = None    # legacy detail JSONB 欄(對應 Silver detail)
    skip_silver_cols: tuple[str, ...] = ()    # PR #19b 暫不填的 Silver 欄,不比


VERIFY_SPECS: list[VerifySpec] = [
    VerifySpec(
        name         = "institutional",
        builder      = institutional,
        silver_table = "institutional_daily_derived",
        legacy_table = "institutional_daily",
        pk_cols      = ("market", "stock_id", "date"),
        compare_cols = (
            "foreign_buy", "foreign_sell",
            "foreign_dealer_self_buy", "foreign_dealer_self_sell",
            "investment_trust_buy", "investment_trust_sell",
            "dealer_buy", "dealer_sell",
            "dealer_hedging_buy", "dealer_hedging_sell",
        ),
        skip_silver_cols = ("gov_bank_net",),    # PR #19c
    ),
    VerifySpec(
        name         = "margin",
        builder      = margin,
        silver_table = "margin_daily_derived",
        legacy_table = "margin_daily",
        pk_cols      = ("market", "stock_id", "date"),
        compare_cols = (
            "margin_purchase", "margin_sell", "margin_balance",
            "short_sale", "short_cover", "short_balance",
        ),
        detail_col   = "detail",
        skip_silver_cols = (
            # 3 alias = 直接從 short_* 抄,builder 寫對沒 round-trip 對齊問題,但
            # legacy 表沒這 3 欄,所以不在 compare_cols 內。
            "margin_short_sales_short_sales",
            "margin_short_sales_short_covering",
            "margin_short_sales_current_day_balance",
            # 3 SBL → PR #19c
            "sbl_short_sales_short_sales",
            "sbl_short_sales_returns",
            "sbl_short_sales_current_day_balance",
        ),
    ),
    VerifySpec(
        name         = "foreign_holding",
        builder      = foreign_holding,
        silver_table = "foreign_holding_derived",
        legacy_table = "foreign_holding",
        pk_cols      = ("market", "stock_id", "date"),
        compare_cols = ("foreign_holding_shares", "foreign_holding_ratio"),
        detail_col   = "detail",
    ),
    VerifySpec(
        name         = "day_trading",
        builder      = day_trading,
        silver_table = "day_trading_derived",
        legacy_table = "day_trading",
        pk_cols      = ("market", "stock_id", "date"),
        compare_cols = ("day_trading_buy", "day_trading_sell"),
        detail_col   = "detail",
    ),
    VerifySpec(
        name         = "valuation",
        builder      = valuation,
        silver_table = "valuation_daily_derived",
        legacy_table = "valuation_daily",
        pk_cols      = ("market", "stock_id", "date"),
        compare_cols = ("per", "pbr", "dividend_yield"),
        skip_silver_cols = ("market_value_weight",),    # PR #19c
    ),
]


# =============================================================================
# 比對邏輯
# =============================================================================

def _select_all(db: Any, table: str, pk_cols: tuple[str, ...],
                 stock_ids: list[str] | None) -> dict[tuple, dict[str, Any]]:
    """SELECT * FROM table,回 {pk_tuple: row_dict}。strip control 欄(source)。"""
    sql = f"SELECT * FROM {table}"
    params: list[Any] = []
    if stock_ids:
        placeholders = ",".join(["%s"] * len(stock_ids))
        sql += f" WHERE stock_id IN ({placeholders})"
        params = list(stock_ids)
    rows = db.query(sql, params if params else None)
    out: dict[tuple, dict[str, Any]] = {}
    for r in rows:
        # 排除 source / is_dirty / dirty_at(Silver 專屬,不對 legacy 比)
        clean = {k: v for k, v in r.items()
                  if k not in ("source", "is_dirty", "dirty_at")}
        out[tuple(r[c] for c in pk_cols)] = clean
    return out


def _compare(silver_by_pk: dict, legacy_by_pk: dict, spec: VerifySpec) -> dict:
    """逐 PK 比對 silver vs legacy。回 diff report。"""
    silver_pks = set(silver_by_pk.keys())
    legacy_pks = set(legacy_by_pk.keys())
    missing  = sorted(legacy_pks - silver_pks)        # legacy 有 silver 沒
    extra    = sorted(silver_pks - legacy_pks)        # silver 多出來
    diffs: list[dict] = []

    for pk in sorted(legacy_pks & silver_pks):
        s = silver_by_pk[pk]
        l = legacy_by_pk[pk]

        # stored col 逐欄比
        for col in spec.compare_cols:
            if not _values_equal(s.get(col), l.get(col)):
                diffs.append({"pk": pk, "col": col, "silver": s.get(col), "legacy": l.get(col)})

        # detail JSONB(若有)
        if spec.detail_col:
            s_detail = _coerce_jsonb(s.get(spec.detail_col))
            l_detail = _coerce_jsonb(l.get(spec.detail_col))
            if not _values_equal(s_detail, l_detail):
                diffs.append({
                    "pk": pk, "col": spec.detail_col,
                    "silver": s_detail, "legacy": l_detail,
                })

    return {
        "match":            not (missing or extra or diffs),
        "silver_count":     len(silver_by_pk),
        "legacy_count":     len(legacy_by_pk),
        "missing_in_silver": missing,
        "extra_in_silver":   extra,
        "value_diffs":       diffs,
    }


# =============================================================================
# main
# =============================================================================

def main() -> int:
    p = argparse.ArgumentParser(description="PR #19b 5 個 Silver builder round-trip 驗證")
    p.add_argument("--stocks", help="逗號分隔股票清單(預設全市場)")
    p.add_argument("--skip-build", action="store_true",
                    help="跳過 builder run(假設 Silver 已有資料,只比對)")
    args = p.parse_args()

    stock_ids = [s.strip() for s in args.stocks.split(",")] if args.stocks else None

    db = create_writer()
    try:
        # 0. Sanity:Bronze 全空時提早報錯,不要默默 0/5 FAIL(過去用戶踩過 trap:
        #    PR #18 rollback smoke 會清空 Bronze,需先跑 verify_pr18_bronze.py --write 重填)
        if not args.skip_build:
            empty_bronzes: list[str] = []
            for spec in VERIFY_SPECS:
                bronze_table = spec.builder.BRONZE_TABLES[0]
                row = db.query_one(f"SELECT COUNT(*) AS cnt FROM {bronze_table}")
                if row and row["cnt"] == 0:
                    empty_bronzes.append(bronze_table)
            if empty_bronzes:
                print()
                print("=" * 80)
                print("Bronze 表是空的,builder 會讀 0 行寫 0 行 → 全 FAIL。")
                print("=" * 80)
                print("空 Bronze 表:")
                for t in empty_bronzes:
                    print(f"  - {t}")
                print()
                print("修法:先跑 verify_pr18_bronze.py --write 從 v2.0 legacy 反推填回 Bronze:")
                print()
                print("  python scripts/verify_pr18_bronze.py --write")
                print("  python scripts/verify_pr19b_silver.py")
                print()
                print("(常見成因:alembic downgrade -1 rollback smoke 會 DROP 5 張 Bronze)")
                return 1

        # 1. 跑 5 個 builder(除非 --skip-build)
        if not args.skip_build:
            print("=" * 80)
            print("Phase 1:跑 5 個 Silver builder(full_rebuild)")
            print("=" * 80)
            for spec in VERIFY_SPECS:
                result = spec.builder.run(db, stock_ids=stock_ids, full_rebuild=True)
                print(f"  [{result['name']:18}] read={result['rows_read']:>6} → "
                      f"wrote={result['rows_written']:>6} ({result['elapsed_ms']}ms)")
            print()

        # 2. 比對
        print("=" * 80)
        print("Phase 2:Silver vs v2.0 legacy round-trip 比對")
        print("=" * 80)
        results: list[tuple[VerifySpec, dict]] = []
        for spec in VERIFY_SPECS:
            silver_by = _select_all(db, spec.silver_table, spec.pk_cols, stock_ids)
            legacy_by = _select_all(db, spec.legacy_table, spec.pk_cols, stock_ids)
            report = _compare(silver_by, legacy_by, spec)
            results.append((spec, report))

        # 3. 印 status
        print()
        print(f"{'builder':<18} {'silver':>8} {'legacy':>8}  {'status':>6}")
        print("-" * 50)
        all_ok = True
        for spec, report in results:
            ok = report["match"]
            all_ok = all_ok and ok
            status = "OK" if ok else "FAIL"
            print(f"{spec.name:<18} "
                  f"{report['silver_count']:>8} "
                  f"{report['legacy_count']:>8}  "
                  f"{status:>6}")
        print("-" * 50)
        print(f"{'TOTAL':<18} {sum(1 for _, r in results if r['match'])}/{len(results)} OK")
        print()

        # 4. 失敗時印 diff
        if not all_ok:
            print("─" * 80)
            print("FAIL diff 細節(每張表前 5 筆):")
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
