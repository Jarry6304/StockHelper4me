# bootstrap_rebuild.ps1
# ──────────────────────────────────────────────────────────────
# 從零重建整條 pipeline(DB 全清重裝後用)。
#
# ⚠️ 前置(本腳本「不」做,需先手動完成 — 見 README/CLAUDE 或對話說明):
#   0a. 重裝 PostgreSQL 17(GUI installer)
#   0b. 建 role + database:
#         psql -U postgres -c "CREATE ROLE twstock LOGIN PASSWORD 'twstock' SUPERUSER;"
#         psql -U postgres -c "CREATE DATABASE twstock OWNER twstock;"
#   0c. .env 內 DATABASE_URL / FINMIND_TOKEN 正確
#
# 本腳本 8 步(每步獨立,前段失敗不阻擋後段,summary 表列出哪步要重跑):
#   1. alembic upgrade head                      (建 schema,baseline 跑 schema_pg.sql → head i5j6k7l8m9n0)
#   2. cargo build --release(rust_compute)       (建 tw_stock_compute + tw_cores 兩 binary;已存在則跳)
#   3. Bronze backfill(全量)                     (FinMind → Bronze;tpex 解鎖後 ~2172 檔,數小時)
#   4. Silver 7c / 7a / 7b --full-rebuild         (後復權 + 13 builder + financial)
#   5. Cross-Stock Cores 8 --full-rebuild         (12 builders)
#   6. M3 Cores run-all --write                   (全市場全核 ~37 分)
#   7. golden fusion                              (levels/resonance/climate 物化)
#   8. recalibrate_kalman(可選,~80 分)          (Kalman 全市場校準 → resonance track2 非 single_track)
#
# 用法:
#   .\scripts\bootstrap_rebuild.ps1                  # 全跑(含 recalibrate)
#   .\scripts\bootstrap_rebuild.ps1 -SkipRecalibrate # 跳過 Kalman 校準(之後另跑 recalibrate_kalman.ps1)
#   .\scripts\bootstrap_rebuild.ps1 -Stocks '2330,2317'  # 限縮(開發測試,不建議正式重建用)
#
# Logs 寫到 logs/bootstrap_rebuild_YYYY-MM-DD.log。
# ──────────────────────────────────────────────────────────────

