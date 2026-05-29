# recalibrate_kalman.ps1
# ──────────────────────────────────────────────────────────────
# Phase 3b — Kalman 統計軌全市場校準(讓 resonance track2 非 single_track)。
# 「最划算路徑」:track2 source 偏好 fusion → kalman_cqr → ...;只要 kalman_cqr 有 band,
# track2 即非 degenerate。Kalman backtest 走 Rust run-backtest(全市場 + 並行)。
#
# 5 步:
#   1. Kalman run-backtest (Rust)            — 全市場逐日 backtest,寫 forecast_log raw
#   2. forecast settle --core kalman_forecast_core
#   3. forecast conformalize → kalman_cqr     — CQR 校準帶(track2 真正讀的)
#   4. forecast settle --core kalman_cqr
#   5. golden fusion --only resonance         — 重新物化 resonance(讓 Web API /resonance 也服務)
#                                               (-SkipMaterialize 可跳;MCP 本就 compute-fallback)
#
# 手動 / 週排程:
#   .\scripts\recalibrate_kalman.ps1                       # 全市場,--since 2022-01-01
#   .\scripts\recalibrate_kalman.ps1 -Stocks '2330,2603'   # 限縮
#   .\scripts\recalibrate_kalman.ps1 -Since 2023-01-01     # 縮窗加速
#   .\scripts\recalibrate_kalman.ps1 -SkipMaterialize      # 只校準,不重物化 resonance
#
# ─── 為什麼是「週排程」不是 daily(前因後果)──────────────────────────────────
# 前因:resonance track2 讀 forecast_log 的統計軌 band(kalman_cqr 等),這些來自
#       backtest + conformalize。daily `refresh` 的 Step7 只跑 emit-neely + fuse(latest),
#       **不重跑統計軌 backtest** → track2 band 會隨時間 stale。
# 成本:本腳本 Kalman 全市場 ~12 min(Rust 並行)+ conformalize ~21 min ≈ 35 min。
#       若併進 daily refresh:daily 從 ~26 min(bronze/silver ~14 + M3 cores ~12)→ ~61 min,
#       對「一鍵每日」過重 → 決議**不放 daily**,獨立成本腳本走週排程。
# 取捨:兩次週跑之間 track2 band 最多 stale ~7 天。resonance 是「結構軌 × 統計軌」confluence,
#       horizon 為 21/63/126 天,數天漂移對判定影響小 → 可接受。
# 未來若要 daily:需把 run-backtest 改「**增量 latest-only**」(只算最新 1 日 forward +
#       conformalize latest,校準窗 history 已在 DB)→ 估 ~2-5 min,才適合併進 refresh Step7。
#       目前 run-backtest 走 [start, today] 全區間,故先週排程。
# ──────────────────────────────────────────────────────────────

param(
    [string]$Stocks = '',
    [string]$Since = '2022-01-01',
    [int]$Concurrency = 8,
    [switch]$SkipMaterialize
)

$ErrorActionPreference = 'Continue'

$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $ProjectRoot

# venv
$VenvActivate = Join-Path $ProjectRoot '.venv\Scripts\Activate.ps1'
if (Test-Path $VenvActivate) { & $VenvActivate }
else { Write-Host "WARNING: .venv 不存在於 $VenvActivate;用系統 Python" }

# Console UTF-8
chcp 65001 | Out-Null
$env:PYTHONIOENCODING = 'utf-8'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

# Load .env(psql / tw_cores / python 都需 DATABASE_URL — 互動式 shell 不會自動載)
$EnvFile = Join-Path $ProjectRoot '.env'
if (Test-Path $EnvFile) {
    Get-Content $EnvFile | ForEach-Object {
        if ($_ -match '^\s*([^#][^=]*)=(.*)$') {
            [Environment]::SetEnvironmentVariable($matches[1].Trim(), $matches[2].Trim().Trim('"'), 'Process')
        }
    }
} else {
    Write-Host "WARNING: .env 不存在;假設 DATABASE_URL 已在系統環境變數"
}

$LogDir = Join-Path $ProjectRoot 'logs'
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$LogFile = Join-Path $LogDir ('recalibrate_kalman_' + (Get-Date -Format 'yyyy-MM-dd') + '.log')

