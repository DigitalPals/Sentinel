// API fetching + polling hook for the Cybex Sentinel backend.
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  AppSettings,
  AuthStatus,
  NetworkHostDetail,
  NetworkHostUnifi,
  NetworkScannerOverview,
  NetworkScannerSettings,
  PushStatus,
  Snapshot,
  SnapshotState,
  SourcesData,
  TestResult,
} from "./api-types";

export type {
  Alert,
  AlertsView,
  AppSettings,
  AuthStatus,
  BandwidthSeries,
  BmcController,
  BmcDrive,
  BmcSensor,
  BmcSource,
  BmcView,
  Dashboard,
  EventsView,
  Guest,
  GuestCount,
  Kpi,
  NodeGuests,
  NodeTile,
  NotificationSettings,
  NetworkDiscoverySettings,
  NetworkHostConnection,
  NetworkHostDetail,
  NetworkHostTraffic,
  NetworkHostUnifi,
  NetworkHostUnifiClient,
  NetworkHostUnifiDevice,
  NetworkPortScanSettings,
  NetworkScanDevice,
  NetworkScanJob,
  NetworkScanPort,
  NetworkScanSchedule,
  NetworkScanSummary,
  NetworkScannerOverview,
  NetworkScannerSettings,
  PortProfile,
  PortScanTechnique,
  PushStatus,
  PortOut,
  PbsBackup,
  PbsCoverageFinding,
  PbsDatastore,
  PbsSource,
  PbsTask,
  ProxmoxSource,
  ProxmoxView,
  RadioOut,
  SentinelEvent,
  Snapshot,
  SnapshotState,
  SourceHealth,
  SourcesData,
  TestResult,
  TopoCounts,
  TopoNode,
  DiscoveryMethod,
  UiPrefs,
  UniDevice,
  UnraidContainer,
  UnraidDisk,
  UnraidNotification,
  UnraidServer,
  UnraidSource,
  UnraidStorage,
  UnraidView,
  UnraidVm,
  UnifiSource,
  UnifiView,
} from "./api-types";

const DEFAULT_POLL_MS = 5000;

/** Wrapper around `fetch` for the Sentinel API: always attaches the session
 *  cookie, never caches, and broadcasts a `sentinel-unauthorized` event when a
 *  request is rejected so the app can fall back to the login screen. */
export async function apiFetch(path: string, init?: RequestInit): Promise<Response> {
  const r = await fetch(path, { cache: "no-store", credentials: "same-origin", ...init });
  if (r.status === 401 && !path.startsWith("/api/auth/")) {
    window.dispatchEvent(new Event("sentinel-unauthorized"));
  }
  return r;
}

/** Polls `/api/snapshot` and exposes the latest snapshot. The poll interval is
 *  taken from the server-side `frontendPollMs` setting. */
