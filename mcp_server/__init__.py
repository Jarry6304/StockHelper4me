"""StockHelper4me MCP Server.

對齊 plan /root/.claude/plans/squishy-foraging-stroustrup.md(Phase D)。

把 src/agg/ aggregation layer + dashboards/charts/ plotly figure builders 包成
MCP(Model Context Protocol)server,讓 Claude Desktop 對話內直接 call tools
查資料 + 看套圖。

Transport: stdio(本機 process)。Phase 2 再 lift 同套 tools 到 HTTP transport
給 mobile / 多人共享。

Tools surface(10 個):
- 4 data tools(JSON):as_of_snapshot / find_facts / list_cores / fetch_ohlc
- 6 render tools(PNG + JSON):render_kline / render_chip / render_fundamental /
  render_environment / render_neely / render_facts_cloud
"""

__version__ = "0.1.0"
