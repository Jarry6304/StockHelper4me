"""
verify_mcp_toolkit_v4_29.py
===========================
v4.29 verify pipeline:13 個 public MCP tool 全覆蓋健康度檢查。

對齊 v4.29 main 合入後的 toolkit(對齊 mcp_server/server.py active mcp.tool() 13 個):

  per-stock(4):
    1. neely_forecast(stock_id, date)
    2. kalman_trend(stock_id, date)
    3. stock_snapshot(stock_id, date)            ← v3.31 10-in-1
    4. stock_levels(stock_id, date)              ← v4.19 B 視角

  cross-stock screens(5):
    5. magic_formula_screen(date)
    6. monthly_screen(date)                       ← v3.32 Toolkit A
    7. quarterly_screen(date)                     ← v3.32 Toolkit B
    8. annual_low_risk_screen(date)               ← v3.32 Toolkit C
    9. scan_wave_impulse(date)                    ← v4.26 cross_cores 12th builder

  consolidated(3):
   10. market_overview(date)                      ← v4.19 D 視角(dashboard + events)
   11. indicators(stock_id, date)                 ← v4.19 E 視角

  dual-track:
   12. monthly_trigger_scan(date)                 ← Layer 5
   13. dual_track_resonance(stock_id, date)       ← v4.25

判定:per tool 跑一次 → 抓 exception / 量檢 payload / sanity 欄位。
退出碼:0 = 全綠 / 1 = 任一 FAIL。

用法:
  python scripts/verify_mcp_toolkit_v4_29.py
  python scripts/verify_mcp_toolkit_v4_29.py --stocks 2330,3030
  python scripts/verify_mcp_toolkit_v4_29.py --as-of 2026-05-15
  python scripts/verify_mcp_toolkit_v4_29.py --verbose         # 顯示每 tool 完整 payload 摘要
"""

from __future__ import annotations

import argparse
import json
import sys
import time
import traceback
from datetime import date as date_t
from pathlib import Path
from typing import Any, Callable

_REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(_REPO_ROOT / "src"))
sys.path.insert(0, str(_REPO_ROOT))

# Payload size budget for MCP context(soft warning > 50KB,hard fail > 1MB)
_PAYLOAD_SOFT_KB = 50
_PAYLOAD_HARD_KB = 1024


def _payload_size_kb(obj: Any) -> float:
    """Estimate MCP-serialized payload size in KB."""
    try:
        return len(json.dumps(obj, default=str, ensure_ascii=False)) / 1024
    except Exception:
        return -1.0


def _check_call(
    name: str, fn: Callable[[], Any], *, must_have_keys: list[str] | None = None,
) -> dict[str, Any]:
    """Generic tool call wrapper:run fn,catch exception,assess payload + keys."""
    t0 = time.monotonic()
    try:
        out = fn()
    except Exception as e:
        return {
            "tool": name,
            "status": "ERROR",
            "elapsed_s": round(time.monotonic() - t0, 2),
            "exc_type": type(e).__name__,
            "note": f"{type(e).__name__}: {e}",
            "traceback": traceback.format_exc(),
        }
    elapsed = round(time.monotonic() - t0, 2)
    kb = _payload_size_kb(out)
    issues: list[str] = []

    if not isinstance(out, dict):
        return {"tool": name, "status": "FAIL", "elapsed_s": elapsed,
                "note": f"output not dict (got {type(out).__name__})", "payload_kb": kb}

    if kb > _PAYLOAD_HARD_KB:
        issues.append(f"PAYLOAD_HARD>{_PAYLOAD_HARD_KB}KB({kb:.0f})")
    elif kb > _PAYLOAD_SOFT_KB:
        issues.append(f"payload>{_PAYLOAD_SOFT_KB}KB({kb:.0f})")

    if must_have_keys:
        for k in must_have_keys:
            if k not in out:
                issues.append(f"missing_key={k}")

    return {
        "tool": name, "elapsed_s": elapsed, "payload_kb": round(kb, 1),
        "status": ("WARN" if (issues and not any(i.startswith("PAYLOAD_HARD") or i.startswith("missing_key") for i in issues))
                   else ("FAIL" if issues else "OK")),
        "note": "; ".join(issues) if issues else _summary_note(name, out),
        "raw": out,
    }


