<#
.SYNOPSIS
    註冊 Windows Task Scheduler 排程,每日跑 refresh_daily.ps1。

.DESCRIPTION
    對齊 user「沒 cron / 想自動拉最新資料」場景。內部:
      - 註冊一個 daily trigger 排程
      - Action 走 scripts/refresh_daily.ps1
      - 預設每天 18:00 跑(對齊 chip_cores §2.3 batch 17:30 + 30 分緩衝)

.PARAMETER At
    每日觸發時間(HH:mm)。預設 18:00。
    台股收盤 13:30,法人籌碼 ~17:30 公布;18:00 跑可拿到當日完整資料。

.PARAMETER TaskName
    排程名稱。預設 "StockHelper4me-Refresh"。

.PARAMETER NoBronze
    註冊 `refresh --skip-bronze` 變種(Bronze 已由其他流程跑)。

.PARAMETER NoCores
    註冊 `refresh --skip-cores` 變種(沒 Rust binary)。

.PARAMETER Stocks
    限縮股票範圍(逗號分隔)。預設 = 全市場。

.EXAMPLE
    # 預設:每天 18:00 全 chain
    .\scripts\install_refresh_task.ps1

.EXAMPLE
    # 改成 19:30 跑
    .\scripts\install_refresh_task.ps1 -At 19:30

.EXAMPLE
    # 改排程名稱
    .\scripts\install_refresh_task.ps1 -TaskName "tw-stock-daily"

.EXAMPLE
    # 只跑 Bronze + Silver(無 Rust binary)
    .\scripts\install_refresh_task.ps1 -NoCores

.NOTES
    註冊後可:
      Get-ScheduledTask -TaskName "StockHelper4me-Refresh"
      Start-ScheduledTask -TaskName "StockHelper4me-Refresh"
      Unregister-ScheduledTask -TaskName "StockHelper4me-Refresh" -Confirm:$false
#>

param(
    [string]$At = '18:00',
    [string]$TaskName = 'StockHelper4me-Refresh',
    [switch]$NoBronze,
    [switch]$NoCores,
    [string]$Stocks = ''
)

$ErrorActionPreference = 'Stop'

# Resolve absolute paths
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$WrapperScript = Join-Path $ProjectRoot 'scripts\refresh_daily.ps1'

if (-not (Test-Path $WrapperScript)) {
    throw "Wrapper script 不存在: $WrapperScript`n  確認 git pull + 在專案 root 跑此 installer"
}

# Build wrapper -RefreshArgs(若 user 有 skip / stocks)
$refreshArgsList = @()
if ($NoBronze) { $refreshArgsList += '--skip-bronze' }
if ($NoCores)  { $refreshArgsList += '--skip-cores' }
if ($Stocks)   { $refreshArgsList += '--stocks'; $refreshArgsList += $Stocks }

# Pass list 給 wrapper(PowerShell args 用單引號 + 空格分隔)
$wrapperArg = if ($refreshArgsList.Count -gt 0) {
    "-NoProfile -ExecutionPolicy Bypass -File `"$WrapperScript`" -RefreshArgs " `
        + (($refreshArgsList | ForEach-Object { "'$_'" }) -join ',')
} else {
    "-NoProfile -ExecutionPolicy Bypass -File `"$WrapperScript`""
}

$Action = New-ScheduledTaskAction `
    -Execute 'powershell.exe' `
    -Argument $wrapperArg

# Trigger: 每日 $At
$Trigger = New-ScheduledTaskTrigger -Daily -At $At

# Settings: 漏跑會補(StartWhenAvailable);電池 OK;最多跑 2 小時;失敗 2 次重試(間隔 15 分)
$Settings = New-ScheduledTaskSettingsSet `
    -StartWhenAvailable `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit (New-TimeSpan -Hours 2) `
    -RestartCount 2 `
    -RestartInterval (New-TimeSpan -Minutes 15)

# Principal: 走當前 user,limited(不需 admin)
$Principal = New-ScheduledTaskPrincipal `
    -UserId $env:USERNAME `
    -LogonType Interactive `
    -RunLevel Limited

# 移除既有(idempotent reinstall)
$existing = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if ($existing) {
    Write-Host "找到既有任務 '$TaskName',先移除..."
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
}

# Register
$desc = "Daily refresh of tw-stock-collector data pipeline (Bronze → Silver → M3 cores)"
if ($refreshArgsList.Count -gt 0) { $desc += " [args: $($refreshArgsList -join ' ')]" }

Register-ScheduledTask `
    -TaskName $TaskName `
    -Action $Action `
    -Trigger $Trigger `
    -Settings $Settings `
    -Principal $Principal `
    -Description $desc | Out-Null

Write-Host ''
Write-Host "OK 排程 '$TaskName' 已註冊" -ForegroundColor Green
Write-Host "  每日觸發時間: $At"
Write-Host "  Wrapper: $WrapperScript"
if ($refreshArgsList.Count -gt 0) {
    Write-Host "  Python args: refresh $($refreshArgsList -join ' ')"
}
Write-Host "  Log 寫入: $ProjectRoot\logs\refresh_YYYY-MM-DD.log"
Write-Host ''
Write-Host "驗證命令:"
Write-Host "  Get-ScheduledTask -TaskName '$TaskName'"
Write-Host "  Start-ScheduledTask -TaskName '$TaskName'    # 立即手動觸發(測試用)"
Write-Host "  Unregister-ScheduledTask -TaskName '$TaskName' -Confirm:`$false  # 移除"
