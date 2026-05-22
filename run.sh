#!/usr/bin/env bash
# Build the frontend bundle, build the backend, and launch Cybex Sentinel.
set -euo pipefail
cd "$(dirname "$0")"

echo "▸ Building frontend (Bun + Vite)…"
( cd frontend && bun install --silent && bun run build )

echo "▸ Building backend (Rust, release)…"
( cd backend && cargo build --release )

echo "▸ Starting Cybex Sentinel on http://localhost:8787"
cd backend && exec ./target/release/cybex-sentinel
