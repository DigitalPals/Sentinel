// Cybex Sentinel — Settings.
//
// Thin shell that hosts the in-page navigation and renders the active section.
// All configuration that used to live in config.toml and in hardcoded constants
// is split across the section components in ./settings/, each owning its own
// state, data fetching, and save logic.
import { type Settings as DisplaySettings } from "../settings";
import { SectionId } from "./settings/index";
import SettingsNav from "./settings/SettingsNav";
import SourcesSection from "./settings/SourcesSection";
import AlertsSection from "./settings/AlertsSection";
import NotificationsSection from "./settings/NotificationsSection";
import PollingSection from "./settings/PollingSection";
import DisplaySection from "./settings/DisplaySection";

type SetSetting = <K extends keyof DisplaySettings>(key: K, value: DisplaySettings[K]) => void;

export default function Settings({
  settings,
  setSetting,
  section,
  onNavigateSection,
}: {
  settings: DisplaySettings;
  setSetting: SetSetting;
  section: SectionId;
  onNavigateSection: (id: SectionId) => void;
}) {
  let body;
  switch (section) {
    case "alerts":
      body = <AlertsSection />;
      break;
    case "notifications":
      body = <NotificationsSection />;
      break;
    case "polling":
      body = <PollingSection />;
      break;
    case "display":
      body = <DisplaySection settings={settings} setSetting={setSetting} />;
      break;
    case "sources":
    default:
      body = <SourcesSection />;
      break;
  }

  return (
    <div className="page settings-page">
      <div className="settings-layout">
        <SettingsNav active={section} onSelect={onNavigateSection} />
        <div className="settings-content">{body}</div>
      </div>
    </div>
  );
}
