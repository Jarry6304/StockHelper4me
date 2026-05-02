"""
_reverse_pivot_lib.py
======================
PR #18 共用 reverse-pivot helper(blueprint v3.2 §六 #11 / §十 PR #5)。

把 v2.0 pivot/pack 表反推回 v3.2 Bronze raw 形狀:

  legacy(寬表) ──reverse_pivot_rows──▶ bronze(瘦長表) ──repivot_for_verify──▶ legacy
                                                                              ▲
                                                            assert_round_trip │
                                                                              ▼
                                                                          原 legacy

兩種反推模式:
  - INVESTOR_PIVOT(institutional 專用):1 寬列 → up to 5 瘦列(每法人 1 列),
    PK 加 investor_type 欄。
  - DETAIL_UNPACK(margin / foreign_holding / day_trading / valuation):
    1 寬列 → 1 寬列,把 detail JSONB 攤平成 top-level 欄,field_rename 反推。

設計原則:
  - Bronze schema 沿用 legacy snake_case 欄名(不繞回 FinMind PascalCase),
    detail key 直接拔掉 `_` 前綴升格成欄。round-trip 等值的判準是「資料」一致,
    不是「欄名格式」一致。
  - 比對採 1e-9 數值容差 + NULL-aware,detail JSONB 比較對 dict 等值。
  - lib 只做 read / write / 比對,不做業務邏輯;呼叫 script 自己決定要不要 commit。
"""

from __future__ import annotations

import json
import logging
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable, Literal

# 讓 scripts/ 能 import src/ 模組(create_writer 走 .env / DATABASE_URL)
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "src"))

from db import DBWriter, create_writer  # noqa: E402

logger = logging.getLogger("scripts.reverse_pivot")

NUMERIC_TOLERANCE = 1e-9


# =============================================================================
# Spec
# =============================================================================

ReversePivotMode = Literal["investor_pivot", "detail_unpack"]


@dataclass(frozen=True)
class ReversePivotSpec:
    """單張表 reverse-pivot 規格。"""

    name: str                          # short id,e.g. "institutional"
    legacy_table: str                  # 來源:v2.0 pivot/pack 表
    bronze_table: str                  # 目的:v3.2 Bronze raw 表
    legacy_pk: tuple[str, ...]         # legacy PRIMARY KEY 欄名
    bronze_pk: tuple[str, ...]         # bronze PRIMARY KEY 欄名(institutional 多 investor_type)

    mode: ReversePivotMode

    # ── INVESTOR_PIVOT 用 ────────────────────────────────────────────────
    # bronze investor_type → (legacy buy 欄, legacy sell 欄)
    # 同時用來 forward(repivot)和 reverse,單一 source-of-truth。
    investor_type_map: dict[str, tuple[str, str]] = field(default_factory=dict)

    # ── DETAIL_UNPACK 用 ────────────────────────────────────────────────
    # legacy 已直存的欄(snake_case,直接 1:1 搬到 Bronze)
    legacy_stored_cols: tuple[str, ...] = ()
    # legacy detail JSONB 內預期的 key(都會升格為 Bronze top-level 欄)
    legacy_detail_keys: tuple[str, ...] = ()


# =============================================================================
# SPECS — 5 張表規格定義(blueprint §八.1 + collector.toml field_rename 對齊)
# =============================================================================

