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
  stale: boolean;
  failureCount: number;
  retryInSec: number | null;
  lastOkAgoSec: number | null;
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
  id: string;
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
  quorum: string | null;
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

export interface UnraidDisk {
  id: string;
  name: string;
  kind: string;
  device: string;
  status: string;
  temp: string;
  size: string;
  used: string;
  usedPct: number;
  spinning: boolean | null;
}

export interface UnraidStorage {
  id: string;
  name: string;
  kind: string;
  status: string;
  used: string;
  total: string;
  usedPct: number;
  members: number;
  temp: string;
}

export interface UnraidContainer {
  id: string;
  name: string;
  image: string;
  state: string;
  status: string;
  cpu: number;
  mem: number;
  memory: string;
  netIo: string;
  blockIo: string;
  autoStart: boolean;
  updateAvailable: boolean;
  updateStatus: string;
  rootFs: string;
  writable: string;
  logSize: string;
  network: string;
  ports: string;
  orphaned: boolean;
}

export interface UnraidVm {
  id: string;
  name: string;
  state: string;
}

export interface UnraidNotification {
  title: string;
  importance: string;
  time: string;
}

export interface UnraidServer {
  name: string;
  source: string;
  host: string;
  status: string;
  lanIp: string;
  localUrl: string;
  version: string;
  apiVersion: string;
  kernel: string;
  uptime: string;
  cpuBrand: string;
  cpuCores: number;
  cpuThreads: number;
  cpu: number;
  mem: number;
  memory: string;
  temp: string;
  tempSensor: string;
  arrayState: string;
  arrayUsed: string;
  arrayTotal: string;
  arrayUsedPct: number;
  storageUsed: string;
  storageTotal: string;
  storageUsedPct: number;
  diskCount: number;
  parityStatus: string;
  parityProgress: number;
  parityErrors: number;
  containersRunning: number;
  containersTotal: number;
  vmsRunning: number;
  vmsTotal: number;
  notificationCount: number;
  softwareUpdateCount: number;
  storage: UnraidStorage[];
  disks: UnraidDisk[];
  containers: UnraidContainer[];
  vms: UnraidVm[];
  notifications: UnraidNotification[];
}

export interface UnraidView {
  kpis: Kpi[];
  servers: UnraidServer[];
  containersRunning: number;
  containersTotal: number;
  vmsRunning: number;
  vmsTotal: number;
  arrayWarn: number;
  softwareUpdateCount: number;
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
  unraid: UnraidView;
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
  layouts: EditableLayoutStore;
}

export interface EditableLayoutItem {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
  hidden?: boolean;
}

export interface EditableLayoutValue {
  version: number;
  items: EditableLayoutItem[];
  updatedAt?: string;
}

export type EditableLayoutStore = Record<string, EditableLayoutValue>;

export interface EmailNotificationSettings {
  enabled: boolean;
  smtpHost: string;
  smtpPort: number;
  smtpUsername: string;
  hasPassword: boolean;
  smtpSecurity: string;
  from: string;
  to: string[];
}

export interface SlackNotificationSettings {
  enabled: boolean;
  hasWebhookUrl: boolean;
}

export interface TelegramNotificationSettings {
  enabled: boolean;
  hasBotToken: boolean;
  chatId: string;
}

export interface PushNotificationSettings {
  enabled: boolean;
  configured: boolean;
  publicKey: string | null;
  vapidSubject: string;
}

export interface NotificationSettings {
  minSeverity: string;
  email: EmailNotificationSettings;
  slack: SlackNotificationSettings;
  telegram: TelegramNotificationSettings;
  push: PushNotificationSettings;
}

export interface PushStatus {
  enabled: boolean;
  configured: boolean;
  publicKey: string;
  subscriptionCount: number;
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
  notifications: NotificationSettings;
  networkScanner: NetworkScannerSettings;
}

export type DiscoveryMethod = "auto" | "arp" | "icmpTcp";
export type PortProfile = "fast" | "top100" | "top1000" | "custom";
export type PortScanTechnique = "syn" | "connect";

export interface NetworkDiscoverySettings {
  method: DiscoveryMethod;
  dnsResolution: boolean;
  dnsServers: string[];
  maxRetries: number;
  hostTimeoutMs: number;
  overallTimeoutSec: number;
  timingTemplate: number;
  minRate: number;
}

