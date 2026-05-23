// Alert thresholds — utilization levels that raise warning and critical alerts.
import React from "react";
import { AppSettings, getSettings, putSettings } from "../../api";
import { Card } from "../../components";
import { Field, Msg, Tone } from "./shared";

const THRESHOLD_GROUPS: { title: string; keys: [string, string][] }[] = [
  {
    title: "Proxmox guests",
    keys: [
      ["guest_mem_crit", "Memory critical %"],
      ["guest_mem_warn", "Memory warning %"],
      ["guest_cpu_warn", "CPU warning %"],
      ["guest_disk_warn", "Disk warning %"],
    ],
  },
  {
    title: "Proxmox nodes",
    keys: [
      ["node_mem_crit", "Memory critical %"],
      ["node_mem_warn", "Memory warning %"],
      ["node_cpu_crit", "CPU critical %"],
      ["node_cpu_warn", "CPU warning %"],
      ["node_disk_warn", "Disk warning %"],
    ],
  },
  {
    title: "UniFi devices",
    keys: [
      ["unifi_cpu_warn", "CPU warning %"],
      ["unifi_mem_warn", "Memory warning %"],
    ],
  },
  {
    title: "Unraid servers",
    keys: [
      ["unraid_cpu_warn", "CPU warning %"],
      ["unraid_mem_warn", "Memory warning %"],
      ["unraid_array_warn", "Array usage warning %"],
      ["unraid_disk_warn", "Disk usage warning %"],
      ["unraid_temp_warn", "Temperature warning C"],
      ["unraid_temp_crit", "Temperature critical C"],
    ],
  },
];

export default function AlertsSection() {
  const [app, setApp] = React.useState<AppSettings | null>(null);
  const [msg, setMsg] = React.useState<Msg>(null);

  React.useEffect(() => {
    getSettings()
      .then(setApp)
      .catch((e) => setMsg({ tone: "err", text: String(e?.message ?? e) }));
  }, []);

  return (
    <>
      {msg && <div className={"set-banner " + msg.tone}>{msg.text}</div>}
      {app && <ThresholdsCard app={app} onSaved={setApp} onMsg={(tone, text) => setMsg({ tone, text })} />}
    </>
  );
}

function ThresholdsCard({
  app,
  onSaved,
  onMsg,
}: {
  app: AppSettings;
  onSaved: (s: AppSettings) => void;
  onMsg: (tone: Tone, text: string) => void;
}) {
  const [t, setT] = React.useState<Record<string, number>>({ ...app.thresholds });
  const [busy, setBusy] = React.useState(false);

  const save = async () => {
    setBusy(true);
    try {
      onSaved(await putSettings({ thresholds: t }));
      onMsg("ok", "Alert thresholds saved.");
    } catch (e: any) {
      onMsg("err", String(e?.message ?? e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card title="Alert Thresholds" sub="utilization levels that raise warning and critical alerts">
      <div style={{ padding: "16px 18px" }}>
        {THRESHOLD_GROUPS.map((g) => (
          <div key={g.title} style={{ marginBottom: 14 }}>
            <div className="set-subhd-sm">{g.title}</div>
            <div className="set-row set-grid-3">
              {g.keys.map(([key, label]) => (
                <Field key={key} label={label}>
                  <input
                    className="set-input"
                    type="number"
                    value={t[key] ?? 0}
                    onChange={(e) => setT({ ...t, [key]: Number(e.target.value) })}
                  />
                </Field>
              ))}
            </div>
          </div>
        ))}
        <div className="set-actions">
          <button className="set-btn primary" disabled={busy} onClick={save}>
            Save
          </button>
        </div>
      </div>
    </Card>
  );
}
