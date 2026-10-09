#!/usr/bin/env bash
# Oneshot: FY Full award files, filtered to mapped UEIs, into work sqlite.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${USASPENDING_AWARDS_BIN:-$ROOT/bin/usaspending-awards}"
STATE="${USASPENDING_AWARDS_STATE:-/var/lib/usaspending-awards}"
DB="${USASPENDING_AWARDS_SQLITE:-$STATE/usaspending-awards.sqlite}"
UEI_DB="${UEI_MAP_SQLITE:-/var/lib/uei-cik/uei-cik.sqlite}"
CACHE="${USASPENDING_AWARDS_CACHE:-$STATE/archive}"
LOCK="${USASPENDING_AWARDS_LOCK:-$STATE/.ingest.lock}"

test -x "$BIN" || {
  echo "missing $BIN — build with: cargo build --release" >&2
  exit 1
}
test -r "$UEI_DB" || {
  echo "missing readable UEI map at $UEI_DB" >&2
  exit 1
}

mkdir -p "$STATE" "$CACHE"

if command -v flock >/dev/null 2>&1; then
  exec 9>"$LOCK"
  if ! flock -n 9; then
    echo "ingest already running (lock $LOCK)" >&2
    exit 1
  fi
else
  if ! mkdir "$LOCK.d" 2>/dev/null; then
    echo "ingest already running (lock $LOCK.d)" >&2
    exit 1
  fi
  trap 'rmdir "$LOCK.d" 2>/dev/null || true' EXIT
fi

echo "== usaspending-awards ingest =="
echo "bin=$BIN db=$DB uei_db=$UEI_DB cache=$CACHE"
"$BIN" --db "$DB" ingest --uei-db "$UEI_DB" --cache-dir "$CACHE"
