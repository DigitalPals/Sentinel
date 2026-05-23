// Cybex Sentinel — dynamic onboarding wizard.
//
// Onboarding is derived from real state, not a stored flag: it reappears after
// a restart whenever something still needs setting up.
//
//  • The integration section shows only while *no* source is
//    configured. As soon as any one source exists, it disappears — restarting
//    the app will not bring it back.
//  • The Welcome and Preferences steps show only in the browser session where
//    the first account was just created (`justSetUp`).
//
// Account creation itself is handled before this wizard, by the login/setup
// screen — see Login.tsx.
import React from "react";
import {
  getSources,
  putSettings,
  saveProxmoxSource,
  saveUnifiSource,
  saveUnraidSource,
  testSource,
} from "./api";
import { ACCENTS, type Accent, type Density, type Settings } from "./settings";
import logoUrl from "./assets/logo.svg";

type SetSetting = <K extends keyof Settings>(key: K, value: Settings[K]) => void;
type StepId = "welcome" | "proxmox" | "unifi" | "unraid" | "preferences" | "done";

export interface OnboardingState {
  /** Whether the wizard overlay should be shown. */
  show: boolean;
  /** True while no monitoring source has been enabled. */
  needsIntegrations: boolean;
  /** Re-check source state — call after a source is added. */
  refresh: () => void;
  /** Close the wizard for the rest of this session. */
  dismiss: () => void;
}

/** Decide, from live state, whether onboarding should be shown. `justSetUp` is
 *  true only in the session where the first account was just created — that is
 *  what brings back the Welcome/Preferences steps. */
