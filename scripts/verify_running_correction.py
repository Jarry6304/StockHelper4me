# -*- coding: utf-8 -*-
"""Running Correction 判準 verify(m3Spec/neely_ch6_gate_running_fix.md 驗收)—
鏡射 sample_level1_impulse.py 反查法。

neely 1.2.0 起 `is_running_correction` = `a > 0 && b > a + c`(c 終點未回到
a 起點;classifier/flat_classifier.rs)。本 script 對全體最新 daily snapshot 的
RunningCorrection scenario(任意 degree)以 wave_tree children 日期反查
monowave_series 端點價算 (|a|, |b|, |c|),**b > a + c 100% 成立**;
不一致 = exit 1(引擎與判準漂移,或舊版 facts 未重生)。

用法:
  .venv\\Scripts\\python.exe scripts\\verify_running_correction.py [--show 20] [--seed 42]
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
    cur = conn.cursor(name="rc_scan")
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

    # (stock, id, degree, (a, b, c), holds)
    rows: list[tuple] = []
    unresolved = 0

    for stock_id, forest, mws in cur:
        if not forest or not mws:
            continue
        start_price = {m["start_date"]: m["start_price"] for m in mws}
        end_price = {m["end_date"]: m["end_price"] for m in mws}
        for sc in forest:
            if sc.get("pattern_type") != "RunningCorrection":
                continue
            wt = sc.get("wave_tree") or {}
            children = wt.get("children") or []
            if len(children) != 3:
                continue
            mags = []
            ok = True
            for c in children:
                sp = start_price.get(c.get("start"))
                ep = end_price.get(c.get("end"))
                if sp is None or ep is None:
                    ok = False
                    break
                mags.append(abs(float(ep) - float(sp)))
            if not ok:
                unresolved += 1
                continue
            a, b, c_mag = mags
            holds = a > 0.0 and b > a + c_mag
            rows.append((stock_id, sc.get("id"), wt.get("degree_level"),
                         (a, b, c_mag), holds))

    cur.close()
    conn.close()

    bad = [r for r in rows if not r[4]]
    print("# RunningCorrection 判準 verify(1.2.0 b > a + c;與 flat_classifier 同源)")
    print(f"檢核母體:{len(rows)} 個 RunningCorrection scenario"
          f"(端點無法反查跳過 {unresolved})")
    print(f"- b > a + c:{len(rows) - len(bad)}/{len(rows)} 成立"
          + (f";不一致:{[(r[0], r[1]) for r in bad[:10]]}" if bad else ""))

    random.seed(args.seed)
    sample = random.sample(rows, min(args.show, len(rows)))
    print()
    print(f"## 隨機樣本(seed={args.seed},{len(sample)} 列;人工核對用)")
    print("stock | id | L | |a| |b| |c| | b>a+c")
    for stock_id, sid, degree, (a, b, c_mag), holds in sample:
        print(f"{stock_id} | {sid} | {degree} | {a:.2f} {b:.2f} {c_mag:.2f} | "
              f"{'ok' if holds else 'FAIL'}")

    verdict = not bad
    print()
    print("verdict:", "PASS(100% 成立)" if verdict else "FAIL(見不一致清單)")
    return 0 if verdict else 1


if __name__ == "__main__":
    sys.exit(main())
