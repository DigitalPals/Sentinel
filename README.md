# Cybex Sentinel

A real-time monitoring tool for **UniFi networks** and **Proxmox VE** clusters.
Dark NOC-style dashboard with live device inventory, per-guest utilization,
network topology, derived alerts and an event stream — all backed by live API
data, no mock data.

```
┌─────────────┐   poll 15s    ┌──────────────────┐   HTTP/JSON   ┌──────────────┐
│ Proxmox VE  │ ◀──────────── │  Rust backend    │ ◀──────────── │  React SPA   │
│ UniFi Net.  │ ◀──────────── │  (axum poller)   │ ──────────────▶  (Bun build) │
└─────────────┘               └──────────────────┘   /api/...    └──────────────┘
```

## Stack

- **Backend** — Rust ([axum](https://github.com/tokio-rs/axum) + [tokio](https://tokio.rs) +
  [reqwest](https://github.com/seanmonstar/reqwest)). A background task polls every
  source on a fixed interval, aggregates one snapshot, and serves it over a small
  JSON API. It also serves the built frontend, so the application is a single binary.
- **Frontend** — React + TypeScript, built with [Vite](https://vite.dev) and
  **Bun** as the package manager / runtime. Polls the backend on a configurable interval.
- **Storage** — PostgreSQL + [TimescaleDB](https://www.timescale.com/). Holds all
  settings and API credentials, and the metric history as a time-series hypertable
  (with retention, compression and an hourly continuous aggregate).

## Pages

| Page | What it shows |
|------|---------------|
| **Dashboard** | Fleet availability, devices online, active alerts, live WAN throughput, a 24h bandwidth chart, active issues, per-node Proxmox tiles, top resource consumers and a topology snapshot. |
| **UniFi Network** | Every adopted device (gateway / switch / AP / UPS) with live clients, throughput, uptime and a per-device detail panel (port activity grid for switches, radios for APs). |
| **Proxmox** | Every node with live CPU/MEM/DISK/NET, and every VM / LXC guest grouped under its node with utilization bars. |
| **Alerts** | Threshold-derived alerts (memory pressure, offline devices, …) with acknowledge / resolve workflow. |
| **Events & Logs** | A live event stream built from Proxmox task history and UniFi device events. |

## Data sources

Everything on screen is derived from live API calls:

- **Proxmox VE** — `/api2/json/cluster/resources`, `/nodes/{n}/rrddata`,
  `/nodes/{n}/tasks`, authenticated with an API token.
- **UniFi Network** — the local Integration API
  (`/proxy/network/integration/v1`, UniFi Network 9.0+), authenticated with an
  API key. Topology is reconstructed from each device's uplink relationship.

The dashboard bandwidth chart accumulates a real WAN time-series in the
`metric_samples` TimescaleDB hypertable; it starts sparse and fills toward 24h
as the backend runs.

## Prerequisites

- [Rust](https://rustup.rs) (stable) — `rustup default stable`
- [Bun](https://bun.sh) ≥ 1.3
- [Docker](https://docs.docker.com/get-docker/) — runs the bundled PostgreSQL +
  TimescaleDB container. (Or bring your own PostgreSQL ≥ 14 with the TimescaleDB
  extension installed.)

## Configuration

All configuration — sources, API credentials, polling/tuning values, alert
thresholds and display preferences — lives in the database. There is no config
file. On a fresh database the backend starts with no sources; add UniFi and
Proxmox endpoints and adjust everything else from the in-app **Settings** page
(backed by the `/api/settings` and `/api/sources` endpoints).

The one thing that cannot live in the database is the database's own address.
The backend reads it from the `DATABASE_URL` environment variable, defaulting to
the bundled container — `postgres://sentinel:sentinel@localhost:5432/sentinel`.

Self-signed certificates on the monitored UniFi/Proxmox hosts are accepted
automatically.

### Migrating from config.toml

If you previously ran Sentinel from a `backend/config.toml` (with a
`backend/data/history.json`), import them into the database once — it copies the
sources, poll interval and bandwidth history, and is safe to re-run:

```bash
cd backend && cargo run --release -- import-config
```

Afterwards the legacy files can be deleted.

## Run

The quickest path — start the database, build the frontend, and launch the
backend (which serves the frontend):

```bash
./run.sh
```

Then open <http://localhost:8787> and add your sources on the **Settings** page.

Or step by step:

```bash
# 1. Start PostgreSQL + TimescaleDB
docker compose up -d db

# 2. Build the frontend bundle
cd frontend && bun install && bun run build

# 3. Build & run the backend (runs migrations, serves API + frontend on :8787)
cd ../backend && cargo run --release
```

### Development mode

Run the backend and the Vite dev server separately for hot-reload:

```bash
docker compose up -d db
cd backend && cargo run                 # API on :8787
cd frontend && bun run dev              # UI on :5173, proxies /api → :8787
```

## API

| Endpoint | Description |
|----------|-------------|
| `GET /api/snapshot` | The complete monitoring snapshot (all pages). |
| `GET /api/health` | Source connectivity summary. |
| `POST /api/alerts/action` | `{ "id": "...", "action": "ack" \| "resolve" \| "reopen" }` |
| `GET /api/settings` | Polling/tuning settings and UI preferences. |
| `PUT /api/settings` | Update any subset of the settings. |
| `GET /api/sources` | Configured UniFi/Proxmox sources (secrets masked). |
| `POST·PUT·DELETE /api/sources/{unifi,proxmox}[/{id}]` | Manage sources. |
| `POST /api/sources/test` | Probe a source's connectivity. |

## Layout

```
backend/    Rust monitoring backend (axum)
  migrations/  PostgreSQL + TimescaleDB schema
  src/
    proxmox.rs   Proxmox VE API client
    unifi.rs     UniFi Integration API client
    db.rs        PostgreSQL / TimescaleDB data access
    config.rs    RuntimeConfig, loaded from the database
    engine.rs    Poller, aggregation, alert/event derivation
    history.rs   In-memory metric working set
    importer.rs  One-time config.toml / history.json import
    model.rs     JSON contract served to the frontend
    routes.rs    HTTP surface
frontend/   React + TypeScript SPA (Vite + Bun)
  src/
    pages/       Dashboard, Unifi, Proxmox, Alerts, Events, Settings
    components.tsx, topology.tsx, api.ts, settings.tsx
docker-compose.yml   TimescaleDB service
```
