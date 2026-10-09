#!/usr/bin/env bash
# Install usaspending-awards under /opt and enable the weekly timer (Linux).
# Usage (as root): ./deploy/install.sh
#
# Prefer building the release binary as a normal user first:
#   cargo build --release
#   sudo ./deploy/install.sh
# Set FORCE_REBUILD=1 to rebuild even when target/release/usaspending-awards exists.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PREFIX="${USASPENDING_AWARDS_INSTALL_PREFIX:-/opt/usaspending-awards}"
STATE="${USASPENDING_AWARDS_STATE_DIR:-/var/lib/usaspending-awards}"
USER_NAME="${USASPENDING_AWARDS_RUN_USER:-usaspend}"
GROUP_NAME="${USASPENDING_AWARDS_RUN_GROUP:-$USER_NAME}"
BIN_SRC="$ROOT/target/release/usaspending-awards"
ENV_DST="$PREFIX/etc/usaspending-awards.env"

if [[ "$(id -u)" -ne 0 ]]; then
  echo "run as root" >&2
  exit 1
fi

build_release() {
  local build_user="${SUDO_USER:-}"
  if [[ -n "$build_user" && "$build_user" != "root" ]] && id -u "$build_user" >/dev/null 2>&1; then
    echo "== build release (as $build_user) =="
    sudo -u "$build_user" -H bash -lc "cd \"$ROOT\" && source \"\$HOME/.cargo/env\" 2>/dev/null || true; cargo build --release"
    return
  fi
  echo "no release binary at $BIN_SRC and no non-root SUDO_USER to build as." >&2
  echo "build first: cargo build --release" >&2
  echo "then re-run: sudo ./deploy/install.sh" >&2
  exit 1
}

if [[ -x "$BIN_SRC" && -z "${FORCE_REBUILD:-}" ]]; then
  echo "== using existing release binary: $BIN_SRC =="
else
  build_release
fi

test -x "$BIN_SRC" || {
  echo "missing $BIN_SRC — build with: cargo build --release" >&2
  exit 1
}

echo "== create user/dirs =="
NLOGIN="/usr/sbin/nologin"
[[ -x "$NLOGIN" ]] || NLOGIN="/sbin/nologin"
if ! id -u "$USER_NAME" >/dev/null 2>&1; then
  useradd --system --home-dir "$STATE" --shell "$NLOGIN" "$USER_NAME" || true
fi
if getent group ueicik >/dev/null 2>&1; then
  usermod -aG ueicik "$USER_NAME" || true
  # A WAL reader creates a shm file next to the map. The sqlite file stays 644.
  if [[ -d /var/lib/uei-cik ]]; then
    chmod 0775 /var/lib/uei-cik || true
  fi
fi
if getent group state-capture >/dev/null 2>&1; then
  usermod -aG state-capture "$USER_NAME" || true
  mkdir -p /var/lib/state-capture/announce
  chgrp state-capture /var/lib/state-capture/announce || true
  chmod 0775 /var/lib/state-capture/announce || true
fi
mkdir -p "$PREFIX"/{bin,scripts,etc,docs} \
  "$STATE/archive" \
  /etc/systemd/system

echo "== install files =="
install -m 0755 "$BIN_SRC" "$PREFIX/bin/usaspending-awards"
install -m 0755 "$ROOT/scripts/run-ingest.sh" "$PREFIX/scripts/run-ingest.sh"
install -m 0644 "$ROOT/docs/DAILY_OPS.md" "$PREFIX/docs/DAILY_OPS.md"
install -m 0644 "$ROOT/docs/CAPTURE.md" "$PREFIX/docs/CAPTURE.md"

if [[ ! -f "$ENV_DST" ]]; then
  if [[ -n "${USASPENDING_AWARDS_ENV_FILE:-}" && -f "$USASPENDING_AWARDS_ENV_FILE" ]]; then
    install -m 0600 "$USASPENDING_AWARDS_ENV_FILE" "$ENV_DST"
  else
    install -m 0600 "$ROOT/deploy/usaspending-awards.env.example" "$ENV_DST"
  fi
fi
chmod 0600 "$ENV_DST"

chown -R "$USER_NAME:$GROUP_NAME" "$STATE"
chown -R root:root "$PREFIX"
chown root:"$GROUP_NAME" "$PREFIX/etc" "$ENV_DST"
chmod 0750 "$PREFIX/etc"
chmod 0600 "$ENV_DST"
chmod 0755 "$PREFIX" "$PREFIX/bin" "$PREFIX/scripts" "$PREFIX/docs"
chmod 0755 "$PREFIX/scripts"/*.sh
# World-traversable so the collector can read the work sqlite.
chmod 0755 "$STATE" "$STATE/archive"

install -m 0644 "$ROOT/deploy/systemd/usaspending-awards-ingest.service" \
  /etc/systemd/system/usaspending-awards-ingest.service
install -m 0644 "$ROOT/deploy/systemd/usaspending-awards-ingest.timer" \
  /etc/systemd/system/usaspending-awards-ingest.timer

if command -v restorecon >/dev/null 2>&1; then
  echo "== SELinux restorecon =="
  restorecon -Rv "$PREFIX" "$STATE" || true
fi

systemctl daemon-reload
systemctl enable usaspending-awards-ingest.timer

echo "installed:"
echo "  prefix=$PREFIX state=$STATE"
echo "  sqlite: $STATE/usaspending-awards.sqlite"
echo "  logs: journalctl -u usaspending-awards-ingest.service"
echo "  timer: usaspending-awards-ingest.timer enabled (Sunday 18:00 UTC + 15m jitter; TimeoutStartSec=4h)"
echo "  env: $ENV_DST (chmod 600)"
echo "  first run: sudo systemctl start usaspending-awards-ingest.service"
