# refresh_full.ps1
# ──────────────────────────────────────────────────────────────
# 完整補完 wrapper —— 整條 pipeline 從打 API 到計算層跑一遍(full-rebuild)。
# 「類別 1:完整補完到最新日期」;每日排程請用 refresh_daily.ps1(類別 2)。
#
# 6 步:
#   1. Bronze incremental                  (FinMind → Bronze)
#   2. Silver 7c                           (Rust 後復權 price_*_fwd)
#   3. Silver 7a --full-rebuild            (13 builder 全表重算)
#   4. Silver 7b --full-rebuild            (financial_statement 跨表)
#   5. Cross-Stock Cores 8 --full-rebuild  (lookback 全部 dates)
#   6. M3 Cores tw_cores run-all --write   (全市場全核)
#
# 何時用:隔很久沒跑 / 遷移後 / 距上次 incremental > 30 天(Silver 7a WRITE 窗,
#         窗外舊 row 不會被 incremental 更新)/ 想確保端到端全部重算。
#         日常每天走 refresh_daily.ps1 即可。
#
# 手動執行:
#   .\scripts\refresh_full.ps1
#   .\scripts\refresh_full.ps1 -Stocks '2330,2317'   # 限縮股票(開發測試)
#   .\scripts\refresh_full.ps1 -SkipCores            # 無 Rust binary 時
#
# Logs 寫到 logs/refresh_full_YYYY-MM-DD.log。
# 每步獨立,前段失敗不阻擋後段(對齊 `python src/main.py refresh` 設計)。
# ──────────────────────────────────────────────────────────────

param(
    [string]$Stocks = '',
    [switch]$SkipCores
)

$ErrorActionPreference = 'Continue'  # 不讓 PowerShell 錯誤短路 chain

# Resolve project root(scripts/.. = 專案 root)
$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $ProjectRoot

# Activate .venv(若存在);沒有走系統 Python
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

# Load .env(讓 DATABASE_URL / FINMIND_TOKEN 可用)
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
$LogFile = Join-Path $LogDir ('refresh_full_' + (Get-Date -Format 'yyyy-MM-dd') + '.log')

# --stocks 引數(splat 進每個 python step)
$StockArgs = @()
if ($Stocks -ne '') { $StockArgs = @('--stocks', $Stocks) }

$StepResults = @()
$TotalSteps = if ($SkipCores) { 5 } else { 6 }

function Invoke-Step {
    param([int]$Idx, [string]$Label, [scriptblock]$Action)
    $banner = "[refresh_full] Step $Idx/${TotalSteps}: $Label  ($(Get-Date -Format 'HH:mm:ss'))"
    Write-Host ('=' * 70)
    Write-Host $banner
    Write-Host ('=' * 70)
    Add-Content -Path $LogFile -Value $banner
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    & $Action 2>&1 | Tee-Object -FilePath $LogFile -Append
    $sw.Stop()
    $code = $LASTEXITCODE
    $secs = [int]$sw.Elapsed.TotalSeconds
    $status = if ($code -eq 0) { 'ok' } else { "exit=$code" }
    $done = "[refresh_full] Step $Idx/$TotalSteps done: $status, elapsed=${secs}s"
    Write-Host $done
    Add-Content -Path $LogFile -Value $done
    $script:StepResults += [PSCustomObject]@{ Step = $Label; Status = $status; Secs = $secs }
}

$startMsg = "[refresh_full] start at $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss'); stocks='$Stocks' skipCores=$SkipCores"
Write-Host $startMsg
Add-Content -Path $LogFile -Value $startMsg
$runStart = [System.Diagnostics.Stopwatch]::StartNew()

Invoke-Step 1 'Bronze incremental' {
    python src/main.py incremental @StockArgs
}
Invoke-Step 2 'Silver 7c' {
    python src/main.py silver phase 7c @StockArgs
}
Invoke-Step 3 'Silver 7a --full-rebuild' {
    python src/main.py silver phase 7a --full-rebuild @StockArgs
}
Invoke-Step 4 'Silver 7b --full-rebuild' {
    python src/main.py silver phase 7b --full-rebuild @StockArgs
}
Invoke-Step 5 'Cross-Stock Cores 8 --full-rebuild' {
    python src/main.py cross_cores phase 8 --full-rebuild
}

if (-not $SkipCores) {
    $TwCores = Join-Path $ProjectRoot 'rust_compute\target\release\tw_cores.exe'
    if (Test-Path $TwCores) {
        Invoke-Step 6 'M3 Cores run-all --write' {
            if ($Stocks -ne '') {
                & $TwCores run-all --write --stocks $Stocks
            } else {
                & $TwCores run-all --write
            }
        }
    } else {
        $warn = "[refresh_full] Step 6/$TotalSteps SKIP: tw_cores binary 不存在 ($TwCores) — 先跑 cargo build --release -p tw_cores"
        Write-Host $warn
        Add-Content -Path $LogFile -Value $warn
        $StepResults += [PSCustomObject]@{ Step = 'M3 Cores run-all'; Status = 'skipped(binary missing)'; Secs = 0 }
    }
}

$runStart.Stop()

# Summary
$summary = @()
$summary += ''
$summary += ('=' * 70)
$summary += 'refresh_full 結果'
$summary += ('=' * 70)
foreach ($r in $StepResults) {
    $summary += ('{0,-36} {1,-24} {2,6}s' -f $r.Step, $r.Status, $r.Secs)
}
$summary += ('-' * 70)
$summary += ('{0,-36} {1,-24} {2,6}s' -f 'total', '', [int]$runStart.Elapsed.TotalSeconds)
$okCount = ($StepResults | Where-Object { $_.Status -eq 'ok' }).Count
$summary += ("OK: {0}/{1} steps" -f $okCount, $StepResults.Count)
$summary += ''
$summary | ForEach-Object { Write-Host $_; Add-Content -Path $LogFile -Value $_ }
