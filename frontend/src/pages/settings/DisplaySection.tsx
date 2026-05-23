// Display preferences — accent, density, sparklines. Shared across browsers
// via the persisted `ui` blob on /api/settings; the actual controls live in
// src/settings.tsx so the floating quick-panel can reuse them.
import { Card } from "../../components";
import { DisplayControls, type Settings as DisplaySettings } from "../../settings";

type SetSetting = <K extends keyof DisplaySettings>(key: K, value: DisplaySettings[K]) => void;

export default function DisplaySection({
  settings,
  setSetting,
}: {
  settings: DisplaySettings;
  setSetting: SetSetting;
}) {
  return (
    <Card title="Display" sub="accent, density and sparklines — shared across browsers">
      <div style={{ padding: "8px 18px 16px" }}>
        <DisplayControls settings={settings} setSetting={setSetting} />
      </div>
    </Card>
  );
}
