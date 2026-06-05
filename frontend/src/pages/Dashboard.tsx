// Cybex Sentinel — Operations Dashboard.
import React from "react";
import type { Snapshot } from "../api";
import { Bar, BandwidthChart, Card, Chip, Icon, KPI_COLORS, KpiTile, MiniBar, StatusDot, fmtMbps } from "../components";
import { EditableGrid, type EditableLayoutValue, type LayoutStore } from "../layouts";
import { TopologyCard, TopologyModal } from "../topology";

const KPI_LABELS = ["Fleet Availability", "UniFi Devices Online", "Active Alerts", "WAN Throughput"];

function fmtData(gb: number): { value: string; unit: string } {
  if (gb >= 1000) return { value: (gb / 1000).toFixed(2), unit: "TB" };
  if (gb >= 1) return { value: gb.toFixed(1), unit: "GB" };
  return { value: (gb * 1000).toFixed(0), unit: "MB" };
}

export default function Dashboard({
  snap,
  editMode,
  layoutStore,
  onLayoutChange,
  onOpenIssue,
}: {
  snap: Snapshot;
  editMode: boolean;
  layoutStore: LayoutStore;
  onLayoutChange: (pageId: string, layout: EditableLayoutValue) => void;
  onOpenIssue: (id: string) => void;
}) {
  const d = snap.dashboard;
  const unraid = snap.unraid;
  const [topoOpen, setTopoOpen] = React.useState(false);

  const bw = d.bandwidth;
  const transferred = fmtData(bw.transferredGb);

  // Top resource consumers — real guests, ranked by CPU across the cluster.
  const allGuests = snap.proxmox.guests.flatMap((g) => g.guests);
  const topConsumers = [...allGuests]
    .filter((g) => g.status !== "stop")
    .sort((a, b) => b.cpu + b.mem - (a.cpu + a.mem))
    .slice(0, 6);

  const cards = [
    ...KPI_LABELS.map((label, i) => ({
      id: `kpi-${i}`,
      label,
      defaultSize: { w: 3, h: 1 },
      minW: 2,
      maxW: 6,
      minH: 1,
      maxH: 1,
      content: (
        <KpiTile
          label={label}
          kpi={d.kpis[i] || { display: "—", unit: "", sub: "", trend: 0, spark: [] }}
          sparkColor={KPI_COLORS[i % KPI_COLORS.length]}
        />
      ),
    })),
    {
      id: "bandwidth",
      label: "Network Bandwidth",
      defaultSize: { w: 8, h: 4 },
      minW: 4,
      minH: 3,
      content: (
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
              <BandwidthChart down={bw.down} up={bw.up} windowLabel={bw.windowLabel} />
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
      ),
    },
    {
      id: "issues",
      label: "Active Issues",
      defaultSize: { w: 4, h: 4 },
      minW: 3,
      minH: 3,
      content: (
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
          {d.issues.map((it) => (
            <button
              className="issue"
              key={it.id}
              type="button"
              onClick={() => onOpenIssue(it.id)}
              aria-label={`Open alert details for ${it.title}`}
            >
              <div className={"issue-icon " + it.sev}>
                <Icon name={it.sev === "info" ? "info" : "alert"} />
              </div>
              <div>
                <div className="issue-title">{it.title}</div>
                <div className="issue-meta">{it.source}</div>
              </div>
              <div className="issue-time">{it.time}</div>
            </button>
          ))}
        </Card>
      ),
    },
    {
      id: "proxmox-servers",
      label: "Proxmox Servers",
      defaultSize: { w: 12, h: 4 },
      minW: 5,
      minH: 3,
      content: (
        <Card
          title="Proxmox Servers"
          sub={`live · ${d.nodes.length} node(s) · cpu / mem / disk / net`}
          actions={
            <>
              {d.quorum && (
                <Chip tone="ok" dot>
                  Quorum {d.quorum}
                </Chip>
              )}
              <Chip>{d.totalGuests} guests</Chip>
            </>
          }
          tight
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
      ),
    },
    {
      id: "unraid-storage",
      label: "Unraid Storage",
      defaultSize: { w: 12, h: 4 },
      minW: 5,
      minH: 3,
      content: (
        <Card
          title="Unraid Storage"
          sub={`live · ${unraid.servers.length} server(s) · array + pools / docker / vm`}
          actions={
            <>
              <Chip tone={unraid.arrayWarn ? "warn" : "ok"} dot>
                {unraid.arrayWarn} capacity warnings
              </Chip>
              <Chip>
                {unraid.containersRunning}/{unraid.containersTotal} containers
              </Chip>
              {unraid.softwareUpdateCount > 0 && <Chip tone="warn">{unraid.softwareUpdateCount} updates</Chip>}
            </>
          }
          tight
        >
          {unraid.servers.length === 0 ? (
            <div className="empty-row">No Unraid servers reachable.</div>
          ) : (
            <div
              className="unraid-dash-grid"
              style={
                unraid.servers.length > 0 && unraid.servers.length <= 3
                  ? { gridTemplateColumns: `repeat(${unraid.servers.length}, 1fr)` }
                  : undefined
              }
            >
              {unraid.servers.map((s) => (
                <div className="unraid-dash-tile" key={s.source}>
                  <div className="node-tile-hd">
                    <StatusDot tone={s.status} />
                    <div className="node-name">{s.name}</div>
                    <Chip dot tone={s.arrayState === "STARTED" ? "ok" : "crit"}>
                      {s.arrayState}
                    </Chip>
                  </div>
                  <div className="node-tile-meta">
                    <span>{s.version || "Unraid"}</span>
                    <span>up {s.uptime}</span>
                  </div>
                  <div className="node-tile-meta">
                    <span>
                      Storage {s.storageUsed} / {s.storageTotal}
                    </span>
                    <span>{s.temp}</span>
                  </div>
                  <div className="node-tile-bars">
                    <Bar label="STORAGE" value={s.storageUsedPct} />
                    <Bar label="CPU" value={s.cpu} />
                    <Bar label="MEM" value={s.mem} />
                  </div>
                  <div className="unraid-tile-foot">
                    <span>{s.diskCount} disks</span>
                    <span>{s.containersRunning}/{s.containersTotal} containers</span>
                    <span>{s.vmsRunning}/{s.vmsTotal} VMs</span>
                    <span>{s.notificationCount} notices</span>
                    {s.softwareUpdateCount > 0 && <span>{s.softwareUpdateCount} updates</span>}
                  </div>
                </div>
              ))}
            </div>
          )}
        </Card>
      ),
    },
    {
      id: "top-consumers",
      label: "Top Resource Consumers",
      defaultSize: { w: 6, h: 4 },
      minW: 4,
      minH: 3,
      content: (
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
      ),
    },
    {
      id: "topology",
      label: "Network Topology",
      defaultSize: { w: 6, h: 4 },
      minW: 4,
      minH: 3,
      content: <TopologyCard topo={snap.topology} onOpenFull={() => setTopoOpen(true)} />,
    },
  ];

  return (
    <div className="page">
      <EditableGrid
        pageId="dashboard"
        editMode={editMode}
        items={cards}
        layoutStore={layoutStore}
        onLayoutChange={onLayoutChange}
      />

      {topoOpen && <TopologyModal topo={snap.topology} onClose={() => setTopoOpen(false)} />}
    </div>
  );
}