export function useSnapshot(): SnapshotState {
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [staleSec, setStaleSec] = useState(0);
  const [pollMs, setPollMs] = useState(DEFAULT_POLL_MS);
  const lastOk = useRef<number>(0);

  const fetchSnapshot = useCallback(async () => {
    try {
      const r = await apiFetch("/api/snapshot");
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      const data: Snapshot = await r.json();
      setSnap(data);
      setError(null);
      lastOk.current = Date.now();
    } catch (e: any) {
      setError(String(e?.message ?? e));
    }
  }, []);

  // Pick up the configured frontend poll interval.
  useEffect(() => {
    apiFetch("/api/settings")
      .then((r) => (r.ok ? r.json() : null))
      .then((d) => {
        if (d && typeof d.frontendPollMs === "number") setPollMs(d.frontendPollMs);
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    let poll: number | undefined;
    let stream: EventSource | null = null;

    const startPollingFallback = () => {
      if (poll) return;
      fetchSnapshot();
      poll = window.setInterval(fetchSnapshot, pollMs);
    };
    const stopPollingFallback = () => {
      if (!poll) return;
      window.clearInterval(poll);
      poll = undefined;
    };

    if (typeof EventSource === "undefined") {
      startPollingFallback();
    } else {
      stream = new EventSource("/api/stream");
      stream.addEventListener("snapshot", (ev) => {
        try {
          const data: Snapshot = JSON.parse((ev as MessageEvent).data);
          if (!data.generatedAt) return;
          setSnap(data);
          setError(null);
          lastOk.current = Date.now();
          stopPollingFallback();
        } catch (e: any) {
          setError(String(e?.message ?? e));
          startPollingFallback();
        }
      });
      stream.addEventListener("sync", () => {
        fetchSnapshot();
      });
      stream.onerror = () => {
        setError("Live stream disconnected; polling fallback active");
        startPollingFallback();
      };
    }

    const tick = setInterval(() => {
      setStaleSec(lastOk.current ? Math.floor((Date.now() - lastOk.current) / 1000) : 0);
    }, 1000);
    return () => {
      stream?.close();
      stopPollingFallback();
      clearInterval(tick);
    };
  }, [fetchSnapshot, pollMs]);

  const ready = !!snap && !!snap.generatedAt;
  return { snap, ready, error, staleSec, refresh: fetchSnapshot };
}

/** Apply an acknowledge / resolve / ignore / reopen action to an alert. */
export async function alertAction(id: string, action: string): Promise<void> {
  await jsonOrThrow(
    await apiFetch("/api/alerts/action", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id, action }),
    }),
  );
}

// ── Settings & sources ──────────────────────────────────────────────────────

/** Parse a JSON response, throwing the API's `error` message on failure. */
async function jsonOrThrow(r: Response): Promise<any> {
  const body = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(body?.error || `HTTP ${r.status}`);
  return body;
}

export async function getSettings(): Promise<AppSettings> {
  return jsonOrThrow(await apiFetch("/api/settings"));
}

export async function putSettings(patch: Record<string, unknown>): Promise<AppSettings> {
  return jsonOrThrow(
    await apiFetch("/api/settings", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(patch),
    }),
  );
}

export async function testNotification(
  channel: "email" | "slack" | "telegram" | "push",
  notifications: Record<string, unknown>,
): Promise<TestResult> {
  return jsonOrThrow(
    await apiFetch("/api/notifications/test", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ channel, notifications }),
    }),
  );
}

export async function getPushStatus(): Promise<PushStatus> {
  return jsonOrThrow(await apiFetch("/api/push/status"));
}

export async function savePushSubscription(subscription: PushSubscriptionJSON): Promise<void> {
  await jsonOrThrow(
    await apiFetch("/api/push/subscriptions", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(subscription),
    }),
  );
}

export async function deletePushSubscription(endpoint: string): Promise<void> {
  await jsonOrThrow(
    await apiFetch("/api/push/subscriptions", {
      method: "DELETE",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ endpoint }),
    }),
  );
}

export async function getSources(): Promise<SourcesData> {
  return jsonOrThrow(await apiFetch("/api/sources"));
}

