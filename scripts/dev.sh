#!/usr/bin/env bash
# Start Guandan backend + TUI client for local development.
#
# Usage:
#   ./scripts/dev.sh
#   ./scripts/dev.sh --release
#   BIND=0.0.0.0:9100 SERVER=ws://127.0.0.1:9100 ./scripts/dev.sh
#
# Env:
#   BIND     server listen address (default 127.0.0.1:9100)
#   SERVER   client WebSocket URL  (default ws://127.0.0.1:9100)
#   SKIP_BUILD=1  skip cargo build

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

BIND="${BIND:-127.0.0.1:9100}"
SERVER="${SERVER:-ws://127.0.0.1:9100}"
PROFILE="debug"
CARGO_FLAGS=()
BIN_DIR="target/debug"

for arg in "$@"; do
  case "$arg" in
    --release|-r)
      PROFILE="release"
      CARGO_FLAGS+=(--release)
      BIN_DIR="target/release"
      ;;
    -h|--help)
      sed -n '2,14p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      echo "Unknown option: $arg (try --help)" >&2
      exit 1
      ;;
  esac
done

SERVER_BIN="$BIN_DIR/guandan-server"
CLIENT_BIN="$BIN_DIR/guandan"
SERVER_PID=""
LOG_DIR="${TMPDIR:-/tmp}/guandan-dev"
mkdir -p "$LOG_DIR"
SERVER_LOG="$LOG_DIR/server.log"

# Extract host:port from BIND for readiness check
PORT="${BIND##*:}"

cleanup() {
  local code=$?
  if [[ -n "${SERVER_PID}" ]] && kill -0 "$SERVER_PID" 2>/dev/null; then
    echo ""
    echo "Stopping server (pid $SERVER_PID)…"
    kill "$SERVER_PID" 2>/dev/null || true
    # Give it a moment, then force
    for _ in 1 2 3 4 5; do
      kill -0 "$SERVER_PID" 2>/dev/null || break
      sleep 0.1
    done
    kill -9 "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  exit "$code"
}
trap cleanup EXIT INT TERM

echo "══ 掼蛋 Guandan · dev ══"
echo "  profile  $PROFILE"
echo "  bind     $BIND"
echo "  client   $SERVER"
echo ""

if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  echo "Building server + client…"
  cargo build -p guandan-server -p guandan-client "${CARGO_FLAGS[@]+"${CARGO_FLAGS[@]}"}"
  echo ""
fi

if [[ ! -x "$SERVER_BIN" || ! -x "$CLIENT_BIN" ]]; then
  echo "error: binaries missing under $BIN_DIR — run without SKIP_BUILD=1" >&2
  exit 1
fi

# Free port if a stale process is listening (optional; only our binary)
if command -v fuser >/dev/null 2>&1; then
  fuser -k "${PORT}/tcp" 2>/dev/null || true
  sleep 0.2
fi

echo "Starting server → $SERVER_LOG"
"$SERVER_BIN" --bind "$BIND" >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!

# Wait until the port accepts connections (max ~15s)
echo -n "Waiting for server"
ready=0
for i in $(seq 1 75); do
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    echo ""
    echo "error: server exited early. Log:" >&2
    tail -n 40 "$SERVER_LOG" >&2 || true
    exit 1
  fi
  if command -v ss >/dev/null 2>&1; then
    if ss -ltn 2>/dev/null | grep -qE ":${PORT}\\b"; then
      ready=1
      break
    fi
  elif command -v nc >/dev/null 2>&1; then
    if nc -z 127.0.0.1 "$PORT" 2>/dev/null; then
      ready=1
      break
    fi
  else
    # Fallback: brief sleep then proceed
    sleep 0.5
    ready=1
    break
  fi
  echo -n "."
  sleep 0.2
done
echo ""

if [[ "$ready" != "1" ]]; then
  echo "error: server did not open port $PORT in time. Log:" >&2
  tail -n 40 "$SERVER_LOG" >&2 || true
  exit 1
fi

echo "Server up (pid $SERVER_PID). Starting TUI…"
echo "  (server log: $SERVER_LOG)"
echo "  Quit client with Ctrl+C / q — server stops with the script."
echo ""

# Client in foreground (needs a real TTY). Do not exec — trap must stop the server.
set +e
"$CLIENT_BIN" --server "$SERVER"
client_code=$?
set -e
exit "$client_code"