SPECS: dict[str, ReversePivotSpec] = {

    # 三大法人:1 寬列 → 5 瘦列(每法人 1 列)
    "institutional": ReversePivotSpec(
        name         = "institutional",
        legacy_table = "institutional_daily",
        bronze_table = "institutional_investors_tw",
        legacy_pk    = ("market", "stock_id", "date"),
        bronze_pk    = ("market", "stock_id", "date", "investor_type"),
        mode         = "investor_pivot",
        # 對齊 src/aggregators.py INSTITUTIONAL_NAME_MAP 的英文 key
        investor_type_map = {
            "Foreign_Investor":     ("foreign_buy",              "foreign_sell"),
            "Foreign_Dealer_Self":  ("foreign_dealer_self_buy",  "foreign_dealer_self_sell"),
            "Investment_Trust":     ("investment_trust_buy",     "investment_trust_sell"),
            "Dealer":               ("dealer_buy",               "dealer_sell"),
            "Dealer_Hedging":       ("dealer_hedging_buy",       "dealer_hedging_sell"),
        },
    ),

    # 融資融券:6 stored + 8 detail key
    "margin": ReversePivotSpec(
        name         = "margin",
        legacy_table = "margin_daily",
        bronze_table = "margin_purchase_short_sale_tw",
        legacy_pk    = ("market", "stock_id", "date"),
        bronze_pk    = ("market", "stock_id", "date"),
        mode         = "detail_unpack",
        legacy_stored_cols = (
            "margin_purchase", "margin_sell", "margin_balance",
            "short_sale", "short_cover", "short_balance",
        ),
        legacy_detail_keys = (
            "margin_cash_repay", "margin_prev_balance", "margin_limit",
            "short_cash_repay", "short_prev_balance", "short_limit",
            "offset_loan_short", "note",
        ),
    ),

    # 外資持股:2 stored + 9 detail key
    "foreign_holding": ReversePivotSpec(
        name         = "foreign_holding",
        legacy_table = "foreign_holding",
        bronze_table = "foreign_investor_share_tw",
        legacy_pk    = ("market", "stock_id", "date"),
        bronze_pk    = ("market", "stock_id", "date"),
        mode         = "detail_unpack",
        legacy_stored_cols = (
            "foreign_holding_shares", "foreign_holding_ratio",
        ),
        legacy_detail_keys = (
            "remaining_shares", "remain_ratio", "upper_limit_ratio",
            "cn_upper_limit", "total_issued", "declare_date",
            "intl_code", "stock_name", "note",
        ),
    ),

    # 當沖:2 stored + 2 detail key
    "day_trading": ReversePivotSpec(
        name         = "day_trading",
        legacy_table = "day_trading",
        bronze_table = "day_trading_tw",
        legacy_pk    = ("market", "stock_id", "date"),
        bronze_pk    = ("market", "stock_id", "date"),
        mode         = "detail_unpack",
        legacy_stored_cols = ("day_trading_buy", "day_trading_sell"),
        legacy_detail_keys = ("day_trading_flag", "volume"),
    ),

    # 估值:3 stored,無 detail
    "valuation": ReversePivotSpec(
        name         = "valuation",
        legacy_table = "valuation_daily",
        bronze_table = "valuation_per_tw",
        legacy_pk    = ("market", "stock_id", "date"),
        bronze_pk    = ("market", "stock_id", "date"),
        mode         = "detail_unpack",
        legacy_stored_cols = ("per", "pbr", "dividend_yield"),
        legacy_detail_keys = (),
    ),
}


# =============================================================================
# Public API
# =============================================================================

# Bronze 不存的「control」欄,讀 legacy 時 strip 掉,避免 round-trip 比對誤報 diff
LEGACY_CONTROL_COLS = frozenset({"source", "created_at", "updated_at"})


def fetch_legacy_pivot(
    db: DBWriter,
    spec: ReversePivotSpec,
    where: str | None = None,
    params: list[Any] | None = None,
) -> list[dict[str, Any]]:
    """從 legacy pivot 表撈 raw row。where 可加 stock_id IN (...) 等過濾。

    自動 strip LEGACY_CONTROL_COLS(source 等),只留資料欄,讓 round-trip 比對乾淨。
    """
    sql = f"SELECT * FROM {spec.legacy_table}"
    if where:
        sql += f" WHERE {where}"
    sql += f" ORDER BY {', '.join(spec.legacy_pk)}"
    rows = db.query(sql, params)
    return [
        {k: v for k, v in r.items() if k not in LEGACY_CONTROL_COLS}
        for r in rows
    ]


def reverse_pivot_rows(
    legacy_rows: list[dict[str, Any]],
    spec: ReversePivotSpec,
) -> list[dict[str, Any]]:
    """legacy 寬列 → Bronze 瘦/平列。"""
    if spec.mode == "investor_pivot":
        return _reverse_investor_pivot(legacy_rows, spec)
    elif spec.mode == "detail_unpack":
        return _reverse_detail_unpack(legacy_rows, spec)
    raise ValueError(f"未知 mode: {spec.mode}")


def upsert_bronze(
    db: DBWriter,
    spec: ReversePivotSpec,
    rows: list[dict[str, Any]],
    batch_size: int = 1000,
) -> int:
    """批次 UPSERT 到 Bronze 表。回傳寫入列數。"""
    if not rows:
        return 0
    total = 0
    for i in range(0, len(rows), batch_size):
        batch = rows[i:i + batch_size]
        total += db.upsert(spec.bronze_table, batch, list(spec.bronze_pk))
    return total


