#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${SENTINEL_SMOKE_IMAGE:-cybex-sentinel:local}"
PORT="${SENTINEL_SMOKE_PORT:-8788}"
USER="${SENTINEL_SMOKE_USER:-smoke}"
PASS="${SENTINEL_SMOKE_PASSWORD:-smoke-password-123}"
NETWORK="${SENTINEL_SMOKE_NETWORK:-sentinel-smoke-net}"
DB="${SENTINEL_SMOKE_DB:-sentinel-smoke-db}"
APP="${SENTINEL_SMOKE_APP:-sentinel-smoke-app}"
KEEP="${SENTINEL_SMOKE_KEEP:-0}"
TMP_FILES=()

cleanup() {
  if ((${#TMP_FILES[@]})); then
    rm -f "${TMP_FILES[@]}"
  fi
  if [[ "$KEEP" == "1" ]]; then
    return
  fi
  docker rm -f "$APP" "$DB" >/dev/null 2>&1 || true
  docker network rm "$NETWORK" >/dev/null 2>&1 || true
}

on_error() {
  echo "Smoke test failed." >&2
  docker logs --tail 120 "$APP" >&2 || true
  docker logs --tail 120 "$DB" >&2 || true
}

trap on_error ERR
trap cleanup EXIT

if [[ "${SENTINEL_SMOKE_SKIP_BUILD:-0}" != "1" ]]; then
  docker build -t "$IMAGE" "$ROOT"
fi

docker rm -f "$APP" "$DB" >/dev/null 2>&1 || true
docker network rm "$NETWORK" >/dev/null 2>&1 || true
docker network create "$NETWORK" >/dev/null

docker run -d --rm --name "$DB" --network "$NETWORK" \
  -e TZ=Europe/Amsterdam \
  -e POSTGRES_USER=sentinel \
  -e POSTGRES_PASSWORD=sentinel \
  -e POSTGRES_DB=sentinel \
  timescale/timescaledb:2.17.2-pg16 \
  postgres -c timezone=Europe/Amsterdam >/dev/null

for i in $(seq 1 60); do
  if docker exec "$DB" pg_isready -U sentinel -d sentinel >/dev/null 2>&1; then
    break
  fi
  sleep 1
  [[ "$i" != "60" ]]
done

docker run -d --rm --name "$APP" --network "$NETWORK" \
  -p "127.0.0.1:${PORT}:8787" \
  -e TZ=Europe/Amsterdam \
  -e DATABASE_URL=postgres://sentinel:sentinel@"$DB":5432/sentinel \
  -e SENTINEL_SECRET_KEY=smoke-test-secret-key \
  -e RUST_LOG=cybex_sentinel=info,tower_http=warn \
  "$IMAGE" >/dev/null

BASE="http://127.0.0.1:${PORT}"
COOKIE="$(mktemp)"
STATUS="$(mktemp)"
SNAPSHOT="$(mktemp)"
STREAM="$(mktemp)"
TMP_FILES=("$COOKIE" "$STATUS" "$SNAPSHOT" "$STREAM")

for i in $(seq 1 60); do
  if curl -fsS "$BASE/api/auth/status" > "$STATUS"; then
    break
  fi
  sleep 1
  [[ "$i" != "60" ]]
done

curl -fsS -c "$COOKIE" \
  -H "Content-Type: application/json" \
  -d "{\"username\":\"${USER}\",\"password\":\"${PASS}\"}" \
  "$BASE/api/auth/setup" >/dev/null

curl -fsS -b "$COOKIE" "$BASE/api/snapshot" > "$SNAPSHOT"
python3 - "$SNAPSHOT" <<'PY'
import json
import sys

snapshot = json.load(open(sys.argv[1]))
for key in ("generatedAt", "sources", "dashboard", "alerts", "events"):
    assert key in snapshot, key
assert "operations" not in snapshot
print("snapshot ok")
PY

timeout 6s curl -fsS -N -b "$COOKIE" "$BASE/api/stream" > "$STREAM" || true
python3 - "$STREAM" <<'PY'
import sys
from pathlib import Path

stream = Path(sys.argv[1]).read_text()
assert "event: snapshot" in stream, stream[:500]
assert '"generatedAt"' in stream, stream[:500]
assert '"operations"' not in stream, stream[:500]
print("stream ok")
PY

curl -fsS "$BASE/" | grep -q "Cybex Sentinel"
echo "frontend ok"

echo "Smoke test passed at $BASE"
