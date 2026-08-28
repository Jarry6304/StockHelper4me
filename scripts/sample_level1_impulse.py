# -*- coding: utf-8 -*-
"""Level-1 Impulse 抽驗(m3Spec/neely_compaction_v2.md r5 §9.2「抽樣 R7/Overlap
端點手算一致」)— 端點數學與 validator 同源重算,人工抽列核對即可。

對每檔最新 daily neely snapshot 的 degree-1 Impulse scenario:
  - 子波幅度 |W1..W5| 由 wave_tree children 日期反查 monowave_series 端點價
    (合成葉:start 取 start_date 對應 monowave 的 start_price、end 取
    end_date 對應 monowave 的 end_price — 與引擎 synth_window 語意一致)
  - R7(validator/core_rules.rs rule_r7):raw fail = W3 < min(W1, W5);
    整體接受套 Ch9 Exception Rule(validator/mod.rs)— 單一 essential fail
    且 gap < 10% 仍 overall_pass,故只有 gap ≥ 10% 才列不一致
  - Overlap_Trending(core_rules.rs rule_overlap_trending):W4 **終點**不可
    進入 W2 區 — Up:W4.end ≥ W2.end;Down:W4.end ≤ W2.end;W1 Neutral
    → NotApplicable。注意:**不是** W1/W4 價格範圍交集(W4 回落進 W1 區間
    但守住 W2 終點是合法 Trending Impulse);範圍交集判準屬 Terminal
    (Diagonal)分類線索,兩條 Overlap 規則互為排他補集
    (core_rules.rs rule_overlap_terminal 註解)

全體 Level-1 Impulse 逐筆檢核(涵蓋面 ≥ spec 的 100 例),不一致 = exit 1;
另印隨機樣本供人工核對。

用法:
  .venv\\Scripts\\python.exe scripts\\sample_level1_impulse.py [--show 20] [--seed 42]
"""
from __future__ import annotations

import argparse
import os
import random
import sys

# validator/mod.rs CH9_EXCEPTION_GAP_PCT — 單一 essential fail 的容差
CH9_EXCEPTION_GAP_PCT = 10.0


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

    # (stock, id, mags(5), r7_gap_pct 或 None=raw pass, overlap_violated)
    rows: list[tuple] = []
    unresolved = 0
    neutral_w1 = 0

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

            # R7:raw fail = W3 < min(W1, W5);gap 同 rule_r7 公式
            min_w1_w5 = min(mags[0], mags[4])
            r7_gap = None
            if mags[2] < min_w1_w5:
                r7_gap = (min_w1_w5 - mags[2]) / max(min_w1_w5, 1e-9) * 100.0

            # Overlap_Trending:W4 終點 vs W2 終點(方向由 W1 端點決定)
            w1_sp, w1_ep = spans[0]
            w2_end = spans[1][1]
            w4_end = spans[3][1]
            if w1_ep > w1_sp:  # Up
                ov_violated = w4_end < w2_end
            elif w1_ep < w1_sp:  # Down
                ov_violated = w4_end > w2_end
            else:  # Neutral → NotApplicable
                neutral_w1 += 1
                ov_violated = False
            rows.append((stock_id, sc.get("id"), mags, r7_gap, ov_violated))

    cur.close()
    conn.close()

    r7_raw = [r for r in rows if r[3] is not None]
    r7_bad = [r for r in r7_raw if r[3] >= CH9_EXCEPTION_GAP_PCT]
    r7_ch9 = len(r7_raw) - len(r7_bad)
    ov_bad = [r for r in rows if r[4]]
    print("# Level-1 Impulse 抽驗(r5 §9.2,判準與 validator 同源)")
    print(f"檢核母體:{len(rows)} 個 degree-1 Impulse"
          f"(端點無法反查跳過 {unresolved};W1 Neutral 之 Overlap N/A {neutral_w1})")
    print(f"- R7(W3 ≥ min(W1,W5),Ch9 容差 gap<{CH9_EXCEPTION_GAP_PCT:.0f}%):"
          f"{len(rows) - len(r7_bad)}/{len(rows)} 過"
          f"(raw fail {len(r7_raw)},其中 Ch9 容忍 {r7_ch9})"
          + (f";不一致:{[(r[0], r[1], round(r[3], 1)) for r in r7_bad[:10]]}" if r7_bad else ""))
    print(f"- Overlap_Trending(W4 終點不進 W2 區):{len(rows) - len(ov_bad)}/{len(rows)} 過"
          + (f";不一致:{[(r[0], r[1]) for r in ov_bad[:10]]}" if ov_bad else ""))

    random.seed(args.seed)
    sample = random.sample(rows, min(args.show, len(rows)))
    print()
    print(f"## 隨機樣本(seed={args.seed},{len(sample)} 列;人工核對用)")
    print("stock | id | |W1| |W2| |W3| |W4| |W5| | R7 | Overlap")
    for stock_id, sid, mags, r7_gap, ov_violated in sample:
        m = " ".join(f"{x:.2f}" for x in mags)
        if r7_gap is None:
            r7s = "ok"
        elif r7_gap < CH9_EXCEPTION_GAP_PCT:
            r7s = f"ch9({r7_gap:.1f}%)"
        else:
            r7s = f"FAIL({r7_gap:.1f}%)"
        print(f"{stock_id} | {sid} | {m} | {r7s} | "
              f"{'VIOLATED' if ov_violated else 'ok'}")

    verdict = not r7_bad and not ov_bad
    print()
    print("verdict:", "PASS(全數端點一致)" if verdict else "FAIL(見不一致清單)")
    return 0 if verdict else 1


if __name__ == "__main__":
    sys.exit(main())
