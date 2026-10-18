param (
    [ValidateSet("User", "Machine")]
    [string]$Scope = "User"
)

$ErrorActionPreference = "Stop"

$Repo = "raushan728/Downloader"
$Url = "https://github.com/$Repo/releases/download/latest/dwdr-windows.exe"
$ChecksumUrl = "https://github.com/$Repo/releases/download/latest/SHA256SUMS"

if ($Scope -eq "Machine") {
    $currentPrincipal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    if (-not $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        Write-Host "ERROR: System-wide installation (-Scope Machine) requires administrative privileges." -ForegroundColor Red
        Write-Host "Please re-run this command from an elevated PowerShell prompt (Run as Administrator)." -ForegroundColor Red
        exit 1
    }
    $BinDir = "$env:ProgramFiles\DWDR\bin"
} else {
    $BinDir = "$env:USERPROFILE\.dwdr\bin"
}
$BinPath = "$BinDir\dwdr.exe"
$TmpPath = "$BinPath.tmp.$PID"

Write-Host "INFO: Downloading DWDR for Windows (Scope: $Scope)..." -ForegroundColor Cyan

if (-not (Test-Path -Path $BinDir)) {
    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
}

Invoke-WebRequest -Uri $Url -OutFile $TmpPath

try {
    $SumsFile = "$TmpPath.sums"
    Invoke-WebRequest -Uri $ChecksumUrl -OutFile $SumsFile -ErrorAction SilentlyContinue
    if (Test-Path -Path $SumsFile) {
        $ExpectedLine = Get-Content -Path $SumsFile | Where-Object { $_ -match "dwdr-windows\.exe$" }
        if ($ExpectedLine) {
            $ExpectedHash = ($ExpectedLine -split '\s+')[0].Trim().ToLower()
            $ActualHash = (Get-FileHash -Path $TmpPath -Algorithm SHA256).Hash.ToLower()
            if ($ActualHash -ne $ExpectedHash) {
                Remove-Item -Path $TmpPath -Force -ErrorAction SilentlyContinue
                Remove-Item -Path $SumsFile -Force -ErrorAction SilentlyContinue
                throw "SHA256 checksum mismatch for dwdr-windows.exe. Download may be corrupted."
            }
            Write-Host "INFO: SHA256 checksum verified." -ForegroundColor Cyan
        }
        Remove-Item -Path $SumsFile -Force -ErrorAction SilentlyContinue
    }
} catch [System.Net.WebException] {
    # Checksum manifest unavailable; proceed
}

Move-Item -Path $TmpPath -Destination $BinPath -Force

$CurrentPath = [Environment]::GetEnvironmentVariable("PATH", $Scope)
if ($CurrentPath -notlike "*$BinDir*") {
    Write-Host "INFO: Adding $BinDir to your $Scope PATH..." -ForegroundColor Cyan
    $NewPath = if ([string]::IsNullOrEmpty($CurrentPath)) { $BinDir } else { "$CurrentPath;$BinDir" }
    [Environment]::SetEnvironmentVariable("PATH", $NewPath, $Scope)
}

Write-Host "SUCCESS: DWDR installed successfully! Restart your terminal to use it." -ForegroundColor Green
