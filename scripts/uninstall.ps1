param (
[ValidateSet("User", "Machine", "Both", "Auto")]
[string]$Scope = "Auto"
)
$ErrorActionPreference = "Stop"
$currentPrincipal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
$isAdmin = $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($Scope -eq "Auto") {
  if ($isAdmin) {
    $hasMachine = Test-Path -Path "$env:ProgramFiles\DWDR"
    $hasUser = Test-Path -Path "$env:USERPROFILE\.dwdr"
    $isInteractive = [Environment]::UserInteractive -and -not [Console]::IsInputRedirected
    if ($isInteractive -and ($hasMachine -or $hasUser)) {
      try {
        Write-Host "INFO: Administrator privileges detected." -ForegroundColor Cyan
        Write-Host "Select uninstallation scope:" -ForegroundColor Yellow
        Write-Host "  [1] All Users / System-wide (C:\Program Files\DWDR) - Recommended"
        Write-Host "  [2] Current User only (~/.dwdr)"
        Write-Host "  [3] Both (clean everything)"
        $choice = Read-Host "Enter choice (1/2/3) [Default: 1]"
        if ($choice -and $choice.Trim() -eq "2") {
          $Scope = "User"
        }
        elseif ($choice -and $choice.Trim() -eq "3") {
          $Scope = "Both"
        }
        else {
          $Scope = "Machine"
        }
      }
      catch {
        $Scope = "Machine"
      }
    }
    else {
      $Scope = "Machine"
    }
  }
  else {
    $Scope = "User"
  }
}
if ($Scope -in @("Machine", "Both") -and -not $isAdmin) {
  Write-Host "ERROR: System-wide uninstallation (-Scope $Scope) requires administrative privileges." -ForegroundColor Red
  Write-Host "Solution: Right-click PowerShell and select 'Run as Administrator', then run:" -ForegroundColor Yellow
  Write-Host "  & ([scriptblock]::Create((irm https://raw.githubusercontent.com/raushan728/Downloader/master/scripts/uninstall.ps1))) -Scope $Scope" -ForegroundColor Cyan
  Write-Host "Tip: To uninstall for your current user without Administrator privileges, run:" -ForegroundColor Yellow
  Write-Host "  irm https://raw.githubusercontent.com/raushan728/Downloader/master/scripts/uninstall.ps1 | iex" -ForegroundColor Cyan
  return
}
$DwdrDir = "$env:USERPROFILE\.dwdr"
$MachineDir = "$env:ProgramFiles\DWDR"
Write-Host "INFO: Starting DWDR uninstallation (Scope: $Scope)..." -ForegroundColor Cyan
# Check if any DWDR processes or child engines are currently executing
$RunningProcesses = Get-Process -Name "dwdr", "yt-dlp", "ffmpeg" -ErrorAction SilentlyContinue
if ($RunningProcesses) {
  $Names = ($RunningProcesses | Select-Object -ExpandProperty ProcessName -Unique) -join ', '
  Write-Warning "Active processes detected: $Names. Please close all running instances before uninstalling."
}
function Remove-PathEntry {
  param(
  [string]$Directory,
  [string]$TargetScope
  )
  $CurrentPath = [Environment]::GetEnvironmentVariable("PATH", $TargetScope)
  if ($CurrentPath -like "*$Directory*") {
    Write-Host "INFO: Removing $Directory from your $TargetScope PATH..." -ForegroundColor Cyan
    $Parts = $CurrentPath -split ';' | Where-Object {
      $_ -ne $Directory -and $_ -ne ""
    }
    $NewPath = $Parts -join ';'
    [Environment]::SetEnvironmentVariable("PATH", $NewPath, $TargetScope)
  }
}
if ($Scope -in @("User", "Both")) {
  if (Test-Path -Path $DwdrDir) {
    Write-Host "INFO: Removing DWDR user directory at $DwdrDir..." -ForegroundColor Cyan
    try {
      Remove-Item -Path $DwdrDir -Recurse -Force
    }
    catch {
      Write-Warning "Could not remove some files in $DwdrDir. A file may still be in use by another process: $_"
    }
  }
  Remove-PathEntry -Directory "$DwdrDir\bin" -TargetScope "User"
}
if ($Scope -in @("Machine", "Both")) {
  if (Test-Path -Path $MachineDir) {
    Write-Host "INFO: Removing DWDR system directory at $MachineDir..." -ForegroundColor Cyan
    try {
      Remove-Item -Path $MachineDir -Recurse -Force
    }
    catch {
      Write-Warning "Could not remove $MachineDir: $_"
    }
  }
  Remove-PathEntry -Directory "$MachineDir\bin" -TargetScope "Machine"
}
# Clean any .old backup binaries left outside ~/.dwdr
$PossibleOldPaths = @(
"$env:USERPROFILE\.cargo\bin\dwdr.old",
"$env:USERPROFILE\.cargo\bin\dwdr.exe.old",
"$DwdrDir\bin\dwdr.old",
"$DwdrDir\bin\dwdr.exe.old",
"$MachineDir\bin\dwdr.old",
"$MachineDir\bin\dwdr.exe.old"
)
foreach ($OldPath in $PossibleOldPaths) {
  if (Test-Path -Path $OldPath) {
    Remove-Item -Path $OldPath -Force -ErrorAction SilentlyContinue
  }
}
Write-Host "SUCCESS: DWDR has been completely uninstalled." -ForegroundColor Green