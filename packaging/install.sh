#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN_DIR="/usr/local/bin"
UNIT_DIR="/etc/systemd/system"
UDEV_DIR="/etc/udev/rules.d"

require_root() {
    if [[ "$(id -u)" -ne 0 ]]; then
        echo "this script must be run as root" >&2
        exit 1
    fi
}

build_user() {
    if [[ -n "${SUDO_USER:-}" && "$SUDO_USER" != "root" ]]; then
        echo "$SUDO_USER"
    fi
}

run_as_build_user() {
    local user
    user="$(build_user)"
    if [[ -n "$user" ]]; then
        sudo -u "$user" -H bash -lc "$1"
    else
        bash -lc "$1"
    fi
}

build_release() {
    if ! run_as_build_user "command -v cargo" >/dev/null 2>&1; then
        local user
        user="$(build_user)"
        if [[ -n "$user" ]]; then
            echo "cargo not found for user $user; install rustup for that user first" >&2
        else
            echo "cargo not found; install rustup first" >&2
        fi
        exit 1
    fi
    run_as_build_user "cd '$REPO_DIR' && cargo build --release"
}

uninstall() {
    systemctl disable --now galed.service 2>/dev/null || true
    rm -f "$UNIT_DIR/galed.service"
    rm -f "$UDEV_DIR/60-gale-corsair.rules"
    rm -f "$BIN_DIR/galed"
    rm -f "$BIN_DIR/gale"
    systemctl daemon-reload
    udevadm control --reload
    echo "gale uninstalled"
}

do_install() {
    build_release

    install -m 0755 "$REPO_DIR/target/release/galed" "$BIN_DIR/galed"
    install -m 0755 "$REPO_DIR/target/release/gale" "$BIN_DIR/gale"
    install -m 0644 "$SCRIPT_DIR/systemd/galed.service" "$UNIT_DIR/galed.service"
    install -m 0644 "$SCRIPT_DIR/udev/60-gale-corsair.rules" "$UDEV_DIR/60-gale-corsair.rules"

    systemctl daemon-reload
    udevadm control --reload

    echo "gale installed, enable with: systemctl enable --now galed.service"
}

main() {
    require_root

    if [[ "${1:-}" == "--uninstall" ]]; then
        uninstall
    else
        do_install
    fi
}

main "$@"
