// TypeScript view of the Sentinel JSON API contract.
//
// Keep this file focused on data shape only. Fetching, polling and mutations
// live in api.ts.

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

export interface SnapshotState {
  snap: Snapshot | null;
  /** True once at least one successful fetch returned real data. */
  ready: boolean;
  error: string | null;
  /** Seconds since the last successful fetch. */
  staleSec: number;
  refresh: () => void;
}

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

export interface AuthStatus {
  /** No account exists yet — the frontend should show first-user setup. */
  needsFirstUser: boolean;
  /** The browser currently holds a valid session. */
  authenticated: boolean;
  username: string | null;
}
