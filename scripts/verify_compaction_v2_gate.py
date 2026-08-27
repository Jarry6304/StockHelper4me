# -*- coding: utf-8 -*-
"""Compaction v2 切換後驗收聚合(m3Spec/neely_compaction_v2.md r4 §9)。

讀 structural_snapshots 每檔最新 daily neely 列(ORDER BY created_at DESC,
同日多 params_hash 陷阱)之 `snapshot->'diagnostics'->'compaction_v2'` 與
`snapshot->'scenario_forest'`(切換後 serving = 凍結 forest),輸出凍結側
驗收報告(shadow 期的 §9.3 召回比對已隨 P0 Gate v3 收案移除 —
docs/benchmarks/neely_compaction_v2_gate_results_2026-08-27.md)。

硬性門檻(任一失敗 exit 1):
  - I1–I6 violation 總和 = 0;w1_violations 總和 = 0
  - Terminal Impulse 存在性:全市場 node_count_by_pattern 含 Diagonal:* > 0
  - **凍結側 forest_size p99 ≤ 40**(§9.2;真 scenario_forest 長度,
    護欄 forest_max_size 200 / BeamSearchFallback 之後)
  - runtime:引擎耗時相對 neely 全程占比 ≤ 2×(stage_8 已為引擎本體)

用法:
  .venv\\Scripts\\python.exe scripts\\verify_compaction_v2_gate.py
  ... --stocks "0050,2330,3363,6547,1312,1213"   # 限定檔複測;PowerShell
      必須加引號 — 未引號逗號串被拆成陣列且 0050 → 50(實測掉檔)
"""
from __future__ import annotations

import argparse
import os
import sys
from collections import Counter


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


