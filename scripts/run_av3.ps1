# run_av3.ps1 — av3 spot-check wrapper(避免 Windows PowerShell UTF-8 亂碼)
#
# 用途:在 PowerShell 5.x / Windows Terminal 跑 psql 對 av3_spot_check.sql 時,
#       中文輸出會因為三層 encoding mismatch 顯示亂碼:
#       1. PS console codepage 預設 cp950(繁中 Windows)
#       2. PS .NET Console.OutputEncoding 預設不是 UTF-8
#       3. PGCLIENTENCODING env 沒設
#
# 本 script 一次設好三層,跑完還原(避免污染 user shell)。
#
# 用法:
#   .\scripts\run_av3.ps1
#
# 永久解(可選):把下面前 4 行 + $env:PGCLIENTENCODING 那行,加進 $PROFILE。
# =============================================================================

# 備份目前 codepage(用 chcp 取目前值,輸出 "Active code page: 950" 之類)
$origCodePage = (chcp) -replace '[^\d]', ''
$origOutEnc   = [Console]::OutputEncoding
$origInEnc    = [Console]::InputEncoding
$origPsEnc    = $OutputEncoding
$origPgEnc    = $env:PGCLIENTENCODING

try {
    # 切到 UTF-8(三層全包)
    chcp 65001 | Out-Null
    [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()
    [Console]::InputEncoding  = [System.Text.UTF8Encoding]::new()
    $OutputEncoding           = [System.Text.UTF8Encoding]::new()
    $env:PGCLIENTENCODING     = "UTF8"

    # 不用 psql -f,改用 PS Get-Content force UTF-8 讀檔 → pipe 給 psql stdin
    # 原因:psql -f 在 Windows 對 SQL 檔 byte 的 encoding handling 有怪事,
    # \echo 中文會亂碼(SELECT 結果 OK 因為走 PG server 那條路)。
    # PS 端 force -Encoding UTF8 讀檔保證 byte stream 正確。
    Get-Content -Raw -Encoding UTF8 scripts\av3_spot_check.sql | psql $env:DATABASE_URL
}
finally {
    # 還原(避免污染 user shell)
    if ($origCodePage) { chcp $origCodePage | Out-Null }
    [Console]::OutputEncoding = $origOutEnc
    [Console]::InputEncoding  = $origInEnc
    $OutputEncoding           = $origPsEnc
    if ($origPgEnc) {
        $env:PGCLIENTENCODING = $origPgEnc
    } else {
        Remove-Item env:PGCLIENTENCODING -ErrorAction SilentlyContinue
    }
}