def repivot_for_verify(
    bronze_rows: list[dict[str, Any]],
    spec: ReversePivotSpec,
) -> list[dict[str, Any]]:
    """Bronze 瘦/平列 → legacy 寬列(round-trip 驗證用)。

    必須 mirror src/aggregators.py 與 src/field_mapper.py 的正向 transform 結果。
    """
    if spec.mode == "investor_pivot":
        return _repivot_investor(bronze_rows, spec)
    elif spec.mode == "detail_unpack":
        return _repivot_detail_pack(bronze_rows, spec)
    raise ValueError(f"未知 mode: {spec.mode}")


def assert_round_trip(
    legacy_rows: list[dict[str, Any]],
    repivoted_rows: list[dict[str, Any]],
    spec: ReversePivotSpec,
) -> dict[str, Any]:
    """比對原 legacy vs repivot 結果。回傳 diff report。

    Report 結構:
        {
            "match": bool,
            "legacy_count": int,
            "repivot_count": int,
            "missing_in_repivot": list[pk_tuple],   # legacy 有但 repivot 沒
            "extra_in_repivot":   list[pk_tuple],   # repivot 多出來
            "value_diffs": list[{pk, col, legacy, repivot}],
        }
    """
    legacy_by_pk = {_pk_tuple(r, spec.legacy_pk): r for r in legacy_rows}
    repivot_by_pk = {_pk_tuple(r, spec.legacy_pk): r for r in repivoted_rows}

    legacy_pks = set(legacy_by_pk.keys())
    repivot_pks = set(repivot_by_pk.keys())

    missing = sorted(legacy_pks - repivot_pks)
    extra   = sorted(repivot_pks - legacy_pks)

    value_diffs: list[dict[str, Any]] = []
    for pk in sorted(legacy_pks & repivot_pks):
        l_row = legacy_by_pk[pk]
        r_row = repivot_by_pk[pk]
        for col in set(l_row.keys()) | set(r_row.keys()):
            l_val = l_row.get(col)
            r_val = r_row.get(col)
            if not _values_equal(l_val, r_val):
                value_diffs.append({
                    "pk": pk, "col": col,
                    "legacy": l_val, "repivot": r_val,
                })

    return {
        "match": not (missing or extra or value_diffs),
        "legacy_count":  len(legacy_rows),
        "repivot_count": len(repivoted_rows),
        "missing_in_repivot": missing,
        "extra_in_repivot":   extra,
        "value_diffs":        value_diffs,
    }


# =============================================================================
# Mode: INVESTOR_PIVOT (institutional)
# =============================================================================

def _reverse_investor_pivot(
    legacy_rows: list[dict[str, Any]],
    spec: ReversePivotSpec,
) -> list[dict[str, Any]]:
    """legacy 1 列 → Bronze 多列(每法人 1 列)。"""
    out: list[dict[str, Any]] = []
    for row in legacy_rows:
        for inv_type, (buy_col, sell_col) in spec.investor_type_map.items():
            buy  = row.get(buy_col)
            sell = row.get(sell_col)
            # 該法人當日無資料(buy/sell 都 NULL)→ 跳過,不寫 Bronze
            # 對應 forward pivot 在 grouped 內初始化時用 None,只有 NAME_MAP hit
            # 才填值。Bronze 不存「空法人」row,re-pivot 才能對得上 legacy NULL。
            if buy is None and sell is None:
                continue
            out.append({
                "market":        row.get("market", "TW"),
                "stock_id":      row.get("stock_id"),
                "date":          row.get("date"),
                "investor_type": inv_type,
                "buy":           buy,
                "sell":          sell,
                "name":          inv_type,  # 對齊原 FinMind name 欄(英文 key 也接受)
            })
    return out


