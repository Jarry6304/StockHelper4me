"""
diag_date_diff.py
-----------------
比對兩張表針對同一 stock_id 的日期差異，找出不對齊處。

用途：debug「institutional_daily 比 price_daily 多 N 筆」這類議題。

執行：
    python scripts/diag_date_diff.py 2330 institutional_daily price_daily

輸出：
    A only：A 表有但 B 表沒有的日期（並印出 A 表那行重點欄位）
    B only：B 表有但 A 表沒有的日期
    若任何一邊只多 1~5 筆，會把那筆完整列出方便研判（連假補登 / bug）。
"""

from __future__ import annotations

import sqlite3
import sys
from pathlib import Path

DB_PATH = Path(__file__).resolve().parent.parent / "data" / "tw_stock.db"


def main(argv: list[str]) -> int:
    if len(argv) != 4:
        print("用法：python scripts/diag_date_diff.py <stock_id> <table_a> <table_b>")
        return 2

    stock_id, table_a, table_b = argv[1], argv[2], argv[3]
    if not DB_PATH.exists():
        print(f"ERROR: DB 不存在: {DB_PATH}")
        return 2

    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    cur = conn.cursor()

    def col_set(table: str) -> set[str]:
        return {r["name"] for r in cur.execute(f"PRAGMA table_info({table})").fetchall()}

    cols_a = col_set(table_a)
    cols_b = col_set(table_b)
    if not cols_a:
        print(f"ERROR: {table_a} 不存在")
        return 2
    if not cols_b:
        print(f"ERROR: {table_b} 不存在")
        return 2

    n_a = cur.execute(
        f"SELECT COUNT(*) FROM {table_a} WHERE stock_id = ?",
        (stock_id,),
    ).fetchone()[0]
    n_b = cur.execute(
        f"SELECT COUNT(*) FROM {table_b} WHERE stock_id = ?",
        (stock_id,),
    ).fetchone()[0]
    print(f"{table_a:<24s} stock_id={stock_id}: {n_a} 筆")
    print(f"{table_b:<24s} stock_id={stock_id}: {n_b} 筆")
    print(f"差異：{n_a - n_b:+d}")
    print()

    # A only
    a_only = cur.execute(
        f"SELECT date FROM {table_a} WHERE stock_id = ? "
        f"EXCEPT "
        f"SELECT date FROM {table_b} WHERE stock_id = ? "
        f"ORDER BY date",
        (stock_id, stock_id),
    ).fetchall()
    # B only
    b_only = cur.execute(
        f"SELECT date FROM {table_b} WHERE stock_id = ? "
        f"EXCEPT "
        f"SELECT date FROM {table_a} WHERE stock_id = ? "
        f"ORDER BY date",
        (stock_id, stock_id),
    ).fetchall()

    print(f"--- {table_a} only（{len(a_only)} 筆，{table_b} 沒有但 {table_a} 有的日期）---")
    for r in a_only:
        # 同時檢查那天在 trading_calendar 裡是不是交易日
        is_trading_day = cur.execute(
            "SELECT 1 FROM trading_calendar WHERE date = ?",
            (r["date"],),
        ).fetchone()
        td_flag = "✓ trading_day" if is_trading_day else "✗ NOT trading day"
        print(f"  {r['date']}  ({td_flag})")
        # 列出該筆 A 表的內容（轉成 dict 印重點）
        if len(a_only) <= 10:
            row_a = cur.execute(
                f"SELECT * FROM {table_a} WHERE stock_id = ? AND date = ?",
                (stock_id, r["date"]),
            ).fetchone()
            if row_a:
                payload = {k: row_a[k] for k in row_a.keys() if k not in ("market", "stock_id", "date", "source")}
                print(f"    內容: {payload}")

    print()
    print(f"--- {table_b} only（{len(b_only)} 筆，{table_a} 沒有但 {table_b} 有的日期）---")
    for r in b_only:
        is_trading_day = cur.execute(
            "SELECT 1 FROM trading_calendar WHERE date = ?",
            (r["date"],),
        ).fetchone()
        td_flag = "✓ trading_day" if is_trading_day else "✗ NOT trading day"
        print(f"  {r['date']}  ({td_flag})")
        if len(b_only) <= 10:
            row_b = cur.execute(
                f"SELECT * FROM {table_b} WHERE stock_id = ? AND date = ?",
                (stock_id, r["date"]),
            ).fetchone()
            if row_b:
                payload = {k: row_b[k] for k in row_b.keys() if k not in ("market", "stock_id", "date", "source")}
                print(f"    內容: {payload}")

    conn.close()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
