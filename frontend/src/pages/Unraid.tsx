// Cybex Sentinel — Unraid storage, Docker and VM monitoring.
import React from "react";
import type {
  Snapshot,
  UnraidContainer,
  UnraidDisk,
  UnraidNotification,
  UnraidServer,
  UnraidStorage,
  UnraidVm,
} from "../api";
import { Bar, Card, Chip, Icon, KpiGrid, MiniBar, StatusDot } from "../components";

const KPI_LABELS = ["Servers Online", "Array Usage", "Docker Containers", "VMs Running"];

type ResourceKind = "storage" | "disk" | "docker" | "vm" | "notice";
type View = "all" | ResourceKind;
type Quick = "" | "attention" | "updates" | "active" | "capacity";

interface ResourceRow {
  key: string;
  kind: ResourceKind;
  icon: string;
  iconClass: string;
  name: string;
  sub: string;
  type: React.ReactNode;
  metric: React.ReactNode;
  detail: React.ReactNode;
  secondary: React.ReactNode;
  update: React.ReactNode;
  status: React.ReactNode;
  search: string;
  attention: boolean;
  updateAvailable: boolean;
  active: boolean;
  highCapacity: boolean;
}

const VIEW_LABELS: Record<View, string> = {
  all: "All categories",
  storage: "Arrays / Pools",
  disk: "Disks",
  docker: "Docker",
  vm: "VM's",
  notice: "Notifications",
};

const CATEGORIES: Array<{ kind: ResourceKind; label: string }> = [
  { kind: "storage", label: "Arrays / Pools" },
  { kind: "disk", label: "Disks" },
  { kind: "docker", label: "Docker" },
  { kind: "vm", label: "VM's" },
  { kind: "notice", label: "Notifications" },
];

const serverTone = (s: UnraidServer) => {
  if (s.status === "crit" || s.parityErrors > 0 || s.arrayState !== "STARTED") return "crit";
  if (s.status === "warn" || s.notificationCount > 0) return "warn";
  return "ok";
};

const arrayTone = (s: UnraidServer) =>
  s.parityErrors > 0 || s.arrayState !== "STARTED" ? "crit" : "ok";

const diskTone = (d: UnraidDisk) =>
  d.status === "OK" ? "ok" : d.status === "—" ? "default" : d.status === "New" ? "warn" : "crit";

const storageTone = (s: UnraidStorage) =>
  s.kind === "Array"
    ? s.status === "STARTED"
      ? "ok"
      : "crit"
    : s.status === "OK"
      ? "ok"
      : s.status === "—"
        ? "default"
        : "crit";

const workloadTone = (state: string) => {
  const s = state.toUpperCase();
  return s === "RUNNING" || s === "STARTED" ? "ok" : s === "PAUSED" ? "warn" : "default";
};

const updateTone = (status: string, available: boolean) =>
  available
    ? "warn"
    : status === "Current"
      ? "ok"
      : status === "Unknown"
        ? "default"
        : "warn";

const tempValue = (temp: string) => {
  const match = temp.match(/-?\d+(\.\d+)?/);
  return match ? Number(match[0]) : 0;
};

const tempSensorLabel = (sensor: string) =>
  sensor
    .replace(/^[^-]+-[^-]+-\d+\s+/, "")
    .replace(/^coretemp-isa-\d+\s+/, "")
    .replace(/^qnap_ec-isa-\d+\s+/, "")
    .trim();

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

const searchable = (...parts: Array<string | number | boolean | null | undefined>) =>
  parts
    .filter((p) => p !== null && p !== undefined && p !== "")
    .join(" ")
    .toLowerCase();