export function useOnboarding(justSetUp: boolean): OnboardingState {
  const [needsIntegrations, setNeedsIntegrations] = React.useState(false);
  const [loaded, setLoaded] = React.useState(false);
  const [dismissed, setDismissed] = React.useState(false);

  const refresh = React.useCallback(() => {
    getSources()
      .then((s) => {
        const configured =
          s.proxmox.some((p) => p.enabled) ||
          s.unifi.some((u) => u.enabled) ||
          s.unraid.some((u) => u.enabled);
        setNeedsIntegrations(!configured);
        setLoaded(true);
      })
      .catch(() => {
        // Backend unreachable — don't block the UI with onboarding.
        setLoaded(true);
      });
  }, []);

  React.useEffect(() => {
    refresh();
  }, [refresh]);

  const dismiss = React.useCallback(() => setDismissed(true), []);
  const show = loaded && !dismissed && (justSetUp || needsIntegrations);
  return { show, needsIntegrations, refresh, dismiss };
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
  justSetUp,
  needsIntegrations,
  settings,
  setSetting,
  onDone,
}: {
  justSetUp: boolean;
  needsIntegrations: boolean;
  settings: Settings;
  setSetting: SetSetting;
  onDone: () => void;
}) {
  // The set of steps is fixed for the lifetime of the wizard — built once from
  // the state captured when it opened, so steps never shift mid-flow.
  const steps = React.useMemo<StepId[]>(() => {
    const s: StepId[] = [];
    if (justSetUp) s.push("welcome");
    if (needsIntegrations) s.push("proxmox", "unifi", "unraid");
    if (justSetUp) s.push("preferences");
    s.push("done");
    return s;
  }, [justSetUp, needsIntegrations]);

  const [idx, setIdx] = React.useState(0);
  const [busy, setBusy] = React.useState(false);
  const [msg, setMsg] = React.useState<{ tone: "ok" | "err"; text: string } | null>(null);
  const [added, setAdded] = React.useState({ proxmox: 0, unifi: 0, unraid: 0 });

  const [px, setPx] = React.useState({ name: "PVE1", host: "", tokenId: "", tokenSecret: "" });
  const [uni, setUni] = React.useState({ name: "UniFi", host: "", apiKey: "" });
  const [unraid, setUnraid] = React.useState({ name: "Unraid", host: "", apiKey: "" });
  const [pollSec, setPollSec] = React.useState(15);

  const current = steps[Math.min(idx, steps.length - 1)];
  const stepNo = idx + 1;
  const go = (n: number) => {
    setMsg(null);
    setIdx(Math.max(0, Math.min(n, steps.length - 1)));
  };
  const next = () => go(idx + 1);

  const runTest = async (kind: "proxmox" | "unifi" | "unraid") => {
    setBusy(true);
    setMsg(null);
    try {
      const r = await testSource(
        kind === "proxmox"
          ? { kind, host: px.host, tokenId: px.tokenId, tokenSecret: px.tokenSecret }
          : kind === "unifi"
            ? { kind, host: uni.host, apiKey: uni.apiKey }
            : { kind, host: unraid.host, apiKey: unraid.apiKey },
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
      next();
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
      next();
    } catch (e: any) {
      setMsg({ tone: "err", text: String(e?.message ?? e) });
    } finally {
      setBusy(false);
    }
  };

  const addUnraid = async () => {
    setBusy(true);
    setMsg(null);
    try {
      await saveUnraidSource(null, {
        name: unraid.name,
        host: unraid.host,
        apiKey: unraid.apiKey,
      });
      setAdded((a) => ({ ...a, unraid: a.unraid + 1 }));
      next();
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
    next();
  };

  // ── Step content ──────────────────────────────────────────────────────────
  let body: React.ReactNode;
  if (current === "welcome") {
    body = (
      <div className="onb-step">
        <div className="onb-hero" />
        <h2 className="onb-title">Welcome to Cybex Sentinel</h2>
        <p className="onb-text">
          Sentinel keeps a live eye on your UniFi network, Proxmox VE infrastructure and Unraid storage.
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
  } else if (current === "proxmox") {
    body = (
      <div className="onb-step">
        <div className="onb-step-tag">Step {stepNo} · Proxmox VE</div>
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
  } else if (current === "unifi") {
    body = (
      <div className="onb-step">
        <div className="onb-step-tag">Step {stepNo} · UniFi Network</div>
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
  } else if (current === "unraid") {
    body = (
      <div className="onb-step">
        <div className="onb-step-tag">Step {stepNo} · Unraid</div>
        <h2 className="onb-title">Add an Unraid server</h2>
        <p className="onb-text">
          Connect an Unraid server with the GraphQL API endpoint and API key. Use the
          same base URL you use for the Unraid web UI.
        </p>
        <div className="set-row set-grid-2">
          <Field label="Name">
            <input
              className="set-input"
              value={unraid.name}
              onChange={(e) => setUnraid({ ...unraid, name: e.target.value })}
            />
          </Field>
          <Field label="Host">
            <input
              className="set-input"
              value={unraid.host}
              placeholder="https://10.0.0.2"
              onChange={(e) => setUnraid({ ...unraid, host: e.target.value })}
            />
          </Field>
          <Field label="API key">
            <input
              className="set-input"
              type="password"
              value={unraid.apiKey}
              onChange={(e) => setUnraid({ ...unraid, apiKey: e.target.value })}
            />
          </Field>
        </div>
        <button className="set-btn" disabled={busy} onClick={() => runTest("unraid")}>
          Test connection
        </button>
      </div>
    );
  } else if (current === "preferences") {
    body = (
      <div className="onb-step">
        <div className="onb-step-tag">Step {stepNo} · Preferences</div>
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
    if (added.unraid) parts.push(`${added.unraid} Unraid server${added.unraid > 1 ? "s" : ""}`);
    body = (
      <div className="onb-step onb-center">
        <div className="onb-check">✓</div>
        <h2 className="onb-title">You're all set</h2>
        <p className="onb-text">
          {parts.length
            ? `Connected ${parts.join(" and ")}. Sentinel is polling now — your dashboard will fill in shortly.`
            : "Sentinel is ready. You can connect or manage sources any time from the Settings page."}
        </p>
      </div>
    );
  }

  // ── Footer ────────────────────────────────────────────────────────────────
  const backBtn =
    idx > 0 ? (
      <button className="set-btn" disabled={busy} onClick={() => go(idx - 1)}>
        Back
      </button>
    ) : null;

  let foot: React.ReactNode;
  if (current === "welcome") {
    foot = (
      <>
        <button className="set-btn" onClick={onDone}>
          Skip setup
        </button>
        <span className="spacer" />
        <button className="set-btn primary" onClick={next}>
          Get started
        </button>
      </>
    );
  } else if (current === "proxmox") {
    foot = (
      <>
        {backBtn}
        <span className="spacer" />
        <button className="set-btn" disabled={busy} onClick={next}>
          Skip
        </button>
        <button className="set-btn primary" disabled={busy} onClick={addProxmox}>
          Add host
        </button>
      </>
    );
  } else if (current === "unifi") {
    foot = (
      <>
        {backBtn}
        <span className="spacer" />
        <button className="set-btn" disabled={busy} onClick={next}>
          Skip
        </button>
        <button className="set-btn primary" disabled={busy} onClick={addUnifi}>
          Add controller
        </button>
      </>
    );
  } else if (current === "unraid") {
    foot = (
      <>
        {backBtn}
        <span className="spacer" />
        <button className="set-btn" disabled={busy} onClick={next}>
          Skip
        </button>
        <button className="set-btn primary" disabled={busy} onClick={addUnraid}>
          Add server
        </button>
      </>
    );
  } else if (current === "preferences") {
    foot = (
      <>
        {backBtn}
        <span className="spacer" />
        <button className="set-btn primary" disabled={busy} onClick={savePrefs}>
          Continue
        </button>
      </>
    );
  } else {
    foot = (
      <>
        {backBtn}
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
          <img src={logoUrl} alt="Cybex Sentinel" className="onb-logo" />
          <div className="onb-tag">{justSetUp ? "first-run setup" : "finish setup"}</div>
          <div className="onb-dots">
            {steps.map((_, i) => (
              <span
                key={i}
                className={"onb-dot" + (i === idx ? " active" : i < idx ? " done" : "")}
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
