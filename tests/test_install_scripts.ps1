# PowerShell Install & Uninstall Test Suite for Windows
$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir
$InstallScript = Join-Path (Join-Path $RepoRoot "scripts") "install.ps1"
$UninstallScript = Join-Path (Join-Path $RepoRoot "scripts") "uninstall.ps1"

Write-Host "=== Testing PowerShell Install & Uninstall Scripts ===" -ForegroundColor Cyan

# 1. AST Syntax Validation
Write-Host "--> Checking PowerShell Script Syntax (AST)..." -ForegroundColor Yellow
$tokens = $null
$errors = $null
$null = [System.Management.Automation.Language.Parser]::ParseFile($UninstallScript, [ref]$tokens, [ref]$errors)
if ($errors.Count -gt 0) {
    foreach ($err in $errors) {
        Write-Error "Syntax Error in uninstall.ps1: $($err.Message) at line $($err.Extent.StartLineNumber)"
    }
    exit 1
}

$null = [System.Management.Automation.Language.Parser]::ParseFile($InstallScript, [ref]$tokens, [ref]$errors)
if ($errors.Count -gt 0) {
    foreach ($err in $errors) {
        Write-Error "Syntax Error in install.ps1: $($err.Message) at line $($err.Extent.StartLineNumber)"
    }
    exit 1
}
Write-Host "PASS: Script syntax verified." -ForegroundColor Green

# 2. Prepare Local Mock Release Files
$TempStage = Join-Path ([System.IO.Path]::GetTempPath()) "dwdr_test_$(Get-Random)"
New-Item -ItemType Directory -Path $TempStage -Force | Out-Null
$MockExe = Join-Path $TempStage "dwdr-windows.exe"
Set-Content -Path $MockExe -Value "fake binary content"

$Sha = (Get-FileHash -Path $MockExe -Algorithm SHA256).Hash.ToLower()
$SumsFile = Join-Path $TempStage "SHA256SUMS"
Set-Content -Path $SumsFile -Value "$Sha  dwdr-windows.exe"

# Start local HTTP server using Python if available, otherwise fall back to local file path
$ServerProcess = $null
$Port = Get-Random -Minimum 18000 -Maximum 19000
try {
    $PythonCmd = if (Get-Command "python" -ErrorAction SilentlyContinue) { "python" } elseif (Get-Command "python3" -ErrorAction SilentlyContinue) { "python3" } else { $null }
    if ($PythonCmd) {
        $ServerProcess = Start-Process -FilePath $PythonCmd -ArgumentList "-m", "http.server", "$Port", "--directory", "$TempStage" -PassThru -WindowStyle Hidden
        Start-Sleep -Seconds 2
        $HttpBase = "http://127.0.0.1:$Port"
        $env:DWDR_INSTALL_URL = "$HttpBase/dwdr-windows.exe"
        $env:DWDR_CHECKSUM_URL = "$HttpBase/SHA256SUMS"
        Write-Host "INFO: Mock release server listening at $HttpBase" -ForegroundColor Cyan
    }
}
catch {
    Write-Warning "Could not start local python HTTP server: $_"
}

if (-not $env:DWDR_INSTALL_URL) {
    $env:DWDR_INSTALL_URL = (New-Object System.Uri($MockExe)).AbsoluteUri
    $env:DWDR_CHECKSUM_URL = (New-Object System.Uri($SumsFile)).AbsoluteUri
}

