// Cybex Sentinel — app shell, authentication gate, routing and live snapshot wiring.
import React from "react";
import { authLogout, getAuthStatus, useSnapshot, type AuthStatus } from "./api";
import { Sidebar, Topbar } from "./components";
import { SettingsPanel, useSettings } from "./settings";
import Dashboard from "./pages/Dashboard";
import Unifi from "./pages/Unifi";
import Proxmox from "./pages/Proxmox";
import Unraid from "./pages/Unraid";
import Alerts from "./pages/Alerts";
import Events from "./pages/Events";
import Settings from "./pages/Settings";
import {
  DEFAULT_SECTION,
  SECTION_CRUMB,
  SectionId,
  isSectionId,
} from "./pages/settings/index";
import { Onboarding, useOnboarding } from "./onboarding";
import { Login } from "./Login";
import logoUrl from "./assets/logo.svg";

const PAGES: Record<string, { crumb: string; title: string }> = {
  dashboard: { crumb: "Overview / Cluster", title: "Operations Dashboard" },
  unifi: { crumb: "Network / UniFi", title: "UniFi Network Devices" },
  proxmox: { crumb: "Compute / Proxmox VE", title: "Proxmox Servers & Guests" },
  unraid: { crumb: "Storage / Unraid", title: "Unraid Servers" },
  alerts: { crumb: "Operations / Alerts", title: "Alerts" },
  logs: { crumb: "Operations / Events & Logs", title: "Events & Logs" },
  settings: { crumb: "System / Configuration", title: "Settings" },
};

