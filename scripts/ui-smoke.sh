#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

WORKDIR="$(mktemp -d)"
DAEMON_PID=""

cleanup() {
  local status=$?
  if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" 2>/dev/null; then
    kill "$DAEMON_PID" 2>/dev/null || true
    wait "$DAEMON_PID" 2>/dev/null || true
  fi
  if [[ $status -ne 0 && -f "$WORKDIR/daemon.log" ]]; then
    echo "--- daemon log ---" >&2
    tail -n 100 "$WORKDIR/daemon.log" >&2 || true
  fi
  rm -rf "$WORKDIR"
  exit "$status"
}
trap cleanup EXIT INT TERM

find_chrome_binary() {
  find "$1" -maxdepth 4 -type f -name chrome 2>/dev/null | head -n1
}

pick_port() {
  if [[ -n "${GALE_UI_SMOKE_PORT:-}" ]]; then
    echo "$GALE_UI_SMOKE_PORT"
    return
  fi
  node -e "
const net = require('node:net');
const srv = net.createServer();
srv.listen(0, '127.0.0.1', () => {
  console.log(srv.address().port);
  srv.close();
});
"
}

detect_chrome() {
  if [[ -n "${GALE_UI_SMOKE_CHROME:-}" ]]; then
    echo "$GALE_UI_SMOKE_CHROME"
    return
  fi
  for bin in chromium chromium-browser google-chrome-stable google-chrome; do
    if command -v "$bin" >/dev/null 2>&1; then
      command -v "$bin"
      return
    fi
  done
  local cache_dir="$HOME/.cache/gale-ui-smoke/chromium"
  local existing
  existing="$(find_chrome_binary "$cache_dir")"
  if [[ -n "$existing" ]]; then
    echo "$existing"
    return
  fi
  if command -v sudo >/dev/null 2>&1 && command -v apt-get >/dev/null 2>&1 \
    && apt-cache policy chromium 2>/dev/null | grep -q "Candidate: [^(]"; then
    sudo apt-get install -y chromium >/dev/null 2>&1 || true
    if command -v chromium >/dev/null 2>&1; then
      command -v chromium
      return
    fi
  fi
  mkdir -p "$cache_dir"
  echo "no system chromium found, downloading via @puppeteer/browsers" >&2
  npx --yes @puppeteer/browsers install chromium@latest --path "$cache_dir" >&2
  find_chrome_binary "$cache_dir"
}

echo "building galed" >&2
cargo build --release --bin galed --manifest-path "$REPO_ROOT/Cargo.toml" >&2

if [[ ! -d "$REPO_ROOT/ui/node_modules/puppeteer-core" ]]; then
  echo "installing ui/ npm dependencies" >&2
  (cd "$REPO_ROOT/ui" && npm install) >&2
fi

CHROME_PATH="$(detect_chrome)"
if [[ -z "$CHROME_PATH" || ! -x "$CHROME_PATH" ]]; then
  echo "could not find or install a headless chromium binary" >&2
  exit 1
fi
echo "using chromium at $CHROME_PATH" >&2

HWMON_ROOT="$WORKDIR/hwmon"
CHIP_DIR="$HWMON_ROOT/hwmon0"
mkdir -p "$CHIP_DIR"
echo "nct6798" >"$CHIP_DIR/name"
echo "45000" >"$CHIP_DIR/temp1_input"
echo "CPUTIN" >"$CHIP_DIR/temp1_label"
echo "50000" >"$CHIP_DIR/temp2_input"
echo "SYSTIN" >"$CHIP_DIR/temp2_label"
echo "1200" >"$CHIP_DIR/fan1_input"
echo "128" >"$CHIP_DIR/pwm1"
echo "5" >"$CHIP_DIR/pwm1_enable"
echo "1" >"$CHIP_DIR/pwm1_mode"

PORT="$(pick_port)"
BIND="127.0.0.1:$PORT"
BASE_URL="http://$BIND"

if curl -sf "$BASE_URL/api/status" >/dev/null 2>&1; then
  echo "something is already answering on $BASE_URL before the scratch daemon has even started; refusing to reuse this port" >&2
  exit 1
fi

CONFIG_PATH="$WORKDIR/config.toml"
cat >"$CONFIG_PATH" <<EOF
tick_interval_ms = 200
active_profile = "default"

[api]
bind = "$BIND"

[profiles.default.sensors.combined]
type = "max"
inputs = ["hwmon/nct6798/temp1", "hwmon/nct6798/temp2"]

[profiles.default.curves.cpu]
type = "point"
sensor = "hwmon/nct6798/temp1"
points = [[30.0, 20.0], [70.0, 100.0]]

[profiles.default.assignments]
"hwmon/nct6798/pwm1" = "cpu"
EOF

RUNTIME_DIR="$WORKDIR/run"
mkdir -p "$RUNTIME_DIR"

echo "starting galed against scratch hwmon tree on $BASE_URL" >&2
GALE_HWMON_ROOT="$HWMON_ROOT" \
  GALE_CONFIG="$CONFIG_PATH" \
  GALE_RUNTIME_DIR="$RUNTIME_DIR" \
  RUST_LOG=warn \
  "$REPO_ROOT/target/release/galed" >"$WORKDIR/daemon.log" 2>&1 &
DAEMON_PID=$!

ready=""
collision=""
for _ in $(seq 1 40); do
  pid_alive=""
  if kill -0 "$DAEMON_PID" 2>/dev/null; then
    pid_alive=1
  fi
  if curl -sf "$BASE_URL/api/status" >/dev/null 2>&1; then
    if [[ -n "$pid_alive" ]]; then
      ready=1
    else
      collision=1
    fi
    break
  fi
  if [[ -z "$pid_alive" ]]; then
    break
  fi
  sleep 0.25
done

if [[ -n "$collision" ]]; then
  echo "our spawned galed (pid $DAEMON_PID) is no longer running, but something else is answering on $BASE_URL; refusing to run the smoke against a foreign daemon" >&2
  exit 1
fi
if [[ -z "$ready" ]]; then
  echo "galed never became ready on $BASE_URL" >&2
  exit 1
fi

GALE_UI_SMOKE_BASE_URL="$BASE_URL" \
  GALE_UI_SMOKE_CHROME="$CHROME_PATH" \
  node "$SCRIPT_DIR/ui-smoke.mjs"
