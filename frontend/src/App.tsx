// Cybex Sentinel — app shell, routing and live snapshot wiring.
import React from "react";
import { useSnapshot } from "./api";
import { Sidebar, Topbar } from "./components";
import { SettingsPanel, useSettings } from "./settings";
import Dashboard from "./pages/Dashboard";
import Unifi from "./pages/Unifi";
import Proxmox from "./pages/Proxmox";
import Alerts from "./pages/Alerts";
import Events from "./pages/Events";

const PAGES: Record<string, { crumb: string; title: string }> = {
  dashboard: { crumb: "Overview / Cluster", title: "Operations Dashboard" },
  unifi: { crumb: "Network / UniFi", title: "UniFi Network Devices" },
  proxmox: { crumb: "Compute / Proxmox VE", title: "Proxmox Servers & Guests" },
  alerts: { crumb: "Operations / Alerts", title: "Alerts" },
  logs: { crumb: "Operations / Events & Logs", title: "Events & Logs" },
};

export default function App() {
  const { snap, ready, error, staleSec, refresh } = useSnapshot();
  const { settings, setSetting } = useSettings();
  const [page, setPage] = React.useState<string>(() => {
    const h = (location.hash || "").replace("#", "");
    return PAGES[h] ? h : "dashboard";
  });
  const [settingsOpen, setSettingsOpen] = React.useState(false);

  React.useEffect(() => {
    location.hash = page;
  }, [page]);

  React.useEffect(() => {
    const onHash = () => {
      const h = (location.hash || "").replace("#", "");
      if (PAGES[h]) setPage(h);
    };
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);

  // Reveal scrollbars only while the user is actively scrolling.
  React.useEffect(() => {
    let timer: number | undefined;
    const onScroll = () => {
      document.documentElement.classList.add("is-scrolling");
      clearTimeout(timer);
      timer = window.setTimeout(
        () => document.documentElement.classList.remove("is-scrolling"),
        700,
      );
    };
    window.addEventListener("scroll", onScroll, { capture: true, passive: true });
    return () => {
      clearTimeout(timer);
      window.removeEventListener("scroll", onScroll, { capture: true });
    };
  }, []);

  if (!ready || !snap) {
    return (
      <div className="boot">
        <div className="brand-mark" />
        {error ? (
          <div className="boot-msg err">Cannot reach the Sentinel backend — {error}. Retrying…</div>
        ) : (
          <>
            <div className="boot-spin" />
            <div className="boot-msg">Connecting to Cybex Sentinel…</div>
          </>
        )}
      </div>
    );
  }

  const meta = PAGES[page] || PAGES.dashboard;
  const alertCount = snap.alerts.alerts.filter((a) => a.status === "open").length;
  const stale = staleSec > Math.max(20, snap.pollIntervalSec * 3);

  let pageEl: React.ReactNode;
  switch (page) {
    case "unifi":
      pageEl = <Unifi snap={snap} />;
      break;
    case "proxmox":
      pageEl = <Proxmox snap={snap} />;
      break;
    case "alerts":
      pageEl = <Alerts snap={snap} refresh={refresh} />;
      break;
    case "logs":
      pageEl = <Events snap={snap} />;
      break;
    default:
      pageEl = <Dashboard snap={snap} />;
  }

  return (
    <div className={"app density-" + settings.density + (settings.showSpark ? "" : " no-spark")}>
      <Sidebar
        page={page}
        onNavigate={setPage}
        onSettings={() => setSettingsOpen(true)}
        alertCount={alertCount}
      />
      <main className="main">
        <Topbar
          crumb={meta.crumb}
          title={meta.title}
          sources={snap.sources}
          pollSec={snap.pollIntervalSec}
          staleSec={staleSec}
          alertCount={alertCount}
          onRefresh={refresh}
          onSettings={() => setSettingsOpen(true)}
          onAlerts={() => setPage("alerts")}
        />
        {stale && (
          <div style={{ padding: "16px 28px 0" }}>
            <div className="conn-banner">
              ⚠ Live data is stale — last successful update {staleSec}s ago. Reconnecting…
            </div>
          </div>
        )}
        {pageEl}
      </main>
      {settingsOpen && (
        <SettingsPanel settings={settings} setSetting={setSetting} onClose={() => setSettingsOpen(false)} />
      )}
    </div>
  );
}
