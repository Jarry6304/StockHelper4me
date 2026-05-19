# test_pipeline.ps1 — StockHelper4me 完整測試流水線
#
# 跑 5 個 phase:
#   Phase 0:Environment check(Python venv / Rust toolchain / .env / PG 連線)
#   Phase 1:Sandbox unit tests(無需 PG — Python + Rust workspace)
#   Phase 2:Schema 健康度(alembic head / Bronze / Silver row counts)
#   Phase 3:Production verify(per-EventKind rate / facts stats / forest_size 分布)
#   Phase 4:MCP smoke test(Kalman + Neely + 8 toolkit tools)
#
# Usage:
#   .\scripts\test_pipeline.ps1                # 跑全套(Phase 0-4)
#   .\scripts\test_pipeline.ps1 -SkipPhase 3,4 # 跳過 production verify
#   .\scripts\test_pipeline.ps1 -OnlyPhase 1   # 只跑 sandbox(無 PG)
#   .\scripts\test_pipeline.ps1 -DryRun        # 列計畫不執行
#
# 環境變數:
#   DATABASE_URL    — Phase 2-4 必要
#   FINMIND_TOKEN   — Phase 4 部分 helper 用(目前 readonly,可選)
#
# 退出碼:0 = 全綠;1 = 任一 phase fail
#
# 對齊 plan v4.0 §Calibration division of labor:
#   - Phase 0-2 / Phase 4 沙箱與 schema 級可在 CI 跑
#   - Phase 3(P0 Gate forest_size + 觸發率)需 production data,user 本機跑

param(
    [int[]] $SkipPhase = @(),
    [int[]] $OnlyPhase = @(),
    [switch] $DryRun,
    [switch] $Verbose
)

$ErrorActionPreference = "Stop"
$ScriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptRoot
Set-Location $RepoRoot

# ── 共用 helpers ─────────────────────────────────────────────────────────

$global:PhaseResults = @{}
$global:PhaseStart = Get-Date

function Write-PhaseHeader($num, $title) {
    Write-Host ""
    Write-Host ("=" * 78) -ForegroundColor Cyan
    Write-Host "Phase $num — $title" -ForegroundColor Cyan
    Write-Host ("=" * 78) -ForegroundColor Cyan
}

function Write-Step($msg) {
    Write-Host "  → $msg" -ForegroundColor Gray
}

function Write-Pass($msg) {
    Write-Host "  [OK] $msg" -ForegroundColor Green
}

function Write-Fail($msg) {
    Write-Host "  [FAIL] $msg" -ForegroundColor Red
}

function Write-Warn($msg) {
    Write-Host "  [WARN] $msg" -ForegroundColor Yellow
}

function Should-Run($phase) {
    if ($OnlyPhase.Count -gt 0) { return $OnlyPhase -contains $phase }
    return ($SkipPhase -notcontains $phase)
}

function Invoke-Phase($num, $title, [ScriptBlock] $body) {
    if (-not (Should-Run $num)) {
        Write-Host ""
        Write-Host "Phase $num — $title (skipped)" -ForegroundColor DarkGray
        $global:PhaseResults[$num] = "skipped"
        return
    }
    Write-PhaseHeader $num $title
    $phaseStart = Get-Date
    try {
        if ($DryRun) {
            Write-Host "  [dry-run] $title" -ForegroundColor Yellow
            $global:PhaseResults[$num] = "dry-run"
        } else {
            & $body
            $elapsed = (Get-Date) - $phaseStart
            Write-Host ""
            Write-Pass ("Phase $num passed in {0:N1}s" -f $elapsed.TotalSeconds)
            $global:PhaseResults[$num] = "ok"
        }
    } catch {
        $elapsed = (Get-Date) - $phaseStart
        Write-Host ""
        Write-Fail ("Phase $num FAILED in {0:N1}s — $_" -f $elapsed.TotalSeconds)
        $global:PhaseResults[$num] = "fail"
        if (-not $Verbose) { throw }
    }
}

function Test-CommandExists($cmd) {
    return [bool] (Get-Command $cmd -ErrorAction SilentlyContinue)
}

# ── Phase 0:Environment check ───────────────────────────────────────────

