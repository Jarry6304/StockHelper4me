# verify_golden_l3_v4_32.ps1
# ──────────────────────────────────────────────────────────────
# v4.32 Golden L3 production verify 流水線 —— 物化 → SQL → MCP serving → (印) Web API / codegen 指令。
#
# 5 步:
#   1. (opt) 安裝 Web API + codegen 依賴      (pip install -e ".[web]" pydantic-to-typescript)
#   2. Golden fusion 物化                      (python src/main.py golden fusion;預設小股集快驗)
#   3. SQL spot-check                          (structural_snapshots *_fusion row count + 取樣)
#   4. MCP serving-from-materialized smoke     (stock_levels / dual_track_resonance / market_context)
#   5. (印) Web API + codegen 手動指令          (uvicorn 長駐 + tsc 需手動;不自動起服務)
#
# 手動執行:
#   .\scripts\verify_golden_l3_v4_32.ps1                       # 預設 2330,3030,1101
#   .\scripts\verify_golden_l3_v4_32.ps1 -Stocks ''           # 全市場物化(重,resonance × 全 universe × 3tf)
#   .\scripts\verify_golden_l3_v4_32.ps1 -SkipInstall          # 已裝依賴
#
# 前置:已跑過 refresh / refresh_full(structural_snapshots 有 neely/trendline/SR/env cores),
#       否則物化讀不到上游。Logs 寫 logs/verify_golden_l3_YYYY-MM-DD.log。
# ──────────────────────────────────────────────────────────────

param(
    [string]$Stocks = '2330,3030,1101',
    [switch]$SkipInstall
)

$ErrorActionPreference = 'Continue'

# Resolve project root
$ProjectRoot = Split-Path -Parent $PSScriptRoot
Set-Location $ProjectRoot

# Activate .venv(若存在)
$VenvActivate = Join-Path $ProjectRoot '.venv\Scripts\Activate.ps1'
if (Test-Path $VenvActivate) { & $VenvActivate }
else { Write-Host "WARNING: .venv 不存在於 $VenvActivate;用系統 Python" }

# Console UTF-8
chcp 65001 | Out-Null
$env:PYTHONIOENCODING = 'utf-8'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

# Load .env
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
$LogFile = Join-Path $LogDir ('verify_golden_l3_' + (Get-Date -Format 'yyyy-MM-dd') + '.log')

$StockArgs = @()
if ($Stocks -ne '') { $StockArgs = @('--stocks', $Stocks) }
$FirstStock = if ($Stocks -ne '') { ($Stocks -split ',')[0].Trim() } else { '2330' }

$StepResults = @()
$TotalSteps = if ($SkipInstall) { 4 } else { 5 }

function Invoke-Step {
    param([int]$Idx, [string]$Label, [scriptblock]$Action)
    $banner = "[verify_golden_l3] Step $Idx/${TotalSteps}: $Label  ($(Get-Date -Format 'HH:mm:ss'))"
    Write-Host ('=' * 70); Write-Host $banner; Write-Host ('=' * 70)
    Add-Content -Path $LogFile -Value $banner
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    & $Action 2>&1 | Tee-Object -FilePath $LogFile -Append
    $sw.Stop()
    $code = $LASTEXITCODE
    $secs = [int]$sw.Elapsed.TotalSeconds
    $status = if ($code -eq 0 -or $null -eq $code) { 'ok' } else { "exit=$code" }
    Write-Host "[verify_golden_l3] Step $Idx/$TotalSteps done: $status, elapsed=${secs}s"
    $script:StepResults += [PSCustomObject]@{ Step = $Label; Status = $status; Secs = $secs }
}

Write-Host "[verify_golden_l3] start $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss'); stocks='$Stocks'"
$idx = 0

# Step 1: 依賴
if (-not $SkipInstall) {
    $idx++; Invoke-Step $idx 'Install Web API + codegen deps' {
        pip install -e ".[web]" pydantic-to-typescript
    }
}

