#!/usr/bin/env bash
# start_mcp.sh — Linux/macOS 版 MCP server 啟動 wrapper(對齊 scripts/start_mcp.ps1)。
#
# 主要用例:
#   (1) Claude Desktop / MCP client config:command 指向本檔
#         {
#           "mcpServers": {
#             "stockhelper4me": {
#               "command": "/path/to/StockHelper4me/scripts/start_mcp.sh"
#             }
#           }
#         }
#   (2) 本機 smoke test:./scripts/start_mcp.sh --smoke
#
# 設計約束:
#   - MCP stdio 走 stdin/stdout JSON-RPC,所有 setup messages 走 stderr(不污染 stdout)
#   - venv / .env / UTF-8 設定對齊 refresh_daily.ps1 慣例
#   - Setup logs 寫 logs/mcp_YYYY-MM-DD.log(只 stderr / startup info)
#   - 失敗 fast(fastmcp / venv / .env 沒裝都 exit 早)

set -e

SMOKE=0
if [ "${1:-}" = "--smoke" ] || [ "${1:-}" = "-s" ]; then
    SMOKE=1
fi

# Resolve project root(scripts/.. = repo root)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

# UTF-8 locale(中文 narrative 不亂碼)
export PYTHONIOENCODING=utf-8
export LC_ALL="${LC_ALL:-en_US.UTF-8}"

# Log dir + dated log file
LOG_DIR="$PROJECT_ROOT/logs"
mkdir -p "$LOG_DIR"
LOG_FILE="$LOG_DIR/mcp_$(date +%Y-%m-%d).log"

log_setup() {
    # stderr + log file;絕不寫 stdout(留給 MCP JSON-RPC)
    local line="[$(date +%H:%M:%S)] $*"
    echo "$line" >&2
    echo "$line" >> "$LOG_FILE"
}

log_setup "─── MCP server wrapper start ───"
log_setup "PROJECT_ROOT: $PROJECT_ROOT"
log_setup "SMOKE: $SMOKE"

# 1. Activate .venv(若存在);否則 fail
VENV_ACTIVATE="$PROJECT_ROOT/.venv/bin/activate"
if [ -f "$VENV_ACTIVATE" ]; then
    # shellcheck disable=SC1090
    . "$VENV_ACTIVATE"
    log_setup ".venv activated: $VENV_ACTIVATE"
else
    log_setup "ERROR: .venv 不存在於 $VENV_ACTIVATE"
    log_setup "       請先跑 'python -m venv .venv' + 'pip install -e .[dev]'"
    exit 1
fi

# 2. Load .env(若存在),否則警告
ENV_FILE="$PROJECT_ROOT/.env"
if [ -f "$ENV_FILE" ]; then
    set -a
    # shellcheck disable=SC1090
    . "$ENV_FILE"
    set +a
    log_setup ".env loaded: $ENV_FILE"
else
    log_setup "WARNING: .env 不存在;假設 DATABASE_URL 已在系統環境變數"
fi

# 3. Sanity check:fastmcp 套件存在
if ! FASTMCP_VER=$(python -c "import fastmcp; print(fastmcp.__version__)" 2>&1); then
    log_setup "ERROR: fastmcp 套件未安裝"
    log_setup "       請跑 'pip install -e .[dev]' 或 'pip install fastmcp>=2.0'"
    log_setup "       Output: $FASTMCP_VER"
    exit 1
fi
log_setup "fastmcp version: $FASTMCP_VER"

# 4. Sanity check:mcp_server.server importable
if ! IMPORT_CHECK=$(python -c "from mcp_server.server import mcp; print(mcp.name)" 2>&1); then
    log_setup "ERROR: mcp_server.server import 失敗"
    log_setup "       Output: $IMPORT_CHECK"
    exit 1
fi
log_setup "MCP server importable: $IMPORT_CHECK"

# 5. Run server
log_setup "Launching: python -m mcp_server"

if [ "$SMOKE" = "1" ]; then
    log_setup "Smoke mode: 啟動 2 秒後終止"
    python -m mcp_server </dev/null 2>>"$LOG_FILE" &
    PID=$!
    sleep 2
    if kill -0 "$PID" 2>/dev/null; then
        kill "$PID" 2>/dev/null || true
        log_setup "Smoke test PASSED — server 起得來(已終止 PID $PID)"
        exit 0
    else
        wait "$PID" || EXIT_CODE=$?
        log_setup "Smoke test FAILED — server 啟動立即 exit(code ${EXIT_CODE:-?})"
        exit "${EXIT_CODE:-1}"
    fi
fi

# Production:exec(取代 shell process,stdin/stdout 直連 caller)
exec python -m mcp_server
