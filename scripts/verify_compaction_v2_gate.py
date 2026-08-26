# -*- coding: utf-8 -*-
"""Compaction v2 P0 Gate v3 全市場聚合(m3Spec/neely_compaction_v2.md §9)。

讀 structural_snapshots 每檔最新 daily neely 列(ORDER BY created_at DESC,
同日多 params_hash 陷阱)之 `snapshot->'diagnostics'->'shadow_compaction'`,
聚合 §9.2 門檻並輸出報告素材(貼入 docs/benchmarks/
neely_compaction_v2_gate_results_<date>.md,格式沿用 P0 Gate v2)。

硬性門檻(任一失敗 exit 1):
  - I1–I6 violation 總和 = 0;w1_violations 總和 = 0
  - Terminal Impulse 存在性:全市場 node_count_by_pattern 含 Diagonal:* > 0
  - §9.3 召回率 ≥ 98%(允許缺口 (a) beam/dedup 時序 (b) 舊引擎 I1/I2 違規聚合;
    低於門檻檔以 --diff 逐檔分類;全聚合並印召回驗屍 —
    未召回舊 scenario 的第一個拒絕階段分布)

forest proxy(收集全量,§7.1 凍結護欄前)自 G2.4 收集修正後轉觀測項,
p99 ≤ 40 門檻於切換後以真 forest_size 判。

runtime:§9.2 門檻的公平量測 = shadow(切換後取代 stage_8)相對整體 neely
compute 的占比;run-all 總 wall time 受 DB 狀態影響僅作附註。RSS 門檻:
diagnostics peak_memory_mb 未填值(全 0,backlog),以工作管理員觀測。

用法:
  .venv\\Scripts\\python.exe scripts\\verify_compaction_v2_gate.py
  ... --stocks "0050,2330,3363,6547,1312,1213"   # 限定檔複測;PowerShell
      必須加引號 — 未引號逗號串被拆成陣列且 0050 → 50(實測掉檔)
  ... --diff 2330                            # 單檔召回缺口分類報告
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


def pattern_tag(pt) -> str:
    """對齊 Rust round_engine::pattern_tag 的 serde JSON 形狀。"""
    if isinstance(pt, str):
        return pt
    if isinstance(pt, dict):
        k = next(iter(pt))
        v = pt[k]
        if isinstance(v, dict):
            if "sub_kind" in v:
                return "%s:%s" % (k, v["sub_kind"])
            if "sub_kinds" in v:
                return "Combination:%s" % "+".join(v["sub_kinds"])
        return k
    return str(pt)


def diff_one_stock(conn, stock_id: str) -> int:
    """§9.3 召回缺口逐檔分類:exact / tag 變體差 / bar 近錯位(±3,Neutral
    橋接嫌疑)/ 視窗缺席。資料源 = 舊 forest(scenario_forest)vs
    diagnostics.degree1_node_keys(G2.4 起序列化)。"""
    cur = conn.cursor()
    cur.execute(
        """SELECT snapshot FROM structural_snapshots
           WHERE stock_id=%s AND core_name='neely_core' AND timeframe='daily'
           ORDER BY created_at DESC LIMIT 1""",
        (stock_id,),
    )
    row = cur.fetchone()
    if not row:
        print(f"{stock_id}: no snapshot")
        return 1
    snap = row[0]
    d = (snap.get("diagnostics") or {}).get("shadow_compaction") or {}
    keys_raw = d.get("degree1_node_keys")
    if keys_raw is None:
        print(f"{stock_id}: engine={d.get('engine')} 無 degree1_node_keys — 需 g2.4+ binary 重跑本檔")
        return 1
    new_exact = set()
    new_by_range = {}
    new_by_tag: dict = {}
    for ks in keys_raw:
        rng, tag = ks.split(":", 1)
        s, e = (int(x) for x in rng.split("-"))
        new_exact.add((s, e, tag))
        new_by_range.setdefault((s, e), []).append(tag)
        new_by_tag.setdefault(tag, []).append((s, e))

    mws = snap.get("monowave_series") or []
    start_map = {}
    end_map = {}
    for m in mws:
        start_map.setdefault(m["start_date"], m["bar_indices"][0])
        end_map.setdefault(m["end_date"], m["bar_indices"][1])

    cats = Counter()
    samples: dict = {}
    for sc in snap.get("scenario_forest") or []:
        wt = sc.get("wave_tree") or {}
        sb = start_map.get(wt.get("start"), end_map.get(wt.get("start")))
        eb = end_map.get(wt.get("end"), start_map.get(wt.get("end")))
        tag = pattern_tag(sc.get("pattern_type"))
        if sb is None or eb is None:
            cats["unresolvable_dates"] += 1
            continue
        if (sb, eb, tag) in new_exact:
            cats["exact"] += 1
            continue
        if (sb, eb) in new_by_range:
            cats["tag_mismatch"] += 1
            samples.setdefault("tag_mismatch", []).append(
                f"{sb}-{eb} old={tag} new={new_by_range[(sb, eb)]}"
            )
            continue
        near = [
            (ns, ne)
            for (ns, ne) in new_by_tag.get(tag, [])
            if abs(ns - sb) <= 3 and abs(ne - eb) <= 3
        ]
        if near:
            cats["bar_offset_le3"] += 1
            samples.setdefault("bar_offset_le3", []).append(
                f"old {sb}-{eb} {tag} vs new {near[:2]}"
            )
        else:
            cats["absent"] += 1
            samples.setdefault("absent", []).append(f"{sb}-{eb} {tag}")

    total = sum(cats.values())
    print(f"# §9.3 召回 diff — {stock_id}(old forest {total} / new degree-1 {len(keys_raw)})")
    for k in ["exact", "tag_mismatch", "bar_offset_le3", "absent", "unresolvable_dates"]:
        if cats[k]:
            pct = cats[k] / total if total else 0
            print(f"- {k}: {cats[k]} ({pct:.1%})")
    for k, rows in samples.items():
        print(f"\n## {k} 樣本(<=10)")
        for r in rows[:10]:
            print(f"  {r}")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stocks", nargs="+",
                    help="逗號或空白分隔 stock_id(PowerShell 請加引號,防陣列拆分)")
    ap.add_argument("--diff", help="單檔 §9.3 召回缺口分類報告")
    args = ap.parse_args()

    import psycopg

    conn = psycopg.connect(load_dsn())
    if args.diff:
        rc = diff_one_stock(conn, args.diff)
        conn.close()
        return rc

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
               snapshot->'diagnostics'->'shadow_compaction',
               (snapshot->'diagnostics'->>'elapsed_ms')::bigint
        FROM structural_snapshots
        WHERE core_name = 'neely_core' AND timeframe = 'daily'{stock_filter}
        ORDER BY stock_id, created_at DESC
        """,
        params,
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
    neely_total_ms = 0
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
    miss_by_stage = Counter()
    low_recall: list[tuple[str, int, int]] = []
    inv_stocks: list[str] = []

    for stock_id, d, neely_ms in cur:
        stocks += 1
        neely_total_ms += neely_ms or 0
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
        for k, v in (d.get("recall_miss_by_stage") or {}).items():
            miss_by_stage[k] += v
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

        ("§9.3 召回率 >= 98%", recall >= 0.98,
         f"{recall_num}/{recall_den} = {recall:.2%}(低於門檻 {len(low_recall)} 檔;--diff 分類)"),
    ]
    hard_fail = False
    for name, ok, detail in gates:
        mark = "PASS" if ok else "FAIL"
        if not ok:
            hard_fail = True
        print(f"- [{mark}] {name} — {detail}")
    print(f"- [觀測] forest proxy(收集全量,§7.1 護欄前;凍結側才判 p99<=40)"
          f" p50={pctile(forest_sizes, 0.5)} p95={pctile(forest_sizes, 0.95)} p99={p99_forest}")
    shadow_total_s = sum(elapsed_ms) / 1000.0
    neely_total_s = neely_total_ms / 1000.0
    if neely_total_s:
        print(f"- [runtime] shadow Σ={shadow_total_s:.1f}s vs neely 全程 Σ={neely_total_s:.1f}s"
              f"(占比 {shadow_total_s / neely_total_s:.1%};切換後取代 stage_8,"
              f"≤ 2× 門檻以此為據;run-all wall time 受 DB 狀態影響僅附註)")
    print("- [手動] RSS <= 1.5x:peak_memory_mb 未填值(backlog),以工作管理員觀測")
    print("- [手動] Level-1 Impulse 抽樣 100 例 R7/Overlap 端點手算一致(§9.2 抽驗)")
    print("- [手動] 前端六檔巢狀 wave_tree Plotly 展開 + 波標密度檢視")
    print()
    print("## 觀測項")
    print(f"- level_cap_hit 率:{cap_hits}/{stocks} = {cap_hits / stocks:.1%}" if stocks else "")
    print(f"- branch cap 命中檔數:{branch_cap_stocks};timed_out:{timed_out}")
    print(f"- shadow 耗時 ms:p50={pctile(elapsed_ms, 0.5):.1f} p99={pctile(elapsed_ms, 0.99):.1f} "
          f"total={shadow_total_s:.1f}s" if elapsed_ms else "")
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
    if miss_by_stage:
        total_miss = sum(miss_by_stage.values())
        print()
        print("## §9.3 召回驗屍(未召回舊 scenario 的第一個拒絕階段)")
        for k, v in miss_by_stage.most_common():
            print(f"- {k}: {v} ({v / total_miss:.1%})")
    print()
    print("## Pattern 分布(top 15)")
    for k, v in pattern_counts.most_common(15):
        print(f"- {k}: {v}")
    if low_recall:
        print()
        print("## 召回率 < 98% 檔(前 20;--diff <stock> 逐檔分類 (a)/(b) 允許類別)")
        for sid, num, den in sorted(low_recall, key=lambda t: t[1] / t[2])[:20]:
            print(f"- {sid}: {num}/{den} = {num / den:.1%}")

    print()
    print("gate:", "FAIL" if hard_fail else "PASS(硬性自動項)")
    return 1 if hard_fail else 0


if __name__ == "__main__":
    sys.exit(main())
