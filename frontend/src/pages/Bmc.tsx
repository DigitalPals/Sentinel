// Cybex Sentinel — IPMI / Redfish BMC hardware telemetry.
import React from "react";
import type { BmcSensor, Snapshot } from "../api";
import { Card, Chip, Icon, KPI_COLORS, KpiTile, StatusDot } from "../components";
import { EditableGrid, type EditableLayoutValue, type LayoutStore } from "../layouts";

const KPI_LABELS = ["BMC Health", "Max Temp", "Sensor Readings", "Warnings"];

function fmtReading(s: BmcSensor): string {
  if (s.reading == null) return s.raw || "—";
  return `${Number.isInteger(s.reading) ? s.reading.toFixed(0) : s.reading.toFixed(1)} ${s.unit}`.trim();
}

export default function Bmc({
  snap,
  editMode,
  layoutStore,
  onLayoutChange,
  onConfigure,
}: {
  snap: Snapshot;
  editMode: boolean;
  layoutStore: LayoutStore;
  onLayoutChange: (pageId: string, layout: EditableLayoutValue) => void;
  onConfigure: () => void;
}) {
  const bmc = snap.bmc;
  const [kind, setKind] = React.useState<"all" | "temperature" | "fan" | "voltage" | "power" | "state">("all");
  const sensors = bmc.sensors.filter((s) => kind === "all" || s.kind === kind);
  const sources = snap.sources.filter((s) => s.kind === "bmc");
  const cards = [
    ...KPI_LABELS.map((label, i) => ({
      id: `kpi-${i}`,
      label,
      defaultSize: { w: 3, h: 1 },
      minW: 2,
      maxW: 6,
      minH: 1,
      maxH: 1,
      content: <KpiTile label={label} kpi={bmc.kpis[i] || { display: "—", unit: "", sub: "", trend: 0, spark: [] }} sparkColor={KPI_COLORS[i % KPI_COLORS.length]} />,
    })),
    {
      id: "controllers",
      label: "Controllers",
      defaultSize: { w: 12, h: 4 },
      minW: 6,
      minH: 3,
      content: <Card title="BMC Controllers" sub="Redfish system and manager inventory" actions={<button className="filter-pill" onClick={onConfigure}><Icon name="settings" /> Configure sources</button>} tight>
        {bmc.controllers.length === 0 && <div className="empty-row" style={{ padding: 28 }}>No BMC source configured yet. Add one under Settings → Sources.</div>}
        {bmc.controllers.map((c) => <div className="src-row" key={c.name}>
          <div className="src-row-id">
            <div className="src-row-name"><StatusDot tone={c.health.toLowerCase() === "ok" ? "ok" : "warn"} /> {c.name}</div>
            <div className="src-row-host">{c.host} · {c.manufacturer} {c.model} · BIOS {c.biosVersion}</div>
            <div className="src-row-host">BMC {c.managerModel} firmware {c.managerFirmware} · IPMI {c.ipmiAvailable ? `${c.ipmiVersion} ${c.ipmiFirmware}` : "not available"}</div>
          </div>
          <Chip tone={c.health.toLowerCase() === "ok" ? "ok" : "warn"}>{c.health}</Chip>
          <div className="src-row-actions" style={{ minWidth: 330 }}>
            <Chip tone={c.powerState.toLowerCase() === "on" ? "ok" : "default"}>{c.powerState}</Chip>
            <Chip>{c.processorCount} CPU</Chip>
            <Chip>{c.memoryGib} GiB RAM</Chip>
          </div>
        </div>)}
      </Card>,
    },
    {
      id: "sensors",
      label: "Sensors",
      defaultSize: { w: 7, h: 7 },
      minW: 5,
      minH: 4,
      content: <Card title="IPMI Sensors" sub="Temperatures, fan speeds, voltages and status sensors" tight>
        <div className="filters" style={{ padding: 14 }}>
          {(["all", "temperature", "fan", "voltage", "power", "state"] as const).map((k) => <button key={k} className={"filter-pill " + (kind === k ? "active" : "")} onClick={() => setKind(k)}>{k}</button>)}
        </div>
        {sensors.length === 0 && <div className="empty-row" style={{ padding: 28 }}>No matching sensors reported.</div>}
        {sensors.map((s) => <div className="src-row" key={`${s.source}:${s.name}`}>
          <div className="src-row-id"><div className="src-row-name"><StatusDot tone={s.tone} /> {s.name}</div><div className="src-row-host">{s.source} · {s.kind} · {s.status}</div></div>
          <Chip tone={s.tone}>{fmtReading(s)}</Chip>
        </div>)}
      </Card>,
    },
    {
      id: "drives",
      label: "Drives",
      defaultSize: { w: 5, h: 7 },
      minW: 4,
      minH: 4,
      content: <Card title="Redfish Drives" sub="Storage health reported by the BMC" tight>
        {bmc.drives.length === 0 && <div className="empty-row" style={{ padding: 28 }}>No Redfish drives reported.</div>}
        {bmc.drives.map((d) => <div className="src-row" key={`${d.source}:${d.name}`}>
          <div className="src-row-id"><div className="src-row-name"><StatusDot tone={d.tone} /> {d.name}</div><div className="src-row-host">{d.manufacturer} {d.model} · {d.serial}</div></div>
          <Chip tone={d.tone}>{d.health}</Chip>
          <div className="src-row-actions"><Chip>{d.capacity}</Chip></div>
        </div>)}
      </Card>,
    },
    {
      id: "sources",
      label: "Sources",
      defaultSize: { w: 12, h: 3 },
      minW: 6,
      minH: 2,
      content: <Card title="BMC Source Health" sub="Configured IPMI / Redfish endpoints" tight>
        {sources.length === 0 && <div className="empty-row" style={{ padding: 24 }}>No BMC source configured.</div>}
        {sources.map((s) => <div className="src-row" key={s.name}><div className="src-row-id"><div className="src-row-name"><StatusDot tone={s.ok ? "ok" : "crit"} /> {s.name}</div><div className="src-row-host">{s.detail}</div>{s.error && <div className="src-row-host" style={{ color: "var(--crit)" }}>{s.error}</div>}</div><Chip tone={s.ok ? "ok" : "crit"}>{s.ok ? "Online" : "Offline"}</Chip></div>)}
      </Card>,
    },
  ];
  return <div className="page"><EditableGrid pageId="bmc" items={cards} editMode={editMode} layoutStore={layoutStore} onLayoutChange={onLayoutChange} /></div>;
}
