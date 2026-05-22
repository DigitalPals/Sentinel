// Cybex Sentinel — first-run onboarding wizard.
//
// Shown once, on a fresh database with no sources configured. Walks the user
// through connecting a Proxmox host and a UniFi controller and picking display
// preferences. Every step is optional and can be skipped.
import React from "react";
import {
  getSettings,
  getSources,
  putSettings,
  saveProxmoxSource,
  saveUnifiSource,
  testSource,
} from "./api";
import { ACCENTS, type Accent, type Density, type Settings } from "./settings";

type SetSetting = <K extends keyof Settings>(key: K, value: Settings[K]) => void;
const STEP_COUNT = 5;

/** Decide whether the first-run onboarding wizard should be shown. */
export function useOnboarding() {
  const [show, setShow] = React.useState(false);

  React.useEffect(() => {
    let alive = true;
    (async () => {
      try {
        const [settings, sources] = await Promise.all([getSettings(), getSources()]);
        if (!alive || settings.onboardingDone) return;
        if (sources.unifi.length || sources.proxmox.length) {
          // Already configured (e.g. via import-config) — nothing to onboard.
          putSettings({ onboardingDone: true }).catch(() => {});
        } else {
          setShow(true);
        }
      } catch {
        /* backend unreachable — don't block the UI with onboarding */
      }
    })();
    return () => {
      alive = false;
    };
  }, []);

  const complete = React.useCallback(() => {
    setShow(false);
    putSettings({ onboardingDone: true }).catch(() => {});
  }, []);

  return { show, complete };
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="set-field">
      <label>{label}</label>
      {children}
    </div>
  );
}