export default function Unraid({ snap }: { snap: Snapshot }) {
  const u = snap.unraid;
  const [view, setView] = React.useState<View>("all");
  const [quick, setQuick] = React.useState<Quick>("");
  const [collapsed, setCollapsed] = React.useState<Record<string, boolean>>({});
  const [categoryCollapsed, setCategoryCollapsed] = React.useState<Record<string, boolean>>({});
  const [query, setQuery] = React.useState("");

  const groups = u.servers.map((server) => ({ server, rows: rowsForServer(server) }));
  const allRows = groups.flatMap((g) => g.rows);
  const counts = allRows.reduce<Record<ResourceKind, number>>(
    (acc, row) => {
      acc[row.kind] += 1;
      return acc;
    },
    { storage: 0, disk: 0, docker: 0, vm: 0, notice: 0 },
  );

  const queryText = query.trim().toLowerCase();
  const toggle = (key: string) => setCollapsed((c) => ({ ...c, [key]: !c[key] }));
  const toggleCategory = (key: string) => setCategoryCollapsed((c) => ({ ...c, [key]: !c[key] }));
  const toggleQuick = (q: Quick) => setQuick((cur) => (cur === q ? "" : q));

  const matchRow = (row: ResourceRow): boolean => {
    if (view !== "all" && row.kind !== view) return false;
    if (quick === "attention" && !row.attention) return false;
    if (quick === "updates" && !row.updateAvailable) return false;
    if (quick === "active" && !row.active) return false;
    if (quick === "capacity" && !row.highCapacity) return false;
    if (queryText && !row.search.includes(queryText)) return false;
    return true;
  };

  return (
    <div className="page">
      <KpiGrid kpis={u.kpis} labels={KPI_LABELS} />

      <Card tight>
        <div className="tabs unraid-tabs">
          {([
            ["all", allRows.length],
            ["storage", counts.storage],
            ["disk", counts.disk],
            ["docker", counts.docker],
            ["vm", counts.vm],
            ["notice", counts.notice],
          ] as Array<[View, number]>).map(([id, count]) => (
            <button
              key={id}
              className={"tab " + (view === id ? "active" : "")}
              onClick={() => setView(id as View)}
            >
              {VIEW_LABELS[id as View]} <span className="count">{count}</span>
            </button>
          ))}
        </div>

        <div className="filters">
          <button
            className={"filter-pill " + (quick === "attention" ? "active" : "")}
            onClick={() => toggleQuick("attention")}
          >
            Needs attention <span className="count">{allRows.filter((r) => r.attention).length}</span>
          </button>
          <button
            className={"filter-pill " + (quick === "updates" ? "active" : "")}
            onClick={() => toggleQuick("updates")}
          >
            Updates <span className="count">{allRows.filter((r) => r.updateAvailable).length}</span>
          </button>
          <button
            className={"filter-pill " + (quick === "active" ? "active" : "")}
            onClick={() => toggleQuick("active")}
          >
            Active <span className="count">{allRows.filter((r) => r.active).length}</span>
          </button>
          <button
            className={"filter-pill " + (quick === "capacity" ? "active" : "")}
            onClick={() => toggleQuick("capacity")}
          >
            High usage <span className="count">{allRows.filter((r) => r.highCapacity).length}</span>
          </button>
          <div className="filters-spacer" />
          <div className="search-mini">
            <Icon name="search" />
            <input
              placeholder="Filter Unraid resources"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
        </div>

        {u.servers.length === 0 && <div className="empty-row">No Unraid servers reachable.</div>}

        <div className="unraid-resource-list">
          {groups.map(({ server, rows }) => {
            const isOpen = !collapsed[server.source];
            const visibleRows = rows.filter(matchRow);
            const categories = CATEGORIES.map((category) => ({
              ...category,
              total: rows.filter((row) => row.kind === category.kind).length,
              rows: visibleRows.filter((row) => row.kind === category.kind),
            })).filter((category) => view === "all" ? category.rows.length > 0 : category.kind === view);
            return (
              <div className="unraid-server-block" key={server.source}>
                <div className="unraid-section-head">
                  <button className={"collapse-btn " + (isOpen ? "" : "collapsed")} onClick={() => toggle(server.source)}>
                    <Icon name="chevron" />
                  </button>
                  <div className="node-sec-name">
                    <StatusDot tone={serverTone(server)} />
                    {server.name}
                  </div>
                  <div className="node-sec-chip-row">
                    <Chip>{server.source}</Chip>
                  </div>
                  <Bar label="CPU" value={server.cpu} />
                  <Bar label="MEM" value={server.mem} />
                  <Bar label="ARRAY" value={server.arrayUsedPct} />
                  <div title={server.tempSensor ? `${server.temp} · ${server.tempSensor}` : server.temp}>
                    <Bar label="SENSOR" value={tempValue(server.temp)} unit=" C" max={90} />
                  </div>
                  <div className="unraid-server-state">
                    <Chip dot tone={arrayTone(server)}>
                      {server.arrayState}
                    </Chip>
                    {server.tempSensor && (
                      <span className="unraid-temp-source" title={`${server.temp} · ${server.tempSensor}`}>
                        {tempSensorLabel(server.tempSensor)}
                      </span>
                    )}
                    <span>{visibleRows.length} rows</span>
                  </div>
                </div>

                {isOpen && (
                  <>
                    {visibleRows.length === 0 && (
                      <div className="unraid-row-empty">
                        No matching {VIEW_LABELS[view].toLowerCase()} on this server.
                      </div>
                    )}

                    {categories.map((category) => {
                      const categoryKey = `${server.source}/${category.kind}`;
                      const categoryOpen = !categoryCollapsed[categoryKey];
                      return (
                        <section className="unraid-category" key={category.kind}>
                          <button
                            className="unraid-category-head"
                            onClick={() => toggleCategory(categoryKey)}
                            aria-expanded={categoryOpen}
                          >
                            <span className="unraid-category-main">
                              <span className={"collapse-btn " + (categoryOpen ? "" : "collapsed")}>
                                <Icon name="chevron" />
                              </span>
                              <span className="unraid-category-title">{category.label}</span>
                            </span>
                            <span className="unraid-category-sub">
                              {category.rows.length} / {category.total} shown
                            </span>
                          </button>
                          {categoryOpen && (
                            <>
                              <ResourceHeader kind={category.kind} />
                              {category.rows.map((row) => <ResourceRowView row={row} key={row.key} />)}
                            </>
                          )}
                        </section>
                      );
                    })}
                  </>
                )}
              </div>
            );
          })}
        </div>
      </Card>
    </div>
  );
}