# stock 清單:顯式 -Stocks 或全市場(用 repo get_connection,自動讀 .env,避開 psql 引號/auth 坑)
if ($Stocks -ne '') {
    $ids = $Stocks
} else {
    Write-Host "[recal] 取全市場 stock 清單..."
    $pyGetIds = @"
import sys; sys.path.insert(0, 'src')
from fusion.raw._db import get_connection
c = get_connection(); cur = c.cursor()
cur.execute("SELECT DISTINCT stock_id FROM price_daily_fwd WHERE market='TW' ORDER BY stock_id")
print(','.join(r['stock_id'] for r in cur.fetchall()))
"@
    $ids = ($pyGetIds | python -).Trim()
}
$idCount = if ($ids -ne '') { ($ids -split ',').Count } else { 0 }
if ($idCount -eq 0) {
    Write-Host "[recal] ERROR: 無 stock 清單(DATABASE_URL? price_daily_fwd 空?),中止"
    exit 1
}

$TwCores = Join-Path $ProjectRoot 'rust_compute\target\release\tw_cores.exe'
$StepResults = @()
$TotalSteps = if ($SkipMaterialize) { 4 } else { 5 }

function Invoke-Step {
    param([int]$Idx, [string]$Label, [scriptblock]$Action)
    $banner = "[recal] Step $Idx/${TotalSteps}: $Label  ($(Get-Date -Format 'HH:mm:ss'))"
    Write-Host ('=' * 70); Write-Host $banner; Write-Host ('=' * 70)
    Add-Content -Path $LogFile -Value $banner
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    & $Action 2>&1 | Tee-Object -FilePath $LogFile -Append
    $sw.Stop()
    $code = $LASTEXITCODE
    $secs = [int]$sw.Elapsed.TotalSeconds
    $status = if ($code -eq 0 -or $null -eq $code) { 'ok' } else { "exit=$code" }
    Write-Host "[recal] Step $Idx/$TotalSteps done: $status, elapsed=${secs}s"
    $script:StepResults += [PSCustomObject]@{ Step = $Label; Status = $status; Secs = $secs }
}

Write-Host "[recal] start $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss'); stocks=$idCount since=$Since concurrency=$Concurrency"
$runStart = [System.Diagnostics.Stopwatch]::StartNew()

# Step 1: Kalman run-backtest(Rust,全市場 + 並行)
if (-not (Test-Path $TwCores)) {
    Write-Host "[recal] ERROR: tw_cores binary 不存在 ($TwCores) — 先 cargo build --release -p tw_cores,中止"
    exit 1
}
Invoke-Step 1 'Kalman run-backtest (Rust)' {
    & $TwCores run-backtest --stocks $ids --start $Since --core kalman_forecast_core --write --concurrency $Concurrency
}

# Step 2: settle raw
Invoke-Step 2 'settle kalman_forecast_core' {
    python src/main.py forecast settle --core kalman_forecast_core
}

# Step 3: conformalize → kalman_cqr(track2 真正讀的校準帶)
Invoke-Step 3 'conformalize → kalman_cqr' {
    python src/main.py forecast conformalize --raw-core kalman_forecast_core --target-core kalman_cqr --stocks $ids --since $Since
}

# Step 4: settle kalman_cqr
Invoke-Step 4 'settle kalman_cqr' {
    python src/main.py forecast settle --core kalman_cqr
}

# Step 5: 重新物化 resonance(讓 Web API /resonance 服務新鮮 band;MCP 本就 compute-fallback)
if (-not $SkipMaterialize) {
    Invoke-Step 5 'golden fusion --only resonance (re-materialize)' {
        if ($Stocks -ne '') {
            python src/main.py golden fusion --only resonance --stocks $Stocks
        } else {
            python src/main.py golden fusion --only resonance
        }
    }
}

$runStart.Stop()

# Summary
Write-Host ''
Write-Host ('=' * 70)
Write-Host 'recalibrate_kalman 結果'
Write-Host ('=' * 70)
$StepResults | Format-Table -AutoSize
$okCount = ($StepResults | Where-Object { $_.Status -eq 'ok' }).Count
Write-Host "OK: $okCount/$($StepResults.Count) steps;total $([int]$runStart.Elapsed.TotalSeconds)s"
Write-Host "Log: $LogFile"
Write-Host ''
Write-Host "驗證(任一非 verify 股應 single_track=False):"
Write-Host "  python -c `"import sys; sys.path.insert(0,'src'); sys.path.insert(0,'.'); from mcp_server.tools.data import dual_track_resonance as f; r=f('2603','$(Get-Date -Format yyyy-MM-dd)'); print('single_track:', r['single_track_mode'], 'findings:', len(r['findings']))`""
