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
    $service = Get-Service galed -ErrorAction SilentlyContinue
    if (-not $service) {
        return
    }
    sc.exe stop galed | Out-Null
    $deadline = (Get-Date).AddSeconds(30)
    while ((Get-Service galed).Status -ne "Stopped") {
        if ((Get-Date) -gt $deadline) {
            throw "Timed out waiting for the galed service to stop"
        }
        Start-Sleep -Milliseconds 500
    }
}

function Install-Gale {
    New-Item -ItemType Directory -Force $BinDir, (Join-Path $DataDir "run") | Out-Null

    $binPath = "`"$BinDir\galed.exe`" --service"
    $serviceExists = [bool](Get-Service galed -ErrorAction SilentlyContinue)
    if ($serviceExists) {
        Stop-GaleService
    }

    Copy-Item (Join-Path $PSScriptRoot "galed.exe"), (Join-Path $PSScriptRoot "gale.exe") $BinDir -Force

    $configPath = Join-Path $DataDir "config.toml"
    if (-not (Test-Path $configPath)) {
        Copy-Item (Join-Path $PSScriptRoot "config.example.toml") $configPath
    }

    if ($serviceExists) {
        sc.exe config galed start= auto binPath= $binPath | Out-Null
    }
    else {
        New-Service -Name galed -DisplayName "Gale fan control" -BinaryPathName $binPath -StartupType Automatic | Out-Null
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
