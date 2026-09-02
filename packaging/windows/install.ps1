param([switch]$Uninstall)

$ErrorActionPreference = "Stop"

$BinDir = Join-Path $env:ProgramFiles "Gale"
$DataDir = Join-Path $env:ProgramData "Gale"

function Assert-Admin {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    $isAdmin = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    if (-not $isAdmin) {
        throw "Run from an elevated PowerShell"
    }
}

function Stop-GaleService {
    if (Get-Service galed -ErrorAction SilentlyContinue) {
        sc.exe stop galed | Out-Null
        Start-Sleep 2
    }
}

function Install-Gale {
    New-Item -ItemType Directory -Force $BinDir, (Join-Path $DataDir "run") | Out-Null

    Copy-Item (Join-Path $PSScriptRoot "galed.exe"), (Join-Path $PSScriptRoot "gale.exe") $BinDir -Force

    $configPath = Join-Path $DataDir "config.toml"
    if (-not (Test-Path $configPath)) {
        Copy-Item (Join-Path $PSScriptRoot "config.example.toml") $configPath
    }

    if (Get-Service galed -ErrorAction SilentlyContinue) {
        Stop-GaleService
    }
    else {
        New-Service -Name galed -DisplayName "Gale fan control" -BinaryPathName "`"$BinDir\galed.exe`" --service" -StartupType Automatic | Out-Null
    }

    sc.exe failure galed reset= 60 actions= restart/5000 | Out-Null
    Start-Service galed

    Write-Host "galed installed. UI: http://127.0.0.1:5250"
}

function Uninstall-Gale {
    Stop-GaleService

    $galedExe = Join-Path $BinDir "galed.exe"
    if (Test-Path $galedExe) {
        & $galedExe restore
    }

    if (Get-Service galed -ErrorAction SilentlyContinue) {
        sc.exe delete galed | Out-Null
    }

    if (Test-Path $BinDir) {
        Remove-Item $BinDir -Recurse -Force
    }

    Write-Host "galed uninstalled. Config and logs kept under $DataDir"
}

Assert-Admin

if ($Uninstall) {
    Uninstall-Gale
}
else {
    Install-Gale
}
