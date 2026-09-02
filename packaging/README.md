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
