// Cybex Sentinel — Events & Logs.
import React from "react";
import type { SentinelEvent, Snapshot } from "../api";
import { Card, Icon, KpiGrid } from "../components";

const KPI_LABELS = ["Events Tracked", "Errors", "Backup Jobs", "Top Source"];

export default function Events({ snap }: { snap: Snapshot }) {
  const live = snap.events.events;
  const [level, setLevel] = React.useState({ error: true, warn: true, info: true, debug: true });
  const [source, setSource] = React.useState("all");
  const [query, setQuery] = React.useState("");
  const [paused, setPaused] = React.useState(false);

  // When paused, freeze the displayed feed at its current contents.
  const frozen = React.useRef<SentinelEvent[]>(live);
  if (!paused) frozen.current = live;
  const events = paused ? frozen.current : live;

  const counts = {
    error: events.filter((e) => e.level === "error").length,
    warn: events.filter((e) => e.level === "warn").length,
    info: events.filter((e) => e.level === "info").length,
    debug: events.filter((e) => e.level === "debug").length,
  };

  const visible = events.filter((e) => {
    if (!level[e.level as keyof typeof level]) return false;
    if (source !== "all" && e.sourceKind !== source) return false;
    if (query) {
      const q = query.toLowerCase();
      if (!(e.source + " " + e.target + " " + e.msg).toLowerCase().includes(q)) return false;
    }
    return true;
  });

  const rate = snap.events.rate;
  const rateMax = Math.max(1, ...rate);

  const exportJson = () => {
    const blob = new Blob([JSON.stringify(visible, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "cybex-sentinel-events.json";
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="page">
      <KpiGrid kpis={snap.events.kpis} labels={KPI_LABELS} />

      <Card title="Event Distribution" sub="observed window · events per bucket" tight style={{ marginBottom: 14 }}>
        <div style={{ padding: "16px 18px 12px" }}>
          <div style={{ display: "flex", alignItems: "flex-end", gap: 2, height: 72 }}>
            {rate.map((v, i) => (
              <div
                key={i}
                style={{
                  flex: 1,
                  height: Math.max(2, (v / rateMax) * 64),
                  background: "linear-gradient(180deg, var(--accent) 0%, color-mix(in oklch, var(--accent) 30%, transparent) 100%)",
                  borderRadius: "2px 2px 0 0",
                  opacity: 0.6 + (i / rate.length) * 0.4,
                }}
                title={`${v} events`}
              />
            ))}
          </div>
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              marginTop: 8,
              fontFamily: "var(--mono)",
              fontSize: 10.5,
              color: "var(--fg-3)",
              letterSpacing: ".08em",
            }}
          >
            <span>oldest</span>
            <span>event timeline</span>
            <span>now</span>
          </div>
        </div>
      </Card>

      <Card tight>
        <div className="filters">
          {(["error", "warn", "info", "debug"] as const).map((lvl) => (
            <button
              key={lvl}
              className={"filter-pill" + (level[lvl] ? " active" : "")}
              onClick={() => setLevel((l) => ({ ...l, [lvl]: !l[lvl] }))}
            >
              <span className={"status-dot " + (lvl === "error" ? "crit" : lvl === "warn" ? "warn" : lvl === "info" ? "info" : "")} />
              {lvl}
              <span className="count">{counts[lvl]}</span>
            </button>
          ))}

          <select className="select-mini" value={source} onChange={(e) => setSource(e.target.value)}>
            <option value="all">All sources</option>
            <option value="unifi">UniFi</option>
            <option value="proxmox">Proxmox</option>
          </select>

          <div className="filters-spacer" />

          <button className={"filter-pill " + (paused ? "" : "active")} onClick={() => setPaused((p) => !p)}>
            {paused ? <Icon name="power" /> : null}
            {paused ? "Resume tail" : "● Tailing live"}
          </button>

          <div className="search-mini" style={{ width: 240 }}>
            <Icon name="search" />
            <input
              placeholder="Filter messages, hosts, sources"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>

          <button className="filter-pill" onClick={exportJson}>
            <Icon name="download" /> Export
          </button>
        </div>

        <div className="log-stream">
          <div className="log-head">
            <span style={{ width: 70 }}>TIME</span>
            <span style={{ width: 58 }}>LEVEL</span>
            <span style={{ width: 180 }}>SOURCE</span>
            <span style={{ width: 130 }}>TARGET</span>
            <span style={{ flex: 1 }}>MESSAGE</span>
          </div>
          <div className="log-body">
            {visible.length === 0 && <div className="empty-row" style={{ padding: 36 }}>No events match the current filters.</div>}
            {visible.map((e, i) => (
              <div className={"log-row log-" + e.level} key={i}>
                <span className="log-t mono">{e.time}</span>
                <span className={"log-lvl log-lvl-" + e.level}>{e.level}</span>
                <span className="log-src mono">{e.source}</span>
                <span className="log-tgt mono">{e.target}</span>
                <span className="log-msg mono">{e.msg}</span>
              </div>
            ))}
          </div>
          <div className="log-foot">
            <span className="mono" style={{ color: "var(--fg-3)" }}>
              Showing {visible.length} of {events.length} events · {paused ? "paused" : "live tail at 5s"}
            </span>
          </div>
        </div>
      </Card>
    </div>
  );
}
