// API types + polling hook for the Cybex Sentinel backend.
import { useCallback, useEffect, useRef, useState } from "react";

export interface Kpi {
  display: string;
  unit: string;
  sub: string;
  trend: number;
  spark: number[];
}

export interface SourceHealth {
  name: string;
  kind: string;
  ok: boolean;
  detail: string;
  error: string | null;
}

export interface GuestCount {
  vm: number;
  lxc: number;
}

export interface NodeTile {
  name: string;
  server: string;
  host: string;
  status: string;
  cpu: number;
  mem: number;
  disk: number;
  net: number;
  netMbps: number;
  guests: GuestCount;
  model: string;
  uptime: string;
}

export interface Guest {
  id: number;
  kind: string;
  name: string;
  status: string;
  cpu: number;
  mem: number;
  disk: number;
  net: number;
  uptime: string;
  tags: string;
  cores: number;
  ram: string;
  node: string;
  server: string;
}

export interface NodeGuests {
  node: string;
  server: string;
  guests: Guest[];
}

export interface Issue {
  sev: string;
  title: string;
  source: string;
  time: string;
}

export interface BandwidthSeries {
  down: number[];
  up: number[];
  points: number;
  windowLabel: string;
  peakDown: number;
  peakUp: number;
  avg: number;
  transferredGb: number;
}

export interface TopoCounts {
  router: number;
  sw: number;
  ap: number;
  ok: number;
  warn: number;
  crit: number;
  total: number;
}

export interface TopoNode {
  kind: string;
  id: string;
  name: string;
  model: string;
  ip: string;
  status: string;
  clients: number;
  ports: string;
  wan: string;
  children: TopoNode[];
}

export interface Dashboard {
  kpis: Kpi[];
  issues: Issue[];
  bandwidth: BandwidthSeries;
  nodes: NodeTile[];
  topologyCounts: TopoCounts;
  totalGuests: number;
  quorum: string;
}

export interface ProxmoxView {
  kpis: Kpi[];
  nodes: NodeTile[];
  guests: NodeGuests[];
  highCpu: number;
  highMem: number;
  running: number;
  stopped: number;
}

export interface PortOut {
  idx: number;
  up: boolean;
  poe: boolean;
  speedMbps: number;
  connector: string;
}

export interface RadioOut {
  band: string;
  channel: number;
  width: number;
  standard: string;
}

export interface UniDevice {
  id: string;
  name: string;
  kind: string;
  model: string;
  ip: string;
  mac: string;
  status: string;
  uptime: string;
  clients: number;
  txMbps: number;
  rxMbps: number;
  fw: string;
  site: string;
  cpu: number;
  mem: number;
  firmwareUpdatable: boolean;
  ports: PortOut[];
  radios: RadioOut[];
}

export interface UnifiView {
  kpis: Kpi[];
  devices: UniDevice[];
  poeActive: number;
  poeCapable: number;
  wirelessClients: number;
  wiredClients: number;
}

export interface Alert {
  id: string;
  sev: string;
  status: string;
  title: string;
  desc: string;
  source: string;
  host: string;
  target: string;
  ageMin: number;
  occurrences: number;
  assignee: string | null;
  rule: string;
}

export interface AlertsView {
  kpis: Kpi[];
  alerts: Alert[];
  histogram: number[];
}

export interface SentinelEvent {
  ts: string;
  time: string;
  level: string;
  source: string;
  sourceKind: string;
  target: string;
  msg: string;
}

export interface EventsView {
  kpis: Kpi[];
  events: SentinelEvent[];
  rate: number[];
}

export interface Snapshot {
  generatedAt: string;
  pollIntervalSec: number;
  sources: SourceHealth[];
  dashboard: Dashboard;
  proxmox: ProxmoxView;
  unifi: UnifiView;
  topology: TopoNode;
  alerts: AlertsView;
  events: EventsView;
}

const POLL_MS = 5000;

export interface SnapshotState {
  snap: Snapshot | null;
  /** True once at least one successful fetch returned real data. */
  ready: boolean;
  error: string | null;
  /** Seconds since the last successful fetch. */
  staleSec: number;
  refresh: () => void;
}

/** Polls `/api/snapshot` every 5s and exposes the latest snapshot. */
export function useSnapshot(): SnapshotState {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [staleSec, setStaleSec] = useState(0);
  const lastOk = useRef<number>(0);

  const fetchSnapshot = useCallback(async () => {
    try {
      const r = await fetch("/api/snapshot", { cache: "no-store" });
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      const data: Snapshot = await r.json();
      setSnap(data);
      setError(null);
      lastOk.current = Date.now();
    } catch (e: any) {
      setError(String(e?.message ?? e));
    }
  }, []);

  useEffect(() => {
    fetchSnapshot();
    const poll = setInterval(fetchSnapshot, POLL_MS);
    const tick = setInterval(() => {
      setStaleSec(lastOk.current ? Math.floor((Date.now() - lastOk.current) / 1000) : 0);
    }, 1000);
    return () => {
      clearInterval(poll);
      clearInterval(tick);
    };
  }, [fetchSnapshot]);

  const ready = !!snap && !!snap.generatedAt;
  return { snap, ready, error, staleSec, refresh: fetchSnapshot };
}

/** Apply an acknowledge / resolve / reopen action to an alert. */
export async function alertAction(id: string, action: string): Promise<void> {
  await fetch("/api/alerts/action", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ id, action }),
  });
}