# Step 2: 物化
$idx++; Invoke-Step $idx 'Golden fusion 物化 (levels/resonance/climate)' {
    python src/main.py golden fusion @StockArgs
}

# Step 3: SQL spot-check
$idx++; Invoke-Step $idx 'SQL spot-check (*_fusion rows)' {
    psql $env:DATABASE_URL -c @"
SELECT core_name, timeframe, COUNT(*) AS rows, MAX(snapshot_date) AS latest
  FROM structural_snapshots
 WHERE core_name LIKE '%_fusion'
 GROUP BY core_name, timeframe
 ORDER BY core_name, timeframe;
"@
    Write-Host "--- $FirstStock levels_fusion 取樣 ---"
    psql $env:DATABASE_URL -c @"
SELECT snapshot->>'level_count' AS level_count,
       jsonb_array_length(snapshot->'levels') AS levels_len
  FROM structural_snapshots
 WHERE stock_id='$FirstStock' AND core_name='levels_fusion' AND timeframe='_all_'
 ORDER BY snapshot_date DESC LIMIT 1;
"@
    Write-Host "--- climate_fusion 取樣 ---"
    psql $env:DATABASE_URL -c @"
SELECT snapshot->>'overall_climate' AS climate, snapshot->>'climate_score' AS score
  FROM structural_snapshots
 WHERE stock_id='_market_' AND core_name='climate_fusion'
 ORDER BY snapshot_date DESC LIMIT 1;
"@
}

# Step 4: MCP serving-from-materialized smoke
$idx++; Invoke-Step $idx 'MCP serving-from-materialized smoke' {
    $py = @"
import sys; sys.path.insert(0,'src'); sys.path.insert(0,'.')
from datetime import date
from mcp_server.tools.data import stock_levels, dual_track_resonance, market_context
sid = '$FirstStock'; as_of = date.today().isoformat()
lv = stock_levels(sid, as_of)
print('stock_levels.key_levels.level_count =', (lv.get('key_levels') or {}).get('level_count'))
dt = dual_track_resonance(sid, as_of)
print('dual_track_resonance.findings =', len(dt.get('findings', [])), '/ single_track =', dt.get('single_track_mode'))
mc = market_context(as_of)
print('market_context.overall_climate =', mc.get('overall_climate'))
"@
    python -c $py
}

# Step 5(印):Web API + codegen 手動指令(長駐服務 / tsc 不自動跑)
Write-Host ('=' * 70)
Write-Host '[verify_golden_l3] Web API + codegen 手動驗證指令(另開 terminal):'
Write-Host ('=' * 70)
Write-Host @"
# ── 唯讀 Web API ──
uvicorn web_api.app:app                                 # 另開,長駐
curl 'http://localhost:8000/stocks/$FirstStock/levels?as_of=$(Get-Date -Format yyyy-MM-dd)'
curl 'http://localhost:8000/market/climate?as_of=$(Get-Date -Format yyyy-MM-dd)'
curl -w '%{http_code}\n' -o NUL "http://localhost:8000/stocks/$FirstStock/neely/forest?as_of=$(Get-Date -Format yyyy-MM-dd)&timeframe=daily"
curl -s -H 'Accept-Encoding: br' -D - -o NUL "http://localhost:8000/stocks/$FirstStock/neely/forest?as_of=$(Get-Date -Format yyyy-MM-dd)" | Select-String -Pattern 'content-encoding'

# ── TS 契約 codegen(Rust 加欄位後重生)──
bash codegen/generate.sh
cd frontend; & "`$(npm root -g)/typescript/bin/tsc" --noEmit -p tsconfig.json; cd ..
"@

# Summary
Write-Host ''
Write-Host ('=' * 70)
Write-Host 'verify_golden_l3 結果'
Write-Host ('=' * 70)
$StepResults | Format-Table -AutoSize
$okCount = ($StepResults | Where-Object { $_.Status -eq 'ok' }).Count
Write-Host "OK: $okCount/$($StepResults.Count) steps"
Write-Host "Log: $LogFile"