def _summary_note(name: str, out: dict[str, Any]) -> str:
    """Per-tool short summary string for verbose output."""
    if name == "neely_forecast":
        p = out.get("primary_scenario") or {}
        return (f"price={out.get('current_price')} waves={p.get('wave_count')} "
                f"degree={p.get('effective_degree')} "
                f"usable={(out.get('quality_caveat') or {}).get('is_usable')}")
    if name == "kalman_trend":
        return (f"smoothed={out.get('smoothed_price')} "
                f"velocity={out.get('trend_velocity')} regime={out.get('regime')}")
    if name == "stock_snapshot":
        sections = ["health","loan_collateral","block_trade","risk_alert","market_context",
                    "commodity_macro","fundamentals","institutional","shareholder","technical_summary"]
        errors = [s for s in sections
                  if isinstance(out.get(s), dict) and out[s].get("error")]
        return f"sections_ok={len(sections)-len(errors)}/{len(sections)} errors={errors}"
    if name == "stock_levels":
        # v4.30 fix:stock_levels 真實 levels 在 out["key_levels"]["levels"](v4.19
        # B 視角整併三段 dict),不在頂層 out["levels"]。原 v4.29 harness summary
        # 讀錯 key → 永遠回 levels=0(2026-05-29 user 揭露);實際 production OK,
        # 2330 有 122 levels / 214 source points,只是 summary 顯示假象。
        kl = out.get("key_levels") or {}
        return (f"levels={len(kl.get('levels') or [])} "
                f"src_points={kl.get('source_point_count', 0)} "
                f"patterns={len(out.get('patterns') or [])} "
                f"stop_loss={'set' if out.get('stop_loss') else 'none'}")
    if name == "magic_formula_screen":
        return f"top_n={len(out.get('top_stocks') or [])}"
    if name in ("monthly_screen", "quarterly_screen", "annual_low_risk_screen"):
        f = out.get("factors") or {}
        return f"factors={len(f)} factor_keys={list(f.keys())}"
    if name == "scan_wave_impulse":
        return (f"top={len(out.get('top_stocks') or [])} "
                f"observe={len(out.get('observe_stocks') or [])} "
                f"cross_tf_aligned={out.get('cross_tf_aligned_count')}")
    if name == "market_overview":
        d = out.get("dashboard") or {}
        return (f"dashboard.components={d.get('component_count')} "
                f"missing={d.get('missing')} events={len(out.get('events') or [])}")
    if name == "indicators":
        i = out.get("indicators") or {}
        with_series = sum(1 for v in i.values()
                          if isinstance(v, dict) and v.get("series"))
        return (f"cores={len(i)} with_series={with_series}/{len(i)} "
                f"missing={out.get('missing')}")
    if name == "monthly_trigger_scan":
        c = out.get("counts") or {}
        return (f"pos_total={c.get('positive_total')} "
                f"neg_total={c.get('negative_total')} "
                f"shown={len(out.get('positive_triggers') or [])}+"
                f"{len(out.get('negative_triggers') or [])}")
    if name == "dual_track_resonance":
        t1 = out.get("track1") or {}
        t2 = out.get("track2") or {}
        return (f"has_snapshot={t1.get('has_snapshot')} "
                f"fib_lines={len(t1.get('fib_lines') or [])} "
                f"primary_band={'set' if t2.get('primary_band') else 'none'} "
                f"findings={len(out.get('findings') or [])} "
                f"single_track={out.get('single_track_mode')}")
    return "(no summary defined)"


