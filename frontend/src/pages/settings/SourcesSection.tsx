// Infrastructure sources — UniFi controllers and Proxmox hosts.
// Credentials are stored in the database; the form supports add, edit, delete,
// and live test-connection.
import React from "react";
import {
  ProxmoxSource,
  SourcesData,
  UnifiSource,
  deleteProxmoxSource,
  deleteUnifiSource,
  getSources,
  saveProxmoxSource,
  saveUnifiSource,
  testSource,
} from "../../api";
import { Card } from "../../components";
import { Field, Msg, Tone, Toggle } from "./shared";

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
        sub="UniFi and Proxmox endpoints Sentinel polls — credentials are stored in the database"
      >
        <div style={{ padding: "16px 18px", display: "flex", flexDirection: "column", gap: 22 }}>
          {!sources && <div className="set-note">Loading…</div>}
          {sources && (
            <>
              <SourceSection
                kind="unifi"
                sources={sources.unifi}
                onChanged={reload}
                onMsg={showMsg}
              />
              <SourceSection
                kind="proxmox"
                sources={sources.proxmox}
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
