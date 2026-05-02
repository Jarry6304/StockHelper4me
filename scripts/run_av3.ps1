# run_av3.ps1 — av3 spot-check wrapper(完整修 Windows PowerShell UTF-8 亂碼)
#
# Root cause(byte-level diagnostic 證實):
#   * psql.exe 寫 stdout / `-o file` 是純 UTF-8 byte(不 transcode)
#   * 但 PowerShell 5.x 對 native command stdout 經 pipe / 直接 console 的
#     encoding handling 有 quirk,大 byte stream 偶爾壞掉導致中文 mojibake
#
# 修法:psql 直接 `-o $tempFile` 寫 file(byte 不經 PS pipe)→ PS 用
#       Get-Content -Encoding UTF8 讀回 → 顯示到 console。完全繞過 PS native
#       command pipe encoding handling。
#
# 用法:
#   .\scripts\run_av3.ps1
# =============================================================================

$tempFile     = Join-Path $env:TEMP "av3_$(Get-Date -Format yyyyMMddHHmmss).txt"
$origCodePage = (chcp) -replace '[^\d]', ''
$origOutEnc   = [Console]::OutputEncoding
$origInEnc    = [Console]::InputEncoding
$origPsEnc    = $OutputEncoding
$origPgEnc    = $env:PGCLIENTENCODING

try {
    # 設好 console UTF-8(影響 Get-Content → console 顯示那段)
    chcp 65001 | Out-Null
    [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()
    [Console]::InputEncoding  = [System.Text.UTF8Encoding]::new()
    $OutputEncoding           = [System.Text.UTF8Encoding]::new()
    $env:PGCLIENTENCODING     = "UTF8"

    # psql 直接寫 file,byte 完全不經 PS pipe(byte-level 已驗證 file 是 UTF-8)
    psql $env:DATABASE_URL -f scripts\av3_spot_check.sql -o $tempFile

    # PS 用 -Encoding UTF8 讀 file(file 內是 UTF-8 byte)→ string → console
    if (Test-Path $tempFile) {
        Get-Content $tempFile -Encoding UTF8
    } else {
        Write-Warning "psql 沒產生 output file: $tempFile"
    }
}
finally {
    if ($tempFile -and (Test-Path $tempFile)) {
        Remove-Item $tempFile -ErrorAction SilentlyContinue
    }
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
