# start_mcp.ps1
# ──────────────────────────────────────────────────────────────
# StockHelper4me MCP server 啟動 wrapper(對齊 scripts/refresh_daily.ps1 結構)。
#
# 主要用例:
#   (1) **Claude Desktop config**(production):config 內 `command` 指向本檔
#       (PowerShell 啟動 → 自動載 venv / .env / UTF-8 → exec python -m mcp_server
#        → MCP stdio 由 Claude Desktop 接管)
#
#       Claude Desktop config 範例(claude_desktop_config.json):
#         {
#           "mcpServers": {
#             "stockhelper4me": {
#               "command": "powershell.exe",
#               "args": [
#                 "-NoProfile", "-ExecutionPolicy", "Bypass",
#                 "-File", "C:\\path\\to\\StockHelper4me\\scripts\\start_mcp.ps1"
#               ]
#             }
#           }
#         }
#
#   (2) **本機 smoke test**(debug):
#       .\scripts\start_mcp.ps1 -Smoke
#       啟動 server 後 2 秒 SIGTERM,只看 startup logs 確認 5 tools register 成功。
#
# 設計約束:
#   - MCP stdio 走 stdin/stdout JSON-RPC,所有 setup messages 必須走 stderr(不污染 stdout)
#   - venv / .env / UTF-8 設定對齊 refresh_daily.ps1 慣例
#   - Setup logs 寫到 logs/mcp_YYYY-MM-DD.log(只 stderr / startup info,不含 protocol 流量)
#   - 失敗 fast(fastmcp 沒裝 / venv 沒裝 / .env 沒填都 exit 早)
# ──────────────────────────────────────────────────────────────

param(
    # Smoke test 模式:啟動後 2 秒 SIGTERM,只看 startup logs 確認 server 起得來
    [switch]$Smoke
)

$ErrorActionPreference = 'Stop'

# Resolve project root(scripts/.. = repo root)
$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $ProjectRoot

# Console UTF-8(中文 narrative 不亂碼)
chcp 65001 | Out-Null
$env:PYTHONIOENCODING = 'utf-8'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

# Log dir + dated log file
$LogDir = Join-Path $ProjectRoot 'logs'
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$LogFile = Join-Path $LogDir ('mcp_' + (Get-Date -Format 'yyyy-MM-dd') + '.log')

function Log-Setup([string]$Message) {
    # 走 stderr + log file;絕不污染 stdout(MCP stdio 保留給 JSON-RPC)
    $line = "[$(Get-Date -Format 'HH:mm:ss')] $Message"
    [Console]::Error.WriteLine($line)
    Add-Content -Path $LogFile -Value $line
}

Log-Setup "─── MCP server wrapper start ───"
Log-Setup "ProjectRoot: $ProjectRoot"
Log-Setup "Smoke: $Smoke"

# 1. Activate .venv(若存在);沒有就 fail(MCP server 對 venv 一致性敏感)
$VenvActivate = Join-Path $ProjectRoot '.venv\Scripts\Activate.ps1'
if (Test-Path $VenvActivate) {
    & $VenvActivate
    Log-Setup ".venv activated: $VenvActivate"
} else {
    Log-Setup "ERROR: .venv 不存在於 $VenvActivate"
    Log-Setup "       請先跑 'python -m venv .venv' + 'pip install -e .[dev]'"
    exit 1
}

# 2. Load .env(若存在),否則警告(MCP tools 需 DATABASE_URL 連 PG)
$EnvFile = Join-Path $ProjectRoot '.env'
if (Test-Path $EnvFile) {
    Get-Content $EnvFile | ForEach-Object {
        if ($_ -match '^\s*([^#][^=]*)=(.*)$') {
            $name = $matches[1].Trim()
            $value = $matches[2].Trim().Trim('"')
            [Environment]::SetEnvironmentVariable($name, $value, 'Process')
        }
    }
    Log-Setup ".env loaded: $EnvFile"
} else {
    Log-Setup "WARNING: .env 不存在;假設 DATABASE_URL 已在系統環境變數"
}

# 3. Sanity check:fastmcp 套件存在
$fastmcpCheck = & python -c "import fastmcp; print(fastmcp.__version__)" 2>&1
if ($LASTEXITCODE -ne 0) {
    Log-Setup "ERROR: fastmcp 套件未安裝"
    Log-Setup "       請跑 'pip install -e .[dev]' 或 'pip install fastmcp>=2.0'"
    Log-Setup "       Output: $fastmcpCheck"
    exit 1
}
Log-Setup "fastmcp version: $fastmcpCheck"

# 4. Sanity check:mcp_server.server module importable(agg layer + 所有 tools loadable)
$importCheck = & python -c "from mcp_server.server import mcp; print(mcp.name)" 2>&1
if ($LASTEXITCODE -ne 0) {
    Log-Setup "ERROR: mcp_server.server import 失敗(可能 agg / dashboards 模組 syntax error)"
    Log-Setup "       Output: $importCheck"
    exit 1
}
Log-Setup "MCP server importable: $importCheck"

# 5. Run server(stdio mode — stdin/stdout 留給 MCP protocol)
Log-Setup "Launching: python -m mcp_server"

if ($Smoke) {
    # Smoke test:啟動 2 秒後 SIGTERM,只看 startup logs
    Log-Setup "Smoke mode: 啟動 2 秒後終止(no MCP client interaction)"
    $proc = Start-Process -FilePath python -ArgumentList '-m', 'mcp_server' `
        -NoNewWindow -PassThru -RedirectStandardError $LogFile
    Start-Sleep -Seconds 2
    if (-not $proc.HasExited) {
        Stop-Process -Id $proc.Id -Force
        Log-Setup "Smoke test PASSED — server 起得來(已終止 PID $($proc.Id))"
        exit 0
    } else {
        Log-Setup "Smoke test FAILED — server 啟動立即 exit(code $($proc.ExitCode))"
        exit $proc.ExitCode
    }
}

# Production mode:exec python(原 process 接 stdio),不開 sub-process
# `python -m mcp_server` 的 stdin/stdout 直連 Claude Desktop / MCP client
& python -m mcp_server

$exitCode = $LASTEXITCODE
Log-Setup "MCP server exited with code $exitCode"
exit $exitCode
