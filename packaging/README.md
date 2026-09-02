# Packaging

Manual install steps for running `galed` as a systemd service on Linux.

## Install

```
sudo packaging/install.sh
```

This builds the release binaries, installs `galed` and `gale` to `/usr/local/bin`,
installs the systemd unit and udev rules, and reloads both daemons.

Enable and start the service:

```
sudo systemctl enable --now galed.service
```

## Uninstall

```
sudo packaging/install.sh --uninstall
```

## Files

- `systemd/galed.service`: systemd unit for the daemon
- `udev/60-gale-corsair.rules`: udev rule granting hidraw access to Corsair devices
- `install.sh`: installs and uninstalls the above

## Windows compile gate

A pre-push hook checks that the workspace still compiles for Windows. To run
it locally:

```
sudo apt-get install mingw-w64 && rustup target add x86_64-pc-windows-gnu
cargo check --workspace --all-targets --target x86_64-pc-windows-gnu
```
