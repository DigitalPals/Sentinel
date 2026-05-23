// Left-rail navigation for the Settings page. The active section comes in via
// props (the URL is the source of truth, parsed in App.tsx).
import { SECTIONS, SectionId } from "./index";

export default function SettingsNav({
  active,
  onSelect,
}: {
  active: SectionId;
  onSelect: (id: SectionId) => void;
}) {
  return (
    <nav className="settings-nav" aria-label="Settings sections">
      {SECTIONS.map((s) => (
        <button
          key={s.id}
          className={"settings-nav-item" + (s.id === active ? " active" : "")}
          aria-current={s.id === active ? "page" : undefined}
          onClick={() => onSelect(s.id)}
        >
          {s.label}
        </button>
      ))}
    </nav>
  );
}