export async function saveUnifiSource(
  id: number | null,
  body: Record<string, unknown>,
): Promise<void> {
  const url = id == null ? "/api/sources/unifi" : `/api/sources/unifi/${id}`;
  await jsonOrThrow(
    await apiFetch(url, {
      method: id == null ? "POST" : "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }),
  );
}

export async function deleteUnifiSource(id: number): Promise<void> {
  await jsonOrThrow(await apiFetch(`/api/sources/unifi/${id}`, { method: "DELETE" }));
}

export async function saveProxmoxSource(
  id: number | null,
  body: Record<string, unknown>,
): Promise<void> {
  const url = id == null ? "/api/sources/proxmox" : `/api/sources/proxmox/${id}`;
  await jsonOrThrow(
    await apiFetch(url, {
      method: id == null ? "POST" : "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }),
  );
}

export async function deleteProxmoxSource(id: number): Promise<void> {
  await jsonOrThrow(await apiFetch(`/api/sources/proxmox/${id}`, { method: "DELETE" }));
}

export async function savePbsSource(
  id: number | null,
  body: Record<string, unknown>,
): Promise<void> {
  const url = id == null ? "/api/sources/pbs" : `/api/sources/pbs/${id}`;
  await jsonOrThrow(
    await apiFetch(url, {
      method: id == null ? "POST" : "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }),
  );
}

export async function saveBmcSource(
  id: number | null,
  body: Record<string, unknown>,
): Promise<void> {
  const url = id == null ? "/api/sources/bmc" : `/api/sources/bmc/${id}`;
  await jsonOrThrow(
    await apiFetch(url, {
      method: id == null ? "POST" : "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }),
  );
}

export async function deleteBmcSource(id: number): Promise<void> {
  await jsonOrThrow(await apiFetch(`/api/sources/bmc/${id}`, { method: "DELETE" }));
}

export async function deletePbsSource(id: number): Promise<void> {
  await jsonOrThrow(await apiFetch(`/api/sources/pbs/${id}`, { method: "DELETE" }));
}

export async function saveUnraidSource(
  id: number | null,
  body: Record<string, unknown>,
): Promise<void> {
  const url = id == null ? "/api/sources/unraid" : `/api/sources/unraid/${id}`;
  await jsonOrThrow(
    await apiFetch(url, {
      method: id == null ? "POST" : "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }),
  );
}

export async function deleteUnraidSource(id: number): Promise<void> {
  await jsonOrThrow(await apiFetch(`/api/sources/unraid/${id}`, { method: "DELETE" }));
}

export async function testSource(body: Record<string, unknown>): Promise<TestResult> {
  return jsonOrThrow(
    await apiFetch("/api/sources/test", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }),
  );
}

// ── Network scanner ────────────────────────────────────────────────────────

export async function getNetworkScanner(): Promise<NetworkScannerOverview> {
  return jsonOrThrow(await apiFetch("/api/network-scanner"));
}

export async function getNetworkHost(target: string): Promise<NetworkHostDetail> {
  return jsonOrThrow(
    await apiFetch(`/api/network-scanner/hosts/${encodeURIComponent(target)}`),
  );
}

export async function getNetworkHostUnifi(target: string): Promise<NetworkHostUnifi> {
  return jsonOrThrow(
    await apiFetch(`/api/network-scanner/hosts/${encodeURIComponent(target)}/unifi`),
  );
}

export async function startNetworkScan(
  settings?: NetworkScannerSettings,
  force = false,
): Promise<{ id: number }> {
  return jsonOrThrow(
    await apiFetch("/api/network-scanner/scan", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ settings, force }),
    }),
  );
}

export async function startHostPortScan(target: string): Promise<{ id: number }> {
  return jsonOrThrow(
    await apiFetch(
      `/api/network-scanner/hosts/${encodeURIComponent(target)}/port-scan`,
      { method: "POST" },
    ),
  );
}

export async function cancelNetworkScan(id: number): Promise<void> {
  await jsonOrThrow(
    await apiFetch(`/api/network-scanner/jobs/${id}/cancel`, { method: "POST" }),
  );
}

// ── Authentication ──────────────────────────────────────────────────────────

/** Public: whether the app needs first-user setup, a login, or neither. */
export async function getAuthStatus(): Promise<AuthStatus> {
  return jsonOrThrow(await apiFetch("/api/auth/status"));
}

/** Create the first administrator account (only works on a fresh install). */
export async function authSetup(username: string, password: string): Promise<void> {
  await jsonOrThrow(
    await apiFetch("/api/auth/setup", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username, password }),
    }),
  );
}

/** Sign in with an existing account. */
export async function authLogin(username: string, password: string): Promise<void> {
  await jsonOrThrow(
    await apiFetch("/api/auth/login", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username, password }),
    }),
  );
}

/** End the current session. */
export async function authLogout(): Promise<void> {
  await apiFetch("/api/auth/logout", { method: "POST" });
}
