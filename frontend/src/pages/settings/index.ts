// Settings sections — single source of truth for the in-page navigation and
// the URL routing layer in App.tsx. New sections (email, slack, …) are added
// by appending an entry to SECTIONS and SECTION_IDS.

export type SectionId =
  | "sources"
  | "network-scanner"
  | "alerts"
  | "notifications"
  | "polling"
  | "display";

export const SECTION_IDS: SectionId[] = [
  "sources",
  "network-scanner",
  "alerts",
  "notifications",
  "polling",
  "display",
];

export const DEFAULT_SECTION: SectionId = "sources";

export const SECTIONS: { id: SectionId; label: string }[] = [
  { id: "sources", label: "Sources" },
  { id: "network-scanner", label: "Network Scanner" },
  { id: "alerts", label: "Alerts" },
  { id: "notifications", label: "Notifications" },
  { id: "polling", label: "Polling" },
  { id: "display", label: "Display" },
];

export const SECTION_CRUMB: Record<SectionId, string> = {
  sources: "Sources",
  "network-scanner": "Network Scanner",
  alerts: "Alerts",
  notifications: "Notifications",
  polling: "Polling",
  display: "Display",
};

export function isSectionId(s: string): s is SectionId {
  return (SECTION_IDS as string[]).includes(s);
}
