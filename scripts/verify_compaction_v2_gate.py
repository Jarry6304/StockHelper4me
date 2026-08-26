# -*- coding: utf-8 -*-
"""Compaction v2 P0 Gate v3 全市場聚合(m3Spec/neely_compaction_v2.md §9)。

讀 structural_snapshots 每檔最新 daily neely 列(ORDER BY created_at DESC,
同日多 params_hash 陷阱)之 `snapshot->'diagnostics'->'shadow_compaction'`,
聚合 §9.2 門檻並輸出報告素材(貼入 docs/benchmarks/
neely_compaction_v2_gate_results_<date>.md,格式沿用 P0 Gate v2)。

硬性門檻(任一失敗 exit 1):
  - I1–I6 violation 總和 = 0;w1_violations 總和 = 0
  - Terminal Impulse 存在性:全市場 node_count_by_pattern 含 Diagonal:* > 0
  - forest_size proxy p99 ≤ 40(shadow 期以 collected degree≥1 節點數近似;
    切換後以真 forest_size 複驗)
  - §9.3 召回率 ≥ 98%(允許缺口 (a) beam/dedup 時序 (b) 舊引擎 I1/I2 違規聚合;
    低於門檻檔列出供逐檔 diff)

runtime / RSS 門檻(≤2× baseline / ≤1.5×)為 wall-clock 級,由 run-all 計時
與工作管理員量測,不在本腳本範圍 — 報告模板留欄。

用法:.venv\\Scripts\\python.exe scripts\\verify_compaction_v2_gate.py
"""
from __future__ import annotations

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
    import psycopg

    conn = psycopg.connect(load_dsn())
    cur = conn.cursor(name="gate_scan")
    cur.itersize = 50
    cur.execute(
        """
        SELECT DISTINCT ON (stock_id) stock_id,
               snapshot->'diagnostics'->'shadow_compaction'
        FROM structural_snapshots
        WHERE core_name = 'neely_core' AND timeframe = 'daily'
        ORDER BY stock_id, created_at DESC
        """
    )

    stocks = 0
    missing = 0
    engines = Counter()
    inv_total = 0
    w1_total = 0
    timed_out = 0
    cap_hits = 0
    branch_cap_stocks = 0
    forest_sizes: list[int] = []
    elapsed_ms: list[float] = []
    pattern_counts = Counter()
    level_counts = Counter()
    complexity_counts = Counter()
    triplexity_total = 0
    boundary = Counter()  # checked / info / warn / skipped
    anchors_union = 0
    anchors_overlap = 0
    q3_windows = 0
    q3_flips = 0
    recall_num = 0
    recall_den = 0
    low_recall: list[tuple[str, int, int]] = []
    inv_stocks: list[str] = []

    for stock_id, d in cur:
        stocks += 1
        if not d:
            missing += 1
            continue
        engines[d.get("engine", "?")] += 1
        inv = sum((d.get("invariant_violations") or {}).values())
        inv_total += inv
        if inv:
            inv_stocks.append(stock_id)
        w1_total += d.get("w1_violations", 0)
        timed_out += 1 if d.get("timed_out") else 0
        cap_hits += 1 if d.get("level_cap_hit") else 0
        branch_cap_stocks += 1 if d.get("round_branch_cap_hits", 0) else 0
        levels = d.get("node_count_by_level") or {}
        forest_sizes.append(sum(levels.values()))
        for k, v in levels.items():
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
        anchors_union += d.get("anchors_union_total", 0)
        anchors_overlap += d.get("anchors_overlap_total", 0)
        q3_windows += d.get("q3_windows", 0)
        q3_flips += d.get("q3_flips", 0)
        den = d.get("old_forest_scenarios", 0)
        num = d.get("old_forest_matched", 0)
        recall_num += num
        recall_den += den
        if den and num / den < 0.98:
            low_recall.append((stock_id, num, den))
        elapsed_ms.append((d.get("elapsed_us") or 0) / 1000.0)

    cur.close()
    conn.close()

    forest_sizes.sort()
    elapsed_ms.sort()
    recall = recall_num / recall_den if recall_den else 0.0
    terminal_nodes = sum(v for k, v in pattern_counts.items() if k.startswith("Diagonal"))
    p99_forest = pctile(forest_sizes, 0.99) or 0

    print("# Compaction v2 P0 Gate v3 — 全市場聚合")
    print()
    print(f"stocks: {stocks}(no shadow diagnostics: {missing})")
    print(f"engines: {dict(engines)}")
    print()
    print("## 硬性門檻")
    gates: list[tuple[str, bool, str]] = [
        ("I1–I6 violations = 0", inv_total == 0,
         f"total={inv_total}" + (f" stocks={inv_stocks[:10]}" if inv_stocks else "")),
        ("w1_violations = 0", w1_total == 0, f"total={w1_total}"),
        ("Terminal Impulse 存在", terminal_nodes > 0, f"Diagonal:* nodes={terminal_nodes}"),
        ("forest_size proxy p99 <= 40", p99_forest <= 40,
         f"p50={pctile(forest_sizes, 0.5)} p95={pctile(forest_sizes, 0.95)} p99={p99_forest}"),
        ("§9.3 召回率 >= 98%", recall >= 0.98,
         f"{recall_num}/{recall_den} = {recall:.2%}(低於門檻 {len(low_recall)} 檔)"),
    ]
    hard_fail = False
    for name, ok, detail in gates:
        mark = "PASS" if ok else "FAIL"
        if not ok:
            hard_fail = True
        print(f"- [{mark}] {name} — {detail}")
    print("- [手動] runtime <= 2x baseline / RSS <= 1.5x:run-all 計時與量測後填入報告")
    print("- [手動] Level-1 Impulse 抽樣 100 例 R7/Overlap 端點手算一致(§9.2 抽驗)")
    print("- [手動] 前端六檔巢狀 wave_tree Plotly 展開 + 波標密度檢視")
    print()
    print("## 觀測項")
    print(f"- level_cap_hit 率:{cap_hits}/{stocks} = {cap_hits / stocks:.1%}" if stocks else "")
    print(f"- branch cap 命中檔數:{branch_cap_stocks};timed_out:{timed_out}")
    print(f"- shadow 耗時 ms:p50={pctile(elapsed_ms, 0.5):.1f} p99={pctile(elapsed_ms, 0.99):.1f} "
          f"total={sum(elapsed_ms) / 1000.0:.1f}s" if elapsed_ms else "")
    print(f"- Level 分布:{dict(sorted(level_counts.items()))}")
    print(f"- Complexity 分布:{dict(sorted(complexity_counts.items()))};triplexity={triplexity_total}")
    checked = boundary["checked"]
    if checked:
        print(f"- §6.1 邊界:checked={checked} info={boundary['info']}({boundary['info'] / checked:.1%}) "
              f"warn={boundary['warn']}({boundary['warn'] / checked:.1%}) skipped={boundary['skipped']}")
    print(f"- A-10 anchors:union={anchors_union} overlap={anchors_overlap}"
          f"(高估 {1 - anchors_union / anchors_overlap:.1%})" if anchors_overlap else "")
    print(f"- Q3 殘差:{q3_flips}/{q3_windows}"
          f" = {q3_flips / q3_windows:.1%}" if q3_windows else "")
    print()
    print("## Pattern 分布(top 15)")
    for k, v in pattern_counts.most_common(15):
        print(f"- {k}: {v}")
    if low_recall:
        print()
        print("## 召回率 < 98% 檔(前 20;逐檔 diff 判 (a)/(b) 允許類別)")
        for sid, num, den in sorted(low_recall, key=lambda t: t[1] / t[2])[:20]:
            print(f"- {sid}: {num}/{den} = {num / den:.1%}")

    print()
    print("gate:", "FAIL" if hard_fail else "PASS(硬性自動項)")
    return 1 if hard_fail else 0


if __name__ == "__main__":
    sys.exit(main())
