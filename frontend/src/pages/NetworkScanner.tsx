// Network Scanner — Nmap-backed device discovery and port inventory.
import React from "react";
import {
  NetworkScanDevice,
  NetworkScanJob,
  NetworkScannerOverview,
  cancelNetworkScan,
  getNetworkScanner,
  startNetworkScan,
} from "../api";
import { Card, Chip, Icon } from "../components";

function fmtDate(value: string | null | undefined): string {
  if (!value) return "—";
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return "—";
  return d.toLocaleString();
}

function fmtMs(value: number | null): string {
  if (value == null) return "—";
  return value >= 100 ? `${Math.round(value)} ms` : `${value.toFixed(1)} ms`;
}

function fmtDuration(ms: number | undefined): string {
  if (!ms) return "—";
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(1)} s`;
}

function statusTone(status: string): string {
  if (status === "succeeded" || status === "up") return "ok";
  if (status === "running" || status === "queued") return "info";
  if (status === "failed") return "crit";
  return "default";
}

function portsText(device: NetworkScanDevice): string {
  if (!device.ports.length) return "—";
  return device.ports
    .slice(0, 8)
    .map((p) => {
      const svc = p.service ? ` ${p.service}` : "";
      return `${p.port}/${p.protocol}${svc}`;
    })
    .join(", ");
}

function ipSortValue(ip: string): number[] {
  const parts = ip.split(".");
  if (parts.length !== 4) return [];
  const octets = parts.map((p) => Number(p));
  return octets.every((n) => Number.isInteger(n) && n >= 0 && n <= 255) ? octets : [];
}

function compareIp(a: NetworkScanDevice, b: NetworkScanDevice): number {
  const aa = ipSortValue(a.ip);
  const bb = ipSortValue(b.ip);
  if (aa.length && bb.length) {
    for (let i = 0; i < aa.length; i += 1) {
      if (aa[i] !== bb[i]) return aa[i] - bb[i];
    }
    return 0;
  }
  return a.ip.localeCompare(b.ip);
}

export default function NetworkScanner({
  onConfigure,
  onOpenHost,
}: {
  onConfigure: () => void;
  onOpenHost: (target: string) => void;
}) {
  const [data, setData] = React.useState<NetworkScannerOverview | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [view, setView] = React.useState<"latest" | "inventory">("latest");
  const [query, setQuery] = React.useState("");
  const [openOnly, setOpenOnly] = React.useState(false);

  const load = React.useCallback(async () => {
    try {
      setData(await getNetworkScanner());
      setError(null);
    } catch (e: any) {
      setError(String(e?.message ?? e));
    }
  }, []);

  React.useEffect(() => {
    load();
  }, [load]);

  React.useEffect(() => {
    const active = data?.activeJob && ["queued", "running"].includes(data.activeJob.status);
    const t = window.setInterval(load, active ? 2500 : 8000);
    return () => window.clearInterval(t);
  }, [data?.activeJob?.status, load]);

  const runScan = async () => {
    setBusy(true);
    try {
      await startNetworkScan();
      await load();
    } catch (e: any) {
      setError(String(e?.message ?? e));
    } finally {
      setBusy(false);
    }
  };

  const cancel = async (job: NetworkScanJob) => {
    setBusy(true);
    try {
      await cancelNetworkScan(job.id);
      await load();
    } catch (e: any) {
      setError(String(e?.message ?? e));
    } finally {
      setBusy(false);
    }
  };

  const devices = (view === "inventory" ? data?.inventory : data?.devices) ?? [];
  const filtered = devices
    .filter((d) => {
      const haystack = [d.ip, d.hostname, d.mac, d.vendor, d.osGuess]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();
      if (query.trim() && !haystack.includes(query.trim().toLowerCase())) return false;
      if (openOnly && d.ports.length === 0) return false;
      return true;
    })
    .sort(compareIp);
  const active = data?.activeJob;
  const latest = data?.latestJob;
  const latestSummary = latest?.summary ?? null;

  return (
    <div className="page network-page">
      {error && <div className="conn-banner">{error}</div>}

      <div className="network-toolbar">
        <div className="network-scope">
          {(data?.settings.ranges ?? []).map((range) => (
            <Chip key={range} tone="info">
              {range}
            </Chip>
          ))}
          {(data?.settings.exclude.length ?? 0) > 0 && (
            <Chip>{data?.settings.exclude.length} excluded</Chip>
          )}
        </div>
        <button className="set-btn" onClick={onConfigure}>
          Configure
        </button>
        <button
          className="set-btn primary"
          disabled={busy || !!active || !data?.settings.enabled}
          onClick={runScan}
        >
          <Icon name="scan" /> Run scan
        </button>
      </div>

      <div className="kpi-grid network-kpis">
        <div className="kpi">
          <div className="kpi-label">Hosts</div>
          <div className="kpi-val">{latestSummary?.hostsUp ?? data?.inventory.length ?? 0}</div>
          <div className="kpi-sub">latest scan</div>
        </div>
        <div className="kpi">
          <div className="kpi-label">Open ports</div>
          <div className="kpi-val">{latestSummary?.openPorts ?? 0}</div>
          <div className="kpi-sub">
            {data?.settings.portScan.enabled ? "port scan enabled" : "port scan disabled"}
          </div>
        </div>
        <div className="kpi">
          <div className="kpi-label">Duration</div>
          <div className="kpi-val">{fmtDuration(latestSummary?.durationMs)}</div>
          <div className="kpi-sub">{latest?.finishedAt ? fmtDate(latest.finishedAt) : "no scan yet"}</div>
        </div>
        <div className="kpi">
          <div className="kpi-label">Status</div>
          <div className="kpi-val">
            {active ? active.status : latest?.status ?? "idle"}
          </div>
          <div className="kpi-sub">{active ? `job ${active.id}` : "worker queue"}</div>
        </div>
      </div>

      {active && (
        <Card
          title="Active Scan"
          sub={`job ${active.id} · ${active.trigger}`}
          actions={
            active.status === "queued" ? (
              <button className="set-btn" disabled={busy} onClick={() => cancel(active)}>
                Cancel
              </button>
            ) : undefined
          }
        >
          <div className="network-active">
            <Chip tone={statusTone(active.status)} dot>
              {active.status}
            </Chip>
            <span>Created {fmtDate(active.createdAt)}</span>
            <span>{active.startedAt ? `Started ${fmtDate(active.startedAt)}` : "Waiting for worker"}</span>
          </div>
        </Card>
      )}

      <Card
        title="Discovered Devices"
        sub={view === "latest" ? "latest completed scan" : "current inventory"}
        actions={
          <>
            <button
              className={"chip" + (view === "latest" ? " info" : "")}
              onClick={() => setView("latest")}
            >
              Latest
            </button>
            <button
              className={"chip" + (view === "inventory" ? " info" : "")}
              onClick={() => setView("inventory")}
            >
              Inventory
            </button>
          </>
        }
      >
        <div className="network-device-tools">
          <div className="search-mini network-search">
            <Icon name="search" />
            <input
              value={query}
              placeholder="Search"
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
          <label className="set-inline">
            <input
              type="checkbox"
              checked={openOnly}
              onChange={(e) => setOpenOnly(e.target.checked)}
            />
            <span>Open ports</span>
          </label>
        </div>
        <div className="network-device-table">
          <div className="network-device-head">
            <span>IP</span>
            <span>Name</span>
            <span>MAC / Vendor</span>
            <span>Latency</span>
            <span>Ports</span>
            <span>Last seen</span>
          </div>
          {filtered.length === 0 && (
            <div className="network-empty">No devices found.</div>
          )}
          {filtered.map((d) => (
            <div
              className="network-device-row"
              key={`${view}-${d.ip}`}
              role="button"
              tabIndex={0}
              onClick={() => onOpenHost(d.ip)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onOpenHost(d.ip);
                }
              }}
            >
              <div className="network-ip">
                <span className={"status-dot " + statusTone(d.status)} />
                <span>{d.ip}</span>
              </div>
              <div>
                <div className="network-primary">{d.hostname || "—"}</div>
                <div className="network-muted">{d.osGuess || d.discoveryMethod}</div>
              </div>
              <div>
                <div className="network-primary">{d.mac || "—"}</div>
                <div className="network-muted">{d.vendor || "unknown"}</div>
              </div>
              <div className="network-mono">{fmtMs(d.latencyMs)}</div>
              <div className="network-ports" title={portsText(d)}>
                {portsText(d)}
              </div>
              <div className="network-mono">{fmtDate(d.lastSeen)}</div>
            </div>
          ))}
        </div>
      </Card>
    </div>
  );
}