def pctile(sorted_vals: list, q: float):
    if not sorted_vals:
        return None
    idx = min(len(sorted_vals) - 1, int(round(q * (len(sorted_vals) - 1))))
    return sorted_vals[idx]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stocks", nargs="+",
                    help="逗號或空白分隔 stock_id(PowerShell 請加引號,防陣列拆分)")
    args = ap.parse_args()

    import psycopg

    conn = psycopg.connect(load_dsn())

    stock_filter = ""
    params: tuple = ()
    if args.stocks:
        ids = [t.strip() for chunk in args.stocks for t in chunk.split(",") if t.strip()]
        stock_filter = " AND stock_id = ANY(%s)"
        params = (ids,)

    cur = conn.cursor(name="gate_scan")
    cur.itersize = 50
    cur.execute(
        f"""
        SELECT DISTINCT ON (stock_id) stock_id,
               snapshot->'diagnostics'->'compaction_v2',
               (snapshot->'diagnostics'->>'elapsed_ms')::bigint,
               jsonb_array_length(COALESCE(snapshot->'scenario_forest', '[]'::jsonb)),
               (snapshot->'diagnostics'->>'overflow_triggered')::boolean
        FROM structural_snapshots
        WHERE core_name = 'neely_core' AND timeframe = 'daily'{stock_filter}
        ORDER BY stock_id, created_at DESC
        """,
        params,
    )

    stocks = 0
    missing = 0
    engines = Counter()
    engine_stocks: dict = {}  # engine -> 前幾檔 stock_id(stale engine 定位用)
    inv_total = 0
    w1_total = 0
    timed_out = 0
    cap_hits = 0
    branch_cap_stocks = 0
    overflow_stocks = 0
    forest_sizes: list[int] = []
    elapsed_ms: list[float] = []
    neely_total_ms = 0
    pattern_counts = Counter()
    level_counts = Counter()
    complexity_counts = Counter()
    triplexity_total = 0
    boundary = Counter()  # checked / info / warn / skipped
    q3_windows = 0
    q3_flips = 0
    w5_rejected = 0
    inv_stocks: list[str] = []
    big_forest: list[tuple[str, int]] = []

    for stock_id, d, neely_ms, forest_len, overflow in cur:
        stocks += 1
        neely_total_ms += neely_ms or 0
        forest_sizes.append(forest_len or 0)
        if forest_len and forest_len > 40:
            big_forest.append((stock_id, forest_len))
        if overflow:
            overflow_stocks += 1
        if not d:
            missing += 1
            continue
        eng = d.get("engine", "?")
        engines[eng] += 1
        lst = engine_stocks.setdefault(eng, [])
        if len(lst) < 10:
            lst.append(stock_id)
        inv = sum((d.get("invariant_violations") or {}).values())
        inv_total += inv
        if inv:
            inv_stocks.append(stock_id)
        w1_total += d.get("w1_violations", 0)
        timed_out += 1 if d.get("timed_out") else 0
        cap_hits += 1 if d.get("level_cap_hit") else 0
        branch_cap_stocks += 1 if d.get("round_branch_cap_hits", 0) else 0
        w5_rejected += d.get("w5_rejected_windows", 0)
        for k, v in (d.get("node_count_by_level") or {}).items():
            level_counts[k] += v
        for k, v in (d.get("node_count_by_pattern") or {}).items():
            pattern_counts[k] += v
        for k, v in (d.get("complexity_count_by_level") or {}).items():
            complexity_counts[k] += v
        triplexity_total += d.get("triplexity_nodes", 0)
        boundary["checked"] += d.get("boundary_pairs_checked", 0)
        boundary["info"] += d.get("boundary_advisory_info", 0)
        boundary["warn"] += d.get("boundary_advisory_warning", 0)
        boundary["skipped"] += d.get("boundary_sides_skipped", 0)
        q3_windows += d.get("q3_windows", 0)
        q3_flips += d.get("q3_flips", 0)
        elapsed_ms.append((d.get("elapsed_us") or 0) / 1000.0)

    cur.close()
    conn.close()

    forest_sizes.sort()
    elapsed_ms.sort()
    terminal_nodes = sum(v for k, v in pattern_counts.items() if k.startswith("Diagonal"))
    p99_forest = pctile(forest_sizes, 0.99) or 0
    shadow_total_s = sum(elapsed_ms) / 1000.0
    neely_total_s = neely_total_ms / 1000.0
    runtime_ratio = (shadow_total_s / neely_total_s) if neely_total_s else 0.0

    print("# Compaction v2 切換後驗收 — 凍結側聚合")
    print()
    print(f"stocks: {stocks}(no compaction_v2 diagnostics: {missing})")
    print(f"engines: {dict(engines)}")
    if len(engines) > 1:
        modal_engine = engines.most_common(1)[0][0]
        for eng, cnt in engines.items():
            if eng != modal_engine:
                print(f"  - stale engine {eng}({cnt} 檔,前 10):{engine_stocks.get(eng, [])}")
    print()
    print("## 硬性門檻(r4 §9.2)")
    gates: list[tuple[str, bool, str]] = [
        ("I1–I6 violations = 0", inv_total == 0,
         f"total={inv_total}" + (f" stocks={inv_stocks[:10]}" if inv_stocks else "")),
        ("w1_violations = 0", w1_total == 0, f"total={w1_total}"),
        ("Terminal Impulse 存在", terminal_nodes > 0, f"Diagonal:* nodes={terminal_nodes}"),
        ("凍結側 forest_size p99 <= 40", p99_forest <= 40,
         f"p50={pctile(forest_sizes, 0.5)} p95={pctile(forest_sizes, 0.95)} p99={p99_forest}"
         + (f";> 40 檔(前 10):{sorted(big_forest, key=lambda t: -t[1])[:10]}" if big_forest else "")),
        ("runtime 引擎占比 <= 2x", runtime_ratio <= 2.0,
         f"engine Σ={shadow_total_s:.1f}s vs neely 全程 Σ={neely_total_s:.1f}s"
         f"(占比 {runtime_ratio:.1%};run-all wall time 受 DB 狀態影響僅附註)"),
    ]
    hard_fail = False
    for name, ok, detail in gates:
        mark = "PASS" if ok else "FAIL"
        if not ok:
            hard_fail = True
        print(f"- [{mark}] {name} — {detail}")
    print("- [手動] RSS <= 1.5x:peak_memory_mb 未填值(backlog),以工作管理員觀測")
    print("- [手動] Level-1 Impulse 抽樣 100 例 R7/Overlap 端點手算一致(§9.2 抽驗)")
    print("- [手動] 前端六檔巢狀 wave_tree Plotly 展開 + 波標密度檢視")
    print()
    print("## 觀測項")
    if stocks:
        print(f"- level_cap_hit 率:{cap_hits}/{stocks} = {cap_hits / stocks:.1%}")
    print(f"- branch cap 命中檔數:{branch_cap_stocks};timed_out:{timed_out};"
          f"overflow(forest_max_size 護欄):{overflow_stocks}")
    if elapsed_ms:
        print(f"- 引擎耗時 ms:p50={pctile(elapsed_ms, 0.5):.1f} p99={pctile(elapsed_ms, 0.99):.1f} "
              f"total={shadow_total_s:.1f}s")
    print(f"- W5 拒絕唯一視窗:{w5_rejected}(RuleRejection 進 diagnostics.rejections)")
    print(f"- Level 分布(收集,護欄前):{dict(sorted(level_counts.items()))}")
    print(f"- Complexity 分布:{dict(sorted(complexity_counts.items()))};triplexity={triplexity_total}")
    checked = boundary["checked"]
    if checked:
        print(f"- §6.1 邊界:checked={checked} info={boundary['info']}({boundary['info'] / checked:.1%}) "
              f"warn={boundary['warn']}({boundary['warn'] / checked:.1%}) skipped={boundary['skipped']}")
    if q3_windows:
        print(f"- Q3 殘差:{q3_flips}/{q3_windows} = {q3_flips / q3_windows:.1%}")
    print()
    print("## Pattern 分布(top 15)")
    for k, v in pattern_counts.most_common(15):
        print(f"- {k}: {v}")

    print()
    print("gate:", "FAIL" if hard_fail else "PASS(硬性自動項)")
    return 1 if hard_fail else 0


if __name__ == "__main__":
    sys.exit(main())
