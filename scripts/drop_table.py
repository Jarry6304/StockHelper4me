"""
drop_table.py
-------------
DROP 指定的資料表（用於 schema 變更後手動重建單一表，避免整個 db 重灌）。

執行：
    python scripts/drop_table.py institutional_market_daily
    python scripts/drop_table.py institutional_market_daily fear_greed_index   # 多張一起
"""

from __future__ import annotations

import sqlite3
import sys
from pathlib import Path

DB_PATH = Path(__file__).resolve().parent.parent / "data" / "tw_stock.db"


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print("用法：python scripts/drop_table.py <table_name> [<table_name> ...]")
        return 2
    if not DB_PATH.exists():
        print(f"ERROR: DB 不存在: {DB_PATH}")
        return 2

    conn = sqlite3.connect(DB_PATH)
    cur = conn.cursor()

    for table in argv[1:]:
        # 確認表存在 + 列出筆數，再決定是否 drop
        row = cur.execute(
            "SELECT name FROM sqlite_master WHERE type='table' AND name = ?",
            (table,),
        ).fetchone()
        if not row:
            print(f"  {table}: (表不存在，略過)")
            continue
        n = cur.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
        cur.execute(f"DROP TABLE {table}")
        print(f"  {table}: dropped (had {n} rows)")

    conn.commit()
    conn.close()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
