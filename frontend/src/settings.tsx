// Local display preferences — accent palette, density, sparklines.
// Persisted to localStorage; applied as CSS custom properties.
import { useCallback, useEffect, useState } from "react";

export type Accent = "cyan" | "amber" | "violet" | "mint";
export type Density = "compact" | "regular" | "comfy";

export interface Settings {
  accent: Accent;
  density: Density;
  showSpark: boolean;
}

const DEFAULTS: Settings = { accent: "cyan", density: "regular", showSpark: true };
const STORAGE_KEY = "sentinel.settings";

const ACCENTS: Record<Accent, { accent: string; accent2: string; swatch: [string, string] }> = {
  cyan: { accent: "oklch(0.82 0.15 200)", accent2: "oklch(0.66 0.18 270)", swatch: ["#7adfff", "#a08bff"] },
  amber: { accent: "oklch(0.82 0.16 82)", accent2: "oklch(0.7 0.21 26)", swatch: ["#ffc56b", "#ff7064"] },
  violet: { accent: "oklch(0.74 0.16 290)", accent2: "oklch(0.66 0.18 200)", swatch: ["#b48bff", "#7adfff"] },
  mint: { accent: "oklch(0.78 0.15 168)", accent2: "oklch(0.66 0.18 240)", swatch: ["#7df0c5", "#7ab8ff"] },
};

export function useSettings() {
  const [settings, setSettings] = useState<Settings>(() => {
    try {
      return { ...DEFAULTS, ...JSON.parse(localStorage.getItem(STORAGE_KEY) || "{}") };
    } catch {
      return DEFAULTS;
    }
  });

  const setSetting = useCallback(<K extends keyof Settings>(key: K, value: Settings[K]) => {
    setSettings((prev) => {
      const next = { ...prev, [key]: value };
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      } catch {
        /* ignore quota errors */
      }
      return next;
    });
  }, []);

  useEffect(() => {
    const a = ACCENTS[settings.accent] ?? ACCENTS.cyan;
    const root = document.documentElement.style;
    root.setProperty("--accent", a.accent);
    root.setProperty("--accent-2", a.accent2);
    root.setProperty("--accent-soft", a.accent.replace(")", " / 0.14)"));
  }, [settings.accent]);

  useEffect(() => {
    const fs = settings.density === "compact" ? 12.5 : settings.density === "comfy" ? 13.5 : 13;
    document.body.style.fontSize = `${fs}px`;
  }, [settings.density]);

  return { settings, setSetting };
}

export function SettingsPanel({
  settings,
  setSetting,
  onClose,
}: {
  settings: Settings;
  setSetting: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  onClose: () => void;
}) {
  return (
    <>
      <div className="settings-backdrop" onClick={onClose} />
      <div className="settings-panel" role="dialog" aria-label="Settings">
        <div className="settings-hd">Display Settings</div>
        <div className="settings-body">
          <div className="settings-row">
            <div className="settings-sect">Accent</div>
            <div className="accent-swatches">
              {(Object.keys(ACCENTS) as Accent[]).map((key) => {
                const sw = ACCENTS[key].swatch;
                return (
                  <button
                    key={key}
                    className="accent-swatch"
                    data-on={settings.accent === key ? "1" : "0"}
                    title={key}
                    style={{ background: `linear-gradient(135deg, ${sw[0]}, ${sw[1]})` }}
                    onClick={() => setSetting("accent", key)}
                  />
                );
              })}
            </div>
          </div>

          <div className="settings-row">
            <div className="settings-sect">Density</div>
            <div className="seg">
              {(["compact", "regular", "comfy"] as Density[]).map((d) => (
                <button
                  key={d}
                  className={settings.density === d ? "on" : ""}
                  onClick={() => setSetting("density", d)}
                >
                  {d}
                </button>
              ))}
            </div>
          </div>

          <div className="settings-row">
            <div className="settings-lbl">
              <span>Show sparklines</span>
              <button
                className="toggle"
                data-on={settings.showSpark ? "1" : "0"}
                aria-pressed={settings.showSpark}
                onClick={() => setSetting("showSpark", !settings.showSpark)}
              >
                <i />
              </button>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}
