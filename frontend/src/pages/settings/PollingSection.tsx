// Polling & tuning — how often sources are queried and how much history is kept.
import React from "react";
import { AppSettings, getSettings, putSettings } from "../../api";
import { Card } from "../../components";
import { Field, Msg, Tone } from "./shared";

export default function PollingSection() {
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
      {app && <TuningCard app={app} onSaved={setApp} onMsg={(tone, text) => setMsg({ tone, text })} />}
    </>
  );
}

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
