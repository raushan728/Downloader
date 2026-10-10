$ErrorActionPreference = "Stop"

$Repo = "raushan728/Downloader"
$Url = "https://github.com/$Repo/releases/download/latest/dwdr-windows.exe"
$BinDir = "$env:USERPROFILE\.dwdr\bin"
$BinPath = "$BinDir\dwdr.exe"

Write-Host "INFO: Downloading DWDR for Windows..." -ForegroundColor Cyan

if (-not (Test-Path -Path $BinDir)) {
    New-Item -ItemType Directory -Path $BinDir | Out-Null
}

Invoke-WebRequest -Uri $Url -OutFile $BinPath

$CurrentPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($CurrentPath -notlike "*$BinDir*") {
    Write-Host "INFO: Adding $BinDir to your PATH..." -ForegroundColor Cyan
    $NewPath = "$CurrentPath;$BinDir"
    [Environment]::SetEnvironmentVariable("PATH", $NewPath, "User")
}

Write-Host "SUCCESS: DWDR installed successfully! Restart your terminal to use it." -ForegroundColor Green
