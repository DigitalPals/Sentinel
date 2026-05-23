// Shared inline icon set.
import React from "react";

const ICONS: Record<string, React.ReactElement> = {
  dashboard: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <rect x="2" y="2" width="5.5" height="6" rx="1" />
      <rect x="2" y="10" width="5.5" height="4" rx="1" />
      <rect x="9" y="2" width="5" height="4" rx="1" />
      <rect x="9" y="8" width="5" height="6" rx="1" />
    </svg>
  ),
  network: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <circle cx="8" cy="3" r="1.5" />
      <circle cx="3" cy="12" r="1.5" />
      <circle cx="13" cy="12" r="1.5" />
      <path d="M8 4.5v6M8 10.5l-4.5 1M8 10.5l4.5 1" strokeLinecap="round" />
    </svg>
  ),
  server: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <rect x="2" y="2" width="12" height="4.5" rx="1" />
      <rect x="2" y="9" width="12" height="4.5" rx="1" />
      <circle cx="5" cy="4.25" r=".5" fill="currentColor" />
      <circle cx="5" cy="11.25" r=".5" fill="currentColor" />
    </svg>
  ),
  storage: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <ellipse cx="8" cy="4" rx="5" ry="2" />
      <path d="M3 4v6.5c0 1.1 2.2 2 5 2s5-.9 5-2V4" />
      <path d="M3 7.3c0 1.1 2.2 2 5 2s5-.9 5-2" />
    </svg>
  ),
  alert: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M8 2 L14 13 H2 Z" strokeLinejoin="round" />
      <path d="M8 6.5v3.5M8 12v.01" strokeLinecap="round" />
    </svg>
  ),
  info: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <circle cx="8" cy="8" r="6" />
      <path d="M8 7.3v3.4M8 5.1v.01" strokeLinecap="round" />
    </svg>
  ),
  logs: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <rect x="2.5" y="2" width="11" height="12" rx="1" />
      <path d="M5 5.5h6M5 8h6M5 10.5h4" strokeLinecap="round" />
    </svg>
  ),
  settings: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <circle cx="8" cy="8" r="2" />
      <path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.5 3.5l1.4 1.4M11.1 11.1l1.4 1.4M3.5 12.5l1.4-1.4M11.1 4.9l1.4-1.4" strokeLinecap="round" />
    </svg>
  ),
  search: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <circle cx="7" cy="7" r="4.5" />
      <path d="M10.5 10.5L14 14" strokeLinecap="round" />
    </svg>
  ),
  bell: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M4 11V7a4 4 0 0 1 8 0v4l1 1.5H3L4 11Z" strokeLinejoin="round" />
      <path d="M6.5 13.5a1.5 1.5 0 0 0 3 0" />
    </svg>
  ),
  refresh: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M3 8a5 5 0 0 1 9-3M13 8a5 5 0 0 1-9 3" strokeLinecap="round" />
      <path d="M12 2v3h-3M4 14v-3h3" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  chevron: (
    <svg viewBox="0 0 16 16" width="10" height="10" fill="none" stroke="currentColor" strokeWidth="1.8">
      <path d="M4 6l4 4 4-4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  download: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M8 2v8M5 7l3 3 3-3M3 13h10" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  upload: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.5">
      <path d="M8 10V2M5 5l3-3 3 3M3 13h10" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  ap: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4">
      <path d="M3 6a7 7 0 0 1 10 0M5 8a4.5 4.5 0 0 1 6 0M7 10a2 2 0 0 1 2 0" strokeLinecap="round" />
      <circle cx="8" cy="12.5" r="1" fill="currentColor" stroke="none" />
    </svg>
  ),
  switch: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4">
      <rect x="2" y="5" width="12" height="6" rx="1" />
      <path d="M4.5 8h.01M6.5 8h.01M8.5 8h.01M10.5 8h.01M12.5 8h.01" strokeLinecap="round" strokeWidth="1.8" />
    </svg>
  ),
  gateway: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4">
      <rect x="2" y="3" width="12" height="10" rx="1.5" />
      <path d="M2 8h12M5 5.5v2M11 9v2" strokeLinecap="round" />
    </svg>
  ),
  camera: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4">
      <rect x="2" y="4" width="9" height="8" rx="1" />
      <path d="M11 7l3-1.5v5L11 9" strokeLinejoin="round" />
    </svg>
  ),
  ups: (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.4">
      <rect x="3" y="2" width="10" height="12" rx="1.5" />
      <path d="M8 4.5l-2 4h4l-2 4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  more: (
    <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
      <circle cx="4" cy="8" r="1.2" />
      <circle cx="8" cy="8" r="1.2" />
      <circle cx="12" cy="8" r="1.2" />
    </svg>
  ),
  power: (
    <svg viewBox="0 0 16 16" width="11" height="11" fill="none" stroke="currentColor" strokeWidth="1.6">
      <path d="M8 2v5" strokeLinecap="round" />
      <path d="M4.5 4.5a5 5 0 1 0 7 0" strokeLinecap="round" />
    </svg>
  ),
};

export function Icon({ name, ...rest }: { name: string } & React.HTMLAttributes<HTMLSpanElement>) {
  const el = ICONS[name];
  if (!el) return null;
  return <span {...rest}>{el}</span>;
}
