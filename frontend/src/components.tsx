// Cybex Sentinel — shared UI primitives + icon set.
import React from "react";
import type { Kpi, SourceHealth } from "./api";

// ── Icon set ────────────────────────────────────────────────
const ICONS: Record<string, React.ReactElement> = {
  dashboard: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <rect x="2" y="2" width="5.5" height="6" rx="1" />
      <rect x="2" y="10" width="5.5" height="4" rx="1" />
      <rect x="9" y="2" width="5" height="4" rx="1" />
      <rect x="9" y="8" width="5" height="6" rx="1" />
    </svg>
  ),
  network: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <circle cx="8" cy="3" r="1.5" />
      <circle cx="3" cy="12" r="1.5" />
      <circle cx="13" cy="12" r="1.5" />
      <path d="M8 4.5v6M8 10.5l-4.5 1M8 10.5l4.5 1" strokeLinecap="round" />
    </svg>
  ),
  server: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <rect x="2" y="2" width="12" height="4.5" rx="1" />
      <rect x="2" y="9" width="12" height="4.5" rx="1" />
      <circle cx="5" cy="4.25" r=".5" fill="currentColor" />
      <circle cx="5" cy="11.25" r=".5" fill="currentColor" />
    </svg>
  ),
  alert: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M8 2 L14 13 H2 Z" strokeLinejoin="round" />
      <path d="M8 6.5v3.5M8 12v.01" strokeLinecap="round" />
    </svg>
  ),
  logs: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <rect x="2.5" y="2" width="11" height="12" rx="1" />
      <path d="M5 5.5h6M5 8h6M5 10.5h4" strokeLinecap="round" />
    </svg>
  ),
  settings: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <circle cx="8" cy="8" r="2" />
      <path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.5 3.5l1.4 1.4M11.1 11.1l1.4 1.4M3.5 12.5l1.4-1.4M11.1 4.9l1.4-1.4" strokeLinecap="round" />
    </svg>
  ),
  search: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <circle cx="7" cy="7" r="4.5" />
      <path d="M10.5 10.5L14 14" strokeLinecap="round" />
    </svg>
  ),
  bell: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M4 11V7a4 4 0 0 1 8 0v4l1 1.5H3L4 11Z" strokeLinejoin="round" />
      <path d="M6.5 13.5a1.5 1.5 0 0 0 3 0" />
    </svg>
  ),
  refresh: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M3 8a5 5 0 0 1 9-3M13 8a5 5 0 0 1-9 3" strokeLinecap="round" />
      <path d="M12 2v3h-3M4 14v-3h3" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  chevron: (
    <svg viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" strokeWidth="1.8">
      <path d="M4 6l4 4 4-4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  download: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M8 2v8M5 7l3 3 3-3M3 13h10" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  upload: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M8 10V2M5 5l3-3 3 3M3 13h10" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  ap: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4">
      <path d="M3 6a7 7 0 0 1 10 0M5 8a4.5 4.5 0 0 1 6 0M7 10a2 2 0 0 1 2 0" strokeLinecap="round" />
      <circle cx="8" cy="12.5" r="1" fill="currentColor" stroke="none" />
    </svg>
  ),
  switch: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4">
      <rect x="2" y="5" width="12" height="6" rx="1" />
      <path d="M4.5 8h.01M6.5 8h.01M8.5 8h.01M10.5 8h.01M12.5 8h.01" strokeLinecap="round" strokeWidth="1.8" />
    </svg>
  ),
  gateway: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4">
      <rect x="2" y="3" width="12" height="10" rx="1.5" />
      <path d="M2 8h12M5 5.5v2M11 9v2" strokeLinecap="round" />
    </svg>
  ),
  camera: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4">
      <rect x="2" y="4" width="9" height="8" rx="1" />
      <path d="M11 7l3-1.5v5L11 9" strokeLinejoin="round" />
    </svg>
  ),
  ups: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4">
      <rect x="3" y="2" width="10" height="12" rx="1.5" />
      <path d="M8 4.5l-2 4h4l-2 4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  more: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
      <circle cx="4" cy="8" r="1.2" />
      <circle cx="8" cy="8" r="1.2" />
      <circle cx="12" cy="8" r="1.2" />
    </svg>
  ),
  power: (
    <svg viewBox="0 0 16 16" width="11" height="11" fill="none" stroke="currentColor" strokeWidth="1.6">
      <path d="M8 2v5" strokeLinecap="round" />
      <path d="M4.5 4.5a5 5 0 1 0 7 0" strokeLinecap="round" />
    </svg>
  ),
};

export function Icon({ name, ...rest }: { name: string } & React.HTMLAttributes<HTMLSpanElement>) {
  const el = ICONS[name];
  if (!el) return null;
  return <span {...rest}>{el}</span>;
}