function ResourceHeader({ kind }: { kind: ResourceKind }) {
  const labels =
    kind === "docker"
      ? ["Container", "Type", "CPU", "Memory", "Disk I/O", "Update", "Status"]
      : ["Resource", "Type", "Usage", "Detail", "Signal", "Update", "Status"];
  return (
    <div className="unraid-resource-row unraid-resource-head">
      <div className="col-h" />
      <div className="col-h">{labels[0]}</div>
      <div className="col-h">{labels[1]}</div>
      <div className="col-h">{labels[2]}</div>
      <div className="col-h">{labels[3]}</div>
      <div className="col-h">{labels[4]}</div>
      <div className="col-h">{labels[5]}</div>
      <div className="col-h align-r">{labels[6]}</div>
    </div>
  );
}

function ResourceRowView({ row }: { row: ResourceRow }) {
  return (
    <div className="unraid-resource-row">
      <div className={"unraid-resource-icon " + row.iconClass}>{row.icon}</div>
      <div className="cell-stack">
        <span className="top">{row.name}</span>
        <span className="bot">{row.sub}</span>
      </div>
      <div>{row.type}</div>
      <div>{row.metric}</div>
      <div className="unraid-resource-meta">{row.detail}</div>
      <div className="unraid-resource-meta">{row.secondary}</div>
      <div className="unraid-row-chips">{row.update}</div>
      <div className="unraid-row-status">{row.status}</div>
    </div>
  );
}

function rowsForServer(server: UnraidServer): ResourceRow[] {
  return [
    ...server.storage.map((storage) => storageRow(server, storage)),
    ...server.disks.map((disk) => diskRow(server, disk)),
    ...server.containers.map((container) => containerRow(server, container)),
    ...server.vms.map((vm) => vmRow(server, vm)),
    ...server.notifications.map((notification, i) => notificationRow(server, notification, i)),
  ];
}

function storageRow(server: UnraidServer, storage: UnraidStorage): ResourceRow {
  const tone = storageTone(storage);
  return {
    key: `${server.source}:storage:${storage.id}`,
    kind: "storage",
    icon: storage.kind === "Array" ? "AR" : "PL",
    iconClass: "storage",
    name: storage.name,
    sub: `${storage.used} / ${storage.total}`,
    type: <Chip>{storage.kind}</Chip>,
    metric: <MiniBar value={storage.usedPct} />,
    detail: plural(storage.members, "member"),
    secondary: storage.temp,
    update: storage.kind === "Array" ? <Chip>Parity {server.parityStatus}</Chip> : <span className="unraid-muted">—</span>,
    status: (
      <Chip dot tone={tone}>
        {storage.status}
      </Chip>
    ),
    search: searchable(server.name, server.source, storage.name, storage.kind, storage.status, storage.used, storage.total),
    attention: (tone !== "ok" && tone !== "default") || storage.usedPct >= 85,
    updateAvailable: false,
    active: storage.status === "STARTED" || storage.status === "OK",
    highCapacity: storage.usedPct >= 85,
  };
}

