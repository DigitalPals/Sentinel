// Network host detail — scanner inventory, scan history and UniFi enrichment.
import React from "react";
import {
  getNetworkHost,
  getNetworkHostUnifi,
  startHostPortScan,
  type NetworkHostDetail as NetworkHostDetailData,
  type NetworkHostUnifi,
  type NetworkScanDevice,
  type NetworkScanPort,
} from "../api";
import { Bar, Card, Chip, Icon, fmtMbps } from "../components";

function fmtDate(value: string | null | undefined): string {
  if (!value) return "—";
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return value;
  return d.toLocaleString();
}

function fmtMs(value: number | null | undefined): string {
  if (value == null) return "—";
  return value >= 100 ? `${Math.round(value)} ms` : `${value.toFixed(1)} ms`;
}

function fmtBytes(value: number | null | undefined): string {
  if (value == null) return "—";
  if (value < 1024) return `${value} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let n = value / 1024;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i += 1;
  }
  return `${n >= 100 ? n.toFixed(0) : n.toFixed(1)} ${units[i]}`;
}

function statusTone(status: string | null | undefined): string {
  if (status === "succeeded" || status === "up" || status === "open" || status === "ONLINE") return "ok";
  if (status === "running" || status === "queued") return "info";
  if (status === "failed" || status === "OFFLINE") return "crit";
  return "default";
}

function portLabel(port: NetworkScanPort): string {
  const service = [port.service, port.product, port.version].filter(Boolean).join(" ");
  return service || "unknown";
}

function uniqPorts(devices: NetworkScanDevice[]): NetworkScanPort[] {
  const byKey = new Map<string, NetworkScanPort>();
  for (const d of devices) {
    for (const p of d.ports) byKey.set(`${p.protocol}/${p.port}`, p);
  }
  return Array.from(byKey.values()).sort((a, b) => a.port - b.port);
}

function matchLabel(value: string | null): string {
  switch (value) {
    case "unifi-device-ip":
      return "UniFi device IP";
    case "unifi-device-mac":
      return "UniFi device MAC";
    case "unifi-client-ip":
      return "UniFi client IP";
    case "unifi-client-mac":
      return "UniFi client MAC";
    default:
      return "No match";
  }
}

function clientNetwork(unifi: NetworkHostUnifi): string | null {
  return unifi.client?.networkName || unifi.client?.ssid || null;
}

function isWiredClient(unifi: NetworkHostUnifi): boolean {
  return unifi.client?.kind?.toUpperCase() === "WIRED" || unifi.connection?.connectionType === "wired";
}

function linkMetricLabel(unifi: NetworkHostUnifi): string {
  return isWiredClient(unifi) ? "Link speed" : "Signal";
}

function linkMetricValue(unifi: NetworkHostUnifi): string | null {
  if (isWiredClient(unifi)) {
    return unifi.connection?.portSpeedMbps == null
      ? unifi.connection?.connectionType ?? null
      : `${unifi.connection.portSpeedMbps} Mbps`;
  }
  return unifi.client?.signal == null ? null : `${unifi.client.signal} dBm`;
}

export default function NetworkHostDetail({
  target,
  onBack,
  onConfigure,
}: {
  target: string;
  onBack: () => void;
  onConfigure: () => void;
}) {
  const [data, setData] = React.useState<NetworkHostDetailData | null>(null);
  const [unifi, setUnifi] = React.useState<NetworkHostUnifi | null>(null);
  const [unifiLoading, setUnifiLoading] = React.useState(false);
  const [unifiError, setUnifiError] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);

  const load = React.useCallback(async () => {
    try {
      setData(await getNetworkHost(target));
      setError(null);
    } catch (e: any) {
      setError(String(e?.message ?? e));
    }
  }, [target]);

  const loadUnifi = React.useCallback(async () => {
    setUnifiLoading(true);
    try {
      setUnifi(await getNetworkHostUnifi(target));
      setUnifiError(null);
    } catch (e: any) {
      setUnifi(null);
      setUnifiError(String(e?.message ?? e));
    } finally {
      setUnifiLoading(false);
    }
  }, [target]);

  React.useEffect(() => {
    setData(null);
    setUnifi(null);
    setUnifiError(null);
    load();
    loadUnifi();
  }, [load, loadUnifi]);

  React.useEffect(() => {
    const active = data?.activeJob && ["queued", "running"].includes(data.activeJob.status);
    const t = window.setInterval(load, active ? 2500 : 8000);
    return () => window.clearInterval(t);
  }, [data?.activeJob?.status, load]);

  const runPortScan = async () => {
    setBusy(true);
    try {
      await startHostPortScan(target);
      await load();
    } catch (e: any) {
      setError(String(e?.message ?? e));
    } finally {
      setBusy(false);
    }
  };

  const host = data?.host;
  const observations = data?.observations ?? [];
  const ports = host?.ports.length ? host.ports : uniqPorts(observations);
  const active = data?.activeJob;
  const unifiTone = unifi?.error ? "crit" : unifi?.matchedBy ? "ok" : unifi?.configured ? "warn" : "default";

  return (
    <div className="page network-page network-host-page">
      {error && <div className="conn-banner">{error}</div>}

      <div className="network-host-bar">
        <button className="set-btn" onClick={onBack}>
          <Icon name="back" /> Scanner
        </button>
        <div className="network-host-title">
          <div className="crumb">Network Scanner / Host</div>
          <h2>{host?.hostname || target}</h2>
          <div className="network-host-sub">
            <span className="network-mono">{host?.ip || target}</span>
            {host?.mac && <span>{host.mac}</span>}
            {host?.vendor && <span>{host.vendor}</span>}
          </div>
        </div>
        <button className="set-btn" onClick={onConfigure}>
          Configure
        </button>
        <button className="set-btn primary" disabled={busy || !!active} onClick={runPortScan}>
          <Icon name="scan" /> Port scan
        </button>
      </div>

      <div className="kpi-grid network-kpis">
        <div className="kpi">
          <div className="kpi-label">Status</div>
          <div className="kpi-val">{host?.status ?? (data ? "unknown" : "loading")}</div>
          <div className="kpi-sub">{host?.discoveryMethod ?? "scanner inventory"}</div>
        </div>
        <div className="kpi">
          <div className="kpi-label">Open ports</div>
          <div className="kpi-val">{ports.length}</div>
          <div className="kpi-sub">{data?.settings.portScan.profile ?? "current profile"}</div>
        </div>
        <div className="kpi">
          <div className="kpi-label">First seen</div>
          <div className="kpi-val network-kpi-date">{fmtDate(host?.firstSeen)}</div>
          <div className="kpi-sub">Last seen {fmtDate(host?.lastSeen)}</div>
        </div>
        <div className="kpi">
          <div className="kpi-label">UniFi</div>
          {unifiLoading ? (
            <>
              <Skeleton className="network-skel-kpi" />
              <Skeleton className="network-skel-sub" />
            </>
          ) : (
            <>
              <div className="kpi-val network-kpi-date">
                {unifi?.matchedBy ? "matched" : unifi?.configured ? "unmatched" : "not configured"}
              </div>
              <div className="kpi-sub">{unifi?.site ?? unifi?.error ?? unifiError ?? "controller enrichment"}</div>
            </>
          )}
        </div>
      </div>

      {active && (
        <Card title="Active Scan" sub={`job ${active.id} · ${active.trigger}`}>
          <div className="network-active">
            <Chip tone={statusTone(active.status)} dot>
              {active.status}
            </Chip>
            <span>Created {fmtDate(active.createdAt)}</span>
            <span>{active.startedAt ? `Started ${fmtDate(active.startedAt)}` : "Waiting for worker"}</span>
          </div>
        </Card>
      )}

      <div className="row-2">
        <Card title="Scanner Identity" sub="Inventory record">
          <div className="detail-grid network-host-detail-grid">
            <Fact label="IP address" value={host?.ip || target} />
            <Fact label="Hostname" value={host?.hostname} />
            <Fact label="MAC" value={host?.mac} />
            <Fact label="Vendor" value={host?.vendor} />
            <Fact label="OS guess" value={host?.osGuess} />
            <Fact label="Latency" value={fmtMs(host?.latencyMs)} />
            <Fact label="First seen" value={fmtDate(host?.firstSeen)} />
            <Fact label="Last seen" value={fmtDate(host?.lastSeen)} />
          </div>
        </Card>

        <Card title="UniFi Link" sub={unifi?.appVersion ? `Network ${unifi.appVersion}` : "Controller enrichment"}>
          {unifiLoading ? (
            <UnifiSkeleton />
          ) : unifiError ? (
            <div className="network-error">{unifiError}</div>
          ) : !unifi ? (
            <div className="network-empty">No UniFi context loaded.</div>
          ) : unifi.error ? (
            <div className="network-error">{unifi.error}</div>
          ) : !unifi.configured ? (
            <div className="network-empty">No enabled UniFi controller is configured.</div>
          ) : (
            <div className="network-unifi-block">
              <div className="network-unifi-chips">
                <Chip tone={unifiTone}>{matchLabel(unifi.matchedBy)}</Chip>
                {unifi.site && <Chip>{unifi.site}</Chip>}
                {unifi.client?.kind && <Chip>{unifi.client.kind.toLowerCase()}</Chip>}
              </div>

              {unifi.client && (
                <div className="detail-grid network-host-detail-grid">
                  <Fact label="Client name" value={unifi.client.name} />
                  <Fact label="Network" value={clientNetwork(unifi)} />
                  <Fact label="Connected" value={fmtDate(unifi.client.connectedAt)} />
                  <Fact label="Last seen" value={fmtDate(unifi.client.lastSeenAt)} />
                  <Fact label={linkMetricLabel(unifi)} value={linkMetricValue(unifi)} />
                  <Fact label="VLAN" value={unifi.client.vlanId == null ? null : String(unifi.client.vlanId)} />
                </div>
              )}

              {unifi.device && (
                <div className="network-unifi-device">
                  <div className="network-primary">{unifi.device.name}</div>
                  <div className="network-muted">
                    {unifi.device.kind} · {unifi.device.model} · {unifi.device.ip}
                  </div>
                  <div className="network-flow-grid">
                    <Bar label="CPU" value={unifi.device.cpu} />
                    <Bar label="Memory" value={unifi.device.mem} />
                  </div>
                </div>
              )}

              {unifi.connection && (
                <div className="network-connection">
                  <div className="dg-k">Connection</div>
                  <div className="network-primary">
                    {unifi.connection.uplinkDevice?.name ||
                      unifi.connection.uplinkDeviceName ||
                      unifi.connection.uplinkDeviceId ||
                      "Unknown uplink"}
                    {unifi.connection.portIdx != null ? ` · port ${unifi.connection.portIdx}` : ""}
                  </div>
                  <div className="network-muted">
                    {unifi.connection.portName || unifi.connection.connectionType}
                    {unifi.connection.portSpeedMbps ? ` · ${unifi.connection.portSpeedMbps} Mbps` : ""}
                    {unifi.connection.poe ? " · PoE" : ""}
                  </div>
                </div>
              )}

              {unifi.traffic && (
                <div className="detail-grid network-host-detail-grid">
                  <Fact label="Download" value={unifi.traffic.rxMbps == null ? null : fmtMbps(unifi.traffic.rxMbps)} />
                  <Fact label="Upload" value={unifi.traffic.txMbps == null ? null : fmtMbps(unifi.traffic.txMbps)} />
                  <Fact label="RX total" value={fmtBytes(unifi.traffic.rxBytes)} />
                  <Fact label="TX total" value={fmtBytes(unifi.traffic.txBytes)} />
                </div>
              )}
            </div>
          )}
        </Card>
      </div>

      <div className="row-2">
        <Card title="Ports and Services" sub={data?.settings.portScan.ports || "Configured port profile"}>
          <div className="network-port-table">
            <div className="network-port-head">
              <span>Port</span>
              <span>Protocol</span>
              <span>Service</span>
              <span>State</span>
            </div>
            {ports.length === 0 ? (
              <div className="network-empty">No open ports recorded. Run a port scan for this host.</div>
            ) : (
              ports.map((p) => (
                <div className="network-port-row" key={`${p.protocol}-${p.port}`}>
                  <span className="network-mono">{p.port}</span>
                  <span>{p.protocol}</span>
                  <span>{portLabel(p)}</span>
                  <Chip tone={statusTone(p.state)}>{p.state}</Chip>
                </div>
              ))
            )}
          </div>
        </Card>

        <Card title="Observation History" sub={`${observations.length} scanner observation(s)`}>
          <div className="network-history-list">
            {observations.length === 0 ? (
              <div className="network-empty">No scan history for this host.</div>
            ) : (
              observations.map((o) => (
                <div className="network-history-row" key={`${o.jobId}-${o.lastSeen}`}>
                  <div>
                    <div className="network-primary">{fmtDate(o.lastSeen)}</div>
                    <div className="network-muted">
                      job {o.jobId ?? "—"} · {o.discoveryMethod} · {fmtMs(o.latencyMs)}
                    </div>
                  </div>
                  <div className="network-history-ports">
                    {o.ports.length ? `${o.ports.length} open` : "no ports"}
                  </div>
                </div>
              ))
            )}
          </div>
        </Card>
      </div>
    </div>
  );
}

function Fact({ label, value }: { label: string; value: React.ReactNode }) {
  const empty = value == null || value === "";
  return (
    <div>
      <div className="dg-k">{label}</div>
      <div className="dg-v">{empty ? "—" : value}</div>
    </div>
  );
}

function Skeleton({ className = "" }: { className?: string }) {
  return <span className={"network-skeleton" + (className ? " " + className : "")} />;
}

function UnifiSkeleton() {
  return (
    <div className="network-unifi-block" aria-busy="true">
      <div className="network-unifi-chips">
        <Skeleton className="network-skel-chip" />
        <Skeleton className="network-skel-chip short" />
        <Skeleton className="network-skel-chip short" />
      </div>
      <div className="detail-grid network-host-detail-grid">
        {["Client name", "Network", "Connected", "Last seen", "Signal", "VLAN"].map((label, i) => (
          <div key={label}>
            <div className="dg-k">{label}</div>
            <Skeleton className={i % 2 === 0 ? "network-skel-line" : "network-skel-line short"} />
          </div>
        ))}
      </div>
      <div className="network-connection">
        <div className="dg-k">Connection</div>
        <Skeleton className="network-skel-line" />
        <Skeleton className="network-skel-line short" />
      </div>
      <div className="detail-grid network-host-detail-grid">
        {["Download", "Upload", "RX total", "TX total"].map((label) => (
          <div key={label}>
            <div className="dg-k">{label}</div>
            <Skeleton className="network-skel-line short" />
          </div>
        ))}
      </div>
    </div>
  );
}
