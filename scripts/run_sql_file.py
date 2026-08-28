# -*- coding: utf-8 -*-
"""psql 不在 PATH 時的 .sql 檔 runner(psycopg 直連)。

用法:
  .venv\\Scripts\\python.exe scripts\\run_sql_file.py scripts\\maintain_facts_stats.sql

已知陷阱處理(CLAUDE.md「已知陷阱」):
  - 逐行剝 `\\` 開頭的 psql 專用行(\\echo / \\timing 非 SQL)與 `--` 註解行,
    再以 `;` 拆語句 — 不以整句開頭是否 `--` 判斷(會誤殺帶前導註解的語句)
  - autocommit 連線:VACUUM / ANALYZE 不能在 transaction block 內執行
  - SELECT 結果直接印出(維護腳本的體檢查詢)
"""
from __future__ import annotations

import os
import sys
import time


def load_dsn() -> str:
    dsn = os.environ.get("DATABASE_URL")
    if dsn:
        return dsn
    try:
        with open(".env", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if line.startswith("DATABASE_URL="):
                    return line.split("=", 1)[1].strip().strip('"').strip("'")
    except OSError:
        pass
    return "postgresql://twstock:twstock@localhost:5432/twstock"


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: run_sql_file.py <path/to/file.sql>")
        return 2
    path = sys.argv[1]
    # utf-8-sig:剝 BOM — Windows PowerShell 5.1 的 Set-Content -Encoding UTF8
    # 寫檔帶 BOM,PG 對 "﻿DELETE" 報 syntax error
    with open(path, encoding="utf-8-sig") as f:
        raw_lines = f.readlines()

    # 剝 psql 專用行與純註解行,保留其餘原樣(含縮排)
    kept: list[str] = []
    for line in raw_lines:
        s = line.strip()
        if s.startswith("\\") or s.startswith("--"):
            continue
        kept.append(line)
    statements = [s.strip() for s in "".join(kept).split(";") if s.strip()]

    import psycopg

    conn = psycopg.connect(load_dsn())
    conn.autocommit = True  # VACUUM / ANALYZE 不可在 transaction 內
    # VACUUM VERBOSE 等 server 訊息直印(否則長句看似無聲卡住)
    conn.add_notice_handler(
        lambda d: print(f"   [notice] {d.message_primary}", flush=True)
    )
    rc = 0
    for i, stmt in enumerate(statements, 1):
        head = " ".join(stmt.split())[:80]
        print(f"[{i}/{len(statements)}] {head} ...", flush=True)
        start = time.monotonic()
        try:
            with conn.cursor() as cur:
                cur.execute(stmt)
                if cur.description is not None:
                    cols = [d.name for d in cur.description]
                    print("   " + " | ".join(cols), flush=True)
                    for row in cur.fetchall():
                        print("   " + " | ".join(str(v) for v in row), flush=True)
        except KeyboardInterrupt:
            print(f"[interrupted] {head} — 使用者中斷,後續語句未執行", flush=True)
            conn.close()
            return 130
        except Exception as e:  # noqa: BLE001 — 維護腳本逐句報錯不中斷
            print(f"[err] {head} — {e}", flush=True)
            rc = 1
            continue
        print(f"[ok] ({time.monotonic() - start:.1f}s)", flush=True)
    conn.close()
    return rc


if __name__ == "__main__":
    sys.exit(main())
