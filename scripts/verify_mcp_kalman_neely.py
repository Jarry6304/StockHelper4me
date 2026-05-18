"""
verify_mcp_kalman_neely.py
==========================
v3.31 verify pipeline:Kalman + Neely MCP tool 對指定股票出值是否正常。

判定(per stock):
  - **Kalman**:
      smoothed_price > 0 ∧ velocity != 0
      ∧ indicator_staleness.is_stale == False
  - **Neely**:
      current_price > 0 ∧ primary_scenario.wave_count > 0
      ∧ scenario_staleness.is_stale == False

任一條件 fail → [FAIL]。

用法:
  python scripts/verify_mcp_kalman_neely.py
  python scripts/verify_mcp_kalman_neely.py --stocks 2330,3030
  python scripts/verify_mcp_kalman_neely.py --as-of 2026-05-15

退出碼:
  0 = 全綠
  1 = 任一 [FAIL]
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# 加入 src + repo root 到 sys.path,讓 `from mcp_server...` 可 import
_REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(_REPO_ROOT / "src"))
sys.path.insert(0, str(_REPO_ROOT))


def _check_kalman(stock_id: str, as_of: str) -> tuple[str, str]:
    """Return (status, note)。status ∈ {OK, FAIL, ERROR}。"""
    try:
        from mcp_server.tools.data import kalman_trend
        r = kalman_trend(stock_id, as_of)
    except Exception as e:
        return "ERROR", f"{type(e).__name__}: {e}"

    smoothed = r.get("smoothed_price")
    velocity = r.get("trend_velocity")
    staleness = r.get("indicator_staleness") or {}
    is_stale = staleness.get("is_stale")

    issues: list[str] = []
    if not (isinstance(smoothed, (int, float)) and smoothed > 0):
        issues.append(f"smoothed_price={smoothed}")
    if not (isinstance(velocity, (int, float)) and velocity != 0):
        issues.append(f"velocity={velocity}")
    if is_stale is True:
        age = staleness.get("age_days")
        issues.append(f"stale({age}d)")

    note = (
        f"smoothed={smoothed} velocity={velocity} regime={r.get('regime')}"
        if not issues else "; ".join(issues)
    )
    return ("OK" if not issues else "FAIL"), note


def _check_neely(stock_id: str, as_of: str) -> tuple[str, str]:
    try:
        from mcp_server.tools.data import neely_forecast
        r = neely_forecast(stock_id, as_of)
    except Exception as e:
        return "ERROR", f"{type(e).__name__}: {e}"

    current_price = r.get("current_price")
    primary = r.get("primary_scenario") or {}
    wave_count = primary.get("wave_count")
    staleness = r.get("scenario_staleness") or {}
    is_stale = staleness.get("is_stale")

    issues: list[str] = []
    if not (isinstance(current_price, (int, float)) and current_price > 0):
        issues.append(f"current_price={current_price}")
    if not (isinstance(wave_count, int) and wave_count > 0):
        issues.append(f"wave_count={wave_count}")
    if is_stale is True:
        age = staleness.get("age_days")
        issues.append(f"stale({age}d)")

    note = (
        f"price={current_price} waves={wave_count} "
        f"label={primary.get('structure_label')!r}"
        if not issues else "; ".join(issues)
    )
    return ("OK" if not issues else "FAIL"), note


def main() -> int:
    p = argparse.ArgumentParser(description="v3.31 MCP Kalman + Neely verify")
    p.add_argument("--stocks", default="2330",
                   help="逗號分隔股票清單(預設 2330)")
    p.add_argument("--as-of", default="2026-05-15",
                   help="查詢日 ISO 字串(預設 2026-05-15)")
    args = p.parse_args()

    stocks = [s.strip() for s in args.stocks.split(",") if s.strip()]
    as_of = args.as_of

    # Status table
    header = f"{'Stock':<8}{'Kalman':<10}{'Neely':<10}Notes"
    print(header)
    print("-" * 80)

    any_fail = False
    for sid in stocks:
        kal_status, kal_note = _check_kalman(sid, as_of)
        nly_status, nly_note = _check_neely(sid, as_of)
        if kal_status != "OK" or nly_status != "OK":
            any_fail = True
        print(f"{sid:<8}[{kal_status:<6}] [{nly_status:<6}] "
              f"K:{kal_note} | N:{nly_note}")

    print("-" * 80)
    if any_fail:
        print("[RESULT] FAIL — 至少 1 個 stock 有 issue")
        print("\n常見 root cause:")
        print("  - smoothed/velocity = 0 → 跑 git pull 拉 v3.30 Kalman path fix")
        print("  - is_stale=true       → 跑 `tw_cores run-all --write` 重算")
        print("  - wave_count = 0      → 跑 git pull 拉 v3.28 regex parse fix")
        return 1
    print("[RESULT] OK — Kalman + Neely 對全部 stock 正常出值")
    return 0


if __name__ == "__main__":
    sys.exit(main())
