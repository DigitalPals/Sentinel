// Cybex Sentinel — Proxmox servers & guests.
import React from "react";
import type { Guest, Snapshot } from "../api";
import { Card, Chip, Icon, KPI_COLORS, KpiTile, MiniBar, StatusDot } from "../components";
import { EditableGrid, type EditableLayoutValue, type LayoutStore } from "../layouts";

const KPI_LABELS = ["Nodes Online", "Virtual Machines", "LXC Containers", "Cluster Storage"];

export default function Proxmox({
  snap,
  editMode,
  layoutStore,
  onLayoutChange,
}: {
  snap: Snapshot;
  editMode: boolean;
  layoutStore: LayoutStore;
  onLayoutChange: (pageId: string, layout: EditableLayoutValue) => void;
}) {
  const p = snap.proxmox;
  const [view, setView] = React.useState<"all" | "vm" | "lxc">("all");
  const [quick, setQuick] = React.useState<"" | "running" | "stopped" | "highcpu" | "highmem">("");
  const [collapsed, setCollapsed] = React.useState<Record<string, boolean>>({});
  const [query, setQuery] = React.useState("");

  const toggle = (key: string) => setCollapsed((c) => ({ ...c, [key]: !c[key] }));
  const toggleQuick = (q: typeof quick) => setQuick((cur) => (cur === q ? "" : q));

  const counts = { vm: 0, lxc: 0 };
  p.guests.forEach((g) =>
    g.guests.forEach((x) => (x.kind === "vm" ? counts.vm++ : counts.lxc++)),
  );

  const matchGuest = (g: Guest): boolean => {
    if (view !== "all" && g.kind !== view) return false;
    if (quick === "running" && g.status === "stop") return false;
    if (quick === "stopped" && g.status !== "stop") return false;
    if (quick === "highcpu" && g.cpu < 80) return false;
    if (quick === "highmem" && g.mem < 80) return false;
    if (query) {
      const q = query.toLowerCase();
      if (!(g.name + " " + g.id + " " + g.tags).toLowerCase().includes(q)) return false;
    }
    return true;
  };

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
          kpi={p.kpis[i] || { display: "—", unit: "", sub: "", trend: 0, spark: [] }}
          sparkColor={KPI_COLORS[i % KPI_COLORS.length]}
        />
      ),
    })),
    {
      id: "guest-inventory",
      label: "Guest Inventory",
      defaultSize: { w: 12, h: 8 },
      minW: 6,
      minH: 4,
      content: (
        <Card tight>
        <div className="tabs">
          <button className={"tab " + (view === "all" ? "active" : "")} onClick={() => setView("all")}>
            All guests <span className="count">{counts.vm + counts.lxc}</span>
          </button>
          <button className={"tab " + (view === "vm" ? "active" : "")} onClick={() => setView("vm")}>
            Virtual Machines <span className="count">{counts.vm}</span>
          </button>
          <button className={"tab " + (view === "lxc" ? "active" : "")} onClick={() => setView("lxc")}>
            LXC Containers <span className="count">{counts.lxc}</span>
          </button>
        </div>

        <div className="filters">
          <button className={"filter-pill " + (quick === "running" ? "active" : "")} onClick={() => toggleQuick("running")}>
            Running <span className="count">{p.running}</span>
          </button>
          <button className={"filter-pill " + (quick === "stopped" ? "active" : "")} onClick={() => toggleQuick("stopped")}>
            Stopped <span className="count">{p.stopped}</span>
          </button>
          <button className={"filter-pill " + (quick === "highcpu" ? "active" : "")} onClick={() => toggleQuick("highcpu")}>
            High CPU <span className="count">{p.highCpu}</span>
          </button>
          <button className={"filter-pill " + (quick === "highmem" ? "active" : "")} onClick={() => toggleQuick("highmem")}>
            High Memory <span className="count">{p.highMem}</span>
          </button>
          <div className="filters-spacer" />
          <div className="search-mini">
            <Icon name="search" />
            <input
              placeholder="Filter guests by name, ID, tag"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
        </div>

        {p.nodes.length === 0 && <div className="empty-row">No Proxmox nodes reachable.</div>}

        {p.nodes.map((n) => {
          const key = n.server + "/" + n.name;
          const isOpen = !collapsed[key];
          const group = p.guests.find((g) => g.node === n.name && g.server === n.server);
          const guests = (group?.guests || []).filter(matchGuest);
          return (
            <div key={key}>
              <div className="node-section-head">
                <button className={"collapse-btn " + (isOpen ? "" : "collapsed")} onClick={() => toggle(key)}>
                  <Icon name="chevron" />
                </button>
                <div className="node-sec-name">
                  <StatusDot tone={n.status} />
                  {n.name}
                </div>
                <div className="node-sec-chip-row">
                  <Chip>{n.server}</Chip>
                </div>
                <div>
                  <div className="bar-label">
                    <span>CPU</span>
                    <span className="bar-val">{n.cpu}%</span>
                  </div>
                  <div className="bar-track">
                    <div className={"bar-fill " + (n.cpu > 85 ? "crit" : n.cpu > 70 ? "warn" : "ok")} style={{ width: n.cpu + "%" }} />
                  </div>
                </div>
                <div>
                  <div className="bar-label">
                    <span>MEM</span>
                    <span className="bar-val">{n.mem}%</span>
                  </div>
                  <div className="bar-track">
                    <div className={"bar-fill " + (n.mem > 85 ? "crit" : n.mem > 70 ? "warn" : "ok")} style={{ width: n.mem + "%" }} />
                  </div>
                </div>
                <div>
                  <div className="bar-label">
                    <span>DISK</span>
                    <span className="bar-val">{n.disk}%</span>
                  </div>
                  <div className="bar-track">
                    <div className={"bar-fill " + (n.disk > 85 ? "crit" : n.disk > 70 ? "warn" : "ok")} style={{ width: n.disk + "%" }} />
                  </div>
                </div>
                <div style={{ fontFamily: "var(--mono)", fontSize: 10.5, color: "var(--fg-2)", textAlign: "right" }}>
                  {guests.length} guests
                </div>
                <div style={{ textAlign: "right" }}>
                  <span className="node-meta">up {n.uptime}</span>
                </div>
              </div>

              {isOpen && (
                <>
                  <div
                    className="guest-row"
                    style={{ background: "var(--bg)", padding: "8px 28px", cursor: "default" }}
                  >
                    <div className="col-h" />
                    <div className="col-h">Guest</div>
                    <div className="col-h">ID</div>
                    <div className="col-h">CPU</div>
                    <div className="col-h">Memory</div>
                    <div className="col-h">Disk</div>
                    <div className="col-h">Uptime</div>
                    <div className="col-h" style={{ textAlign: "right" }}>
                      Status
                    </div>
                  </div>

                  {guests.length === 0 && (
                    <div
                      style={{
                        padding: "20px 28px",
                        color: "var(--fg-3)",
                        fontFamily: "var(--mono)",
                        fontSize: 11.5,
                        background: "var(--bg-1)",
                      }}
                    >
                      No matching guests on this node.
                    </div>
                  )}

                  {guests.map((g) => (
                    <div className="guest-row" key={g.id}>
                      <div className={"guest-icon " + g.kind}>{g.kind === "vm" ? "VM" : "CT"}</div>
                      <div>
                        <div className="guest-name">{g.name}</div>
                        <div className="guest-id">
                          {g.tags ? g.tags + " · " : ""}
                          {g.cores} core · {g.ram}
                        </div>
                      </div>
                      <div className="guest-id" style={{ fontSize: 11.5 }}>
                        {g.id}
                      </div>
                      <MiniBar value={g.cpu} />
                      <MiniBar value={g.mem} />
                      <MiniBar value={g.disk} />
                      <div className="guest-id">{g.uptime}</div>
                      <div style={{ textAlign: "right" }}>
                        {g.status === "stop" ? (
                          <Chip tone="default">
                            <Icon name="power" /> Stopped
                          </Chip>
                        ) : (
                          <Chip dot tone={g.status}>
                            {g.status === "ok" ? "Running" : g.status === "warn" ? "Warning" : "Critical"}
                          </Chip>
                        )}
                      </div>
                    </div>
                  ))}
                </>
              )}
            </div>
          );
        })}
        </Card>
      ),
    },
  ];

  return (
    <div className="page">
      <EditableGrid
        pageId="proxmox"
        editMode={editMode}
        items={cards}
        layoutStore={layoutStore}
        onLayoutChange={onLayoutChange}
      />
    </div>
  );
}
