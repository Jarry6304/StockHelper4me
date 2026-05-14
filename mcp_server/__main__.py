"""`python -m mcp_server` 入口 — 啟動 stdio MCP server。

Claude Desktop config 用:
    {
      "mcpServers": {
        "stockhelper": {
          "command": "python",
          "args": ["-m", "mcp_server"],
          "cwd": "C:\\\\path\\\\to\\\\StockHelper4me"
        }
      }
    }
"""

from mcp_server.server import mcp


if __name__ == "__main__":
    mcp.run()
