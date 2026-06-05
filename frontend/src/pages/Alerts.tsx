// Cybex Sentinel — Alerts.
import React from "react";
import { alertAction, type Alert, type Snapshot } from "../api";
import { Card, Chip, Icon, KpiGrid } from "../components";

const KPI_LABELS = ["Open Alerts", "Acknowledged", "Resolved", "Tracked Conditions"];

function fmtAgo(min: number): string {
  if (min < 1) return "just now";
  if (min < 60) return `${min}m ago`;
  if (min < 1440) return `${Math.floor(min / 60)}h ${min % 60}m ago`;
  return `${Math.floor(min / 1440)}d ago`;
}

const sevLabel = (s: string) => (s === "crit" ? "Critical" : s === "warn" ? "Warning" : "Info");

export default function Alerts({
  snap,
  refresh,
  selectedId: routedSelectedId,
  onSelectAlert,
}: {
  snap: Snapshot;
  refresh: () => void;
  selectedId?: string | null;
  onSelectAlert?: (id: string) => void;
}) {
  const all = snap.alerts.alerts;
  const [tab, setTab] = React.useState<"open" | "ack" | "resolved" | "all">("open");
  const [sev, setSev] = React.useState("all");
  const [source, setSource] = React.useState("all");
  const [selectedId, setSelectedId] = React.useState<string | null>(routedSelectedId ?? null);
  const [busy, setBusy] = React.useState(false);

  React.useEffect(() => {
    setSelectedId(routedSelectedId ?? null);
  }, [routedSelectedId]);

  const counts = {
    open: all.filter((a) => a.status === "open").length,
    ack: all.filter((a) => a.status === "ack").length,
    resolved: all.filter((a) => a.status === "resolved").length,
    all: all.length,
  };

  const filtered = all.filter((a) => {
    if (tab !== "all" && a.status !== tab) return false;
    if (sev !== "all" && a.sev !== sev) return false;
    if (source !== "all" && a.source !== source) return false;
    return true;
  });

  const sel: Alert | undefined = all.find((a) => a.id === selectedId) || filtered[0];
  const hist = snap.alerts.histogram;
  const histMax = Math.max(1, ...hist);

  const selectAlert = (id: string) => {
    setSelectedId(id);
    onSelectAlert?.(id);
  };

  const act = async (action: string) => {
    if (!sel) return;
    setBusy(true);
    try {
      await alertAction(sel.id, action);
      await refresh();
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="page">
      <KpiGrid kpis={snap.alerts.kpis} labels={KPI_LABELS} />

      <Card title="Alert Activity" sub="last 24h · by hour raised" tight style={{ marginBottom: 14 }}>
        <div style={{ padding: "18px 22px", display: "grid", gridTemplateColumns: "1fr auto", gap: 24, alignItems: "center" }}>
          <div style={{ display: "flex", alignItems: "flex-end", gap: 4, height: 56 }}>
            {hist.map((v, i) => {
              const h = Math.max(2, (v / histMax) * 50);
              const tone = v >= 3 ? "var(--crit)" : v >= 2 ? "var(--warn)" : v >= 1 ? "var(--info)" : "var(--line-2)";
              return (
                <div key={i} style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", gap: 4 }}>
                  <div style={{ width: "100%", height: h, background: tone, opacity: v === 0 ? 0.4 : 1, borderRadius: 2 }} />
                  {i % 3 === 0 && (
                    <div style={{ fontFamily: "var(--mono)", fontSize: 10.5, color: "var(--fg-3)" }}>
                      {String(i).padStart(2, "0")}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
          <div style={{ display: "flex", gap: 16 }}>
            <div>
              <div style={{ fontFamily: "var(--mono)", fontSize: 10.5, color: "var(--fg-3)", textTransform: "uppercase", letterSpacing: ".1em" }}>
                24h total
              </div>
              <div style={{ fontFamily: "var(--mono)", fontSize: 18, marginTop: 4 }}>{hist.reduce((a, b) => a + b, 0)}</div>
            </div>
            <div>
              <div style={{ fontFamily: "var(--mono)", fontSize: 10.5, color: "var(--fg-3)", textTransform: "uppercase", letterSpacing: ".1em" }}>
                Tracked
              </div>
              <div style={{ fontFamily: "var(--mono)", fontSize: 18, marginTop: 4 }}>{counts.all}</div>
            </div>
          </div>
        </div>
      </Card>

      <div className="split">
        <Card tight>
          <div className="tabs">
            {(["open", "ack", "resolved", "all"] as const).map((t) => (
              <button key={t} className={"tab " + (tab === t ? "active" : "")} onClick={() => setTab(t)}>
                {t === "open" ? "Open" : t === "ack" ? "Acknowledged" : t === "resolved" ? "Resolved" : "All"}
                <span className="count">{counts[t]}</span>
              </button>
            ))}
          </div>

          <div className="filters">
            {[
              ["all", "All sev"],
              ["crit", "Critical"],
              ["warn", "Warning"],
              ["info", "Info"],
            ].map(([key, label]) => (
              <button
                key={key}
                className={"filter-pill " + (sev === key ? "active" : "")}
                onClick={() => setSev(key)}
              >
                {key !== "all" && <span className={"status-dot " + key} />}
                {label}
              </button>
            ))}
            <div className="filters-spacer" />
            <select className="select-mini" value={source} onChange={(e) => setSource(e.target.value)}>
              <option value="all">All sources</option>
              <option value="unifi">UniFi</option>
              <option value="proxmox">Proxmox</option>
            </select>
          </div>

          <div className="scroll-area" style={{ maxHeight: "calc(100vh - 460px)", minHeight: 320 }}>
            {filtered.length === 0 && <div className="empty-row" style={{ padding: 36 }}>No alerts match the current filters.</div>}
            {filtered.map((a) => (
              <div
                key={a.id}
                onClick={() => selectAlert(a.id)}
                className={"alert-row" + (sel && a.id === sel.id ? " selected" : "")}
              >
                <div className={"alert-sev " + a.sev}>
                  <Icon name={a.sev === "info" ? "info" : "alert"} />
                </div>
                <div className="alert-body">
                  <div className="alert-hd">
                    <span className="alert-title">{a.title}</span>
                    {a.status === "ack" && <Chip tone="warn">Acknowledged</Chip>}
                    {a.status === "resolved" && <Chip tone="ok">Resolved</Chip>}
                    {a.occurrences > 1 && <Chip>×{a.occurrences}</Chip>}
                  </div>
                  <div className="alert-meta">
                    <span className="mono">{a.source}</span>
                    <span className="dot-sep" />
                    <span className="mono">{a.host}</span>
                    <span className="dot-sep" />
                    <span className="mono">{a.target}</span>
                  </div>
                </div>
                <div className="alert-time">
                  <span className="alert-ago">{fmtAgo(a.ageMin)}</span>
                  {a.assignee && <span className="alert-assignee">↳ {a.assignee}</span>}
                </div>
              </div>
            ))}
          </div>
        </Card>

        {sel && (
          <Card tight>
            <div className="detail-h">
              <div className="crumb">{sel.source} · {sel.host}</div>
              <h3 className="detail-title">{sel.title}</h3>
              <div style={{ marginTop: 10, display: "flex", gap: 6, flexWrap: "wrap" }}>
                <Chip dot tone={sel.sev}>
                  {sevLabel(sel.sev)}
                </Chip>
                <Chip tone={sel.status === "open" ? "crit" : sel.status === "ack" ? "warn" : "ok"}>
                  {sel.status === "open" ? "Open" : sel.status === "ack" ? "Acknowledged" : "Resolved"}
                </Chip>
                {sel.occurrences > 1 && <Chip>{sel.occurrences} occurrences</Chip>}
              </div>
            </div>

            <div style={{ padding: "14px 18px", fontSize: 12.5, color: "var(--fg-1)", lineHeight: 1.55 }}>{sel.desc}</div>

            <hr className="div" />

            <div className="detail-grid">
              <div>
                <div className="dg-k">Raised</div>
                <div className="dg-v">{fmtAgo(sel.ageMin)}</div>
              </div>
              <div>
                <div className="dg-k">Source</div>
                <div className="dg-v">{sel.source}</div>
              </div>
              <div>
                <div className="dg-k">Host</div>
                <div className="dg-v">{sel.host}</div>
              </div>
              <div>
                <div className="dg-k">Occurrences</div>
                <div className="dg-v">{sel.occurrences}</div>
              </div>
              <div style={{ gridColumn: "1 / -1" }}>
                <div className="dg-k">Target</div>
                <div className="dg-v">{sel.target}</div>
              </div>
              <div style={{ gridColumn: "1 / -1" }}>
                <div className="dg-k">Trigger Rule</div>
                <div className="dg-v" style={{ color: "var(--accent)" }}>
                  {sel.rule}
                </div>
              </div>
              <div>
                <div className="dg-k">Assignee</div>
                <div className="dg-v">{sel.assignee || "Unassigned"}</div>
              </div>
            </div>

            <hr className="div" />

            <div style={{ padding: "14px 18px" }}>
              <div className="dg-k">Timeline</div>
              <div className="tl">
                <div className="tl-row">
                  <span className={"tl-dot " + sel.sev} />
                  <div>
                    <div className="tl-msg">Alert raised by Sentinel rule engine</div>
                    <div className="tl-when mono">{fmtAgo(sel.ageMin)}</div>
                  </div>
                </div>
                {sel.occurrences > 1 && (
                  <div className="tl-row">
                    <span className="tl-dot warn" />
                    <div>
                      <div className="tl-msg">Condition re-fired ({sel.occurrences - 1}×)</div>
                      <div className="tl-when mono">while monitoring</div>
                    </div>
                  </div>
                )}
                {sel.status === "ack" && (
                  <div className="tl-row">
                    <span className="tl-dot info" />
                    <div>
                      <div className="tl-msg">Acknowledged by {sel.assignee || "operator"}</div>
                      <div className="tl-when mono">awaiting resolution</div>
                    </div>
                  </div>
                )}
                {sel.status === "resolved" && (
                  <div className="tl-row">
                    <span className="tl-dot ok" />
                    <div>
                      <div className="tl-msg">Marked resolved</div>
                      <div className="tl-when mono">closed by operator</div>
                    </div>
                  </div>
                )}
              </div>
            </div>

            <div style={{ padding: "12px 18px", display: "flex", gap: 8, borderTop: "1px solid var(--line)", flexWrap: "wrap" }}>
              {sel.status === "open" && (
                <>
                  <button className="filter-pill active" disabled={busy} onClick={() => act("ack")}>
                    Acknowledge
                  </button>
                  <button className="filter-pill" disabled={busy} onClick={() => act("resolve")}>
                    Resolve
                  </button>
                </>
              )}
              {sel.status === "ack" && (
                <>
                  <button className="filter-pill active" disabled={busy} onClick={() => act("resolve")}>
                    Mark resolved
                  </button>
                  <button className="filter-pill" disabled={busy} onClick={() => act("reopen")}>
                    Reopen
                  </button>
                </>
              )}
              {sel.status === "resolved" && (
                <button className="filter-pill" disabled={busy} onClick={() => act("reopen")}>
                  Reopen
                </button>
              )}
            </div>
          </Card>
        )}
      </div>
    </div>
  );
}
