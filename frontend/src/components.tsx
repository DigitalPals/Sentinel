// Cybex Sentinel — shared UI primitives.
import React from "react";
import type { Kpi, SourceHealth } from "./api-types";
import logoUrl from "./assets/logo.svg";
import { Sparkline } from "./charts";
import { Icon } from "./icons";

export { BandwidthChart, fmtMbps, Sparkline } from "./charts";
export { Icon } from "./icons";

/** Single-letter avatar initial derived from a username. */
function initial(name: string): string {
  return (name.trim()[0] ?? "").toUpperCase();
}

export function Sidebar({
  page,
  onNavigate,
  alertCount,
  username,
  open = false,
  onClose,
}: {
  page: string;
  onNavigate: (p: string) => void;
  alertCount: number;
  username: string;
  open?: boolean;
  onClose?: () => void;
}) {
  const items = [
    { id: "dashboard", label: "Dashboard", icon: "dashboard" },
    { id: "unifi", label: "UniFi Network", icon: "unifi" },
    { id: "network-scanner", label: "Network Scanner", icon: "scan" },
    { id: "proxmox", label: "Proxmox", icon: "proxmox" },
    { id: "unraid", label: "Unraid", icon: "unraid" },
  ];
  const utilities = [
    { id: "alerts", label: "Alerts", icon: "alert", badge: alertCount },
    { id: "logs", label: "Events & Logs", icon: "logs" },
  ];
  const go = (id: string) => {
    onNavigate(id);
    onClose?.();
  };
  return (
    <aside
      className={"sidebar" + (open ? " open" : "")}
      id="app-sidebar"
      aria-label="Primary navigation"
    >
      <div className="brand">
        <img src={logoUrl} alt="Cybex Sentinel" className="brand-logo" />
        <span className="brand-mark" aria-hidden="true">S</span>
      </div>

      <div className="nav-label">Monitoring</div>
      {items.map((it) => (
        <button
          key={it.id}
          className={
            "nav-item" +
            (page === it.id || (page === "network-host" && it.id === "network-scanner") ? " active" : "")
          }
          onClick={() => go(it.id)}
          title={it.label}
        >
          <span className="nav-icon">
            <Icon name={it.icon} />
          </span>
          <span className="nav-text">{it.label}</span>
        </button>
      ))}

      <div className="nav-label">Operations</div>
      {utilities.map((it) => (
        <button
          key={it.id}
          className={"nav-item" + (page === it.id ? " active" : "")}
          onClick={() => go(it.id)}
          title={it.label}
        >
          <span className="nav-icon">
            <Icon name={it.icon} />
          </span>
          <span className="nav-text">{it.label}</span>
          {it.badge ? <span className="nav-badge">{it.badge}</span> : null}
        </button>
      ))}
      <button
        className={"nav-item" + (page === "settings" ? " active" : "")}
        onClick={() => go("settings")}
        title="Settings"
      >
        <span className="nav-icon">
          <Icon name="settings" />
        </span>
        <span className="nav-text">Settings</span>
      </button>

      <div className="sidebar-footer">
        <div className="avatar">{initial(username) || "?"}</div>
        <div className="sidebar-user" style={{ minWidth: 0, flex: 1 }}>
          <div
            style={{ fontSize: 12, fontWeight: 500, overflow: "hidden", textOverflow: "ellipsis" }}
            title={username}
          >
            {username || "—"}
          </div>
          <div style={{ fontFamily: "var(--mono)", fontSize: 10, color: "var(--fg-3)" }}>administrator</div>
        </div>
      </div>
    </aside>
  );
}

export function Topbar({
  onMenu,
  crumb,
  title,
  sources,
  pollSec,
  staleSec,
  alertCount,
  onRefresh,
  onSettings,
  onAlerts,
  onLogout,
  actions,
}: {
  onMenu?: () => void;
  crumb: string;
  title: string;
  sources: SourceHealth[];
  pollSec: number;
  staleSec: number;
  alertCount: number;
  onRefresh: () => void;
  onSettings: () => void;
  onAlerts: () => void;
  onLogout?: () => void;
  actions?: React.ReactNode;
}) {
  const stale = staleSec > Math.max(20, pollSec * 3);
  return (
    <header className="topbar">
      {onMenu && (
        <button
          className="icon-btn menu-btn"
          title="Toggle navigation"
          aria-controls="app-sidebar"
          onClick={onMenu}
        >
          <Icon name="menu" />
        </button>
      )}
      <div>
        <div className="crumb">{crumb}</div>
        <div className="page-title">{title}</div>
      </div>
      <div className="topbar-spacer" />
      <div className="src-health">
        {sources.map((s) => (
          <span
            key={s.name}
            className={"src-pill " + (s.ok ? "ok" : s.stale ? "stale" : "down")}
            title={
              s.stale
                ? `${s.detail}${s.retryInSec ? ` · retry in ${s.retryInSec}s` : ""}`
                : s.ok ? s.detail : s.error || "unreachable"
            }
          >
            <span className={"status-dot " + (s.ok ? "ok" : s.stale ? "warn" : "crit")} />
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
      {onLogout && (
        <button className="icon-btn" title="Sign out" onClick={onLogout}>
          <Icon name="power" />
        </button>
      )}
      {actions}
    </header>
  );
}

export function Card({
  title,
  sub,
  actions,
  children,
  className,
  tight,
  style,
}: {
  title?: React.ReactNode;
  sub?: React.ReactNode;
  actions?: React.ReactNode;
  children?: React.ReactNode;
  className?: string;
  tight?: boolean;
  style?: React.CSSProperties;
}) {
  return (
    <section className={"card" + (className ? " " + className : "")} style={style}>
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

export const KPI_COLORS = [
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
