#!/usr/bin/env bash
# Start the database, build the frontend bundle, build the backend, and launch
# Cybex Sentinel.
set -euo pipefail
cd "$(dirname "$0")"

echo "▸ Starting TimescaleDB (docker compose)…"
docker compose up -d db

echo "▸ Waiting for the database to accept connections…"
for _ in $(seq 1 30); do
  if docker compose exec -T db pg_isready -U sentinel -d sentinel >/dev/null 2>&1; then
    break
  fi
  sleep 1
done

echo "▸ Building frontend (Bun + Vite)…"
( cd frontend && bun install --silent && bun run build )

echo "▸ Building backend (Rust, release)…"
( cd backend && cargo build --release )

echo "▸ Starting Cybex Sentinel on http://localhost:8787"
# Schema migrations run automatically on startup. To import an existing
# config.toml / data/history.json into the database, run once:
#   ( cd backend && ./target/release/cybex-sentinel import-config )
cd backend && exec ./target/release/cybex-sentinel
