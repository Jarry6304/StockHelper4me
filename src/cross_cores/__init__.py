"""
cross_cores/__init__.py
=======================
Layer 2.5:Cross-Stock Cores(v3.5 R3 新層)。

跨股 ranking / 分群 / 相關性 / sector 邏輯,輸入全市場 universe,輸出
cross-stock keyed 結果(ranks / clusters / corr matrices / 相對強度)。

跟 Layer 2 Silver per-stock builder 的契約對立:
  - Silver per-stock(`silver/builders/`)輸入 stock_id,輸出 per-stock 衍生欄
  - Cross-Stock(本層)            輸入 date(全市場),輸出 cross-stock 排名 / 分群

跟 Layer 3 M3 Cores 的差別:
  - Layer 3:per-stock compute → facts / indicator_values / structural_snapshots
  - Layer 2.5:cross-stock compute → Silver-like `*_derived` 表(共用 PG schema)

首個成員(v3.5 R3 從 silver/builders/ 搬):
  - magic_formula:Greenblatt 2005 EBIT/EV + ROIC cross-rank

未來成員(留 follow-up):pairs_trading / sector_rotation / correlation_matrix

模組結構:
  - _base.py       CrossStockBuilder ABC
  - _common.py     共用 helper(fetch_market_wide 等)
  - orchestrator.py  Phase 8 排程(CLI:python src/main.py cross_cores phase 8)
  - magic_formula.py 首個 cross-stock core
"""

from cross_cores._base import CrossStockBuilder

__all__ = ["CrossStockBuilder"]
