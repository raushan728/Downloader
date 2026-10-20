param (
[ValidateSet("User", "Machine", "Auto")]
[string]$Scope = "Auto"
)
$ErrorActionPreference = "Stop"
$Repo = "raushan728/Downloader"
$Url = "https://github.com/$Repo/releases/download/latest/dwdr-windows.exe"
$ChecksumUrl = "https://github.com/$Repo/releases/download/latest/SHA256SUMS"
$currentPrincipal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
$isAdmin = $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if ($Scope -eq "Auto") {
  if ($isAdmin) {
    # When running as Administrator interactively, ask user whether they prefer system-wide or user-only scope.
    $isInteractive = [Environment]::UserInteractive -and -not [Console]::IsInputRedirected
    if ($isInteractive) {
      try {
        Write-Host "INFO: Administrator privileges detected." -ForegroundColor Cyan
        Write-Host "Select installation scope:" -ForegroundColor Yellow
        Write-Host "  [1] All Users / System-wide (C:\Program Files\DWDR\bin) - Recommended"
        Write-Host "  [2] Current User only (~/.dwdr\bin)"
        $choice = Read-Host "Enter choice (1/2) [Default: 1]"
        if ($choice -and ($choice.Trim() -eq "2" -or $choice.Trim() -match '^[Uu]')) {
          $Scope = "User"
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
if ($Scope -eq "Machine") {
  if (-not $isAdmin) {
    Write-Host "ERROR: System-wide installation (-Scope Machine) requires administrative privileges." -ForegroundColor Red
    Write-Host "Solution: Right-click PowerShell and select 'Run as Administrator', then run:" -ForegroundColor Yellow
    Write-Host "  & ([scriptblock]::Create((irm https://raw.githubusercontent.com/raushan728/Downloader/master/scripts/install.ps1))) -Scope Machine" -ForegroundColor Cyan
    Write-Host "Tip: To install for your current user without Administrator privileges, run:" -ForegroundColor Yellow
    Write-Host "  irm https://raw.githubusercontent.com/raushan728/Downloader/master/scripts/install.ps1 | iex" -ForegroundColor Cyan
    return
  }
  $BinDir = "$env:ProgramFiles\DWDR\bin"
}
else {
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
    $ExpectedLine = Get-Content -Path $SumsFile | Where-Object {
      $_ -match "dwdr-windows\.exe$"
    }
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
}
catch [System.Net.WebException], [System.Net.Http.HttpRequestException] {
  # Checksum manifest unavailable;
  proceed
}
Move-Item -Path $TmpPath -Destination $BinPath -Force
$CurrentPath = [Environment]::GetEnvironmentVariable("PATH", $Scope)
if ($CurrentPath -notlike "*$BinDir*") {
  Write-Host "INFO: Adding $BinDir to your $Scope PATH..." -ForegroundColor Cyan
  $NewPath = if ([string]::IsNullOrEmpty($CurrentPath)) {
    $BinDir
  }
  else {
    "$CurrentPath;$BinDir"
  }
  [Environment]::SetEnvironmentVariable("PATH", $NewPath, $Scope)
}
Write-Host "SUCCESS: DWDR installed successfully! Restart your terminal to use it." -ForegroundColor Green
Write-Host ""
Write-Host "Quick Start Commands:" -ForegroundColor Cyan
Write-Host "  dwdr <URL>             - Download media directly with smart quality selection"
Write-Host "  dwdr                   - Open interactive shell (queue, live status, format picker)"
Write-Host "  dwdr config            - Configure download directory, default quality, and limits"
Write-Host "  dwdr update            - Update core yt-dlp & ffmpeg engines"
Write-Host "  dwdr self-update       - Update DWDR CLI tool itself"
Write-Host "  dwdr --help            - Display complete command reference"