try {
    # 3. Test Scope User Lifecycle
    Write-Host "--> Testing -Scope User Lifecycle..." -ForegroundColor Yellow
    & $InstallScript -Scope User
    $UserBin = "$env:USERPROFILE\.dwdr\bin\dwdr.exe"
    if (-not (Test-Path -Path $UserBin)) {
        throw "FAIL: User binary was not created at $UserBin"
    }
    if ([System.Environment]::OSVersion.Platform -match "Win") {
        $UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
        if ($UserPath -notlike "*$env:USERPROFILE\.dwdr\bin*") {
            throw "FAIL: User PATH does not contain .dwdr\bin"
        }
    }

    & $UninstallScript -Scope User
    if (Test-Path -Path "$env:USERPROFILE\.dwdr") {
        throw "FAIL: User directory $env:USERPROFILE\.dwdr still exists after uninstall"
    }
    if ([System.Environment]::OSVersion.Platform -match "Win") {
        $CleanUserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
        if ($CleanUserPath -like "*$env:USERPROFILE\.dwdr\bin*") {
            throw "FAIL: User PATH still contains .dwdr\bin after uninstall"
        }
    }
    Write-Host "PASS: Scope User lifecycle verified." -ForegroundColor Green

    # 4. Test Scope Machine Lifecycle (if running elevated)
    $isAdmin = if ([System.Environment]::OSVersion.Platform -match "Win") {
        (New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    } else {
        $false
    }
    if ($isAdmin) {
        Write-Host "--> Testing -Scope Machine Lifecycle (Admin)..." -ForegroundColor Yellow
        & $InstallScript -Scope Machine
        $MachineBin = "$env:ProgramFiles\DWDR\bin\dwdr.exe"
        if (-not (Test-Path -Path $MachineBin)) {
            throw "FAIL: Machine binary was not created at $MachineBin"
        }
        $MachinePath = [Environment]::GetEnvironmentVariable("PATH", "Machine")
        if ($MachinePath -notlike "*$env:ProgramFiles\DWDR\bin*") {
            throw "FAIL: Machine PATH does not contain DWDR\bin"
        }

        & $UninstallScript -Scope Machine
        if (Test-Path -Path "$env:ProgramFiles\DWDR") {
            throw "FAIL: Machine directory still exists after uninstall"
        }
        $CleanMachinePath = [Environment]::GetEnvironmentVariable("PATH", "Machine")
        if ($CleanMachinePath -like "*$env:ProgramFiles\DWDR\bin*") {
            throw "FAIL: Machine PATH still contains DWDR\bin after uninstall"
        }
        Write-Host "PASS: Scope Machine lifecycle verified." -ForegroundColor Green

        # 5. Test Scope Both Lifecycle
        Write-Host "--> Testing -Scope Both Lifecycle..." -ForegroundColor Yellow
        & $InstallScript -Scope User
        & $InstallScript -Scope Machine
        & $UninstallScript -Scope Both
        if (Test-Path -Path "$env:USERPROFILE\.dwdr") {
            throw "FAIL: User directory still exists after -Scope Both"
        }
        if (Test-Path -Path "$env:ProgramFiles\DWDR") {
            throw "FAIL: Machine directory still exists after -Scope Both"
        }
        Write-Host "PASS: Scope Both lifecycle verified." -ForegroundColor Green
    } else {
        Write-Host "SKIP: Skipping Machine scope tests (not running as Administrator)." -ForegroundColor Cyan
    }

    # 6. Test Idempotency (Uninstall when not installed)
    Write-Host "--> Testing Idempotency (Uninstall when not installed)..." -ForegroundColor Yellow
    & $UninstallScript -Scope User
    if ($isAdmin) {
        & $UninstallScript -Scope Machine
        & $UninstallScript -Scope Both
    }
    Write-Host "PASS: Idempotent uninstallation verified." -ForegroundColor Green
}
finally {
    if ($ServerProcess -and -not $ServerProcess.HasExited) {
        Stop-Process -Id $ServerProcess.Id -Force -ErrorAction SilentlyContinue
    }
    Remove-Item -Path $TempStage -Recurse -Force -ErrorAction SilentlyContinue
    $env:DWDR_INSTALL_URL = $null
    $env:DWDR_CHECKSUM_URL = $null
}

Write-Host "=== All Windows script tests completed successfully! ===" -ForegroundColor Green
