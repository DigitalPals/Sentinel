// API fetching + polling hook for the Cybex Sentinel backend.
import { useCallback, useEffect, useRef, useState } from "react";
import type {
  AppSettings,
  AuthStatus,
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
  Dashboard,
  EventsView,
  Guest,
  GuestCount,
  Kpi,
  NodeGuests,
  NodeTile,
  NotificationSettings,
  PortOut,
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
    fetchSnapshot();
    const poll = setInterval(fetchSnapshot, pollMs);
    const tick = setInterval(() => {
      setStaleSec(lastOk.current ? Math.floor((Date.now() - lastOk.current) / 1000) : 0);
    }, 1000);
    return () => {
      clearInterval(poll);
      clearInterval(tick);
    };
  }, [fetchSnapshot, pollMs]);

  const ready = !!snap && !!snap.generatedAt;
  return { snap, ready, error, staleSec, refresh: fetchSnapshot };
}

/** Apply an acknowledge / resolve / reopen action to an alert. */
export async function alertAction(id: string, action: string): Promise<void> {
  await apiFetch("/api/alerts/action", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ id, action }),
  });
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
  channel: "email" | "slack" | "telegram",
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
