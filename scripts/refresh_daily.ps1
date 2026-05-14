# refresh_daily.ps1
# ──────────────────────────────────────────────────────────────
# Daily refresh wrapper for Windows Task Scheduler。
# 串完整 chain:Bronze incremental → Silver 7c/7a/7b → M3 cores run-all --dirty。
#
# 由 scripts/install_refresh_task.ps1 註冊的排程觸發。也可手動執行:
#   .\scripts\refresh_daily.ps1
#   .\scripts\refresh_daily.ps1 -RefreshArgs '--skip-cores'
#   .\scripts\refresh_daily.ps1 -RefreshArgs '--stocks','2330,2317'
#
# Logs 寫到 logs/refresh_YYYY-MM-DD.log。
# 內部 `python src/main.py refresh` 每段 exception handling 獨立,前段失敗不阻擋後段。
# ──────────────────────────────────────────────────────────────

param(
    # 傳給 `python src/main.py refresh` 的額外 args(例:'--skip-cores' / '--stocks','2330')
    [string[]]$RefreshArgs = @()
)

$ErrorActionPreference = 'Continue'  # 不讓 PowerShell 錯誤短路 chain;Python 自己處理

# Resolve project root(相對於本 script:scripts/.. = 專案 root)
$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $ProjectRoot

# Activate .venv(若存在);沒有 .venv 走系統 Python
$VenvActivate = Join-Path $ProjectRoot '.venv\Scripts\Activate.ps1'
if (Test-Path $VenvActivate) {
    & $VenvActivate
} else {
    Write-Host "WARNING: .venv 不存在於 $VenvActivate;用系統 Python"
}

# Console UTF-8(中文 log 不亂碼)
chcp 65001 | Out-Null
$env:PYTHONIOENCODING = 'utf-8'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

# Load .env(若存在),讓 DATABASE_URL / FINMIND_TOKEN 等可用
$EnvFile = Join-Path $ProjectRoot '.env'
if (Test-Path $EnvFile) {
    Get-Content $EnvFile | ForEach-Object {
        if ($_ -match '^\s*([^#][^=]*)=(.*)$') {
            $name = $matches[1].Trim()
            $value = $matches[2].Trim().Trim('"')
            [Environment]::SetEnvironmentVariable($name, $value, 'Process')
        }
    }
} else {
    Write-Host "WARNING: .env 不存在;假設 DATABASE_URL / FINMIND_TOKEN 已在系統環境變數"
}

# Log dir + dated log file
$LogDir = Join-Path $ProjectRoot 'logs'
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$LogFile = Join-Path $LogDir ('refresh_' + (Get-Date -Format 'yyyy-MM-dd') + '.log')

$argsStr = if ($RefreshArgs.Count -gt 0) { ' ' + ($RefreshArgs -join ' ') } else { '' }
$startMsg = "[refresh_daily] start at $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss');" `
          + " args: refresh$argsStr"
Write-Host $startMsg
Add-Content -Path $LogFile -Value $startMsg

# Run refresh,tee 全部 stdout+stderr 到 log file
# 使用 splatting 把 -RefreshArgs 攤平傳給 python
python src/main.py refresh @RefreshArgs 2>&1 | Tee-Object -FilePath $LogFile -Append

$exitCode = $LASTEXITCODE
$endMsg = "[refresh_daily] end at $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), exit code $exitCode"
Write-Host $endMsg
Add-Content -Path $LogFile -Value $endMsg

exit $exitCode