/** Resolve the active page from the URL path, with legacy #hash fallback. */
function resolvePage(): string {
  const path = location.pathname.replace(/^\/+|\/+$/g, "");
  if (PAGES[path]) return path;
  // /settings/<section> still resolves to the settings page.
  if (path === "settings" || path.startsWith("settings/")) return "settings";
  const hash = location.hash.replace(/^#/, "");
  if (PAGES[hash]) return hash;
  return "dashboard";
}

/** Parse the sub-section out of /settings/<section>, falling back to the default. */
function resolveSettingsSection(): SectionId {
  const m = location.pathname.match(/^\/settings\/([^/?#]+)/);
  const s = m?.[1];
  return s && isSectionId(s) ? s : DEFAULT_SECTION;
}

const pathForPage = (p: string, section?: string): string => {
  if (p === "dashboard") return "/";
  if (p === "settings") return "/settings/" + (section ?? DEFAULT_SECTION);
  return "/" + p;
};

/** Tracks authentication status and re-checks it whenever a session is lost. */
function useAuth() {
  const [status, setStatus] = React.useState<AuthStatus | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  const refresh = React.useCallback(async () => {
    try {
      setStatus(await getAuthStatus());
      setError(null);
    } catch (e: any) {
      setError(String(e?.message ?? e));
    }
  }, []);

  React.useEffect(() => {
    refresh();
  }, [refresh]);

  // Keep retrying while the backend is unreachable.
  React.useEffect(() => {
    if (status || !error) return;
    const t = window.setTimeout(refresh, 2000);
    return () => window.clearTimeout(t);
  }, [status, error, refresh]);

  // A dropped session (a 401 from any API call) sends us back to re-check.
  React.useEffect(() => {
    const onUnauth = () => refresh();
    window.addEventListener("sentinel-unauthorized", onUnauth);
    return () => window.removeEventListener("sentinel-unauthorized", onUnauth);
  }, [refresh]);

  return { status, error, refresh };
}

/** Authentication gate: shows the login / first-run setup screen until a valid
 *  session exists, only then mounting the dashboard (and its polling). */
export default function App() {
  const { status, error, refresh } = useAuth();
  // True only in the session where the first account was just created — drives
  // how much of the onboarding wizard is shown.
  const [justSetUp, setJustSetUp] = React.useState(false);

  const onAuthenticated = React.useCallback(
    (didSetUp: boolean) => {
      if (didSetUp) setJustSetUp(true);
      refresh();
    },
    [refresh],
  );

  const onLogout = React.useCallback(async () => {
    try {
      await authLogout();
    } catch {
      /* ignore — the cookie is cleared regardless, dropping us to login */
    }
    setJustSetUp(false);
    refresh();
  }, [refresh]);

  if (!status) {
    return (
      <div className="boot">
        <img src={logoUrl} alt="Cybex Sentinel" className="boot-logo" />
        {error ? (
          <div className="boot-msg err">
            Cannot reach the Sentinel backend — {error}. Retrying…
          </div>
        ) : (
          <>
            <div className="boot-spin" />
            <div className="boot-msg">Connecting to Cybex Sentinel…</div>
          </>
        )}
      </div>
    );
  }

  if (status.needsFirstUser) {
    return <Login needsFirstUser onAuthenticated={onAuthenticated} />;
  }
  if (!status.authenticated) {
    return <Login onAuthenticated={onAuthenticated} />;
  }
  return (
    <AppBody
      justSetUp={justSetUp}
      onLogout={onLogout}
      username={status.username ?? ""}
    />
  );
}

/** The authenticated dashboard. Mounted only once a session exists, so snapshot
 *  polling never runs on the login screen. */
function AppBody({
  justSetUp,
  onLogout,
  username,
}: {
  justSetUp: boolean;
  onLogout: () => void;
  username: string;
}) {
  const { snap, ready, error, staleSec, refresh } = useSnapshot();
  const { settings, setSetting } = useSettings();
  const onboarding = useOnboarding(justSetUp);
  const [page, setPageState] = React.useState<string>(resolvePage);
  const [settingsSection, setSettingsSection] =
    React.useState<SectionId>(resolveSettingsSection);
  const [settingsOpen, setSettingsOpen] = React.useState(false);

  const navigate = React.useCallback((p: string, section?: string) => {
    const target = pathForPage(p, section);
    if (location.pathname !== target) {
      window.history.pushState({}, "", target);
    }
    setPageState(p);
    if (p === "settings") {
      setSettingsSection(
        section && isSectionId(section) ? section : DEFAULT_SECTION,
      );
    }
    window.scrollTo(0, 0);
  }, []);

  // Path-based routing: react to back/forward, and migrate legacy #hash URLs.
  React.useEffect(() => {
    const current = location.pathname.replace(/^\/+|\/+$/g, "");
    if (current === "settings") {
      // Bare /settings → default sub-section.
      window.history.replaceState({}, "", "/settings/" + DEFAULT_SECTION);
    } else if (!PAGES[current] && !current.startsWith("settings/")) {
      const hash = location.hash.replace(/^#/, "");
      if (PAGES[hash]) window.history.replaceState({}, "", pathForPage(hash));
    }
    const onPop = () => {
      setPageState(resolvePage());
      setSettingsSection(resolveSettingsSection());
    };
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, []);

  // Keep the document title in sync with the active page.
  React.useEffect(() => {
    if (page === "settings") {
      document.title = `Cybex Sentinel · Settings · ${SECTION_CRUMB[settingsSection]}`;
    } else {
      document.title = PAGES[page]
        ? `Cybex Sentinel · ${PAGES[page].title}`
        : "Cybex Sentinel";
    }
  }, [page, settingsSection]);

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

  const onboardingEl = onboarding.show ? (
    <Onboarding
      justSetUp={justSetUp}
      needsIntegrations={onboarding.needsIntegrations}
      settings={settings}
      setSetting={setSetting}
      onDone={() => {
        onboarding.dismiss();
        onboarding.refresh();
      }}
    />
  ) : null;

  if (!ready || !snap) {
    return (
      <>
        {onboardingEl}
        <div className="boot">
          <img src={logoUrl} alt="Cybex Sentinel" className="boot-logo" />
          {error ? (
            <div className="boot-msg err">Cannot reach the Sentinel backend — {error}. Retrying…</div>
          ) : (
            <>
              <div className="boot-spin" />
              <div className="boot-msg">Connecting to Cybex Sentinel…</div>
            </>
          )}
        </div>
      </>
    );
  }

  const baseMeta = PAGES[page] || PAGES.dashboard;
  const meta =
    page === "settings"
      ? {
          ...baseMeta,
          crumb: baseMeta.crumb + " / " + SECTION_CRUMB[settingsSection],
        }
      : baseMeta;
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
    case "unraid":
      pageEl = <Unraid snap={snap} />;
      break;
    case "alerts":
      pageEl = <Alerts snap={snap} refresh={refresh} />;
      break;
    case "logs":
      pageEl = <Events snap={snap} />;
      break;
    case "settings":
      pageEl = (
        <Settings
          settings={settings}
          setSetting={setSetting}
          section={settingsSection}
          onNavigateSection={(s) => navigate("settings", s)}
        />
      );
      break;
    default:
      pageEl = <Dashboard snap={snap} />;
  }

  return (
    <>
      {onboardingEl}
      <div className={"app density-" + settings.density + (settings.showSpark ? "" : " no-spark")}>
      <Sidebar page={page} onNavigate={navigate} alertCount={alertCount} username={username} />
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
          onAlerts={() => navigate("alerts")}
          onLogout={onLogout}
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
    </>
  );
}
