$ErrorActionPreference = "Stop"

$DwdrDir = "$env:USERPROFILE\.dwdr"
$BinDir = "$DwdrDir\bin"

Write-Host "INFO: Starting DWDR uninstallation..." -ForegroundColor Cyan

if (Test-Path -Path $DwdrDir) {
    Write-Host "INFO: Removing DWDR directory at $DwdrDir..." -ForegroundColor Cyan
    Remove-Item -Path $DwdrDir -Recurse -Force
}

$CurrentPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($CurrentPath -like "*$BinDir*") {
    Write-Host "INFO: Removing $BinDir from your PATH..." -ForegroundColor Cyan
    $Parts = $CurrentPath -split ';' | Where-Object { $_ -ne $BinDir -and $_ -ne "" }
    $NewPath = $Parts -join ';'
    [Environment]::SetEnvironmentVariable("PATH", $NewPath, "User")
}

Write-Host "SUCCESS: DWDR has been completely uninstalled." -ForegroundColor Green