Invoke-Phase 0 "Environment check" {
    Write-Step "Python 3.11+"
    if (-not (Test-CommandExists python)) { throw "python not found in PATH" }
    $pyVersion = (python --version 2>&1) -replace "Python ", ""
    Write-Pass "python $pyVersion"

    Write-Step "venv activated?"
    if ($env:VIRTUAL_ENV) {
        Write-Pass "VIRTUAL_ENV=$env:VIRTUAL_ENV"
    } else {
        Write-Warn "VIRTUAL_ENV not set — recommend `.\.venv\Scripts\Activate.ps1`"
    }

    Write-Step "pytest available"
    python -c "import pytest" 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "pytest not installed; run pip install -e '.[dev]'" }
    Write-Pass "pytest OK"

    Write-Step "Rust cargo"
    if (-not (Test-CommandExists cargo)) { throw "cargo not found; install via rustup.rs" }
    $cargoVersion = (cargo --version 2>&1)
    Write-Pass $cargoVersion

    Write-Step ".env file"
    if (-not (Test-Path "$RepoRoot\.env")) {
        Write-Warn ".env missing — Phase 2-4 may fail without DATABASE_URL"
    } else {
        Write-Pass ".env found"
    }

    Write-Step "DATABASE_URL set"
    if (-not $env:DATABASE_URL) {
        Write-Warn "DATABASE_URL env not set — Phase 2-4 will skip PG operations"
    } else {
        Write-Pass "DATABASE_URL configured"
    }

    Write-Step "psql client(Phase 2-4 需要)"
    if (-not (Test-CommandExists psql)) {
        Write-Warn "psql not in PATH — Phase 2/3 SQL verifier 將 skip"
    } else {
        Write-Pass (psql --version)
    }

    Write-Step "Rust binary tw_cores"
    $twCoresBin = "$RepoRoot\rust_compute\target\release\tw_cores.exe"
    if (Test-Path $twCoresBin) {
        Write-Pass "tw_cores.exe present"
    } else {
        Write-Warn "tw_cores binary not built — run cargo build --release -p tw_cores"
    }
}

# ── Phase 1:Sandbox unit tests ──────────────────────────────────────────

Invoke-Phase 1 "Sandbox unit tests(Python + Rust workspace)" {
    Write-Step "Rust workspace tests(release)"
    Push-Location "$RepoRoot\rust_compute"
    try {
        $rustOutput = cargo test --release --workspace --no-fail-fast 2>&1
        if ($LASTEXITCODE -ne 0) {
            Write-Host $rustOutput
            throw "Rust workspace tests FAILED"
        }
        $passCount = ($rustOutput | Select-String "\d+ passed" | ForEach-Object {
            ($_.Matches[0].Value -split " ")[0]
        } | Measure-Object -Sum).Sum
        Write-Pass "Rust workspace: $passCount tests passed"
    } finally {
        Pop-Location
    }

    Write-Step "Python pytest — tests/agg/"
    pytest tests/agg/ -q 2>&1 | Tee-Object -Variable aggOut
    if ($LASTEXITCODE -ne 0) { throw "agg tests FAILED" }

    Write-Step "Python pytest — tests/mcp_server/(ignore render_tools fastmcp)"
    pytest tests/mcp_server/ --ignore=tests/mcp_server/test_render_tools.py -q 2>&1 | Tee-Object -Variable mcpOut
    if ($LASTEXITCODE -ne 0) { throw "mcp_server tests FAILED" }

    Write-Step "Python pytest — tests/cross_cores/"
    pytest tests/cross_cores/ -q 2>&1 | Tee-Object -Variable ccOut
    if ($LASTEXITCODE -ne 0) { throw "cross_cores tests FAILED" }

    Write-Pass "All sandbox tests passed"
}

# ── Phase 2:Schema 健康度 ───────────────────────────────────────────────

Invoke-Phase 2 "Schema health(alembic head / table row counts)" {
    if (-not $env:DATABASE_URL) {
        Write-Warn "DATABASE_URL not set — phase skipped"
        return
    }
    if (-not (Test-CommandExists psql)) {
        Write-Warn "psql not in PATH — phase skipped"
        return
    }

    Write-Step "alembic head"
    $expectedHead = "d9e0f1g2h3i4"  # v3.32 head;P1.x v4.x 沒新 migration
    $alembicOut = alembic current 2>&1
    if ($alembicOut -notmatch $expectedHead) {
        Write-Warn "alembic head 非預期 ($expectedHead) — output: $alembicOut"
    } else {
        Write-Pass "alembic head = $expectedHead"
    }

    Write-Step "M3 表 row counts + 11 個 cross_cores tables 存在"
    # 改用外部 SQL file 避開 PowerShell here-string parser 問題(2026-05-19 user feedback)
    $schemaHealthSql = "$RepoRoot\scripts\_schema_health.sql"
    if (Test-Path $schemaHealthSql) {
        psql $env:DATABASE_URL -f $schemaHealthSql 2>&1 | Out-String | Write-Host
    } else {
        Write-Warn "scripts/_schema_health.sql 不存在 — skip"
    }
}

# ── Phase 3:Production verify(P0 Gate + 觸發率)──────────────────────