def _repivot_investor(
    bronze_rows: list[dict[str, Any]],
    spec: ReversePivotSpec,
) -> list[dict[str, Any]]:
    """Bronze 多列(每法人 1 列)→ legacy 1 寬列。

    Mirror src/aggregators.py:aggregate_institutional 的初始化策略:
    所有 10 個 buy/sell 欄初始為 None,Bronze hit 才填值。
    """
    grouped: dict[tuple, dict[str, Any]] = {}

    for row in bronze_rows:
        key = (row.get("market"), row.get("stock_id"), row.get("date"))
        if key not in grouped:
            agg = {
                "market":   row.get("market"),
                "stock_id": row.get("stock_id"),
                "date":     row.get("date"),
            }
            for buy_col, sell_col in spec.investor_type_map.values():
                agg[buy_col]  = None
                agg[sell_col] = None
            grouped[key] = agg

        inv_type = row.get("investor_type", "")
        cols = spec.investor_type_map.get(inv_type)
        if cols:
            buy_col, sell_col = cols
            grouped[key][buy_col]  = row.get("buy")
            grouped[key][sell_col] = row.get("sell")

    return list(grouped.values())


# =============================================================================
# Mode: DETAIL_UNPACK (margin / foreign_holding / day_trading / valuation)
# =============================================================================

def _reverse_detail_unpack(
    legacy_rows: list[dict[str, Any]],
    spec: ReversePivotSpec,
) -> list[dict[str, Any]]:
    """legacy 1 列(含 detail JSONB)→ Bronze 1 列(detail 攤平成 top-level)。"""
    out: list[dict[str, Any]] = []
    for row in legacy_rows:
        bronze_row: dict[str, Any] = {
            "market":   row.get("market", "TW"),
            "stock_id": row.get("stock_id"),
            "date":     row.get("date"),
        }
        # 直存欄 1:1 搬
        for col in spec.legacy_stored_cols:
            bronze_row[col] = row.get(col)
        # detail JSONB 攤平。psycopg3 拿到 jsonb 已是 dict,但 SqliteWriter
        # 可能拿到 str,兩種情況都要處理。
        detail_obj = _coerce_jsonb(row.get("detail"))
        for k in spec.legacy_detail_keys:
            bronze_row[k] = detail_obj.get(k) if detail_obj else None
        out.append(bronze_row)
    return out


def _repivot_detail_pack(
    bronze_rows: list[dict[str, Any]],
    spec: ReversePivotSpec,
) -> list[dict[str, Any]]:
    """Bronze 1 列(平)→ legacy 1 列(detail 重新打包成 JSONB)。

    Mirror src/field_mapper.py: detail JSONB 從 `_` 前綴欄收集,key 拔掉前綴。
    """
    out: list[dict[str, Any]] = []
    for row in bronze_rows:
        legacy_row: dict[str, Any] = {
            "market":   row.get("market"),
            "stock_id": row.get("stock_id"),
            "date":     row.get("date"),
        }
        # 直存欄 1:1 搬
        for col in spec.legacy_stored_cols:
            legacy_row[col] = row.get(col)
        # detail key 重新打包成 dict(_values_equal 對 dict 走 _coerce_jsonb 比對)
        if spec.legacy_detail_keys:
            legacy_row["detail"] = {k: row.get(k) for k in spec.legacy_detail_keys}
        out.append(legacy_row)
    return out


# =============================================================================
# helpers
# =============================================================================

def _pk_tuple(row: dict[str, Any], pk_cols: Iterable[str]) -> tuple:
    return tuple(row.get(c) for c in pk_cols)


def _coerce_jsonb(val: Any) -> dict[str, Any]:
    """psycopg3 jsonb 已是 dict;sqlite / 序列化後可能是 str。"""
    if val is None:
        return {}
    if isinstance(val, dict):
        return val
    if isinstance(val, str):
        try:
            parsed = json.loads(val)
            return parsed if isinstance(parsed, dict) else {}
        except json.JSONDecodeError:
            return {}
    return {}


def _values_equal(a: Any, b: Any) -> bool:
    """NULL-aware + 數值容差 + dict / JSONB 等值比對(兩邊 None-only entry normalize 掉)。"""
    if a is None and b is None:
        return True
    # 一邊 None、另一邊是 dict / 序列化過的 JSONB — 若 dict 內全 None,視為等價於 None
    # 對應「FinMind 完全沒回 _* 欄 → legacy detail=NULL」vs「repivot 產出 {all-None dict}」
    if a is None and isinstance(b, (dict, str)):
        return not _normalize_detail(_coerce_jsonb(b))
    if b is None and isinstance(a, (dict, str)):
        return not _normalize_detail(_coerce_jsonb(a))
    if a is None or b is None:
        return False
    # JSONB / dict — 兩邊各自把 None-value 的 entry 拔掉再比
    # 避免 FieldMapper 寫入時 FinMind 漏某 detail key(legacy 缺欄)vs repivot 全填 None
    # 的 false-negative。同時遞迴對 dict value 自己也走 _values_equal(數值容差等)
    if isinstance(a, dict) or isinstance(b, dict):
        ad = _normalize_detail(_coerce_jsonb(a))
        bd = _normalize_detail(_coerce_jsonb(b))
        if ad.keys() != bd.keys():
            return False
        return all(_values_equal(ad[k], bd[k]) for k in ad)
    # 數值容差(int / float / decimal.Decimal 都走 float 換算)
    try:
        af = float(a); bf = float(b)
        if af == bf:
            return True
        return abs(af - bf) < NUMERIC_TOLERANCE
    except (ValueError, TypeError):
        pass
    return a == b


