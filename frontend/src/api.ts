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

const DEFAULT_POLL_MS = 5000;

/** Wrapper around `fetch` for the Sentinel API: always attaches the session
 *  cookie, never caches, and broadcasts a `sentinel-unauthorized` event when a
 *  request is rejected so the app can fall back to the login screen. */
export async function apiFetch(path: string, init?: RequestInit): Promise<Response> {
  const r = await fetch(path, { cache: "no-store", credentials: "same-origin", ...init });
  if (r.status === 401 && !path.startsWith("/api/auth/")) {
    window.dispatchEvent(new Event("sentinel-unauthorized"));
  }
  return r;
}

export interface SnapshotState {
  snap: Snapshot | null;
  /** True once at least one successful fetch returned real data. */
  ready: boolean;
  error: string | null;
  /** Seconds since the last successful fetch. */
  staleSec: number;
  refresh: () => void;
}

/** Polls `/api/snapshot` and exposes the latest snapshot. The poll interval is
 *  taken from the server-side `frontendPollMs` setting. */
export function useSnapshot(): SnapshotState {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [staleSec, setStaleSec] = useState(0);
  const [pollMs, setPollMs] = useState(DEFAULT_POLL_MS);
  const lastOk = useRef<number>(0);

  const fetchSnapshot = useCallback(async () => {
    try {
      const r = await apiFetch("/api/snapshot");
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      const data: Snapshot = await r.json();
      setSnap(data);
      setError(null);
      lastOk.current = Date.now();
    } catch (e: any) {
      setError(String(e?.message ?? e));
    }
  }, []);

  // Pick up the configured frontend poll interval.
  useEffect(() => {
    apiFetch("/api/settings")
      .then((r) => (r.ok ? r.json() : null))
      .then((d) => {
        if (d && typeof d.frontendPollMs === "number") setPollMs(d.frontendPollMs);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    fetchSnapshot();
    const poll = setInterval(fetchSnapshot, pollMs);
    const tick = setInterval(() => {
      setStaleSec(lastOk.current ? Math.floor((Date.now() - lastOk.current) / 1000) : 0);
    }, 1000);
    return () => {
      clearInterval(poll);
      clearInterval(tick);
    };
  }, [fetchSnapshot, pollMs]);

  const ready = !!snap && !!snap.generatedAt;
  return { snap, ready, error, staleSec, refresh: fetchSnapshot };
}

/** Apply an acknowledge / resolve / reopen action to an alert. */
export async function alertAction(id: string, action: string): Promise<void> {
  await apiFetch("/api/alerts/action", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ id, action }),
  });
}

// ── Settings & sources ──────────────────────────────────────────────────────

export interface UiPrefs {
  accent: string;
  density: string;
  showSpark: boolean;
}

export interface AppSettings {
  pollIntervalSec: number;
  bind: string;
  httpTimeoutSec: number;
  historyMaxSamples: number;
  historyRetentionDays: number;
  frontendPollMs: number;
  /** Alert thresholds, keyed by rule name (e.g. `guest_mem_crit`). */
  thresholds: Record<string, number>;
  ui: UiPrefs;
}

export interface UnifiSource {
  id: number;
  name: string;
  host: string;
  /** True if an API key is stored — the key itself is never sent to the UI. */
  hasSecret: boolean;
  enabled: boolean;
}

export interface ProxmoxSource {
  id: number;
  name: string;
  host: string;
  tokenId: string;
  /** True if a token secret is stored. */
  hasSecret: boolean;
  enabled: boolean;
}

export interface SourcesData {
  unifi: UnifiSource[];
  proxmox: ProxmoxSource[];
}

export interface TestResult {
  ok: boolean;
  detail: string;
}

/** Parse a JSON response, throwing the API's `error` message on failure. */
async function jsonOrThrow(r: Response): Promise<any> {
  const body = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(body?.error || `HTTP ${r.status}`);
  return body;
}

export async function getSettings(): Promise<AppSettings> {
  return jsonOrThrow(await apiFetch("/api/settings"));
}

export async function putSettings(patch: Record<string, unknown>): Promise<AppSettings> {
  return jsonOrThrow(
    await apiFetch("/api/settings", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(patch),
    }),
  );
}

export async function getSources(): Promise<SourcesData> {
  return jsonOrThrow(await apiFetch("/api/sources"));
}

export async function saveUnifiSource(
  id: number | null,
  body: Record<string, unknown>,
): Promise<void> {
  const url = id == null ? "/api/sources/unifi" : `/api/sources/unifi/${id}`;
  await jsonOrThrow(
    await apiFetch(url, {
      method: id == null ? "POST" : "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }),
  );
}

export async function deleteUnifiSource(id: number): Promise<void> {
  await jsonOrThrow(await apiFetch(`/api/sources/unifi/${id}`, { method: "DELETE" }));
}

export async function saveProxmoxSource(
  id: number | null,
  body: Record<string, unknown>,
): Promise<void> {
  const url = id == null ? "/api/sources/proxmox" : `/api/sources/proxmox/${id}`;
  await jsonOrThrow(
    await apiFetch(url, {
      method: id == null ? "POST" : "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }),
  );
}

export async function deleteProxmoxSource(id: number): Promise<void> {
  await jsonOrThrow(await apiFetch(`/api/sources/proxmox/${id}`, { method: "DELETE" }));
}

export async function testSource(body: Record<string, unknown>): Promise<TestResult> {
  return jsonOrThrow(
    await apiFetch("/api/sources/test", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }),
  );
}

// ── Authentication ──────────────────────────────────────────────────────────

export interface AuthStatus {
  /** No account exists yet — the frontend should show first-user setup. */
  needsFirstUser: boolean;
  /** The browser currently holds a valid session. */
  authenticated: boolean;
  username: string | null;
}

/** Public: whether the app needs first-user setup, a login, or neither. */
export async function getAuthStatus(): Promise<AuthStatus> {
  return jsonOrThrow(await apiFetch("/api/auth/status"));
}

/** Create the first administrator account (only works on a fresh install). */
export async function authSetup(username: string, password: string): Promise<void> {
  await jsonOrThrow(
    await apiFetch("/api/auth/setup", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username, password }),
    }),
  );
}

/** Sign in with an existing account. */
export async function authLogin(username: string, password: string): Promise<void> {
  await jsonOrThrow(
    await apiFetch("/api/auth/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username, password }),
    }),
  );
}

/** End the current session. */
export async function authLogout(): Promise<void> {
  await apiFetch("/api/auth/logout", { method: "POST" });
}