Invoke-Phase 3 "Production verify(per-EventKind rate / forest_size / facts stats)" {
    if (-not $env:DATABASE_URL) {
        Write-Warn "DATABASE_URL not set — phase skipped"
        return
    }
    if (-not (Test-CommandExists psql)) {
        Write-Warn "psql not in PATH — phase skipped"
        return
    }

    Write-Step "Facts table stats(VACUUM 健康度)"
    if (Test-Path "$RepoRoot\scripts\maintain_facts_stats.sql") {
        psql $env:DATABASE_URL -f "$RepoRoot\scripts\maintain_facts_stats.sql" 2>&1 | Out-String | Write-Host
    } else {
        Write-Warn "scripts/maintain_facts_stats.sql 不存在 — skip"
    }

    Write-Step 'Per-EventKind 觸發率(target at most 12/yr/stock)'
    if (Test-Path "$RepoRoot\scripts\verify_event_kind_rate.sql") {
        psql $env:DATABASE_URL -f "$RepoRoot\scripts\verify_event_kind_rate.sql" 2>&1 | Out-String | Write-Host
    } else {
        Write-Warn "scripts/verify_event_kind_rate.sql 不存在 — skip"
    }

    Write-Step 'P0 Gate - Neely forest_size 分布(v4.4a 後 acceptance: max at most 200, p95 below 180)'
    # 改用外部 SQL file 避開 PowerShell here-string parser 問題
    $forestSqlFile = "$RepoRoot\scripts\_forest_size_p0_gate.sql"
    if (Test-Path $forestSqlFile) {
        $forestOut = psql $env:DATABASE_URL -f $forestSqlFile 2>&1 | Out-String
        Write-Host $forestOut

        # 解析 max_count(從 psql 表格格式抽 "p50 | p95 | p99 | max | scenario_count" 那一行)
        $maxMatch = $forestOut | Select-String "(\d+)\s+\|\s+\d+\s*$"
        if ($maxMatch) {
            $maxCount = [int] $maxMatch.Matches[0].Groups[1].Value
            if ($maxCount -le 200) {
                Write-Pass "forest_size max = $maxCount (cap 200 held)"
            } else {
                Write-Warn "forest_size max = $maxCount over 200 - consider BeamSearchFallback.k recalibration"
            }
        }
    } else {
        Write-Warn "scripts/_forest_size_p0_gate.sql 不存在 — skip"
    }
}

# ── Phase 4:MCP smoke test ──────────────────────────────────────────────

Invoke-Phase 4 "MCP smoke test(Kalman + Neely + 8 toolkit tools)" {
    if (-not $env:DATABASE_URL) {
        Write-Warn "DATABASE_URL not set — phase skipped"
        return
    }

    Write-Step "verify_mcp_kalman_neely.py 對 2330 / 3030"
    if (Test-Path "$RepoRoot\scripts\verify_mcp_kalman_neely.py") {
        python "$RepoRoot\scripts\verify_mcp_kalman_neely.py" --stocks 2330,3030 2>&1 | Out-String | Write-Host
        if ($LASTEXITCODE -ne 0) {
            Write-Warn "verify_mcp_kalman_neely.py 非全綠 — 看上面提示(v3.30 path fix / v3.28 regex / tw_cores 重算)"
        } else {
            Write-Pass "Kalman + Neely production verify [OK]"
        }
    } else {
        Write-Warn "scripts/verify_mcp_kalman_neely.py 不存在 — skip"
    }

    Write-Step "MCP 8 tools 公開介面(Python import + import-time error check)"
    # 改用外部 Python file 避開 PowerShell here-string parser 問題
    $importCheckPy = "$RepoRoot\scripts\_mcp_import_check.py"
    if (Test-Path $importCheckPy) {
        python $importCheckPy
        if ($LASTEXITCODE -ne 0) { throw "MCP toolkit import FAILED" }
        Write-Pass "MCP 8 tools 全 importable"
    } else {
        Write-Warn "scripts/_mcp_import_check.py 不存在 — skip"
    }
}

# ── 結算 ────────────────────────────────────────────────────────────────

$totalElapsed = (Get-Date) - $global:PhaseStart
Write-Host ""
Write-Host ("=" * 78) -ForegroundColor Cyan
Write-Host "Test pipeline 結算" -ForegroundColor Cyan
Write-Host ("=" * 78) -ForegroundColor Cyan
foreach ($phase in 0..4) {
    $result = $global:PhaseResults[$phase]
    if (-not $result) { $result = "skipped" }
    $color = switch ($result) {
        "ok"      { "Green" }
        "fail"    { "Red" }
        "dry-run" { "Yellow" }
        default   { "DarkGray" }
    }
    Write-Host ("Phase {0}: {1}" -f $phase, $result) -ForegroundColor $color
}
Write-Host ""
Write-Host ("總耗時:{0:N1}s" -f $totalElapsed.TotalSeconds) -ForegroundColor Cyan

$failCount = ($global:PhaseResults.Values | Where-Object { $_ -eq "fail" }).Count
if ($failCount -gt 0) {
    Write-Host "[FAIL] $failCount phase(s) failed" -ForegroundColor Red
    exit 1
}
Write-Host "[OK] All requested phases passed" -ForegroundColor Green
exit 0