// ── Sidebar ─────────────────────────────────────────────────
export function Sidebar({
  page,
  onNavigate,
  alertCount,
}: {
  page: string;
  onNavigate: (p: string) => void;
  alertCount: number;
}) {
  const items = [
    { id: "dashboard", label: "Dashboard", icon: "dashboard" },
    { id: "unifi", label: "UniFi Network", icon: "network" },
    { id: "proxmox", label: "Proxmox", icon: "server" },
  ];
  const utilities = [
    { id: "alerts", label: "Alerts", icon: "alert", badge: alertCount },
    { id: "logs", label: "Events & Logs", icon: "logs" },
  ];
  return (
    <aside className="sidebar">
      <div className="brand">
        <div className="brand-mark" />
        <div>
          <div className="brand-name">Cybex Sentinel</div>
          <div className="brand-tag">live monitor</div>
        </div>
      </div>

      <div className="nav-label">Monitoring</div>
      {items.map((it) => (
        <button
          key={it.id}
          className={"nav-item" + (page === it.id ? " active" : "")}
          onClick={() => onNavigate(it.id)}
        >
          <span className="nav-icon">
            <Icon name={it.icon} />
          </span>
          {it.label}
        </button>
      ))}

      <div className="nav-label">Operations</div>
      {utilities.map((it) => (
        <button
          key={it.id}
          className={"nav-item" + (page === it.id ? " active" : "")}
          onClick={() => onNavigate(it.id)}
        >
          <span className="nav-icon">
            <Icon name={it.icon} />
          </span>
          {it.label}
          {it.badge ? <span className="nav-badge">{it.badge}</span> : null}
        </button>
      ))}
      <button
        className={"nav-item" + (page === "settings" ? " active" : "")}
        onClick={() => onNavigate("settings")}
      >
        <span className="nav-icon">
          <Icon name="settings" />
        </span>
        Settings
      </button>

      <div className="sidebar-footer">
        <div className="avatar">JP</div>
        <div style={{ minWidth: 0, flex: 1 }}>
          <div style={{ fontSize: 12, fontWeight: 500 }}>J. Pals</div>
          <div style={{ fontFamily: "var(--mono)", fontSize: 10, color: "var(--fg-3)" }}>noc · admin</div>
        </div>
      </div>
    </aside>
  );
}

// ── Topbar ──────────────────────────────────────────────────
export function Topbar({
  crumb,
  title,
  sources,
  pollSec,
  staleSec,
  alertCount,
  onRefresh,
  onSettings,
  onAlerts,
  actions,
}: {
  crumb: string;
  title: string;
  sources: SourceHealth[];
  pollSec: number;
  staleSec: number;
  alertCount: number;
  onRefresh: () => void;
  onSettings: () => void;
  onAlerts: () => void;
  actions?: React.ReactNode;
}) {
  const stale = staleSec > Math.max(20, pollSec * 3);
  return (
    <header className="topbar">
      <div>
        <div className="crumb">{crumb}</div>
        <div className="page-title">{title}</div>
      </div>
      <div className="topbar-spacer" />
      <div className="src-health">
        {sources.map((s) => (
          <span
            key={s.name}
            className={"src-pill " + (s.ok ? "ok" : "down")}
            title={s.ok ? s.detail : s.error || "unreachable"}
          >
            <span className={"status-dot " + (s.ok ? "ok" : "crit")} />
            {s.name}
          </span>
        ))}
      </div>
      <span className={"live-pill" + (stale ? " stale" : "")}>
        <span className="live-dot" />
        {stale ? `Stale · ${staleSec}s` : `Live · ${pollSec}s polling`}
      </span>
      <button
        className="icon-btn"
        title={alertCount > 0 ? `${alertCount} open alert(s)` : "Alerts"}
        onClick={onAlerts}
      >
        <Icon name="bell" />
        {alertCount > 0 && <span className="dot" />}
      </button>
      <button className="icon-btn" title="Refresh now" onClick={onRefresh}>
        <Icon name="refresh" />
      </button>
      <button className="icon-btn" title="Settings" onClick={onSettings}>
        <Icon name="settings" />
      </button>
      {actions}
    </header>
  );
}

// ── Card ────────────────────────────────────────────────────
export function Card({
  title,
  sub,
  actions,
  children,
  tight,
  style,
}: {
  title?: React.ReactNode;
  sub?: React.ReactNode;
  actions?: React.ReactNode;
  children?: React.ReactNode;
  tight?: boolean;
  style?: React.CSSProperties;
}) {
  return (
    <section className="card" style={style}>
      {(title || sub || actions) && (
        <div className="card-hd">
          {title && <div className="card-title">{title}</div>}
          {sub && <div className="card-sub">{sub}</div>}
          {actions && <div className="card-actions">{actions}</div>}
        </div>
      )}
      <div className={"card-body" + (tight ? " tight" : "")}>{children}</div>
    </section>
  );
}