param(
    [string]$Stocks = '',
    [switch]$SkipRecalibrate
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
$LogFile = Join-Path $LogDir ('bootstrap_rebuild_' + (Get-Date -Format 'yyyy-MM-dd') + '.log')

# --stocks 引數(splat 進每個 python step)
$StockArgs = @()
if ($Stocks -ne '') { $StockArgs = @('--stocks', $Stocks) }

$StepResults = @()

function Invoke-Step {
    param([string]$Label, [scriptblock]$Action)
    $banner = "[bootstrap] $Label  ($(Get-Date -Format 'HH:mm:ss'))"
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
    $done = "[bootstrap] done: $Label -> $status, elapsed=${secs}s"
    Write-Host $done
    Add-Content -Path $LogFile -Value $done
    $script:StepResults += [PSCustomObject]@{ Step = $Label; Status = $status; Secs = $secs }
}

$startMsg = "[bootstrap] start at $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss'); stocks='$Stocks' skipRecalibrate=$SkipRecalibrate"
Write-Host $startMsg
Add-Content -Path $LogFile -Value $startMsg
$runStart = [System.Diagnostics.Stopwatch]::StartNew()

# ── 1. Schema ──
Invoke-Step '1. alembic upgrade head' {
    alembic upgrade head
}

# ── 2. Rust binaries(兩 binary:tw_stock_compute for 7c + tw_cores for M3)──
$TwCores = Join-Path $ProjectRoot 'rust_compute\target\release\tw_cores.exe'
$TwCompute = Join-Path $ProjectRoot 'rust_compute\target\release\tw_stock_compute.exe'
if ((Test-Path $TwCores) -and (Test-Path $TwCompute)) {
    $msg = "[bootstrap] 2. cargo build SKIP: 兩 binary 已存在"
    Write-Host $msg; Add-Content -Path $LogFile -Value $msg
    $StepResults += [PSCustomObject]@{ Step = '2. cargo build'; Status = 'skipped(exists)'; Secs = 0 }
} else {
    Invoke-Step '2. cargo build --release (rust_compute)' {
        Push-Location (Join-Path $ProjectRoot 'rust_compute')
        cargo build --release
        Pop-Location
    }
}

# ── 3. Bronze 全量 backfill ──
Invoke-Step '3. Bronze backfill (full)' {
    python src/main.py backfill @StockArgs
}

# ── 4. Silver 7c / 7a / 7b ──
Invoke-Step '4a. Silver 7c --full-rebuild' {
    python src/main.py silver phase 7c --full-rebuild @StockArgs
}
Invoke-Step '4b. Silver 7a --full-rebuild' {
    python src/main.py silver phase 7a --full-rebuild @StockArgs
}
Invoke-Step '4c. Silver 7b --full-rebuild' {
    python src/main.py silver phase 7b --full-rebuild @StockArgs
}

# ── 5. Cross-Stock Cores ──
Invoke-Step '5. Cross-Stock Cores 8 --full-rebuild' {
    python src/main.py cross_cores phase 8 --full-rebuild
}

# ── 6. M3 Cores ──
if (Test-Path $TwCores) {
    Invoke-Step '6. M3 Cores run-all --write' {
        if ($Stocks -ne '') {
            & $TwCores run-all --write --stocks $Stocks
        } else {
            & $TwCores run-all --write
        }
    }
} else {
    $warn = "[bootstrap] 6. M3 Cores SKIP: tw_cores binary 不存在($TwCores)"
    Write-Host $warn; Add-Content -Path $LogFile -Value $warn
    $StepResults += [PSCustomObject]@{ Step = '6. M3 Cores run-all'; Status = 'skipped(binary missing)'; Secs = 0 }
}

# ── 7. Golden L3 物化 ──
Invoke-Step '7. golden fusion' {
    python src/main.py golden fusion
}

# ── 8. Kalman 全市場校準(可選)──
if (-not $SkipRecalibrate) {
    $Recal = Join-Path $ProjectRoot 'scripts\recalibrate_kalman.ps1'
    if (Test-Path $Recal) {
        Invoke-Step '8. recalibrate_kalman (Kalman 全市場校準)' {
            & $Recal
        }
    } else {
        $warn = "[bootstrap] 8. recalibrate SKIP: $Recal 不存在"
        Write-Host $warn; Add-Content -Path $LogFile -Value $warn
        $StepResults += [PSCustomObject]@{ Step = '8. recalibrate_kalman'; Status = 'skipped(missing)'; Secs = 0 }
    }
} else {
    $msg = "[bootstrap] 8. recalibrate SKIP(-SkipRecalibrate);之後另跑 scripts\recalibrate_kalman.ps1"
    Write-Host $msg; Add-Content -Path $LogFile -Value $msg
    $StepResults += [PSCustomObject]@{ Step = '8. recalibrate_kalman'; Status = 'skipped(-SkipRecalibrate)'; Secs = 0 }
}

$runStart.Stop()

# Summary
$summary = @()
$summary += ''
$summary += ('=' * 70)
$summary += 'bootstrap_rebuild 結果'
$summary += ('=' * 70)
foreach ($r in $StepResults) {
    $summary += ('{0,-44} {1,-26} {2,7}s' -f $r.Step, $r.Status, $r.Secs)
}
$summary += ('-' * 70)
$summary += ('{0,-44} {1,-26} {2,7}s' -f 'total', '', [int]$runStart.Elapsed.TotalSeconds)
$okCount = ($StepResults | Where-Object { $_.Status -eq 'ok' }).Count
$summary += ("OK: {0}/{1} steps" -f $okCount, $StepResults.Count)
$summary += ''
$summary += 'verify(跑完後手動確認):'
$summary += '  python -m pytest tests/cross_cores/test_magic_formula.py -q'
$summary += '  (psql) SELECT COUNT(*) FILTER (WHERE is_top_n), COUNT(*) FROM magic_formula_ranked_derived WHERE date=(SELECT MAX(date) FROM magic_formula_ranked_derived);'
$summary += ''
$summary | ForEach-Object { Write-Host $_; Add-Content -Path $LogFile -Value $_ }