export function Onboarding({
  settings,
  setSetting,
  onDone,
}: {
  settings: Settings;
  setSetting: SetSetting;
  onDone: () => void;
}) {
  const [step, setStep] = React.useState(0);
  const [busy, setBusy] = React.useState(false);
  const [msg, setMsg] = React.useState<{ tone: "ok" | "err"; text: string } | null>(null);
  const [added, setAdded] = React.useState({ proxmox: 0, unifi: 0 });

  const [px, setPx] = React.useState({ name: "PVE1", host: "", tokenId: "", tokenSecret: "" });
  const [uni, setUni] = React.useState({ name: "UniFi", host: "", apiKey: "" });
  const [pollSec, setPollSec] = React.useState(15);

  const go = (n: number) => {
    setMsg(null);
    setStep(n);
  };

  const runTest = async (kind: "proxmox" | "unifi") => {
    setBusy(true);
    setMsg(null);
    try {
      const r = await testSource(
        kind === "proxmox"
          ? { kind, host: px.host, tokenId: px.tokenId, tokenSecret: px.tokenSecret }
          : { kind, host: uni.host, apiKey: uni.apiKey },
      );
      setMsg({ tone: r.ok ? "ok" : "err", text: r.detail });
    } catch (e: any) {
      setMsg({ tone: "err", text: String(e?.message ?? e) });
    } finally {
      setBusy(false);
    }
  };

  const addProxmox = async () => {
    setBusy(true);
    setMsg(null);
    try {
      await saveProxmoxSource(null, {
        name: px.name,
        host: px.host,
        tokenId: px.tokenId,
        tokenSecret: px.tokenSecret,
      });
      setAdded((a) => ({ ...a, proxmox: a.proxmox + 1 }));
      go(2);
    } catch (e: any) {
      setMsg({ tone: "err", text: String(e?.message ?? e) });
    } finally {
      setBusy(false);
    }
  };

  const addUnifi = async () => {
    setBusy(true);
    setMsg(null);
    try {
      await saveUnifiSource(null, { name: uni.name, host: uni.host, apiKey: uni.apiKey });
      setAdded((a) => ({ ...a, unifi: a.unifi + 1 }));
      go(3);
    } catch (e: any) {
      setMsg({ tone: "err", text: String(e?.message ?? e) });
    } finally {
      setBusy(false);
    }
  };

  const savePrefs = async () => {
    if (pollSec !== 15) {
      try {
        await putSettings({ pollIntervalSec: pollSec });
      } catch {
        /* non-fatal — the default stays in effect */
      }
    }
    go(4);
  };

  // ── Step content ──────────────────────────────────────────────────────────
  let body: React.ReactNode;
  if (step === 0) {
    body = (
      <div className="onb-step">
        <div className="onb-hero" />
        <h2 className="onb-title">Welcome to Cybex Sentinel</h2>
        <p className="onb-text">
          Sentinel keeps a live eye on your UniFi network and Proxmox VE infrastructure.
          Let's connect your first sources — it only takes a minute, and every step is
          optional.
        </p>
        <ul className="onb-features">
          <li>Live device &amp; guest inventory</li>
          <li>Threshold-based alerts with an acknowledge workflow</li>
          <li>A rolling 24-hour metric history</li>
        </ul>
      </div>
    );
  } else if (step === 1) {
    body = (
      <div className="onb-step">
        <div className="onb-step-tag">Step 1 · Proxmox VE</div>
        <h2 className="onb-title">Add a Proxmox host</h2>
        <p className="onb-text">
          Connect a Proxmox VE node or cluster with an API token — create one under
          Datacenter → Permissions → API Tokens (PVEAuditor on <code>/</code> is enough).
        </p>
        <div className="set-row set-grid-2">
          <Field label="Name">
            <input
              className="set-input"
              value={px.name}
              onChange={(e) => setPx({ ...px, name: e.target.value })}
            />
          </Field>
          <Field label="Host">
            <input
              className="set-input"
              value={px.host}
              placeholder="https://10.0.0.1:8006"
              onChange={(e) => setPx({ ...px, host: e.target.value })}
            />
          </Field>
          <Field label="Token ID">
            <input
              className="set-input"
              value={px.tokenId}
              placeholder="user@pve!token-name"
              onChange={(e) => setPx({ ...px, tokenId: e.target.value })}
            />
          </Field>
          <Field label="Token secret">
            <input
              className="set-input"
              type="password"
              value={px.tokenSecret}
              onChange={(e) => setPx({ ...px, tokenSecret: e.target.value })}
            />
          </Field>
        </div>
        <button className="set-btn" disabled={busy} onClick={() => runTest("proxmox")}>
          Test connection
        </button>
      </div>
    );
  } else if (step === 2) {
    body = (
      <div className="onb-step">
        <div className="onb-step-tag">Step 2 · UniFi Network</div>
        <h2 className="onb-title">Add a UniFi controller</h2>
        <p className="onb-text">
          Connect a UniFi Network controller (9.0+) with an API key — create one in the
          UniFi Network app under Settings → Control Plane → Integrations.
        </p>
        <div className="set-row set-grid-2">
          <Field label="Name">
            <input
              className="set-input"
              value={uni.name}
              onChange={(e) => setUni({ ...uni, name: e.target.value })}
            />
          </Field>
          <Field label="Host">
            <input
              className="set-input"
              value={uni.host}
              placeholder="https://10.0.0.1"
              onChange={(e) => setUni({ ...uni, host: e.target.value })}
            />
          </Field>
          <Field label="API key">
            <input
              className="set-input"
              type="password"
              value={uni.apiKey}
              onChange={(e) => setUni({ ...uni, apiKey: e.target.value })}
            />
          </Field>
        </div>
        <button className="set-btn" disabled={busy} onClick={() => runTest("unifi")}>
          Test connection
        </button>
      </div>
    );
  } else if (step === 3) {
    body = (
      <div className="onb-step">
        <div className="onb-step-tag">Step 3 · Preferences</div>
        <h2 className="onb-title">Make it yours</h2>
        <p className="onb-text">
          Pick a look and how often Sentinel polls your sources. All of this can be
          changed later from the Settings page.
        </p>
        <Field label="Accent">
          <div className="accent-swatches">
            {(Object.keys(ACCENTS) as Accent[]).map((key) => {
              const sw = ACCENTS[key].swatch;
              return (
                <button
                  key={key}
                  className="accent-swatch"
                  data-on={settings.accent === key ? "1" : "0"}
                  title={key}
                  style={{ background: `linear-gradient(135deg, ${sw[0]}, ${sw[1]})` }}
                  onClick={() => setSetting("accent", key)}
                />
              );
            })}
          </div>
        </Field>
        <Field label="Density">
          <div className="seg">
            {(["compact", "regular", "comfy"] as Density[]).map((d) => (
              <button
                key={d}
                className={settings.density === d ? "on" : ""}
                onClick={() => setSetting("density", d)}
              >
                {d}
              </button>
            ))}
          </div>
        </Field>
        <div style={{ maxWidth: 220 }}>
          <Field label="Poll interval (seconds)">
            <input
              className="set-input"
              type="number"
              value={pollSec}
              onChange={(e) => setPollSec(Number(e.target.value))}
            />
          </Field>
        </div>
      </div>
    );
  } else {
    const parts: string[] = [];
    if (added.proxmox) parts.push(`${added.proxmox} Proxmox host${added.proxmox > 1 ? "s" : ""}`);
    if (added.unifi) parts.push(`${added.unifi} UniFi controller${added.unifi > 1 ? "s" : ""}`);
    body = (
      <div className="onb-step onb-center">
        <div className="onb-check">✓</div>
        <h2 className="onb-title">You're all set</h2>
        <p className="onb-text">
          {parts.length
            ? `Connected ${parts.join(" and ")}. Sentinel is polling now — your dashboard will fill in shortly.`
            : "No sources connected yet — you can add them any time from the Settings page."}
        </p>
      </div>
    );
  }

  // ── Footer ────────────────────────────────────────────────────────────────
  let foot: React.ReactNode;
  if (step === 0) {
    foot = (
      <>
        <button className="set-btn" onClick={onDone}>
          Skip setup
        </button>
        <span className="spacer" />
        <button className="set-btn primary" onClick={() => go(1)}>
          Get started
        </button>
      </>
    );
  } else if (step === 1) {
    foot = (
      <>
        <button className="set-btn" disabled={busy} onClick={() => go(0)}>
          Back
        </button>
        <span className="spacer" />
        <button className="set-btn" disabled={busy} onClick={() => go(2)}>
          Skip
        </button>
        <button className="set-btn primary" disabled={busy} onClick={addProxmox}>
          Add host
        </button>
      </>
    );
  } else if (step === 2) {
    foot = (
      <>
        <button className="set-btn" disabled={busy} onClick={() => go(1)}>
          Back
        </button>
        <span className="spacer" />
        <button className="set-btn" disabled={busy} onClick={() => go(3)}>
          Skip
        </button>
        <button className="set-btn primary" disabled={busy} onClick={addUnifi}>
          Add controller
        </button>
      </>
    );
  } else if (step === 3) {
    foot = (
      <>
        <button className="set-btn" disabled={busy} onClick={() => go(2)}>
          Back
        </button>
        <span className="spacer" />
        <button className="set-btn primary" disabled={busy} onClick={savePrefs}>
          Continue
        </button>
      </>
    );
  } else {
    foot = (
      <>
        <button className="set-btn" onClick={() => go(3)}>
          Back
        </button>
        <span className="spacer" />
        <button className="set-btn primary" onClick={onDone}>
          Finish
        </button>
      </>
    );
  }

  return (
    <div className="onb-backdrop">
      <div className="onb-card">
        <div className="onb-head">
          <div className="brand-mark" />
          <div>
            <div className="brand-name">Cybex Sentinel</div>
            <div className="brand-tag">first-run setup</div>
          </div>
          <div className="onb-dots">
            {Array.from({ length: STEP_COUNT }).map((_, i) => (
              <span
                key={i}
                className={"onb-dot" + (i === step ? " active" : i < step ? " done" : "")}
              />
            ))}
          </div>
        </div>
        <div className="onb-body">{body}</div>
        {msg && <div className={"onb-msg " + msg.tone}>{msg.text}</div>}
        <div className="onb-foot">{foot}</div>
      </div>
    </div>
  );
}
