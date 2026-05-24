// Network scanner settings — ranges, exclusions, timing and optional port scan.
import React from "react";
import {
  AppSettings,
  NetworkScannerSettings,
  PortProfile,
  PortScanTechnique,
  DiscoveryMethod,
  getSettings,
  putSettings,
  startNetworkScan,
} from "../../api";
import { Card } from "../../components";
import { Field, Msg, Tone, Toggle } from "./shared";

const splitTargets = (text: string): string[] =>
  text
    .split(/[\n,]+/)
    .map((v) => v.trim())
    .filter(Boolean);

const joinTargets = (values: string[]): string => values.join("\n");

export default function NetworkScannerSection() {
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
      {app && (
        <ScannerCard
          settings={app.networkScanner}
          onSaved={setApp}
          onMsg={(tone, text) => setMsg({ tone, text })}
        />
      )}
    </>
  );
}

function ScannerCard({
  settings,
  onSaved,
  onMsg,
}: {
  settings: NetworkScannerSettings;
  onSaved: (s: AppSettings) => void;
  onMsg: (tone: Tone, text: string) => void;
}) {
  const [d, setD] = React.useState(settings);
  const [rangesText, setRangesText] = React.useState(joinTargets(settings.ranges));
  const [excludeText, setExcludeText] = React.useState(joinTargets(settings.exclude));
  const [dnsServersText, setDnsServersText] = React.useState(
    joinTargets(settings.discovery.dnsServers ?? []),
  );
  const [busy, setBusy] = React.useState(false);

  const discovery = <K extends keyof NetworkScannerSettings["discovery"]>(
    key: K,
    value: NetworkScannerSettings["discovery"][K],
  ) => setD({ ...d, discovery: { ...d.discovery, [key]: value } });

  const portScan = <K extends keyof NetworkScannerSettings["portScan"]>(
    key: K,
    value: NetworkScannerSettings["portScan"][K],
  ) => setD({ ...d, portScan: { ...d.portScan, [key]: value } });

  const schedule = <K extends keyof NetworkScannerSettings["schedule"]>(
    key: K,
    value: NetworkScannerSettings["schedule"][K],
  ) => setD({ ...d, schedule: { ...d.schedule, [key]: value } });

  const materialize = (): NetworkScannerSettings => ({
    ...d,
    ranges: splitTargets(rangesText),
    exclude: splitTargets(excludeText),
    discovery: {
      ...d.discovery,
      dnsServers: splitTargets(dnsServersText),
    },
  });

  const save = async () => {
    setBusy(true);
    try {
      const next = await putSettings({ networkScanner: materialize() });
      onSaved(next);
      setD(next.networkScanner);
      setRangesText(joinTargets(next.networkScanner.ranges));
      setExcludeText(joinTargets(next.networkScanner.exclude));
      setDnsServersText(joinTargets(next.networkScanner.discovery.dnsServers ?? []));
      onMsg("ok", "Network scanner settings saved.");
    } catch (e: any) {
      onMsg("err", String(e?.message ?? e));
    } finally {
      setBusy(false);
    }
  };

  const runNow = async () => {
    setBusy(true);
    try {
      const scan = await startNetworkScan(materialize(), true);
      onMsg("ok", `Network scan job ${scan.id} queued.`);
    } catch (e: any) {
      onMsg("err", String(e?.message ?? e));
    } finally {
      setBusy(false);
    }
  };

  const num = (value: string) => Number(value || 0);

  return (
    <Card title="Network Scanner" sub="Nmap-backed LAN discovery and optional port inventory">
      <div className="network-settings">
        <div className="set-section">
          <div className="set-subhd">
            <span>Scope</span>
            <label className="set-inline">
              <Toggle on={d.enabled} onChange={(v) => setD({ ...d, enabled: v })} />
              <span>Enabled</span>
            </label>
          </div>
          <div className="set-row set-grid-2">
            <Field label="IP ranges" hint="CIDR, single IP, or Nmap range syntax">
              <textarea
                className="set-input set-textarea"
                value={rangesText}
                onChange={(e) => setRangesText(e.target.value)}
              />
            </Field>
            <Field label="Exclude IPs" hint="one per line or comma separated">
              <textarea
                className="set-input set-textarea"
                value={excludeText}
                onChange={(e) => setExcludeText(e.target.value)}
              />
            </Field>
          </div>
        </div>

        <div className="set-section">
          <div className="set-subhd">Discovery</div>
          <div className="set-row set-grid-3">
            <Field label="Method">
              <select
                className="set-input"
                value={d.discovery.method}
                onChange={(e) => discovery("method", e.target.value as DiscoveryMethod)}
              >
                <option value="auto">Auto</option>
                <option value="arp">ARP</option>
                <option value="icmpTcp">ICMP + TCP</option>
              </select>
            </Field>
            <Field label="Timing">
              <select
                className="set-input"
                value={d.discovery.timingTemplate}
                onChange={(e) => discovery("timingTemplate", num(e.target.value))}
              >
                <option value={3}>T3</option>
                <option value={4}>T4</option>
                <option value={5}>T5</option>
              </select>
            </Field>
            <Field label="DNS resolution">
              <label className="set-inline">
                <Toggle
                  on={d.discovery.dnsResolution}
                  onChange={(v) => discovery("dnsResolution", v)}
                />
                <span>{d.discovery.dnsResolution ? "On" : "Off"}</span>
              </label>
            </Field>
            <Field label="DNS servers" hint="optional override, e.g. 10.10.0.1">
              <input
                className="set-input"
                value={dnsServersText}
                onChange={(e) => setDnsServersText(e.target.value)}
              />
            </Field>
            <Field label="Max retries">
              <input
                className="set-input"
                type="number"
                min={0}
                max={10}
                value={d.discovery.maxRetries}
                onChange={(e) => discovery("maxRetries", num(e.target.value))}
              />
            </Field>
            <Field label="Host timeout (ms)">
              <input
                className="set-input"
                type="number"
                min={250}
                value={d.discovery.hostTimeoutMs}
                onChange={(e) => discovery("hostTimeoutMs", num(e.target.value))}
              />
            </Field>
            <Field label="Overall timeout (s)">
              <input
                className="set-input"
                type="number"
                min={10}
                value={d.discovery.overallTimeoutSec}
                onChange={(e) => discovery("overallTimeoutSec", num(e.target.value))}
              />
            </Field>
            <Field label="Min packet rate">
              <input
                className="set-input"
                type="number"
                min={0}
                value={d.discovery.minRate}
                onChange={(e) => discovery("minRate", num(e.target.value))}
              />
            </Field>
            <Field label="Retention (days)">
              <input
                className="set-input"
                type="number"
                min={1}
                value={d.retentionDays}
                onChange={(e) => setD({ ...d, retentionDays: num(e.target.value) })}
              />
            </Field>
          </div>
        </div>

        <div className="set-section">
          <div className="set-subhd">
            <span>Port scan</span>
            <label className="set-inline">
              <Toggle
                on={d.portScan.enabled}
                onChange={(v) => portScan("enabled", v)}
              />
              <span>{d.portScan.enabled ? "Enabled" : "Disabled"}</span>
            </label>
          </div>
          <div className="set-row set-grid-3">
            <Field label="Profile">
              <select
                className="set-input"
                value={d.portScan.profile}
                onChange={(e) => portScan("profile", e.target.value as PortProfile)}
              >
                <option value="fast">Fast list</option>
                <option value="top100">Top 100</option>
                <option value="top1000">Top 1000</option>
                <option value="custom">Custom</option>
              </select>
            </Field>
            <Field label="Technique">
              <select
                className="set-input"
                value={d.portScan.scanTechnique}
                onChange={(e) => portScan("scanTechnique", e.target.value as PortScanTechnique)}
              >
                <option value="syn">SYN</option>
                <option value="connect">Connect</option>
              </select>
            </Field>
            <Field label="Host discovery">
              <label className="set-inline">
                <Toggle
                  on={d.portScan.skipHostDiscovery}
                  onChange={(v) => portScan("skipHostDiscovery", v)}
                />
                <span>{d.portScan.skipHostDiscovery ? "Skip" : "Probe"}</span>
              </label>
            </Field>
            <Field label="Ports" hint="used by fast and custom profiles">
              <input
                className="set-input"
                value={d.portScan.ports}
                onChange={(e) => portScan("ports", e.target.value)}
              />
            </Field>
            <Field label="Service detection">
              <label className="set-inline">
                <Toggle
                  on={d.portScan.serviceDetection}
                  onChange={(v) => portScan("serviceDetection", v)}
                />
                <span>{d.portScan.serviceDetection ? "On" : "Off"}</span>
              </label>
            </Field>
            <Field label="OS detection">
              <label className="set-inline">
                <Toggle
                  on={d.portScan.osDetection}
                  onChange={(v) => portScan("osDetection", v)}
                />
                <span>{d.portScan.osDetection ? "On" : "Off"}</span>
              </label>
            </Field>
            <Field label="UDP scan">
              <label className="set-inline">
                <Toggle on={d.portScan.udpScan} onChange={(v) => portScan("udpScan", v)} />
                <span>{d.portScan.udpScan ? "On" : "Off"}</span>
              </label>
            </Field>
            <Field label="Only discovered hosts">
              <label className="set-inline">
                <Toggle
                  on={d.portScan.onlyScanDiscovered}
                  onChange={(v) => portScan("onlyScanDiscovered", v)}
                />
                <span>{d.portScan.onlyScanDiscovered ? "Yes" : "No"}</span>
              </label>
            </Field>
          </div>
        </div>

        <div className="set-section">
          <div className="set-subhd">
            <span>Schedule</span>
            <label className="set-inline">
              <Toggle on={d.schedule.enabled} onChange={(v) => schedule("enabled", v)} />
              <span>{d.schedule.enabled ? "Enabled" : "Disabled"}</span>
            </label>
          </div>
          <div className="set-row set-grid-3">
            <Field label="Interval (min)">
              <input
                className="set-input"
                type="number"
                min={5}
                value={d.schedule.intervalMinutes}
                onChange={(e) => schedule("intervalMinutes", num(e.target.value))}
              />
            </Field>
            <Field label="Run at start">
              <label className="set-inline">
                <Toggle
                  on={d.schedule.runAtStart}
                  onChange={(v) => schedule("runAtStart", v)}
                />
                <span>{d.schedule.runAtStart ? "Yes" : "No"}</span>
              </label>
            </Field>
          </div>
        </div>

        <div className="set-actions">
          <button className="set-btn" disabled={busy} onClick={runNow}>
            Run scan now
          </button>
          <button className="set-btn primary" disabled={busy} onClick={save}>
            Save
          </button>
        </div>
      </div>
    </Card>
  );
}
