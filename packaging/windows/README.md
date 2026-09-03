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

This copies `galed.exe`, `gale.exe` and `gale-tray.exe` to
`%ProgramFiles%\Gale`, seeds `%ProgramData%\Gale\config.toml` from
`config.example.toml` if no config exists yet, registers the `galed` service
with automatic restart on failure, starts it, and registers and starts the
tray icon for the current user. Open `http://127.0.0.1:5250` for the
dashboard.

## Uninstall

```powershell
.\install.ps1 -Uninstall
```

This stops the tray icon and removes its autostart entry, stops the
service, runs `galed restore` to release any fan overrides back to hardware
control, deletes the service, and removes `%ProgramFiles%\Gale`. Config
under `%ProgramData%\Gale` is kept.

## Where things live

- Binaries: `%ProgramFiles%\Gale`
- Config: `%ProgramData%\Gale\config.toml`
- Runtime state: `%ProgramData%\Gale\run`

## Logs

`galed` writes its log to stderr through `tracing`. Running under the
service control manager gives it no console, so the `galed` service writes
no log file today; a log file is a planned follow-up. To see the log, stop
the service and run the binary directly from an elevated terminal:

```powershell
& "$env:ProgramFiles\Gale\galed.exe"
```

Set `RUST_LOG` for more detail, for example `$env:RUST_LOG = "debug"` before
running it.

## Tray

`install.ps1` also registers `gale-tray.exe` to start with your Windows
session (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run\GaleTray`) and
starts it immediately after installing. Because `install.ps1` runs elevated,
this Run entry lives in the HKCU hive of the account that ran the elevated
installer, not necessarily the account signed in afterward. The tray icon
shows the `galed` service status and offers:

- Open UI, which launches `http://127.0.0.1:5250` in your browser
- Start and Stop, which control the `galed` service
- Quit, which removes the icon without touching the service

Start and Stop run without elevation when they can, and fall back to a UAC
prompt when the current session cannot control the service directly. The
tray assumes the default bind address `127.0.0.1:5250`. A custom `api.bind`
in `config.toml` is not yet reflected in the Open UI menu item; that is a
planned follow-up.

`install.ps1` launches `gale-tray.exe` through `explorer.exe` rather than
directly, because a process started by explorer inherits the interactive
user's unelevated token instead of the elevated installer's.

To stop the tray from starting automatically, delete the `GaleTray` value
under `HKCU:\Software\Microsoft\Windows\CurrentVersion\Run`, or run
`.\install.ps1 -Uninstall` and reinstall.

## Motherboard fans

Motherboard fan control on Windows goes through PawnIO, a separate driver
that gives Gale access to the Super I/O chip. Install it from
https://pawnio.eu before or after installing Gale, then restart the `galed`
service. The banner on the dashboard disappears once the NCT67xx chip is
found. If the board has no Nuvoton Super I/O chip, the daemon logs it and no
banner is shown.

Setting `GALE_PAWNIOLIB` to a path overrides where `galed` loads
`PawnIOLib.dll` from, but PawnIO still has to be installed (its registry
uninstall key present) for that library to be used at all.

## Restoring fan control manually

If the service is stopped without going through `install.ps1 -Uninstall`,
release any manual overrides back to hardware control with:

```powershell
& "$env:ProgramFiles\Gale\galed.exe" restore
```