function diskRow(server: UnraidServer, disk: UnraidDisk): ResourceRow {
  const tone = diskTone(disk);
  return {
    key: `${server.source}:disk:${disk.id}`,
    kind: "disk",
    icon: "DS",
    iconClass: "disk",
    name: disk.name,
    sub: disk.size,
    type: <Chip>{disk.kind}</Chip>,
    metric: <MiniBar value={disk.usedPct} />,
    detail: disk.device || "—",
    secondary: disk.temp,
    update: disk.spinning === null ? (
      <span className="unraid-muted">—</span>
    ) : (
      <Chip tone={disk.spinning ? "ok" : "default"}>{disk.spinning ? "Spinning" : "Standby"}</Chip>
    ),
    status: (
      <Chip dot tone={tone}>
        {disk.status}
      </Chip>
    ),
    search: searchable(server.name, server.source, disk.name, disk.kind, disk.device, disk.status, disk.size),
    attention: (tone !== "ok" && tone !== "default") || disk.usedPct >= 90,
    updateAvailable: false,
    active: disk.spinning !== false,
    highCapacity: disk.usedPct >= 85,
  };
}

function containerRow(server: UnraidServer, container: UnraidContainer): ResourceRow {
  const tone = workloadTone(container.state);
  return {
    key: `${server.source}:docker:${container.id}`,
    kind: "docker",
    icon: "DO",
    iconClass: "docker",
    name: container.name,
    sub: container.image || `${container.autoStart ? "Autostart" : "Manual"} container`,
    type: <Chip>Docker</Chip>,
    metric: <MiniBar value={container.cpu} />,
    detail: <MiniBar value={container.mem} />,
    secondary: <TextMetric value={container.blockIo || "—"} label="Block I/O" />,
    update: (
      <Chip tone={updateTone(container.updateStatus, container.updateAvailable)}>
        {container.updateStatus}
      </Chip>
    ),
    status: (
      <>
        <Chip dot tone={tone}>
          {container.state}
        </Chip>
      </>
    ),
    search: searchable(
      server.name,
      server.source,
      container.name,
      container.image,
      container.state,
      container.status,
      container.updateStatus,
      container.memory,
      container.netIo,
      container.blockIo,
      container.network,
      container.ports,
    ),
    attention: container.updateAvailable || tone === "warn",
    updateAvailable: container.updateAvailable,
    active: container.state.toUpperCase() === "RUNNING",
    highCapacity: false,
  };
}

function vmRow(server: UnraidServer, vm: UnraidVm): ResourceRow {
  const tone = workloadTone(vm.state);
  return {
    key: `${server.source}:vm:${vm.id}`,
    kind: "vm",
    icon: "VM",
    iconClass: "vm",
    name: vm.name,
    sub: "Virtual machine",
    type: <Chip>VM</Chip>,
    metric: <TextMetric value={vm.state} label="State" />,
    detail: "Libvirt",
    secondary: "—",
    update: <span className="unraid-muted">—</span>,
    status: (
      <Chip dot tone={tone}>
        {vm.state}
      </Chip>
    ),
    search: searchable(server.name, server.source, vm.name, vm.state),
    attention: tone === "warn",
    updateAvailable: false,
    active: vm.state.toUpperCase() === "RUNNING",
    highCapacity: false,
  };
}

function notificationRow(server: UnraidServer, notification: UnraidNotification, index: number): ResourceRow {
  const tone = notification.importance === "ALERT" ? "crit" : "warn";
  return {
    key: `${server.source}:notice:${index}:${notification.title}`,
    kind: "notice",
    icon: notification.importance === "ALERT" ? "AL" : "WN",
    iconClass: tone,
    name: notification.title,
    sub: notification.time,
    type: <Chip>{notification.importance}</Chip>,
    metric: <TextMetric value={notification.time} label="Reported" />,
    detail: server.name,
    secondary: server.host,
    update: <span className="unraid-muted">—</span>,
    status: (
      <Chip dot tone={tone}>
        {notification.importance}
      </Chip>
    ),
    search: searchable(server.name, server.source, server.host, notification.title, notification.importance, notification.time),
    attention: true,
    updateAvailable: false,
    active: false,
    highCapacity: false,
  };
}

function TextMetric({ value, label }: { value: React.ReactNode; label: React.ReactNode }) {
  return (
    <div className="unraid-text-metric">
      <span>{value || "—"}</span>
      <span>{label}</span>
    </div>
  );
}
