"""FastMCP server app + tool registration。

對齊 plan Phase D §架構設計。

註冊 10 個 tools(在 mcp_server.tools.data / mcp_server.tools.render 內定義):
- 4 data tools:as_of_snapshot / find_facts / list_cores / fetch_ohlc
- 6 render tools:render_kline / render_chip / render_fundamental /
  render_environment / render_neely / render_facts_cloud
"""

from __future__ import annotations

# 觸發 sys.path 設定(agg / dashboards 可 import)
from mcp_server import _conn  # noqa: F401

from fastmcp import FastMCP

# 等 sys.path 設好後再 import tools
from mcp_server.tools import data as _data_tools  # noqa: E402
from mcp_server.tools import render as _render_tools  # noqa: E402


mcp = FastMCP(
    name="StockHelper4me",
    instructions=(
        "Taiwan stock M3 aggregation layer + dashboards。\n\n"
        "Data tools 回 JSON:as_of_snapshot / find_facts / list_cores / fetch_ohlc。\n"
        "Render tools 回 PNG image + JSON 摘要:render_kline / render_chip / "
        "render_fundamental / render_environment / render_neely / render_facts_cloud。\n\n"
        "所有 tool 強制 as_of date(回測 / 即時同介面);facts 已過 look-ahead bias 防衛。"
    ),
)


# Register data tools
mcp.tool(_data_tools.as_of_snapshot)
mcp.tool(_data_tools.find_facts)
mcp.tool(_data_tools.list_cores)
mcp.tool(_data_tools.fetch_ohlc)

# Register render tools
mcp.tool(_render_tools.render_kline)
mcp.tool(_render_tools.render_chip)
mcp.tool(_render_tools.render_fundamental)
mcp.tool(_render_tools.render_environment)
mcp.tool(_render_tools.render_neely)
mcp.tool(_render_tools.render_facts_cloud)
