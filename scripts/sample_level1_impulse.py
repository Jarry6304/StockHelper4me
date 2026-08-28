# -*- coding: utf-8 -*-
"""Level-1 Impulse 抽驗(m3Spec/neely_compaction_v2.md r5 §9.2「抽樣 R7/Overlap
端點手算一致」)— 把端點數學直接算出來,人工抽列核對即可。

對每檔最新 daily neely snapshot 的 degree-1 Impulse scenario:
  - 子波幅度 |W1..W5| 由 wave_tree children 日期反查 monowave_series 端點價
    (合成葉:start 取 start_date 對應 monowave 的 start_price、end 取
    end_date 對應 monowave 的 end_price — 與引擎橋接語意一致)
  - R7:W3 非最短(不同時短於 W1 與 W5)
  - Overlap_Trending:W1 端點區間與 W4 端點區間不重疊(Trending Impulse
    要件;Terminal 以 Diagonal 表徵,不在本抽驗)

全體 Level-1 Impulse 逐筆檢核(涵蓋面 ≥ spec 的 100 例),違反 = exit 1;
另印隨機樣本供人工核對。

用法:
  .venv\\Scripts\\python.exe scripts\\sample_level1_impulse.py [--show 20] [--seed 42]
"""
from __future__ import annotations

import argparse
import os
import random
import sys


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
    ap = argparse.ArgumentParser()
    ap.add_argument("--show", type=int, default=20, help="印出的隨機樣本列數")
    ap.add_argument("--seed", type=int, default=42)
    args = ap.parse_args()

    import psycopg

    conn = psycopg.connect(load_dsn())
    cur = conn.cursor(name="l1_scan")
    cur.itersize = 20
    cur.execute(
        """
        SELECT DISTINCT ON (stock_id) stock_id,
               snapshot->'scenario_forest',
               snapshot->'monowave_series'
        FROM structural_snapshots
        WHERE core_name = 'neely_core' AND timeframe = 'daily'
        ORDER BY stock_id, created_at DESC
        """
    )

    rows: list[tuple] = []  # (stock, id, mags(5), r7_ok, overlap)
    unresolved = 0

    for stock_id, forest, mws in cur:
        if not forest or not mws:
            continue
        start_price = {m["start_date"]: m["start_price"] for m in mws}
        end_price = {m["end_date"]: m["end_price"] for m in mws}
        for sc in forest:
            pt = sc.get("pattern_type")
            if pt != "Impulse":
                continue
            wt = sc.get("wave_tree") or {}
            if wt.get("degree_level") != 1:
                continue
            children = wt.get("children") or []
            if len(children) != 5:
                continue
            spans = []
            ok = True
            for c in children:
                sp = start_price.get(c.get("start"))
                ep = end_price.get(c.get("end"))
                if sp is None or ep is None:
                    ok = False
                    break
                spans.append((float(sp), float(ep)))
            if not ok:
                unresolved += 1
                continue
            mags = [abs(ep - sp) for sp, ep in spans]
            r7_ok = not (mags[2] < mags[0] and mags[2] < mags[4])
            w1_lo, w1_hi = min(spans[0]), max(spans[0])
            w4_lo, w4_hi = min(spans[3]), max(spans[3])
            overlap = w1_lo <= w4_hi and w4_lo <= w1_hi
            rows.append((stock_id, sc.get("id"), mags, r7_ok, overlap))

    cur.close()
    conn.close()

    r7_bad = [r for r in rows if not r[3]]
    ov_bad = [r for r in rows if r[4]]
    print("# Level-1 Impulse 抽驗(r5 §9.2)")
    print(f"檢核母體:{len(rows)} 個 degree-1 Impulse(端點無法反查跳過 {unresolved})")
    print(f"- R7(W3 非最短):{len(rows) - len(r7_bad)}/{len(rows)} 過"
          + (f";違反:{[(r[0], r[1]) for r in r7_bad[:10]]}" if r7_bad else ""))
    print(f"- Overlap_Trending(W1/W4 端點區間不重疊):{len(rows) - len(ov_bad)}/{len(rows)} 過"
          + (f";違反:{[(r[0], r[1]) for r in ov_bad[:10]]}" if ov_bad else ""))

    random.seed(args.seed)
    sample = random.sample(rows, min(args.show, len(rows)))
    print()
    print(f"## 隨機樣本(seed={args.seed},{len(sample)} 列;人工核對用)")
    print("stock | id | |W1| |W2| |W3| |W4| |W5| | R7 | W1∩W4")
    for stock_id, sid, mags, r7_ok, overlap in sample:
        m = " ".join(f"{x:.2f}" for x in mags)
        print(f"{stock_id} | {sid} | {m} | {'ok' if r7_ok else 'FAIL'} | "
              f"{'overlap!' if overlap else 'none'}")

    verdict = not r7_bad and not ov_bad
    print()
    print("verdict:", "PASS(全數端點一致)" if verdict else "FAIL(見違反清單)")
    return 0 if verdict else 1


if __name__ == "__main__":
    sys.exit(main())
