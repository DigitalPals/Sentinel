// Cybex Sentinel — Operations Dashboard.
import React from "react";
import type { Snapshot } from "../api";
import { Bar, BandwidthChart, Card, Chip, KpiGrid, MiniBar, Sparkline, StatusDot, fmtMbps } from "../components";
import { TopologyCard, TopologyModal } from "../topology";

const KPI_LABELS = ["Fleet Availability", "UniFi Devices Online", "Active Alerts", "WAN Throughput"];

function fmtData(gb: number): { value: string; unit: string } {
  if (gb >= 1000) return { value: (gb / 1000).toFixed(2), unit: "TB" };
  if (gb >= 1) return { value: gb.toFixed(1), unit: "GB" };
  return { value: (gb * 1000).toFixed(0), unit: "MB" };
}

const issueGlyph = (sev: string) => (sev === "info" ? "i" : "!");

export default function Dashboard({ snap }: { snap: Snapshot }) {
  const d = snap.dashboard;
  const [topoOpen, setTopoOpen] = React.useState(false);

  const bw = d.bandwidth;
  const transferred = fmtData(bw.transferredGb);

  // Top resource consumers — real guests, ranked by CPU across the cluster.
  const allGuests = snap.proxmox.guests.flatMap((g) => g.guests);
  const topConsumers = [...allGuests]
    .filter((g) => g.status !== "stop")
    .sort((a, b) => b.cpu + b.mem - (a.cpu + a.mem))
    .slice(0, 6);

  return (
    <div className="page">
      <KpiGrid kpis={d.kpis} labels={KPI_LABELS} />

      <div className="row-2">
        <Card
          title="Network Bandwidth"
          sub={`WAN · ${bw.windowLabel}`}
          actions={
            <div className="chart-legend">
              <span>
                <span className="sw" style={{ background: "var(--accent)" }} />
                Download
              </span>
              <span>
                <span className="sw" style={{ background: "var(--accent-2)" }} />
                Upload
              </span>
            </div>
          }
          tight
        >
          {bw.points >= 2 ? (
            <div className="chart-wrap">
              <BandwidthChart down={bw.down} up={bw.up} />
            </div>
          ) : (
            <div className="chart-empty">Collecting WAN throughput history…</div>
          )}
          <div className="chart-stat-row">
            <div className="chart-stat">
              <div className="chart-stat-l">Peak Down</div>
              <div className="chart-stat-v">{fmtMbps(bw.peakDown)}</div>
            </div>
            <div className="chart-stat">
              <div className="chart-stat-l">Peak Up</div>
              <div className="chart-stat-v">{fmtMbps(bw.peakUp)}</div>
            </div>
            <div className="chart-stat">
              <div className="chart-stat-l">Avg Total</div>
              <div className="chart-stat-v">{fmtMbps(bw.avg)}</div>
            </div>
            <div className="chart-stat">
              <div className="chart-stat-l">Transferred</div>
              <div className="chart-stat-v">
                {transferred.value} <span className="unit">{transferred.unit}</span>
              </div>
            </div>
          </div>
        </Card>

        <Card
          title="Active Issues"
          sub="live · sorted by severity"
          actions={
            <Chip tone={d.issues.length ? "crit" : "ok"} dot>
              {d.issues.length} open
            </Chip>
          }
          tight
        >
          {d.issues.length === 0 && <div className="empty-row">No active issues — all systems nominal.</div>}
          {d.issues.map((it, i) => (
            <div className="issue" key={i}>
              <div className={"issue-icon " + it.sev}>{issueGlyph(it.sev)}</div>
              <div>
                <div className="issue-title">{it.title}</div>
                <div className="issue-meta">{it.source}</div>
              </div>
              <div className="issue-time">{it.time}</div>
            </div>
          ))}
        </Card>
      </div>

      <Card
        title="Proxmox Servers"
        sub={`live · ${d.nodes.length} node(s) · cpu / mem / disk / net`}
        actions={
          <>
            <Chip tone="ok" dot>
              Quorum {d.quorum}
            </Chip>
            <Chip>{d.totalGuests} guests</Chip>
          </>
        }
        tight
        style={{ marginBottom: 14 }}
      >
        {d.nodes.length === 0 ? (
          <div className="empty-row">No Proxmox nodes reachable.</div>
        ) : (
          <div
            className="node-grid"
            style={
              d.nodes.length > 0 && d.nodes.length <= 4
                ? { gridTemplateColumns: `repeat(${d.nodes.length}, 1fr)` }
                : undefined
            }
          >
            {d.nodes.map((n) => (
              <div className="node-tile" key={n.server + n.name}>
                <div className="node-tile-hd">
                  <StatusDot tone={n.status} />
                  <div className="node-name">{n.name}</div>
                  <Chip dot tone={n.status}>
                    {n.status === "ok" ? "Online" : n.status === "warn" ? "Warning" : "Offline"}
                  </Chip>
                </div>
                <div className="node-tile-meta">
                  <span>{n.server}</span>
                  <span>{n.model}</span>
                </div>
                <div className="node-tile-meta">
                  <span>up {n.uptime}</span>
                  <span>
                    {n.guests.vm} VM · {n.guests.lxc} LXC
                  </span>
                </div>
                <div className="node-tile-bars">
                  <Bar label="CPU" value={n.cpu} />
                  <Bar label="MEM" value={n.mem} />
                  <Bar label="DISK" value={n.disk} />
                  <Bar label="NET" value={n.net} />
                </div>
              </div>
            ))}
          </div>
        )}
      </Card>

      <div className="row-2" style={{ gridTemplateColumns: "1fr 1fr" }}>
        <Card title="Top Resource Consumers" sub="across cluster · live" tight>
          <table className="tbl">
            <thead>
              <tr>
                <th>Guest</th>
                <th>Node</th>
                <th>CPU</th>
                <th>Memory</th>
                <th>Disk</th>
                <th style={{ width: 84 }}>Status</th>
              </tr>
            </thead>
            <tbody>
              {topConsumers.length === 0 && (
                <tr>
                  <td colSpan={6} className="empty-row">
                    No running guests.
                  </td>
                </tr>
              )}
              {topConsumers.map((g) => (
                <tr key={g.server + g.id}>
                  <td>
                    <div className="cell-stack">
                      <span className="top">{g.name}</span>
                      <span className="bot">
                        {g.kind.toUpperCase()} {g.id} · {g.cores} core · {g.ram}
                      </span>
                    </div>
                  </td>
                  <td className="mono">{g.node}</td>
                  <td>
                    <MiniBar value={g.cpu} />
                  </td>
                  <td>
                    <MiniBar value={g.mem} />
                  </td>
                  <td>
                    <MiniBar value={g.disk} />
                  </td>
                  <td>
                    <Chip dot tone={g.status === "stop" ? "default" : g.status}>
                      {g.status === "ok" ? "Healthy" : g.status === "warn" ? "Warning" : g.status === "crit" ? "Critical" : "Stopped"}
                    </Chip>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </Card>

        <TopologyCard topo={snap.topology} onOpenFull={() => setTopoOpen(true)} />
      </div>

      {topoOpen && <TopologyModal topo={snap.topology} onClose={() => setTopoOpen(false)} />}
    </div>
  );
}
