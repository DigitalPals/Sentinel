// Form primitives and message types shared across every settings section.
import React from "react";

export type Tone = "ok" | "err";
export type Msg = { tone: Tone; text: string } | null;

export function Field({
  label,
  children,
  hint,
}: {
  label: string;
  children: React.ReactNode;
  hint?: string;
}) {
  return (
    <div className="set-field">
      <label>{label}</label>
      {children}
      {hint && <span className="set-note">{hint}</span>}
    </div>
  );
}

export function Toggle({
  on,
  onChange,
}: {
  on: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      className="toggle"
      data-on={on ? "1" : "0"}
      aria-pressed={on}
      onClick={() => onChange(!on)}
    >
      <i />
    </button>
  );
}
