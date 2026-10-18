param (
    [ValidateSet("User", "Machine")]
    [string]$Scope = "User"
)

$ErrorActionPreference = "Stop"

$DwdrDir = "$env:USERPROFILE\.dwdr"
if ($Scope -eq "Machine") {
    $currentPrincipal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    if (-not $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
        Write-Host "ERROR: System-wide uninstallation (-Scope Machine) requires administrative privileges." -ForegroundColor Red
        Write-Host "Please re-run this command from an elevated PowerShell prompt (Run as Administrator)." -ForegroundColor Red
        exit 1
    }
    $BinDir = "$env:ProgramFiles\DWDR\bin"
} else {
    $BinDir = "$DwdrDir\bin"
}

Write-Host "INFO: Starting DWDR uninstallation..." -ForegroundColor Cyan

# Check if any DWDR processes or child engines are currently executing
$RunningProcesses = Get-Process -Name "dwdr", "yt-dlp", "ffmpeg" -ErrorAction SilentlyContinue
if ($RunningProcesses) {
    $Names = ($RunningProcesses | Select-Object -ExpandProperty ProcessName -Unique) -join ', '
    Write-Warning "Active processes detected: $Names. Please close all running instances before uninstalling."
}

if (Test-Path -Path $DwdrDir) {
    Write-Host "INFO: Removing DWDR directory at $DwdrDir..." -ForegroundColor Cyan
    try {
        Remove-Item -Path $DwdrDir -Recurse -Force
    } catch {
        Write-Warning "Could not remove some files in $DwdrDir. A file may still be in use by another process: $_"
    }
}

if ($Scope -eq "Machine" -and (Test-Path -Path "$env:ProgramFiles\DWDR")) {
    try {
        Remove-Item -Path "$env:ProgramFiles\DWDR" -Recurse -Force
    } catch {
        Write-Warning "Could not remove $env:ProgramFiles\DWDR: $_"
    }
}

# Clean any .old backup binaries left outside ~/.dwdr
$PossibleOldPaths = @(
    "$env:USERPROFILE\.cargo\bin\dwdr.old",
    "$env:USERPROFILE\.cargo\bin\dwdr.exe.old",
    "$BinDir\dwdr.old",
    "$BinDir\dwdr.exe.old"
)
foreach ($OldPath in $PossibleOldPaths) {
    if (Test-Path -Path $OldPath) {
        Remove-Item -Path $OldPath -Force -ErrorAction SilentlyContinue
    }
}

$CurrentPath = [Environment]::GetEnvironmentVariable("PATH", $Scope)
if ($CurrentPath -like "*$BinDir*") {
    Write-Host "INFO: Removing $BinDir from your $Scope PATH..." -ForegroundColor Cyan
    $Parts = $CurrentPath -split ';' | Where-Object { $_ -ne $BinDir -and $_ -ne "" }
    $NewPath = $Parts -join ';'
    [Environment]::SetEnvironmentVariable("PATH", $NewPath, $Scope)
}

Write-Host "SUCCESS: DWDR has been completely uninstalled." -ForegroundColor Green
