"""Plotly figure builders for Aggregation dashboard(Phase C)。

對齊 m3Spec/aggregation_layer.md §11 Phase B-3 + plan
/root/.claude/plans/squishy-foraging-stroustrup.md。

模組分工:
- _base:共用 palette / layout helpers
- candlestick:K-line + Volume
- overlays:bollinger band fill / MA 多線 / neely zigzag(主圖疊圖)
- indicators:macd / rsi / kd / adx / atr / obv subplots
- chip:institutional / margin / foreign / day_trading / shareholder
- fundamental:revenue / valuation / financial_statement
- environment:taiex / us_market / exchange_rate / fear_greed / market_margin / business
- neely_wave:scenario forest deep-dive(scenario picker + Fib zones)
- facts_cloud:facts 散點雲 + panel-to-core mapping
"""

from dashboards.charts import _base  # noqa: F401

__all__ = ["_base"]