def main() -> int:
    p = argparse.ArgumentParser(
        description="v4.29 MCP toolkit (13 tools) full coverage verify")
    p.add_argument("--stocks", default="2330",
                   help="逗號分隔股票清單(預設 2330)")
    p.add_argument("--as-of", default=date_t.today().isoformat(),
                   help="查詢日 ISO 字串(預設 today)")
    p.add_argument("--verbose", action="store_true",
                   help="顯示每 tool payload 完整 summary")
    args = p.parse_args()

    stocks = [s.strip() for s in args.stocks.split(",") if s.strip()]
    as_of = args.as_of
    primary_stock = stocks[0]

    from mcp_server.tools import data as d

    # Per-stock toolkit(skip cross-stock dups across stocks):
    per_stock_tools = [
        ("neely_forecast",      lambda s: lambda: d.neely_forecast(s, as_of)),
        ("kalman_trend",        lambda s: lambda: d.kalman_trend(s, as_of)),
        ("stock_snapshot",      lambda s: lambda: d.stock_snapshot(s, as_of)),
        ("stock_levels",        lambda s: lambda: d.stock_levels(s, as_of)),
        ("indicators",          lambda s: lambda: d.indicators(s, as_of)),
        ("dual_track_resonance", lambda s: lambda: d.dual_track_resonance(s, as_of)),
    ]
    # Market-level toolkit(only test on primary_stock as_of):
    market_tools = [
        ("magic_formula_screen",   lambda: d.magic_formula_screen(as_of)),
        ("monthly_screen",         lambda: d.monthly_screen(as_of)),
        ("quarterly_screen",       lambda: d.quarterly_screen(as_of)),
        ("annual_low_risk_screen", lambda: d.annual_low_risk_screen(as_of)),
        ("scan_wave_impulse",      lambda: d.scan_wave_impulse(as_of)),
        ("market_overview",        lambda: d.market_overview(as_of)),
        ("monthly_trigger_scan",   lambda: d.monthly_trigger_scan(as_of)),  # default top_n_per_type=20
    ]

    print(f"\n{'='*78}\nv4.29 MCP toolkit verify  as_of={as_of}  stocks={stocks}\n{'='*78}")
    print(f"{'tool':<25}{'stock':<8}{'status':<8}{'elapsed_s':>10}{'payload_kb':>12}  note")
    print("-" * 78)

    results: list[dict[str, Any]] = []

    # Per-stock tools
    for sid in stocks:
        for tname, factory in per_stock_tools:
            r = _check_call(tname, factory(sid))
            r["stock"] = sid
            results.append(r)
            print(f"{tname:<25}{sid:<8}{r['status']:<8}{r['elapsed_s']:>10}"
                  f"{r.get('payload_kb', '-')!s:>12}  {r['note'][:80]}")

    # Market-level tools(once each)
    for tname, fn in market_tools:
        r = _check_call(tname, fn)
        r["stock"] = "_market_"
        results.append(r)
        print(f"{tname:<25}{'_market_':<8}{r['status']:<8}{r['elapsed_s']:>10}"
              f"{r.get('payload_kb', '-')!s:>12}  {r['note'][:80]}")

    print("-" * 78)
    n_ok   = sum(1 for r in results if r["status"] == "OK")
    n_warn = sum(1 for r in results if r["status"] == "WARN")
    n_fail = sum(1 for r in results if r["status"] == "FAIL")
    n_err  = sum(1 for r in results if r["status"] == "ERROR")
    print(f"TOTAL: OK={n_ok}  WARN={n_warn}  FAIL={n_fail}  ERROR={n_err}  "
          f"of {len(results)}")

    # Detail dump of any non-OK
    if n_warn + n_fail + n_err > 0:
        print("\n--- Issues detail ---")
        for r in results:
            if r["status"] in ("WARN", "FAIL", "ERROR"):
                print(f"\n[{r['status']}] {r['tool']} (stock={r['stock']})")
                print(f"  note: {r['note']}")
                if r.get("traceback"):
                    print(f"  traceback:\n{r['traceback']}")
                if args.verbose and r.get("raw"):
                    raw_summary = json.dumps(r["raw"], default=str, ensure_ascii=False)[:1500]
                    print(f"  payload (first 1500 chars):\n{raw_summary}")

    if args.verbose:
        print("\n--- Verbose OK summary ---")
        for r in results:
            if r["status"] == "OK":
                print(f"  {r['tool']:<25}({r['stock']}): {r['note']}")

    print()
    if n_fail + n_err > 0:
        print(f"[RESULT] FAIL — {n_fail + n_err} 個 tool 有 hard issue(WARN 可接受)")
        return 1
    if n_warn > 0:
        print(f"[RESULT] PASS WITH WARNINGS — {n_warn} 個 tool 有 soft warning"
              f"(payload size 略大但 < 1MB)")
        return 0
    print(f"[RESULT] PASS — 全部 {len(results)} 個 tool call 都 OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
