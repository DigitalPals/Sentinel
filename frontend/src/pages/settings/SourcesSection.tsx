// Infrastructure sources — Unraid servers, UniFi controllers and Proxmox hosts.
// Credentials are stored in the database; the form supports add, edit, delete,
// and live test-connection.
import React from "react";
import {
  BmcSource,
  PbsSource,
  ProxmoxSource,
  SourcesData,
  UnraidSource,
  UnifiSource,
  deleteBmcSource,
  deletePbsSource,
  deleteProxmoxSource,
  deleteUnraidSource,
  deleteUnifiSource,
  getSources,
  saveBmcSource,
  savePbsSource,
  saveProxmoxSource,
  saveUnraidSource,
  saveUnifiSource,
  testSource,
} from "../../api";
import { Card } from "../../components";
import { Icon } from "../../icons";
import { Field, Msg, Tone, Toggle } from "./shared";

type Kind = "unifi" | "proxmox" | "pbs" | "bmc" | "unraid";

// Keep in sync with simple-icons brand hexes used by <Icon name="…">.
const BRAND_HEX: Record<Kind, string> = {
  unraid: "#F15A2C",
  unifi: "#0559C9",
  proxmox: "#E57000",
  pbs: "#7adfff",
  bmc: "#A855F7",
};
const LABEL: Record<Kind, string> = {
  unraid: "Unraid",
  unifi: "UniFi Network",
  proxmox: "Proxmox VE",
  pbs: "Proxmox Backup Server",
  bmc: "IPMI / Redfish BMC",
};
const TAGLINE: Record<Kind, string> = {
  unraid: "NAS & storage",
  unifi: "Network controller",
  proxmox: "Virtualisation hosts",
  pbs: "Backup datastores",
  bmc: "Server hardware telemetry",
};
const ADD_NOUN: Record<Kind, string> = {
  unraid: "server",
  unifi: "controller",
  proxmox: "host",
  pbs: "server",
  bmc: "controller",
};

