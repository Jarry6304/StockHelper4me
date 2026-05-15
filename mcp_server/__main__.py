"""`python -m mcp_server` 入口 — 啟動 stdio MCP server。

**推薦走 wrapper**(scripts/start_mcp.ps1 / start_mcp.sh)— 自動處理 venv /
.env / UTF-8 / fastmcp sanity check。直接 `python -m mcp_server` 需 caller
端自己準備好 venv + DATABASE_URL。

Claude Desktop config(直接呼 python 版,fallback):
    {
      "mcpServers": {
        "stockhelper": {
          "command": "python",
          "args": ["-m", "mcp_server"],
          "cwd": "C:\\\\path\\\\to\\\\StockHelper4me"
        }
      }
    }

Wrapper 版 config 見 `mcp_server/README.md` §「Claude Desktop 設定 方案 A」。
"""

from mcp_server.server import mcp


if __name__ == "__main__":
    mcp.run()