export interface NetworkPortScanSettings {
  enabled: boolean;
  profile: PortProfile;
  ports: string;
  serviceDetection: boolean;
  osDetection: boolean;
  scanTechnique: PortScanTechnique;
  udpScan: boolean;
  onlyScanDiscovered: boolean;
  skipHostDiscovery: boolean;
}

export interface NetworkScanSchedule {
  enabled: boolean;
  intervalMinutes: number;
  runAtStart: boolean;
}

export interface NetworkScannerSettings {
  enabled: boolean;
  ranges: string[];
  exclude: string[];
  discovery: NetworkDiscoverySettings;
  portScan: NetworkPortScanSettings;
  schedule: NetworkScanSchedule;
  retentionDays: number;
}

export interface NetworkScanSummary {
  scanner: string;
  rangeCount: number;
  hostsUp: number;
  openPorts: number;
  durationMs: number;
  discoveryMethod: DiscoveryMethod;
  portScanEnabled: boolean;
}

export interface NetworkScanJob {
  id: number;
  status: string;
  trigger: string;
  summary: NetworkScanSummary | null;
  error: string | null;
  createdAt: string;
  startedAt: string | null;
  finishedAt: string | null;
}

export interface NetworkScanPort {
  protocol: string;
  port: number;
  state: string;
  service: string | null;
  product: string | null;
  version: string | null;
}

export interface NetworkScanDevice {
  id: number;
  jobId: number | null;
  ip: string;
  hostname: string | null;
  mac: string | null;
  vendor: string | null;
  status: string;
  discoveryMethod: string;
  latencyMs: number | null;
  ports: NetworkScanPort[];
  osGuess: string | null;
  firstSeen: string | null;
  lastSeen: string;
}

export interface NetworkHostTraffic {
  txMbps: number | null;
  rxMbps: number | null;
  txBytes: number | null;
  rxBytes: number | null;
}

export interface NetworkHostUnifiDevice {
  id: string;
  name: string;
  kind: string;
  model: string;
  ip: string;
  mac: string;
  state: string;
  site: string;
  txMbps: number;
  rxMbps: number;
  clients: number;
  cpu: number;
  mem: number;
  firmware: string;
}

export interface NetworkHostUnifiClient {
  id: string | null;
  name: string | null;
  kind: string | null;
  ip: string | null;
  mac: string | null;
  networkName: string | null;
  ssid: string | null;
  connectedAt: string | null;
  lastSeenAt: string | null;
  signal: number | null;
  channel: number | null;
  vlanId: number | null;
}

export interface NetworkHostConnection {
  connectionType: string;
  uplinkDevice: NetworkHostUnifiDevice | null;
  uplinkDeviceId: string | null;
  uplinkDeviceName: string | null;
  portIdx: number | null;
  portName: string | null;
  portState: string | null;
  portSpeedMbps: number | null;
  poe: boolean | null;
}

export interface NetworkHostUnifi {
  configured: boolean;
  site: string | null;
  appVersion: string | null;
  matchedBy: string | null;
  client: NetworkHostUnifiClient | null;
  device: NetworkHostUnifiDevice | null;
  connection: NetworkHostConnection | null;
  traffic: NetworkHostTraffic | null;
  error: string | null;
}

export interface NetworkHostDetail {
  target: string;
  settings: NetworkScannerSettings;
  activeJob: NetworkScanJob | null;
  host: NetworkScanDevice | null;
  observations: NetworkScanDevice[];
  unifi: NetworkHostUnifi | null;
}

export interface NetworkScannerOverview {
  settings: NetworkScannerSettings;
  activeJob: NetworkScanJob | null;
  latestJob: NetworkScanJob | null;
  jobs: NetworkScanJob[];
  devices: NetworkScanDevice[];
  inventory: NetworkScanDevice[];
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

export interface UnraidSource {
  id: number;
  name: string;
  host: string;
  /** True if an API key is stored — the key itself is never sent to the UI. */
  hasSecret: boolean;
  enabled: boolean;
}

export interface SourcesData {
  unifi: UnifiSource[];
  proxmox: ProxmoxSource[];
  unraid: UnraidSource[];
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