// ── Chip ────────────────────────────────────────────────────
export function Chip({
  tone = "default",
  children,
  dot,
}: {
  tone?: string;
  children?: React.ReactNode;
  dot?: boolean;
}) {
  return (
    <span className={"chip" + (tone !== "default" ? " " + tone : "")}>
      {dot && <span className={"status-dot " + tone} />}
      {children}
    </span>
  );
}

export function StatusDot({ tone }: { tone: string }) {
  return <span className={"status-dot " + tone} />;
}

// ── Sparkline ───────────────────────────────────────────────
export function Sparkline({
  data,
  width = 96,
  height = 28,
  color = "var(--accent)",
  area = true,
  strokeWidth = 1.4,
}: {
  data: number[];
  width?: number;
  height?: number;
  color?: string;
  area?: boolean;
  strokeWidth?: number;
}) {
  if (!data || !data.length) return null;
  const min = Math.min(...data);
  const max = Math.max(...data);
  const range = max - min || 1;
  const pts = data.map((v, i) => {
    const x = data.length === 1 ? width / 2 : (i / (data.length - 1)) * (width - 2) + 1;
    const y = height - 1 - ((v - min) / range) * (height - 2);
    return [x, y] as [number, number];
  });
  const d = pts.map(([x, y], i) => (i ? "L" : "M") + x.toFixed(1) + " " + y.toFixed(1)).join(" ");
  const areaD = d + ` L${width - 1} ${height - 1} L1 ${height - 1} Z`;
  const id = "sg-" + Math.random().toString(36).slice(2, 8);
  return (
    <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`}>
      {area && (
        <>
          <defs>
            <linearGradient id={id} x1="0" x2="0" y1="0" y2="1">
              <stop offset="0%" stopColor={color} stopOpacity="0.4" />
              <stop offset="100%" stopColor={color} stopOpacity="0" />
            </linearGradient>
          </defs>
          <path d={areaD} fill={`url(#${id})`} />
        </>
      )}
      <path d={d} fill="none" stroke={color} strokeWidth={strokeWidth} strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

// ── KPI tile ────────────────────────────────────────────────
export function KpiTile({
  label,
  kpi,
  sparkColor,
}: {
  label: string;
  kpi: Kpi;
  sparkColor?: string;
}) {
  return (
    <div className="kpi">
      <div className="kpi-label">{label}</div>
      <div className="kpi-val">
        {kpi.display}
        {kpi.unit && <span className="unit">{kpi.unit}</span>}
      </div>
      <div className="kpi-sub">
        {kpi.trend !== 0 && (
          <span className={"kpi-trend " + (kpi.trend >= 0 ? "up" : "down")}>
            {kpi.trend >= 0 ? "▲" : "▼"} {Math.abs(kpi.trend)}
          </span>
        )}
        {kpi.sub}
      </div>
      {kpi.spark && kpi.spark.length > 0 && (
        <div className="kpi-spark">
          <Sparkline data={kpi.spark} width={88} height={28} color={sparkColor || "var(--accent)"} />
        </div>
      )}
    </div>
  );
}

const KPI_COLORS = [
  "oklch(0.76 0.16 152)",
  "oklch(0.78 0.13 232)",
  "oklch(0.7 0.21 26)",
  "var(--accent)",
];

export function KpiGrid({ kpis, labels }: { kpis: Kpi[]; labels: string[] }) {
  return (
    <div className="kpi-grid">
      {labels.map((label, i) => (
        <KpiTile key={label} label={label} kpi={kpis[i] || EMPTY_KPI} sparkColor={KPI_COLORS[i % 4]} />
      ))}
    </div>
  );
}

const EMPTY_KPI: Kpi = { display: "—", unit: "", sub: "", trend: 0, spark: [] };

// ── Bar (utilization) ───────────────────────────────────────
export function Bar({
  label,
  value,
  unit = "%",
  tone,
  max = 100,
}: {
  label: string;
  value: number;
  unit?: string;
  tone?: string;
  max?: number;
}) {
  const pct = Math.min(100, (value / max) * 100);
  const auto = pct > 85 ? "crit" : pct > 70 ? "warn" : "ok";
  const t = tone || auto;
  return (
    <div>
      <div className="bar-label">
        <span>{label}</span>
        <span className="bar-val">
          {value}
          {unit}
        </span>
      </div>
      <div className="bar-track">
        <div className={"bar-fill " + t} style={{ width: pct + "%" }} />
      </div>
    </div>
  );
}

// ── Mini bar (compact metric, used in tables) ───────────────
export function MiniBar({ value, max = 100, tone }: { value: number; max?: number; tone?: string }) {
  const pct = Math.min(100, (value / max) * 100);
  const auto = pct > 85 ? "crit" : pct > 70 ? "warn" : "ok";
  return (
    <div className="metric-mini">
      <span style={{ width: 30, textAlign: "right" }}>{value}%</span>
      <div className="mm-bar">
        <div className={"mm-fill " + (tone || auto)} style={{ width: pct + "%" }} />
      </div>
    </div>
  );
}

// ── Bandwidth chart (dual area) ─────────────────────────────
export function BandwidthChart({
  down,
  up,
  height = 220,
}: {
  down: number[];
  up: number[];
  height?: number;
}) {
  const W = 760;
  const H = height;
  const padL = 40;
  const padR = 8;
  const padT = 12;
  const padB = 22;
  const n = down.length;
  const allMax = Math.max(...down, ...up, 1);
  const yMax = niceMax(allMax);
  const xAt = (i: number) => padL + (n <= 1 ? 0.5 : i / (n - 1)) * (W - padL - padR);
  const yAt = (v: number) => H - padB - (v / yMax) * (H - padT - padB);
  const pathFor = (arr: number[]) =>
    arr.map((v, i) => (i ? "L" : "M") + xAt(i).toFixed(1) + " " + yAt(v).toFixed(1)).join(" ");
  const areaFor = (arr: number[]) =>
    pathFor(arr) + ` L${xAt(n - 1).toFixed(1)} ${H - padB} L${padL} ${H - padB} Z`;
  const gridY = [0, 0.25, 0.5, 0.75, 1].map((p) => yMax - p * yMax);
  return (
    <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none">
      <defs>
        <linearGradient id="bw-down" x1="0" x2="0" y1="0" y2="1">
          <stop offset="0%" stopColor="var(--accent)" stopOpacity="0.45" />
          <stop offset="100%" stopColor="var(--accent)" stopOpacity="0" />
        </linearGradient>
        <linearGradient id="bw-up" x1="0" x2="0" y1="0" y2="1">
          <stop offset="0%" stopColor="var(--accent-2)" stopOpacity="0.38" />
          <stop offset="100%" stopColor="var(--accent-2)" stopOpacity="0" />
        </linearGradient>
      </defs>
      {gridY.map((v, i) => (
        <g key={i}>
          <line
            x1={padL}
            x2={W - padR}
            y1={yAt(v)}
            y2={yAt(v)}
            stroke="var(--line)"
            strokeWidth="1"
            strokeDasharray={i === 4 ? "0" : "2 4"}
          />
          <text x={padL - 6} y={yAt(v) + 3} fontSize="9.5" textAnchor="end" fill="var(--fg-3)" fontFamily="var(--mono)">
            {fmtAxis(v)}
          </text>
        </g>
      ))}
      <path d={areaFor(down)} fill="url(#bw-down)" />
      <path d={areaFor(up)} fill="url(#bw-up)" />
      <path d={pathFor(down)} fill="none" stroke="var(--accent)" strokeWidth="1.6" strokeLinejoin="round" />
      <path d={pathFor(up)} fill="none" stroke="var(--accent-2)" strokeWidth="1.6" strokeLinejoin="round" />
      <line
        x1={xAt(n - 1)}
        x2={xAt(n - 1)}
        y1={padT}
        y2={H - padB}
        stroke="var(--accent)"
        strokeWidth="1"
        strokeDasharray="2 3"
        opacity="0.5"
      />
      <circle cx={xAt(n - 1)} cy={yAt(down[n - 1])} r="3.5" fill="var(--accent)" stroke="var(--bg)" strokeWidth="1.5" />
      <circle cx={xAt(n - 1)} cy={yAt(up[n - 1])} r="3.5" fill="var(--accent-2)" stroke="var(--bg)" strokeWidth="1.5" />
    </svg>
  );
}

function niceMax(v: number): number {
  if (v <= 10) return Math.ceil(v / 2) * 2 || 2;
  if (v <= 100) return Math.ceil(v / 20) * 20;
  if (v <= 1000) return Math.ceil(v / 200) * 200;
  return Math.ceil(v / 500) * 500;
}

function fmtAxis(v: number): string {
  if (v >= 1000) return (v / 1000).toFixed(1) + "G";
  return String(Math.round(v));
}

// ── Throughput formatter ────────────────────────────────────
export function fmtMbps(mbps: number | null | undefined): string {
  if (mbps == null) return "—";
  if (mbps >= 1000) return (mbps / 1000).toFixed(2) + " Gbps";
  if (mbps >= 10) return Math.round(mbps) + " Mbps";
  return mbps.toFixed(1) + " Mbps";
}
