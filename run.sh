#!/usr/bin/env bash
# Build and start the full Cybex Sentinel stack (TimescaleDB + app) in Docker.
# Schema migrations run automatically on backend startup.
set -euo pipefail
cd "$(dirname "$0")"

echo "▸ Building and starting Cybex Sentinel (docker compose)…"
docker compose up -d --build

echo "▸ Cybex Sentinel is up on http://localhost:8787"
echo "  Logs:   docker compose logs -f app"
echo "  Stop:   docker compose down"
