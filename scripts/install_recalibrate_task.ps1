<#
.SYNOPSIS
    註冊 Windows Task Scheduler 排程,每週跑 recalibrate_kalman.ps1(Phase 3b Kalman 全市場校準)。

.DESCRIPTION
    讓 resonance track2 的 kalman_cqr band 週期性刷新。**預設註冊「週增量」**(recalibrate_kalman.ps1
    -Incremental,只刷最近 LookbackDays 窗,~2h)—— 2022 全量 seed 是一次性手動(~32h,不走排程)。
    預設每週日 02:00(離峰)。Action 走 scripts/recalibrate_kalman.ps1。

.PARAMETER At
    觸發時間(HH:mm)。預設 02:00。

.PARAMETER DayOfWeek
    觸發星期。預設 Sunday。

.PARAMETER TaskName
    排程名稱。預設 "StockHelper4me-RecalibrateKalman"。

.PARAMETER LookbackDays
    週增量回看窗(天)。預設 45(夠 cover 一週新交易日 + conformalize settle buffer)。

.PARAMETER FullSeed
    改註冊「全量 seed」(--since,~32h)而非增量。罕用(seed 通常手動跑一次即可)。

.PARAMETER Since
    僅 -FullSeed 時生效的校準起日。預設 2022-01-01。

.PARAMETER Stocks
    限縮股票(逗號分隔)。預設 = 全市場。

.PARAMETER SkipMaterialize
    跳過 Step5(golden fusion --only resonance 重物化)。

.EXAMPLE
    .\scripts\install_recalibrate_task.ps1                       # 每週日 02:00 全市場「週增量」(~2h)
.EXAMPLE
    .\scripts\install_recalibrate_task.ps1 -DayOfWeek Saturday -At 03:30 -LookbackDays 60
.EXAMPLE
    .\scripts\install_recalibrate_task.ps1 -FullSeed             # 罕用:排程跑全量(~32h,需配大 ExecutionTimeLimit)

.NOTES
    Get-ScheduledTask -TaskName "StockHelper4me-RecalibrateKalman"
    Start-ScheduledTask -TaskName "StockHelper4me-RecalibrateKalman"
    Unregister-ScheduledTask -TaskName "StockHelper4me-RecalibrateKalman" -Confirm:$false
#>

param(
    [string]$At = '02:00',
    [ValidateSet('Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday')]
    [string]$DayOfWeek = 'Sunday',
    [string]$TaskName = 'StockHelper4me-RecalibrateKalman',
    [int]$LookbackDays = 45,
    [switch]$FullSeed,
    [string]$Since = '2022-01-01',
    [string]$Stocks = '',
    [switch]$SkipMaterialize
)

$ErrorActionPreference = 'Stop'

$ProjectRoot = Split-Path -Parent $PSScriptRoot
$WrapperScript = Join-Path $ProjectRoot 'scripts\recalibrate_kalman.ps1'
if (-not (Test-Path $WrapperScript)) {
    throw "Wrapper script 不存在: $WrapperScript`n  確認 git pull + 在專案 root 跑此 installer"
}

# Build wrapper args:週排程預設走 -Incremental(只刷最近 LookbackDays 窗,~2h;
# 全量 seed 是一次性手動,不該每週重跑 32h)。-FullSeed 才註冊全量(罕用)。
$wargs = @()
if ($FullSeed) {
    $wargs += "-Since `"$Since`""
} else {
    $wargs += '-Incremental'
    $wargs += "-LookbackDays $LookbackDays"
}
if ($Stocks) { $wargs += "-Stocks `"$Stocks`"" }
if ($SkipMaterialize) { $wargs += '-SkipMaterialize' }
$wrapperArg = "-NoProfile -ExecutionPolicy Bypass -File `"$WrapperScript`" " + ($wargs -join ' ')

$Action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument $wrapperArg

# Trigger: 每週 $DayOfWeek $At
$Trigger = New-ScheduledTaskTrigger -Weekly -DaysOfWeek $DayOfWeek -At $At

# Settings: 漏跑補(StartWhenAvailable);電池 OK;最多 6 小時(增量 ~2h + buffer;
# 全量 seed ~32h 是手動跑,不走排程);失敗重試 1 次(間隔 30 分)
$Settings = New-ScheduledTaskSettingsSet `
    -StartWhenAvailable `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit (New-TimeSpan -Hours 6) `
    -RestartCount 1 `
    -RestartInterval (New-TimeSpan -Minutes 30)

# Principal: 當前 user,limited(不需 admin)
$Principal = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive -RunLevel Limited

# 移除既有(idempotent reinstall)
$existing = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if ($existing) {
    Write-Host "找到既有任務 '$TaskName',先移除..."
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
}

$mode = if ($FullSeed) { "full-seed --since $Since" } else { "incremental --LookbackDays $LookbackDays" }
$desc = "Weekly Phase 3b Kalman recalibration ($mode) — refresh resonance track2 kalman_cqr band"
if ($SkipMaterialize) { $desc += " [skip-materialize]" }
if ($Stocks) { $desc += " [stocks: $Stocks]" }

Register-ScheduledTask `
    -TaskName $TaskName `
    -Action $Action `
    -Trigger $Trigger `
    -Settings $Settings `
    -Principal $Principal `
    -Description $desc | Out-Null

Write-Host ''
Write-Host "OK 排程 '$TaskName' 已註冊" -ForegroundColor Green
Write-Host "  每週觸發: $DayOfWeek $At"
Write-Host "  模式: $mode$(if ($Stocks) { " / stocks=$Stocks" })$(if ($SkipMaterialize) { ' / skip-materialize' })"
Write-Host "  Wrapper: $WrapperScript $($wargs -join ' ')"
Write-Host "  Log 寫入: $ProjectRoot\logs\recalibrate_kalman_YYYY-MM-DD.log"
Write-Host ''
Write-Host "⚠ 一次性全量 seed(校準歷史首建,~32h)請【手動】跑一次:.\scripts\recalibrate_kalman.ps1"
Write-Host "  此排程是【週增量】(-Incremental,~2h),不會重跑全量 seed。"
Write-Host ''
Write-Host "驗證命令:"
Write-Host "  Get-ScheduledTask -TaskName '$TaskName'"
Write-Host "  Start-ScheduledTask -TaskName '$TaskName'    # 立即手動觸發(增量 ~2h)"
Write-Host "  Unregister-ScheduledTask -TaskName '$TaskName' -Confirm:`$false  # 移除"
