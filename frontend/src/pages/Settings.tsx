// Cybex Sentinel — Settings.
//
// Manages everything that used to live in config.toml and in hardcoded
// constants: infrastructure sources (with their API credentials), polling and
// tuning values, alert thresholds and display preferences. All of it is backed
// by the database through /api/settings and /api/sources.
import React from "react";
import {
  AppSettings,
  ProxmoxSource,
  SourcesData,
  UnifiSource,
  deleteProxmoxSource,
  deleteUnifiSource,
  getSettings,
  getSources,
  putSettings,
  saveProxmoxSource,
  saveUnifiSource,
  testSource,
} from "../api";
import { Card } from "../components";
import { DisplayControls, type Settings as DisplaySettings } from "../settings";

type Tone = "ok" | "err";
type Msg = { tone: Tone; text: string } | null;
type SetSetting = <K extends keyof DisplaySettings>(key: K, value: DisplaySettings[K]) => void;

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
];

// ── Small form primitives ───────────────────────────────────────────────────

function Field({
  label,
  children,
  hint,
}: {
  label: string;
  children: React.ReactNode;
  hint?: string;
}) {
  return (
    <div className="set-field">
      <label>{label}</label>
      {children}
      {hint && <span className="set-note">{hint}</span>}
    </div>
  );
}

