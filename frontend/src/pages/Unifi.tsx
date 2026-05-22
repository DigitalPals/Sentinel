// Cybex Sentinel — UniFi network devices.
import React from "react";
import type { Snapshot, UniDevice } from "../api";
import { Bar, Card, Chip, Icon, KpiGrid, fmtMbps } from "../components";

const KPI_LABELS = ["Devices Adopted", "Wireless Clients", "Active PoE Ports", "WAN Throughput"];

const typeIcon = (kind: string) =>
  kind === "Gateway"
    ? "gateway"
    : kind === "Switch"
      ? "switch"
      : kind === "Access Point"
        ? "ap"
        : kind === "UPS"
          ? "ups"
          : "network";

const statusLabel = (s: string) => (s === "ok" ? "Online" : s === "warn" ? "Warning" : "Offline");

export default function Unifi({ snap }: { snap: Snapshot }) {
  const u = snap.unifi;
  const all = u.devices;
  const [filter, setFilter] = React.useState("All");
  const [site, setSite] = React.useState("All sites");
  const [query, setQuery] = React.useState("");
  const [selectedId, setSelectedId] = React.useState<string | null>(null);

  const types = ["All", ...Array.from(new Set(all.map((d) => d.kind)))];
  const counts: Record<string, number> = { All: all.length };
  types.slice(1).forEach((t) => (counts[t] = all.filter((d) => d.kind === t).length));
  const sites = ["All sites", ...Array.from(new Set(all.map((d) => d.site)))];

  const filtered = all.filter((d) => {
    if (filter !== "All" && d.kind !== filter) return false;
    if (site !== "All sites" && d.site !== site) return false;
    if (query && !(d.name + d.ip + d.mac + d.model).toLowerCase().includes(query.toLowerCase())) return false;
    return true;
  });

  const sel: UniDevice | undefined = filtered.find((d) => d.id === selectedId) || filtered[0];

  return (
    <div className="page">
      <KpiGrid kpis={u.kpis} labels={KPI_LABELS} />

      <div className="split">
        <Card tight>
          <div className="filters">
            {types.map((t) => (
              <button
                key={t}
                className={"filter-pill " + (filter === t ? "active" : "")}
                onClick={() => setFilter(t)}
              >
                {t}
                <span className="count">{counts[t] ?? 0}</span>
              </button>
            ))}
            <div className="filters-spacer" />
            {sites.length > 2 && (
              <select className="select-mini" value={site} onChange={(e) => setSite(e.target.value)}>
                {sites.map((s) => (
                  <option key={s}>{s}</option>
                ))}
              </select>
            )}
            <div className="search-mini">
              <Icon name="search" />
              <input
                placeholder="Filter by name / IP / MAC"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </div>
          </div>

          <div className="scroll-area" style={{ maxHeight: "calc(100vh - 320px)" }}>
            <table className="tbl">
              <thead>
                <tr>
                  <th style={{ width: 28 }} />
                  <th>Device</th>
                  <th>IP · MAC</th>
                  <th>Model</th>
                  <th>Clients</th>
                  <th>Throughput</th>
                  <th>Uptime</th>
                  <th>Status</th>
                </tr>
              </thead>
              <tbody>
                {filtered.length === 0 && (
                  <tr>
                    <td colSpan={8} className="empty-row">
                      No devices match the current filters.
                    </td>
                  </tr>
                )}
                {filtered.map((d) => (
                  <tr
                    key={d.id}
                    className={sel && d.id === sel.id ? "selected" : ""}
                    onClick={() => setSelectedId(d.id)}
                  >
                    <td>
                      <span style={{ color: "var(--fg-2)", display: "inline-flex", alignItems: "center" }}>
                        <Icon name={typeIcon(d.kind)} />
                      </span>
                    </td>
                    <td>
                      <div className="cell-stack">
                        <span className="top">{d.name}</span>
                        <span className="bot">
                          {d.kind} · {d.site}
                        </span>
                      </div>
                    </td>
                    <td>
                      <div className="cell-stack">
                        <span className="mono">{d.ip || "—"}</span>
                        <span className="bot">{d.mac}</span>
                      </div>
                    </td>
                    <td className="mono muted">{d.model}</td>
                    <td className="mono">{d.clients > 0 ? d.clients : <span className="muted">—</span>}</td>
                    <td className="mono">
                      {d.status === "crit" ? (
                        <span className="muted">—</span>
                      ) : (
                        <>
                          <span style={{ color: "oklch(0.82 0.15 200)" }}>↓ {fmtMbps(d.rxMbps)}</span>
                          {" · "}
                          <span style={{ color: "oklch(0.66 0.18 270)" }}>↑ {fmtMbps(d.txMbps)}</span>
                        </>
                      )}
                    </td>
                    <td className="mono muted">{d.uptime}</td>
                    <td>
                      <Chip dot tone={d.status}>
                        {statusLabel(d.status)}
                      </Chip>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Card>

        {sel && <DeviceDetail device={sel} />}
      </div>
    </div>
  );
}

function DeviceDetail({ device: d }: { device: UniDevice }) {
  return (
    <Card tight>
      <div className="detail-h">
        <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 6 }}>
          <span style={{ color: "var(--fg-2)", display: "inline-flex" }}>
            <Icon name={typeIcon(d.kind)} />
          </span>
          <h3 className="detail-title" style={{ marginBottom: 0 }}>
            {d.name}
          </h3>
        </div>
        <div className="detail-sub">
          {d.model} · {d.site}
        </div>
        <div style={{ marginTop: 10, display: "flex", gap: 6, flexWrap: "wrap" }}>
          <Chip dot tone={d.status}>
            {statusLabel(d.status)}
          </Chip>
          <Chip>FW {d.fw}</Chip>
          <Chip>{d.kind}</Chip>
          {d.firmwareUpdatable && <Chip tone="info">Update available</Chip>}
        </div>
      </div>

      <div className="detail-grid">
        <div>
          <div className="dg-k">IP Address</div>
          <div className="dg-v">{d.ip || "—"}</div>
        </div>
        <div>
          <div className="dg-k">MAC</div>
          <div className="dg-v">{d.mac}</div>
        </div>
        <div>
          <div className="dg-k">Uptime</div>
          <div className="dg-v">{d.uptime}</div>
        </div>
        <div>
          <div className="dg-k">Clients</div>
          <div className="dg-v">{d.clients}</div>
        </div>
        <div>
          <div className="dg-k">Download</div>
          <div className="dg-v">{fmtMbps(d.rxMbps)}</div>
        </div>
        <div>
          <div className="dg-k">Upload</div>
          <div className="dg-v">{fmtMbps(d.txMbps)}</div>
        </div>
      </div>

      <hr className="div" />

      <div style={{ padding: "14px 18px" }}>
        <div className="dg-k">Live Load</div>
        <div style={{ marginTop: 10, display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
          <Bar label="CPU" value={d.cpu} />
          <Bar label="Memory" value={d.mem} />
        </div>
      </div>

      <hr className="div" />

      <div style={{ padding: "14px 18px" }}>
        <div className="dg-k">{d.radios.length > 0 ? "Radios" : "Port Activity"}</div>
        {d.ports.length > 0 ? (
          <>
            <div className="port-grid">
              {d.ports.map((p) => (
                <div
                  key={p.idx}
                  className={"port-cell" + (p.poe ? " poe" : p.up ? " up" : "")}
                  title={`Port ${p.idx} · ${p.up ? `up ${p.speedMbps} Mbps` : "down"}${p.poe ? " · PoE" : ""}`}
                >
                  <span>{p.idx}</span>
                </div>
              ))}
            </div>
            <div
              style={{
                marginTop: 12,
                fontFamily: "var(--mono)",
                fontSize: 10.5,
                color: "var(--fg-2)",
                display: "flex",
                gap: 14,
              }}
            >
              <span>{d.ports.filter((p) => p.up).length} active</span>
              <span>{d.ports.filter((p) => p.poe).length} PoE</span>
              <span>{d.ports.length} total</span>
            </div>
          </>
        ) : d.radios.length > 0 ? (
          <div style={{ marginTop: 10, display: "flex", flexDirection: "column", gap: 10 }}>
            {d.radios.map((r, i) => (
              <div
                key={i}
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  fontFamily: "var(--mono)",
                  fontSize: 11,
                  color: "var(--fg-1)",
                  paddingBottom: 8,
                  borderBottom: i < d.radios.length - 1 ? "1px solid var(--line)" : "0",
                }}
              >
                <span style={{ color: "var(--accent)" }}>{r.band}</span>
                <span style={{ color: "var(--fg-2)" }}>
                  ch {r.channel} · {r.width} MHz · {r.standard}
                </span>
              </div>
            ))}
          </div>
        ) : (
          <div style={{ marginTop: 10, fontFamily: "var(--mono)", fontSize: 11, color: "var(--fg-2)" }}>
            No port or radio detail reported for this device.
          </div>
        )}
      </div>
    </Card>
  );
}
