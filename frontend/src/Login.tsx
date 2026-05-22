// Cybex Sentinel — login & first-run account setup.
//
// One component, two modes: when the database has no user yet it creates the
// first administrator account; otherwise it signs an existing user in. On
// success it calls `onAuthenticated`, passing whether this was a fresh setup
// so the app can decide how much onboarding to show.
import React from "react";
import { authLogin, authSetup } from "./api";

const MIN_PASSWORD_LEN = 8;

export function Login({
  needsFirstUser = false,
  onAuthenticated,
}: {
  needsFirstUser?: boolean;
  onAuthenticated: (justSetUp: boolean) => void;
}) {
  const [username, setUsername] = React.useState("");
  const [password, setPassword] = React.useState("");
  const [confirm, setConfirm] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setErr(null);
    if (!username.trim()) {
      setErr("Enter a username.");
      return;
    }
    if (needsFirstUser) {
      if (password.length < MIN_PASSWORD_LEN) {
        setErr(`Password must be at least ${MIN_PASSWORD_LEN} characters.`);
        return;
      }
      if (password !== confirm) {
        setErr("Passwords do not match.");
        return;
      }
    }
    setBusy(true);
    try {
      if (needsFirstUser) await authSetup(username.trim(), password);
      else await authLogin(username.trim(), password);
      onAuthenticated(needsFirstUser);
    } catch (e: any) {
      setErr(String(e?.message ?? e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="onb-backdrop">
      <form className="onb-card login-card" onSubmit={submit}>
        <div className="onb-head">
          <div className="brand-mark" />
          <div>
            <div className="brand-name">Cybex Sentinel</div>
            <div className="brand-tag">{needsFirstUser ? "first-run setup" : "sign in"}</div>
          </div>
        </div>

        <div className="onb-body onb-step">
          <h2 className="onb-title">
            {needsFirstUser ? "Create your account" : "Welcome back"}
          </h2>
          <p className="onb-text">
            {needsFirstUser
              ? "Set up the administrator account for this Sentinel instance — you'll use it to sign in from now on."
              : "Sign in to access the Sentinel dashboard."}
          </p>

          <div className="set-field">
            <label>Username</label>
            <input
              className="set-input"
              autoFocus
              value={username}
              autoComplete="username"
              onChange={(e) => setUsername(e.target.value)}
            />
          </div>
          <div className="set-field">
            <label>Password</label>
            <input
              className="set-input"
              type="password"
              value={password}
              autoComplete={needsFirstUser ? "new-password" : "current-password"}
              onChange={(e) => setPassword(e.target.value)}
            />
          </div>
          {needsFirstUser && (
            <div className="set-field">
              <label>Confirm password</label>
              <input
                className="set-input"
                type="password"
                value={confirm}
                autoComplete="new-password"
                onChange={(e) => setConfirm(e.target.value)}
              />
            </div>
          )}
        </div>

        {err && <div className="onb-msg err">{err}</div>}

        <div className="onb-foot">
          <span className="spacer" />
          <button type="submit" className="set-btn primary" disabled={busy}>
            {busy ? "Please wait…" : needsFirstUser ? "Create account" : "Sign in"}
          </button>
        </div>
      </form>
    </div>
  );
}
