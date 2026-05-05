"""
cleanup_non_trading_days.py
---------------------------
一次性清理：刪掉 institutional_daily / institutional_market_daily
中 date 不在 trading_calendar 的鬼資料（FinMind 偶爾在週六回殘留值）。

執行：
    python scripts/cleanup_non_trading_days.py             # dry-run，只列出會被刪的列
    python scripts/cleanup_non_trading_days.py --apply     # 實際刪除

之後新版 aggregator 會在資料寫入前用 trading_calendar 過濾，這個 script
只是清現存歷史資料。
"""

from __future__ import annotations

import sqlite3
import sys
from pathlib import Path

DB_PATH = Path(__file__).resolve().parent.parent / "data" / "tw_stock.db"

TARGET_TABLES = ["institutional_daily", "institutional_market_daily"]


def main(argv: list[str]) -> int:
    apply = "--apply" in argv[1:]
    if not DB_PATH.exists():
        print(f"ERROR: DB 不存在: {DB_PATH}")
        return 2

    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    cur = conn.cursor()

    n_cal = cur.execute("SELECT COUNT(*) FROM trading_calendar").fetchone()[0]
    if n_cal == 0:
        print("ERROR: trading_calendar 為空，無法判斷哪些是非交易日。先跑 Phase 1。")
        return 2
    print(f"trading_calendar 有 {n_cal} 個交易日")
    print(f"模式：{'APPLY (實際刪除)' if apply else 'DRY-RUN (只列出)'}")
    print()

    total_deleted = 0
    for tbl in TARGET_TABLES:
        # 確認表存在
        exists = cur.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?",
            (tbl,),
        ).fetchone()
        if not exists:
            print(f"  {tbl}: (表不存在，略過)")
            continue

        rogue = cur.execute(
            f"SELECT date, COUNT(*) AS n FROM {tbl} "
            f"WHERE date NOT IN (SELECT date FROM trading_calendar) "
            f"GROUP BY date ORDER BY date"
        ).fetchall()

        if not rogue:
            print(f"  {tbl}: 無鬼資料")
            continue

        n_total = sum(r["n"] for r in rogue)
        print(f"  {tbl}: 找到 {len(rogue)} 個非交易日，共 {n_total} 列")
        for r in rogue:
            print(f"    {r['date']}  {r['n']:>4d} 列")

        if apply:
            res = cur.execute(
                f"DELETE FROM {tbl} "
                f"WHERE date NOT IN (SELECT date FROM trading_calendar)"
            )
            print(f"    → 已刪除 {res.rowcount} 列")
            total_deleted += res.rowcount

    if apply:
        conn.commit()
        print()
        print(f"總共刪除 {total_deleted} 列")
    else:
        print()
        print("DRY-RUN 結束。確認上面清單後加 --apply 真的刪除。")

    conn.close()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