function Toggle({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return (
    <button className="toggle" data-on={on ? "1" : "0"} aria-pressed={on} onClick={() => onChange(!on)}>
      <i />
    </button>
  );
}

// ── Source editor ───────────────────────────────────────────────────────────

function SourceForm({
  kind,
  source,
  onDone,
  onCancel,
  onMsg,
}: {
  kind: "unifi" | "proxmox";
  source: UnifiSource | ProxmoxSource | null;
  onDone: () => void;
  onCancel: () => void;
  onMsg: (tone: Tone, text: string) => void;
}) {
  const isProxmox = kind === "proxmox";
  const editing = source != null;
  const [name, setName] = React.useState(source?.name ?? (isProxmox ? "" : "UniFi"));
  const [host, setHost] = React.useState(source?.host ?? "");
  const [tokenId, setTokenId] = React.useState((source as ProxmoxSource | null)?.tokenId ?? "");
  const [secret, setSecret] = React.useState("");
  const [enabled, setEnabled] = React.useState(source?.enabled ?? true);
  const [busy, setBusy] = React.useState(false);
  const [test, setTest] = React.useState<{ tone: Tone; text: string } | null>(null);

  const secretSet = editing && source?.hasSecret;
  const secretLabel = isProxmox ? "Token secret" : "API key";

  const save = async () => {
    setBusy(true);
    setTest(null);
    try {
      if (isProxmox) {
        await saveProxmoxSource(editing ? source!.id : null, {
          name,
          host,
          tokenId,
          tokenSecret: secret || undefined,
          enabled,
        });
      } else {
        await saveUnifiSource(editing ? source!.id : null, {
          name,
          host,
          apiKey: secret || undefined,
          enabled,
        });
      }
      onMsg("ok", `${isProxmox ? "Proxmox" : "UniFi"} source ${editing ? "updated" : "added"}.`);
      onDone();
    } catch (e: any) {
      onMsg("err", String(e?.message ?? e));
    } finally {
      setBusy(false);
    }
  };

  const runTest = async () => {
    setBusy(true);
    setTest(null);
    try {
      const r = await testSource(
        isProxmox
          ? { kind, id: source?.id, host, tokenId, tokenSecret: secret || undefined }
          : { kind, id: source?.id, host, apiKey: secret || undefined },
      );
      setTest({ tone: r.ok ? "ok" : "err", text: r.detail });
    } catch (e: any) {
      setTest({ tone: "err", text: String(e?.message ?? e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="src-form">
      <div className="set-row set-grid-2">
        <Field label="Name">
          <input className="set-input" value={name} onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label="Host" hint="Include scheme and port, e.g. https://10.0.0.1:8006">
          <input
            className="set-input"
            value={host}
            placeholder="https://…"
            onChange={(e) => setHost(e.target.value)}
          />
        </Field>
      </div>
      <div className="set-row set-grid-2">
        {isProxmox && (
          <Field label="Token ID" hint="user@realm!token-name">
            <input
              className="set-input"
              value={tokenId}
              onChange={(e) => setTokenId(e.target.value)}
            />
          </Field>
        )}
        <Field label={secretLabel} hint={secretSet ? "Leave blank to keep the stored value" : undefined}>
          <input
            className="set-input"
            type="password"
            value={secret}
            placeholder={secretSet ? "•••••••• (unchanged)" : ""}
            onChange={(e) => setSecret(e.target.value)}
          />
        </Field>
      </div>
      <div className="src-form-foot">
        <label className="set-inline">
          <Toggle on={enabled} onChange={setEnabled} />
          <span>Enabled</span>
        </label>
        {test && <span className={"set-msg " + test.tone}>{test.text}</span>}
        <div className="set-actions">
          <button className="set-btn" disabled={busy} onClick={runTest}>
            Test connection
          </button>
          <button className="set-btn" disabled={busy} onClick={onCancel}>
            Cancel
          </button>
          <button className="set-btn primary" disabled={busy} onClick={save}>
            {editing ? "Save" : "Add source"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Source section (list + add/edit) ────────────────────────────────────────

function SourceSection({
  kind,
  sources,
  onChanged,
  onMsg,
}: {
  kind: "unifi" | "proxmox";
  sources: (UnifiSource | ProxmoxSource)[];
  onChanged: () => void;
  onMsg: (tone: Tone, text: string) => void;
}) {
  const [editId, setEditId] = React.useState<number | "new" | null>(null);
  const label = kind === "proxmox" ? "Proxmox VE" : "UniFi Network";

  const remove = async (id: number, name: string) => {
    if (!window.confirm(`Delete source "${name}"?`)) return;
    try {
      if (kind === "proxmox") await deleteProxmoxSource(id);
      else await deleteUnifiSource(id);
      onMsg("ok", `Source "${name}" deleted.`);
      onChanged();
    } catch (e: any) {
      onMsg("err", String(e?.message ?? e));
    }
  };

  const done = () => {
    setEditId(null);
    onChanged();
  };

  return (
    <div className="set-section">
      <div className="set-subhd">
        <span>{label}</span>
        {editId !== "new" && (
          <button className="set-btn" onClick={() => setEditId("new")}>
            + Add {kind === "proxmox" ? "host" : "controller"}
          </button>
        )}
      </div>

      {editId === "new" && (
        <SourceForm
          kind={kind}
          source={null}
          onDone={done}
          onCancel={() => setEditId(null)}
          onMsg={onMsg}
        />
      )}

      {sources.length === 0 && editId !== "new" && (
        <div className="set-note">No {label} sources configured.</div>
      )}

      {sources.map((s) =>
        editId === s.id ? (
          <SourceForm
            key={s.id}
            kind={kind}
            source={s}
            onDone={done}
            onCancel={() => setEditId(null)}
            onMsg={onMsg}
          />
        ) : (
          <div className="src-item" key={s.id}>
            <span className={"status-dot " + (s.enabled ? "ok" : "")} />
            <div style={{ minWidth: 0, flex: 1 }}>
              <div className="src-name">{s.name}</div>
              <div className="src-host">{s.host}</div>
            </div>
            {!s.enabled && <span className="set-note">disabled</span>}
            <button className="set-btn" onClick={() => setEditId(s.id)}>
              Edit
            </button>
            <button className="set-btn danger" onClick={() => remove(s.id, s.name)}>
              Delete
            </button>
          </div>
        ),
      )}
    </div>
  );
}

// ── Polling & tuning ────────────────────────────────────────────────────────

function TuningCard({
  app,
  onSaved,
  onMsg,
}: {
  app: AppSettings;
  onSaved: (s: AppSettings) => void;
  onMsg: (tone: Tone, text: string) => void;
}) {
  const [d, setD] = React.useState({
    pollIntervalSec: app.pollIntervalSec,
    httpTimeoutSec: app.httpTimeoutSec,
    historyMaxSamples: app.historyMaxSamples,
    historyRetentionDays: app.historyRetentionDays,
    frontendPollMs: app.frontendPollMs,
    bind: app.bind,
  });
  const [busy, setBusy] = React.useState(false);
  const num = (k: keyof typeof d) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setD({ ...d, [k]: Number(e.target.value) });

  const save = async () => {
    setBusy(true);
    try {
      onSaved(await putSettings(d));
      onMsg("ok", "Polling & tuning settings saved.");
    } catch (e: any) {
      onMsg("err", String(e?.message ?? e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card title="Polling & Tuning" sub="how often sources are queried and how much history is kept">
      <div style={{ padding: "16px 18px" }}>
        <div className="set-row set-grid-3">
          <Field label="Poll interval (s)" hint="minimum 5">
            <input
              className="set-input"
              type="number"
              value={d.pollIntervalSec}
              onChange={num("pollIntervalSec")}
            />
          </Field>
          <Field label="HTTP timeout (s)">
            <input
              className="set-input"
              type="number"
              value={d.httpTimeoutSec}
              onChange={num("httpTimeoutSec")}
            />
          </Field>
          <Field label="Frontend poll (ms)">
            <input
              className="set-input"
              type="number"
              value={d.frontendPollMs}
              onChange={num("frontendPollMs")}
            />
          </Field>
          <Field label="History working set" hint="samples kept in memory">
            <input
              className="set-input"
              type="number"
              value={d.historyMaxSamples}
              onChange={num("historyMaxSamples")}
            />
          </Field>
          <Field label="History retention (days)" hint="raw samples are dropped after this">
            <input
              className="set-input"
              type="number"
              value={d.historyRetentionDays}
              onChange={num("historyRetentionDays")}
            />
          </Field>
          <Field label="Bind address" hint="applied on restart">
            <input
              className="set-input"
              value={d.bind}
              onChange={(e) => setD({ ...d, bind: e.target.value })}
            />
          </Field>
        </div>
        <div className="set-actions" style={{ marginTop: 16 }}>
          <button className="set-btn primary" disabled={busy} onClick={save}>
            Save
          </button>
        </div>
      </div>
    </Card>
  );
}

// ── Alert thresholds ────────────────────────────────────────────────────────

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

// ── Page ────────────────────────────────────────────────────────────────────

export default function Settings({
  settings,
  setSetting,
}: {
  settings: DisplaySettings;
  setSetting: SetSetting;
}) {
  const [app, setApp] = React.useState<AppSettings | null>(null);
  const [sources, setSources] = React.useState<SourcesData | null>(null);
  const [msg, setMsg] = React.useState<Msg>(null);

  const showMsg = React.useCallback((tone: Tone, text: string) => setMsg({ tone, text }), []);

  const reloadSources = React.useCallback(() => {
    getSources()
      .then(setSources)
      .catch((e) => showMsg("err", String(e?.message ?? e)));
  }, [showMsg]);

  React.useEffect(() => {
    getSettings()
      .then(setApp)
      .catch((e) => showMsg("err", String(e?.message ?? e)));
    reloadSources();
  }, [reloadSources, showMsg]);

  return (
    <div className="page">
      {msg && <div className={"set-banner " + msg.tone}>{msg.text}</div>}

      <Card
        title="Infrastructure Sources"
        sub="UniFi and Proxmox endpoints Sentinel polls — credentials are stored in the database"
      >
        <div style={{ padding: "16px 18px", display: "flex", flexDirection: "column", gap: 22 }}>
          {!sources && <div className="set-note">Loading…</div>}
          {sources && (
            <>
              <SourceSection
                kind="unifi"
                sources={sources.unifi}
                onChanged={reloadSources}
                onMsg={showMsg}
              />
              <SourceSection
                kind="proxmox"
                sources={sources.proxmox}
                onChanged={reloadSources}
                onMsg={showMsg}
              />
            </>
          )}
        </div>
      </Card>

      {app && <TuningCard app={app} onSaved={setApp} onMsg={showMsg} />}
      {app && <ThresholdsCard app={app} onSaved={setApp} onMsg={showMsg} />}

      <Card title="Display" sub="accent, density and sparklines — shared across browsers">
        <div style={{ padding: "8px 18px 16px" }}>
          <DisplayControls settings={settings} setSetting={setSetting} />
        </div>
      </Card>
    </div>
  );
}
