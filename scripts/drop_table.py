"""
drop_table.py
-------------
DROP 指定的資料表（用於 schema 變更後手動重建單一表，避免整個 db 重灌）。
連帶清掉 api_sync_progress 中對應該表的進度記錄，避免重跑時被誤判 completed。

執行：
    python scripts/drop_table.py institutional_market_daily
    python scripts/drop_table.py institutional_market_daily fear_greed_index   # 多張一起
"""

from __future__ import annotations

import sqlite3
import sys
import tomllib
from pathlib import Path

ROOT       = Path(__file__).resolve().parent.parent
DB_PATH    = ROOT / "data" / "tw_stock.db"
TOML_PATH  = ROOT / "config" / "collector.toml"


def _load_table_to_apis() -> dict[str, list[str]]:
    """從 collector.toml 建立 target_table → [api_name, ...] 反向索引"""
    if not TOML_PATH.exists():
        return {}
    with TOML_PATH.open("rb") as f:
        cfg = tomllib.load(f)
    mapping: dict[str, list[str]] = {}
    for api in cfg.get("api", []):
        tgt  = api.get("target_table")
        name = api.get("name")
        if tgt and name:
            mapping.setdefault(tgt, []).append(name)
    return mapping


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print("用法：python scripts/drop_table.py <table_name> [<table_name> ...]")
        return 2
    if not DB_PATH.exists():
        print(f"ERROR: DB 不存在: {DB_PATH}")
        return 2

    table_to_apis = _load_table_to_apis()

    conn = sqlite3.connect(DB_PATH)
    cur = conn.cursor()

    for table in argv[1:]:
        api_names = table_to_apis.get(table, [])

        # 確認表存在 + 列出筆數，再決定是否 drop
        row = cur.execute(
            "SELECT name FROM sqlite_master WHERE type='table' AND name = ?",
            (table,),
        ).fetchone()
        if row:
            n = cur.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
            cur.execute(f"DROP TABLE {table}")
            drop_msg = f"dropped (had {n} rows)"
        else:
            drop_msg = "(表不存在)"

        # 不論表存在與否，都清掉對應 api_sync_progress（避免重跑被當 completed 跳過）
        progress_deleted = 0
        if api_names:
            placeholders = ", ".join(["?"] * len(api_names))
            res = cur.execute(
                f"DELETE FROM api_sync_progress WHERE api_name IN ({placeholders})",
                api_names,
            )
            progress_deleted = res.rowcount
            print(
                f"  {table}: {drop_msg} + "
                f"cleared {progress_deleted} progress rows for api={api_names}"
            )
        else:
            print(
                f"  {table}: {drop_msg} "
                f"(無對應 api_name，未清 api_sync_progress)"
            )

    conn.commit()
    conn.close()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
