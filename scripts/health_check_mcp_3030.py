"""
health_check_mcp_3030.py
========================
台股技術分析 MCP 工具組對 3030(德律)的全套健康檢查。

對齊用戶提案 — 8 個 mcp__stockhelper__* 工具走 Python 直接 import 路徑
(與 MCP server 同 code path),回傳成功 / 失敗 + 資料完整度 + 錯誤摘要。

純文字 stdout 輸出。0 檔案、0 PDF、0 圖表。
"""

from __future__ import annotations

import json
import sys
import traceback
from datetime import date
from pathlib import Path
from typing import Any

_REPO_ROOT = Path(__file__).resolve().parent.parent
for p in (str(_REPO_ROOT / "src"), str(_REPO_ROOT)):
    if p not in sys.path:
        sys.path.insert(0, p)

# 個股測試用代號
STOCK_ID = "3030"
# 查詢日(latest production data)
AS_OF = "2026-05-15"


def _section(title: str) -> None:
    print("\n" + "=" * 70)
    print(title)
    print("=" * 70)


def _print_status(label: str, status: str, summary: str) -> None:
    mark = "✓" if status == "OK" else "✗"
    print(f"  {mark} {label:<28} [{status:^6}] {summary}")


def _summarize_dict(d: Any, max_keys: int = 5) -> str:
    if not isinstance(d, dict):
        return f"type={type(d).__name__}"
    keys = list(d.keys())[:max_keys]
    nkeys = len(d)
    return f"{nkeys} keys: {keys}{'...' if nkeys > max_keys else ''}"


def _run_tool(label: str, fn, *args, **kwargs) -> tuple[str, str, dict | None]:
    try:
        result = fn(*args, **kwargs)
    except Exception as e:
        err = f"{type(e).__name__}: {e}"
        return "FAIL", err, None

    if not isinstance(result, dict):
        return "FAIL", f"return 非 dict (type={type(result).__name__})", None

    if "error" in result and "section" not in result:
        return "FAIL", f"top-level error: {result.get('error')}", result

    return "OK", _summarize_dict(result), result


