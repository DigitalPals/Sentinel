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
  JSON API. It also serves the built frontend, so deployment is a single binary.
- **Frontend** — React + TypeScript, built with [Vite](https://vite.dev) and
  **Bun** as the package manager / runtime. Polls the backend every 5s.

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

The dashboard bandwidth chart accumulates a real WAN time-series in
`backend/data/history.json`; it starts sparse and fills toward 24h as the
backend runs.

## Prerequisites

- [Rust](https://rustup.rs) (stable) — `rustup default stable`
- [Bun](https://bun.sh) ≥ 1.3

## Configuration

Backend configuration lives in `backend/config.toml` (see
`backend/config.example.toml` for the template). It is pre-filled with the
target infrastructure:

```toml
poll_interval_sec = 15
bind = "0.0.0.0:8787"

[unifi]
host = "https://10.10.0.1"
api_key = "…"

[[proxmox]]
name = "PVE1"
host = "https://10.10.0.30:8006"
token_id = "monitoring@pve!monitoring-tool"
token_secret = "…"
```

Add more `[[proxmox]]` blocks for additional hosts. Self-signed certificates
are accepted automatically.

## Run

The quickest path — build the frontend and launch the backend (which serves it):

```bash
./run.sh
```

Then open <http://localhost:8787>.

Or step by step:

```bash
# 1. Build the frontend bundle
cd frontend
bun install
bun run build

# 2. Build & run the backend (serves API + frontend on :8787)
cd ../backend
cargo run --release
```

### Development mode

Run the backend and the Vite dev server separately for hot-reload:

```bash
cd backend && cargo run                 # API on :8787
cd frontend && bun run dev              # UI on :5173, proxies /api → :8787
```

## API

| Endpoint | Description |
|----------|-------------|
| `GET /api/snapshot` | The complete monitoring snapshot (all pages). |
| `GET /api/health` | Source connectivity summary. |
| `POST /api/alerts/action` | `{ "id": "...", "action": "ack" \| "resolve" \| "reopen" }` |

## Layout

```
backend/    Rust monitoring backend (axum)
  src/
    proxmox.rs   Proxmox VE API client
    unifi.rs     UniFi Integration API client
    engine.rs    Poller, aggregation, alert/event derivation
    model.rs     JSON contract served to the frontend
    routes.rs    HTTP surface
frontend/   React + TypeScript SPA (Vite + Bun)
  src/
    pages/       Dashboard, Unifi, Proxmox, Alerts, Events
    components.tsx, topology.tsx, api.ts, settings.tsx
```