function SourceForm({
  kind,
  source,
  onDone,
  onCancel,
  onMsg,
}: {
  kind: Kind;
  source: UnifiSource | ProxmoxSource | PbsSource | BmcSource | UnraidSource | null;
  onDone: () => void;
  onCancel: () => void;
  onMsg: (tone: Tone, text: string) => void;
}) {
  const isProxmox = kind === "proxmox";
  const isPbs = kind === "pbs";
  const isBmc = kind === "bmc";
  const isUnraid = kind === "unraid";
  const editing = source != null;
  const [name, setName] = React.useState(source?.name ?? (isProxmox ? "" : isPbs ? "PBS BlackBox" : isBmc ? "The Beast IPMI" : isUnraid ? "Unraid" : "UniFi"));
  const [host, setHost] = React.useState(source?.host ?? "");
  const [tokenId, setTokenId] = React.useState((source as ProxmoxSource | PbsSource | null)?.tokenId ?? "");
  const [username, setUsername] = React.useState((source as BmcSource | null)?.username ?? "admin");
  const [secret, setSecret] = React.useState("");
  const [enabled, setEnabled] = React.useState(source?.enabled ?? true);
  const [busy, setBusy] = React.useState(false);
  const [test, setTest] = React.useState<{ tone: Tone; text: string } | null>(null);

  const secretSet = editing && source?.hasSecret;
  const secretLabel = isProxmox || isPbs ? "Token secret" : isBmc ? "Password" : "API key";

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
      } else if (isPbs) {
        await savePbsSource(editing ? source!.id : null, {
          name,
          host,
          tokenId,
          tokenSecret: secret || undefined,
          enabled,
        });
      } else if (isBmc) {
        await saveBmcSource(editing ? source!.id : null, {
          name,
          host,
          username,
          password: secret || undefined,
          enabled,
        });
      } else if (isUnraid) {
        await saveUnraidSource(editing ? source!.id : null, {
          name,
          host,
          apiKey: secret || undefined,
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
      onMsg("ok", `${isProxmox ? "Proxmox" : isPbs ? "PBS" : isBmc ? "BMC" : isUnraid ? "Unraid" : "UniFi"} source ${editing ? "updated" : "added"}.`);
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
        isProxmox || isPbs
          ? { kind, id: source?.id, host, tokenId, tokenSecret: secret || undefined }
          : isBmc
            ? { kind, id: source?.id, host, username, password: secret || undefined }
          : isUnraid
            ? { kind, id: source?.id, host, apiKey: secret || undefined }
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
        <Field
          label="Host"
          hint={
            isProxmox
              ? "Include scheme and port, e.g. https://10.0.0.1:8006"
              : isPbs
                ? "Include scheme and port, e.g. https://10.10.0.41:8007"
              : isBmc
                ? "Include scheme, e.g. https://10.10.0.14"
              : isUnraid
                ? "Include scheme when known, e.g. https://10.10.0.40"
                : "Include scheme and port when needed, e.g. https://10.0.0.1"
          }
        >
          <input
            className="set-input"
            value={host}
            placeholder="https://…"
            onChange={(e) => setHost(e.target.value)}
          />
        </Field>
      </div>
      <div className="set-row set-grid-2">
        {(isProxmox || isPbs) && (
          <Field label="Token ID" hint="user@realm!token-name">
            <input
              className="set-input"
              value={tokenId}
              onChange={(e) => setTokenId(e.target.value)}
            />
          </Field>
        )}
        {isBmc && (
          <Field label="Username">
            <input
              className="set-input"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
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

function ServicePanel({
  kind,
  sources,
  onChanged,
  onMsg,
}: {
  kind: Kind;
  sources: (UnifiSource | ProxmoxSource | PbsSource | BmcSource | UnraidSource)[];
  onChanged: () => void;
  onMsg: (tone: Tone, text: string) => void;
}) {
  const [editId, setEditId] = React.useState<number | "new" | null>(null);
  const label = LABEL[kind];
  const addNoun = ADD_NOUN[kind];

  const remove = async (id: number, name: string) => {
    if (!window.confirm(`Delete source "${name}"?`)) return;
    try {
      if (kind === "proxmox") await deleteProxmoxSource(id);
      else if (kind === "pbs") await deletePbsSource(id);
      else if (kind === "bmc") await deleteBmcSource(id);
      else if (kind === "unraid") await deleteUnraidSource(id);
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

  const brandStyle = { ["--brand" as any]: BRAND_HEX[kind] } as React.CSSProperties;

  return (
    <section className={`src-service src-service-${kind}`} style={brandStyle}>
      <header className="src-service-hd">
        <span className="src-service-mark">
          <Icon name={kind} size={22} />
        </span>
        <div className="src-service-titles">
          <div className="src-service-title">{label}</div>
          <div className="src-service-sub">{TAGLINE[kind]}</div>
        </div>
        <span className="src-service-count">{sources.length}</span>
        {editId !== "new" && (
          <button className="set-btn primary src-service-add" onClick={() => setEditId("new")}>
            <Icon name="plus" size={12} /> Add {addNoun}
          </button>
        )}
      </header>

      <div className="src-service-body">
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
          <div className="src-empty">
            <span className="src-empty-mark">
              <Icon name={kind} size={28} />
            </span>
            <div className="src-empty-text">No {label} sources yet</div>
            <div className="src-empty-hint">Add a {addNoun} to start polling.</div>
          </div>
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
            <div className="src-row" key={s.id}>
              <div className="src-row-id">
                <div className="src-row-name">{s.name}</div>
                <div className="src-row-host">{s.host}</div>
              </div>
              <span className={"src-row-pill " + (s.enabled ? "ok" : "off")}>
                {s.enabled ? "Active" : "Disabled"}
              </span>
              <div className="src-row-actions">
                <button className="set-btn" onClick={() => setEditId(s.id)}>
                  Edit
                </button>
                <button className="set-btn danger" onClick={() => remove(s.id, s.name)}>
                  Delete
                </button>
              </div>
            </div>
          ),
        )}
      </div>
    </section>
  );
}

export default function SourcesSection() {
  const [sources, setSources] = React.useState<SourcesData | null>(null);
  const [msg, setMsg] = React.useState<Msg>(null);

  const showMsg = React.useCallback((tone: Tone, text: string) => setMsg({ tone, text }), []);

  const reload = React.useCallback(() => {
    getSources()
      .then(setSources)
      .catch((e) => showMsg("err", String(e?.message ?? e)));
  }, [showMsg]);

  React.useEffect(() => {
    reload();
  }, [reload]);

  return (
    <>
      {msg && <div className={"set-banner " + msg.tone}>{msg.text}</div>}
      <Card
        title="Infrastructure Sources"
        sub="Unraid, UniFi, Proxmox, PBS and IPMI/Redfish endpoints Sentinel polls — credentials are stored in the database"
      >
        <div className="src-services">
          {!sources && <div className="set-note">Loading…</div>}
          {sources && (
            <>
              <ServicePanel
                kind="unraid"
                sources={sources.unraid}
                onChanged={reload}
                onMsg={showMsg}
              />
              <ServicePanel
                kind="unifi"
                sources={sources.unifi}
                onChanged={reload}
                onMsg={showMsg}
              />
              <ServicePanel
                kind="proxmox"
                sources={sources.proxmox}
                onChanged={reload}
                onMsg={showMsg}
              />
              <ServicePanel
                kind="pbs"
                sources={sources.pbs}
                onChanged={reload}
                onMsg={showMsg}
              />
              <ServicePanel
                kind="bmc"
                sources={sources.bmc}
                onChanged={reload}
                onMsg={showMsg}
              />
            </>
          )}
        </div>
      </Card>
    </>
  );
}