def _normalize_detail(d: dict[str, Any]) -> dict[str, Any]:
    """把 dict 內值為 None 的 entry 拔掉。供 detail JSONB round-trip 比對用。"""
    return {k: v for k, v in d.items() if v is not None}


# =============================================================================
# 一站式 runner — 呼叫 script 用這個就好
# =============================================================================

def run_reverse_pivot(
    spec_name: str,
    *,
    stock_ids: list[str] | None = None,
    dry_run: bool = False,
) -> dict[str, Any]:
    """完整跑一張表 reverse-pivot:fetch → reverse → (upsert) → repivot → assert。

    回傳值:
        {
            "spec": str,
            "legacy_count": int,
            "bronze_count": int,
            "repivot_count": int,
            "wrote": int,            # 0 if dry_run
            "round_trip": dict,      # assert_round_trip report
        }
    """
    if spec_name not in SPECS:
        raise ValueError(f"未知 spec: {spec_name}。可用:{sorted(SPECS)}")
    spec = SPECS[spec_name]

    print("=" * 70)
    print(f"reverse_pivot {spec.name}  legacy={spec.legacy_table}  "
          f"bronze={spec.bronze_table}  dry_run={dry_run}  "
          f"stocks={stock_ids or 'ALL'}")
    print("=" * 70)

    db = create_writer()
    try:
        where = None
        params: list[Any] | None = None
        if stock_ids:
            placeholders = ",".join(["%s"] * len(stock_ids))
            where = f"stock_id IN ({placeholders})"
            params = list(stock_ids)

        legacy = fetch_legacy_pivot(db, spec, where=where, params=params)
        print(f"[1] legacy 讀取:{len(legacy)} 筆")

        bronze = reverse_pivot_rows(legacy, spec)
        print(f"[2] reverse-pivot:{len(bronze)} 筆")
        if spec.mode == "investor_pivot" and legacy:
            avg_per_day = len(bronze) / len(legacy)
            print(f"    平均每日 {avg_per_day:.2f} 法人(理論上限 5;< 5 表示部分法人當日無資料)")

        wrote = 0
        if dry_run:
            print("[3] dry_run=True 跳過寫入")
        else:
            wrote = upsert_bronze(db, spec, bronze)
            print(f"[3] 寫入 Bronze:{wrote} 列")

        # round-trip(讀 in-memory bronze;不是讀 DB,避開 dry_run)
        repivot = repivot_for_verify(bronze, spec)
        print(f"[4] repivot:{len(repivot)} 筆(預期 = {len(legacy)})")

        report = assert_round_trip(legacy, repivot, spec)
        if report["match"]:
            print()
            print(f"OK round-trip 100% 等值 ✓  ({len(legacy)} legacy ↔ "
                  f"{len(bronze)} bronze ↔ {len(repivot)} repivot)")
        else:
            print()
            print("FAIL round-trip 不等值 ✗")
            print(f"  missing_in_repivot: {len(report['missing_in_repivot'])}")
            print(f"  extra_in_repivot:   {len(report['extra_in_repivot'])}")
            print(f"  value_diffs:        {len(report['value_diffs'])}")
            for d in report["value_diffs"][:10]:
                print(f"    pk={d['pk']} col={d['col']} "
                      f"legacy={d['legacy']!r} != repivot={d['repivot']!r}")
            if len(report["value_diffs"]) > 10:
                print(f"    ... +{len(report['value_diffs']) - 10} more")

        return {
            "spec":          spec_name,
            "legacy_count":  len(legacy),
            "bronze_count":  len(bronze),
            "repivot_count": len(repivot),
            "wrote":         wrote,
            "round_trip":    report,
        }
    finally:
        db.close()