def main() -> int:
    results: list[tuple[str, str, str, dict | None]] = []
    payload_dump: dict[str, Any] = {}

    print(f"\n台股 MCP 工具組健康檢查 — 個股 {STOCK_ID} / as_of {AS_OF}")
    print(f"(直接呼叫 mcp_server.tools.data 內 wrapper,與 MCP server 同 code path)")

    # 1. stock_snapshot(6-in-1)— v3.31
    _section("[1/8] stock_snapshot(個股 6-in-1 基本資料)")
    from mcp_server.tools.data import stock_snapshot
    status, summary, payload = _run_tool(
        "stock_snapshot", stock_snapshot, STOCK_ID, AS_OF,
    )
    _print_status("stock_snapshot", status, summary)
    if payload and isinstance(payload, dict):
        for sub in ("health", "loan_collateral", "block_trade",
                    "risk_alert", "market_context", "commodity_macro"):
            sub_data = payload.get(sub)
            if not isinstance(sub_data, dict):
                _print_status(f"  └─ {sub}", "FAIL", f"非 dict ({type(sub_data).__name__})")
                continue
            if "error" in sub_data:
                _print_status(f"  └─ {sub}", "FAIL", sub_data["error"][:80])
            else:
                _print_status(f"  └─ {sub}", "OK", _summarize_dict(sub_data, max_keys=4))
        narr = payload.get("narrative") or ""
        print(f"    narrative: {narr[:100]}{'...' if len(narr) > 100 else ''}")
    results.append(("stock_snapshot", status, summary, payload))
    payload_dump["stock_snapshot"] = payload

    # 2. kalman_trend
    _section("[2/8] kalman_trend(1-D Kalman + 5-class regime)")
    from mcp_server.tools.data import kalman_trend
    status, summary, payload = _run_tool("kalman_trend", kalman_trend, STOCK_ID, AS_OF)
    _print_status("kalman_trend", status, summary)
    if payload:
        for key in ("current_price", "smoothed_price", "trend_velocity",
                    "uncertainty_band", "regime", "regime_label"):
            print(f"    {key:<22} = {payload.get(key)}")
        st = payload.get("indicator_staleness") or {}
        print(f"    indicator_staleness    = age_days={st.get('age_days')} "
              f"is_stale={st.get('is_stale')}")
    results.append(("kalman_trend", status, summary, payload))
    payload_dump["kalman_trend"] = payload

    # 3. neely_forecast
    _section("[3/8] neely_forecast(NEoWave 預測 4 個時間框架)")
    from mcp_server.tools.data import neely_forecast
    status, summary, payload = _run_tool("neely_forecast", neely_forecast, STOCK_ID, AS_OF)
    _print_status("neely_forecast", status, summary)
    if payload:
        primary = payload.get("primary_scenario") or {}
        print(f"    current_price          = {payload.get('current_price')}")
        print(f"    invalidation_price     = {payload.get('invalidation_price')}")
        print(f"    primary.structure      = {primary.get('structure_label')}")
        print(f"    primary.wave_count     = {primary.get('wave_count')}")
        print(f"    primary.power_rating   = {primary.get('power_rating')}")
        st = payload.get("scenario_staleness") or {}
        print(f"    scenario_staleness     = age_days={st.get('age_days')} "
              f"is_stale={st.get('is_stale')}")
        fcs = payload.get("forecasts") or []
        print(f"    forecasts(4 timeframe) = {len(fcs)} items")
        for f in fcs[:4]:
            print(f"      └─ {f.get('timeframe'):<10} prob_up={f.get('prob_up')} "
                  f"range=[{f.get('range_low')}, {f.get('range_high')}]")
    results.append(("neely_forecast", status, summary, payload))
    payload_dump["neely_forecast"] = payload

    # 4. monthly_trigger_scan
    _section("[4/8] monthly_trigger_scan(Layer 5 conviction adjustment)")
    from mcp_server.tools.data import monthly_trigger_scan
    status, summary, payload = _run_tool("monthly_trigger_scan", monthly_trigger_scan, AS_OF)
    _print_status("monthly_trigger_scan", status, summary)
    if payload:
        pos = payload.get("positive_triggers") or []
        neg = payload.get("negative_triggers") or []
        print(f"    signal_date            = {payload.get('signal_date')}")
        print(f"    positive_triggers      = {len(pos)} stocks")
        print(f"    negative_triggers      = {len(neg)} stocks")
        # 查 3030 是否在 list 內
        in_pos = any(t.get("stock_id") == STOCK_ID for t in pos)
        in_neg = any(t.get("stock_id") == STOCK_ID for t in neg)
        print(f"    {STOCK_ID} in positive   = {in_pos}")
        print(f"    {STOCK_ID} in negative   = {in_neg}")
    results.append(("monthly_trigger_scan", status, summary, payload))
    payload_dump["monthly_trigger_scan"] = payload

    # 5. monthly_screen(Toolkit A)
    _section("[5/8] monthly_screen(Toolkit A 月度 — Mom + Rev + Inst + vol overlay)")
    from mcp_server.tools.data import monthly_screen
    status, summary, payload = _run_tool("monthly_screen", monthly_screen, AS_OF, 30)
    _print_status("monthly_screen", status, summary)
    if payload:
        factors = payload.get("factors") or {}
        for fname in ("persistent_momentum", "revenue_momentum", "institutional_concert"):
            f = factors.get(fname) or {}
            top = f.get("top_stocks") or []
            if "error" in f:
                _print_status(f"  └─ {fname}", "FAIL", f["error"][:80])
            else:
                # 查 3030 是否在 top
                in_top = next(((i + 1, t) for i, t in enumerate(top)
                              if t.get("stock_id") == STOCK_ID), None)
                in_marker = f" / {STOCK_ID} @ rank {in_top[0]}" if in_top else f" / {STOCK_ID} 不在 top {len(top)}"
                _print_status(f"  └─ {fname}", "OK", f"top {len(top)}{in_marker}")
        ov = payload.get("vol_managed_overlay") or {}
        print(f"    vol_managed_overlay    = scale={ov.get('scale')}")
    results.append(("monthly_screen", status, summary, payload))
    payload_dump["monthly_screen"] = payload

    # 6. quarterly_screen(Toolkit B)
    _section("[6/8] quarterly_screen(Toolkit B 季度 — F-Score + Low Vol + Industry-Adj GP)")
    from mcp_server.tools.data import quarterly_screen
    status, summary, payload = _run_tool("quarterly_screen", quarterly_screen, AS_OF, 30)
    _print_status("quarterly_screen", status, summary)
    if payload:
        factors = payload.get("factors") or {}
        for fname in ("f_score", "low_volatility", "industry_adj_gp"):
            f = factors.get(fname) or {}
            top = f.get("top_stocks") or []
            if "error" in f:
                _print_status(f"  └─ {fname}", "FAIL", f["error"][:80])
            else:
                in_top = next(((i + 1, t) for i, t in enumerate(top)
                              if t.get("stock_id") == STOCK_ID), None)
                in_marker = f" / {STOCK_ID} @ rank {in_top[0]}" if in_top else f" / {STOCK_ID} 不在 top {len(top)}"
                _print_status(f"  └─ {fname}", "OK", f"top {len(top)}{in_marker}")
    results.append(("quarterly_screen", status, summary, payload))
    payload_dump["quarterly_screen"] = payload

    # 7. annual_low_risk_screen(Toolkit C)
    _section("[7/8] annual_low_risk_screen(Toolkit C 年度 — LT Low Vol + Div Yield + 12-1 Mom)")
    from mcp_server.tools.data import annual_low_risk_screen
    status, summary, payload = _run_tool(
        "annual_low_risk_screen", annual_low_risk_screen, AS_OF, 30,
    )
    _print_status("annual_low_risk_screen", status, summary)
    if payload:
        factors = payload.get("factors") or {}
        for fname in ("long_term_low_vol", "dividend_yield", "mom_12_1"):
            f = factors.get(fname) or {}
            top = f.get("top_stocks") or []
            if "error" in f:
                _print_status(f"  └─ {fname}", "FAIL", f["error"][:80])
            else:
                in_top = next(((i + 1, t) for i, t in enumerate(top)
                              if t.get("stock_id") == STOCK_ID), None)
                in_marker = f" / {STOCK_ID} @ rank {in_top[0]}" if in_top else f" / {STOCK_ID} 不在 top {len(top)}"
                _print_status(f"  └─ {fname}", "OK", f"top {len(top)}{in_marker}")
    results.append(("annual_low_risk_screen", status, summary, payload))
    payload_dump["annual_low_risk_screen"] = payload

    # 8. magic_formula_screen
    _section("[8/8] magic_formula_screen(Greenblatt 2005 跨股篩選)")
    from mcp_server.tools.data import magic_formula_screen
    status, summary, payload = _run_tool(
        "magic_formula_screen", magic_formula_screen, AS_OF, 30,
    )
    _print_status("magic_formula_screen", status, summary)
    if payload:
        print(f"    ranking_date           = {payload.get('ranking_date')}")
        print(f"    universe_size          = {payload.get('universe_size')}")
        top = payload.get("top_stocks") or []
        in_top = next(((i + 1, t) for i, t in enumerate(top)
                      if t.get("stock_id") == STOCK_ID), None)
        in_marker = f" / {STOCK_ID} @ rank {in_top[0]}" if in_top else f" / {STOCK_ID} 不在 top {len(top)}"
        print(f"    top_stocks             = {len(top)}{in_marker}")
        stats = payload.get("stats") or {}
        print(f"    stats.median_ey        = {stats.get('median_ey')}")
        print(f"    stats.median_roic      = {stats.get('median_roic')}")
    results.append(("magic_formula_screen", status, summary, payload))
    payload_dump["magic_formula_screen"] = payload

    # === 總結 ===
    _section("總結 SUMMARY")
    ok_count = sum(1 for _, s, _, _ in results if s == "OK")
    fail_count = len(results) - ok_count
    print(f"  總計:{len(results)} 工具 / 成功:{ok_count} / 失敗:{fail_count}")
    print()
    print("  逐項狀態:")
    for name, status, summary, _ in results:
        mark = "✓" if status == "OK" else "✗"
        print(f"    {mark} {name:<28} [{status}]")
    print()
    if fail_count > 0:
        print("  失敗工具:")
        for name, status, summary, _ in results:
            if status != "OK":
                print(f"    - {name}: {summary}")
        print()
        print("  建議排查:")
        print("    1. 看上方錯誤訊息(import / SQL / KeyError 等)")
        print("    2. 跑 scripts/verify_mcp_kalman_neely.py 個別驗 Kalman/Neely")
        print("    3. SQL 直查 *_ranked_derived 表 row count 確認 builder 寫入有資料")
    else:
        print("  ✅ 全部 8 個 MCP 工具對 3030 正常出值 production-ready")
    return 0 if fail_count == 0 else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception:
        traceback.print_exc()
        sys.exit(2)
