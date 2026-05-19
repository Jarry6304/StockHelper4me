#!/usr/bin/env python3
"""_mcp_import_check.py — Phase 4 MCP toolkit import smoke test。

由 test_pipeline.ps1 / test_pipeline.sh 透過 `python scripts/_mcp_import_check.py` 呼叫
(避免 PS1 here-string 解析問題)。

驗證 8 個 public MCP tools 全部 importable:
  v3.31 4 個個股 / 整合:neely_forecast / kalman_trend /
    magic_formula_screen / stock_snapshot
  v3.32 4 個 cross-stock factor screens:monthly_screen / quarterly_screen /
    annual_low_risk_screen / monthly_trigger_scan

退碼:0 = 全 importable / 1 = 任一 tool 缺
"""

from __future__ import annotations

import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "src"))
sys.path.insert(0, str(REPO_ROOT))

EXPECTED_TOOLS = [
    "neely_forecast",
    "kalman_trend",
    "magic_formula_screen",
    "stock_snapshot",
    "monthly_screen",
    "quarterly_screen",
    "annual_low_risk_screen",
    "monthly_trigger_scan",
]


def main() -> int:
    try:
        from mcp_server.tools import data as d
    except ImportError as e:
        print(f"FAIL: mcp_server.tools.data import error: {e}", file=sys.stderr)
        return 1

    missing = [t for t in EXPECTED_TOOLS if not hasattr(d, t)]
    if missing:
        print(f"FAIL: Missing tools: {missing}", file=sys.stderr)
        return 1

    print(f"All {len(EXPECTED_TOOLS)} MCP tools importable")
    return 0


if __name__ == "__main__":
    sys.exit(main())
