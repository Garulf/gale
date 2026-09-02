# Windows packaging

Installs `galed` as a Windows service named `galed` ("Gale fan control").

## Prerequisites

- Windows 10 or later
- An elevated PowerShell prompt (Run as Administrator)

## Install

Extract the release zip, then from an elevated PowerShell in that folder:

```powershell
.\install.ps1
```

This copies `galed.exe` and `gale.exe` to `%ProgramFiles%\Gale`, seeds
`%ProgramData%\Gale\config.toml` from `config.example.toml` if no config
exists yet, registers the `galed` service with automatic restart on failure,
and starts it. Open `http://127.0.0.1:5250` for the dashboard.

## Uninstall

```powershell
.\install.ps1 -Uninstall
```

This stops the service, runs `galed restore` to release any fan overrides
back to hardware control, deletes the service, and removes
`%ProgramFiles%\Gale`. Config and logs under `%ProgramData%\Gale` are kept.

## Where things live

- Binaries: `%ProgramFiles%\Gale`
- Config: `%ProgramData%\Gale\config.toml`
- Runtime state: `%ProgramData%\Gale\run`

## Restoring fan control manually

If the service is stopped without going through `install.ps1 -Uninstall`,
release any manual overrides back to hardware control with:

```powershell
& "$env:ProgramFiles\Gale\galed.exe" restore
```
