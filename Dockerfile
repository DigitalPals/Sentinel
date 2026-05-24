# Multi-stage build for Cybex Sentinel.
#
#   stage 1 (frontend): builds the React/Vite bundle with Bun → /frontend/dist
#   stage 2 (backend):  compiles the Rust backend in release mode
#   stage 3 (runtime):  slim image with just the binary and the static bundle
#
# The backend serves the bundle itself, so the result is a single container.

# ---- frontend ----------------------------------------------------------------
FROM oven/bun:1.3 AS frontend
WORKDIR /frontend

COPY frontend/package.json frontend/bun.lock ./
RUN bun install --frozen-lockfile

COPY frontend/ ./
RUN bun run build

# ---- backend -----------------------------------------------------------------
FROM rust:1-bookworm AS backend
WORKDIR /backend

# Cache the dependency build by compiling against a stub main first.
COPY backend/Cargo.toml backend/Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && \
    cargo build --release && \
    rm -rf src target/release/deps/cybex_sentinel* target/release/cybex-sentinel*

COPY backend/ ./
RUN cargo build --release

# ---- runtime -----------------------------------------------------------------
FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates nmap && \
    rm -rf /var/lib/apt/lists/*

COPY --from=backend  /backend/target/release/cybex-sentinel /usr/local/bin/cybex-sentinel
COPY --from=frontend /frontend/dist                        /app/dist

EXPOSE 8787
ENTRYPOINT ["/usr/local/bin/cybex-sentinel"]
