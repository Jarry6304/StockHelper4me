"""
silver/__init__.py
==================
v3.2 Silver 層:dirty 驅動的計算層,從 Bronze raw 算出 *_derived 表。

模組結構(per blueprint v3.2 §三):
  - _common.py            共用工具(_filter_to_trading_days 等)
  - orchestrator.py       Phase 7a/7b/7c 排程入口
  - builders/             14 個 per-table builder

PR #19a 落地骨架(本檔 + 14 個 builder stubs);
PR #19b 補 5 個簡單 builder 邏輯(institutional / valuation / day_trading / margin / foreign_holding);
PR #19c 補剩 9 個 + orchestrator 真實邏輯 + Phase 7 CLI。
"""
