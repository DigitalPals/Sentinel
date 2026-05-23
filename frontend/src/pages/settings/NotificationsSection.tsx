// Notifications — outbound channels for alert delivery.
import React from "react";
import { AppSettings, getSettings, putSettings, testNotification } from "../../api";
import { Card } from "../../components";
import { Field, Msg, Tone, Toggle } from "./shared";

type Channel = "email" | "slack" | "telegram";

const HELP: Record<Channel, { title: string; steps: string[] }> = {
  email: {
    title: "Email setup",
    steps: [
      "Use your mail provider's SMTP host and port.",
      "Choose STARTTLS for port 587, implicit TLS for port 465, or plain SMTP for a trusted local relay.",
      "Enter the username and app password if your SMTP server requires authentication.",
      "Set From to the sender address and Recipients to one or more target addresses.",
      "Save, then send a test email.",
    ],
  },
  slack: {
    title: "Slack setup",
    steps: [
      "Create or open a Slack app at api.slack.com/apps.",
      "Enable Incoming Webhooks for the app.",
      "Add a webhook to the channel that should receive Sentinel alerts.",
      "Paste the generated https://hooks.slack.com/services/... URL here.",
      "Save, then send a test Slack notification.",
    ],
  },
  telegram: {
    title: "Telegram setup",
    steps: [
      "Message @BotFather in Telegram and run /newbot.",
      "Copy the bot token BotFather returns.",
      "Start a chat with the new bot, or add it to the target group, then send a message.",
      "Open https://api.telegram.org/bot<token>/getUpdates and find message.chat.id.",
      "Paste the bot token and chat ID here, save, then send a test Telegram notification.",
    ],
  },
};

type Draft = {
  minSeverity: string;
  email: {
    enabled: boolean;
    smtpHost: string;
    smtpPort: number;
    smtpUsername: string;
    smtpPassword: string;
    smtpSecurity: string;
    from: string;
    toText: string;
  };
  slack: {
    enabled: boolean;
    webhookUrl: string;
  };
  telegram: {
    enabled: boolean;
    botToken: string;
    chatId: string;
  };
};

export default function NotificationsSection() {
  const [app, setApp] = React.useState<AppSettings | null>(null);
  const [msg, setMsg] = React.useState<Msg>(null);

  React.useEffect(() => {
    getSettings()
      .then(setApp)
      .catch((e) => setMsg({ tone: "err", text: String(e?.message ?? e) }));
  }, []);

  return (
    <>
      {msg && <div className={"set-banner " + msg.tone}>{msg.text}</div>}
      {app && (
        <NotificationsCard
          app={app}
          onSaved={setApp}
          onMsg={(tone, text) => setMsg({ tone, text })}
        />
      )}
    </>
  );
}

function NotificationsCard({
  app,
  onSaved,
  onMsg,
}: {
  app: AppSettings;
  onSaved: (s: AppSettings) => void;
  onMsg: (tone: Tone, text: string) => void;
}) {
  const [draft, setDraft] = React.useState<Draft>(() => draftFromSettings(app));
  const [busy, setBusy] = React.useState<"save" | Channel | null>(null);

  React.useEffect(() => {
    setDraft(draftFromSettings(app));
  }, [app]);

  const setEmail = <K extends keyof Draft["email"]>(key: K, value: Draft["email"][K]) =>
    setDraft((d) => ({ ...d, email: { ...d.email, [key]: value } }));
  const setSlack = <K extends keyof Draft["slack"]>(key: K, value: Draft["slack"][K]) =>
    setDraft((d) => ({ ...d, slack: { ...d.slack, [key]: value } }));
  const setTelegram = <K extends keyof Draft["telegram"]>(key: K, value: Draft["telegram"][K]) =>
    setDraft((d) => ({ ...d, telegram: { ...d.telegram, [key]: value } }));

  const save = async () => {
    setBusy("save");
    try {
      const saved = await putSettings({ notifications: buildUpdate(draft) });
      onSaved(saved);
      onMsg("ok", "Notification settings saved.");
    } catch (e: any) {
      onMsg("err", String(e?.message ?? e));
    } finally {
      setBusy(null);
    }
  };

  const runTest = async (channel: Channel) => {
    setBusy(channel);
    try {
      const r = await testNotification(channel, buildUpdate(draft));
      onMsg(r.ok ? "ok" : "err", r.detail);
    } catch (e: any) {
      onMsg("err", String(e?.message ?? e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Card
      title="Notifications"
      sub="send newly raised warning and critical alerts"
      style={{ overflow: "visible" }}
    >
      <div className="notify-settings">
        <div className="set-section">
          <div className="set-subhd">
            <span>Delivery threshold</span>
            <div className="seg">
              {(["warn", "crit"] as const).map((sev) => (
                <button
                  key={sev}
                  className={draft.minSeverity === sev ? "on" : ""}
                  onClick={() => setDraft((d) => ({ ...d, minSeverity: sev }))}
                >
                  {sev === "warn" ? "Warnings + critical" : "Critical only"}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="notify-channel">
          <div className="notify-channel-hd">
            <div className="notify-channel-main">
              <label className="set-inline">
                <Toggle on={draft.email.enabled} onChange={(v) => setEmail("enabled", v)} />
                <span className="notify-channel-title">Email</span>
              </label>
              <ProviderHelp channel="email" />
            </div>
            <button
              className="set-btn"
              disabled={busy != null}
              onClick={() => runTest("email")}
            >
              Test email
            </button>
          </div>
          <div className="set-row set-grid-3">
            <Field label="SMTP host">
              <input
                className="set-input"
                value={draft.email.smtpHost}
                onChange={(e) => setEmail("smtpHost", e.target.value)}
              />
            </Field>
            <Field label="SMTP port">
              <input
                className="set-input"
                type="number"
                value={draft.email.smtpPort}
                onChange={(e) => setEmail("smtpPort", Number(e.target.value))}
              />
            </Field>
            <Field label="Security">
              <select
                className="set-input"
                value={draft.email.smtpSecurity}
                onChange={(e) => setEmail("smtpSecurity", e.target.value)}
              >
                <option value="starttls">STARTTLS</option>
                <option value="tls">Implicit TLS</option>
                <option value="none">Plain SMTP</option>
              </select>
            </Field>
            <Field label="Username">
              <input
                className="set-input"
                value={draft.email.smtpUsername}
                onChange={(e) => setEmail("smtpUsername", e.target.value)}
              />
            </Field>
            <Field label="Password" hint={app.notifications.email.hasPassword ? "Leave blank to keep the stored password" : undefined}>
              <input
                className="set-input"
                type="password"
                value={draft.email.smtpPassword}
                placeholder={app.notifications.email.hasPassword ? "stored password" : ""}
                onChange={(e) => setEmail("smtpPassword", e.target.value)}
              />
            </Field>
            <Field label="From address">
              <input
                className="set-input"
                value={draft.email.from}
                placeholder="Sentinel <sentinel@example.com>"
                onChange={(e) => setEmail("from", e.target.value)}
              />
            </Field>
          </div>
          <Field label="Recipients" hint="One address per line, or comma-separated">
            <textarea
              className="set-input set-textarea"
              value={draft.email.toText}
              onChange={(e) => setEmail("toText", e.target.value)}
            />
          </Field>
        </div>

        <div className="notify-channel">
          <div className="notify-channel-hd">
            <div className="notify-channel-main">
              <label className="set-inline">
                <Toggle on={draft.slack.enabled} onChange={(v) => setSlack("enabled", v)} />
                <span className="notify-channel-title">Slack</span>
              </label>
              <ProviderHelp channel="slack" />
            </div>
            <button
              className="set-btn"
              disabled={busy != null}
              onClick={() => runTest("slack")}
            >
              Test Slack
            </button>
          </div>
          <Field
            label="Incoming webhook URL"
            hint={app.notifications.slack.hasWebhookUrl ? "Leave blank to keep the stored webhook URL" : undefined}
          >
            <input
              className="set-input"
              type="password"
              value={draft.slack.webhookUrl}
              placeholder={app.notifications.slack.hasWebhookUrl ? "stored webhook URL" : "https://hooks.slack.com/services/..."}
              onChange={(e) => setSlack("webhookUrl", e.target.value)}
            />
          </Field>
        </div>

        <div className="notify-channel">
          <div className="notify-channel-hd">
            <div className="notify-channel-main">
              <label className="set-inline">
                <Toggle on={draft.telegram.enabled} onChange={(v) => setTelegram("enabled", v)} />
                <span className="notify-channel-title">Telegram</span>
              </label>
              <ProviderHelp channel="telegram" />
            </div>
            <button
              className="set-btn"
              disabled={busy != null}
              onClick={() => runTest("telegram")}
            >
              Test Telegram
            </button>
          </div>
          <div className="set-row set-grid-2">
            <Field
              label="Bot token"
              hint={app.notifications.telegram.hasBotToken ? "Leave blank to keep the stored token" : undefined}
            >
              <input
                className="set-input"
                type="password"
                value={draft.telegram.botToken}
                placeholder={app.notifications.telegram.hasBotToken ? "stored bot token" : "123456:ABC-DEF..."}
                onChange={(e) => setTelegram("botToken", e.target.value)}
              />
            </Field>
            <Field label="Chat ID">
              <input
                className="set-input"
                value={draft.telegram.chatId}
                placeholder="-1001234567890"
                onChange={(e) => setTelegram("chatId", e.target.value)}
              />
            </Field>
          </div>
        </div>

        <div className="set-actions">
          <span className="set-note">{busy && busy !== "save" ? `Sending ${busy} test...` : ""}</span>
          <button className="set-btn primary" disabled={busy != null} onClick={save}>
            Save
          </button>
        </div>
      </div>
    </Card>
  );
}

function ProviderHelp({ channel }: { channel: Channel }) {
  const help = HELP[channel];
  const id = `notify-help-${channel}`;
  return (
    <span className="provider-help">
      <button
        type="button"
        className="provider-help-btn"
        aria-label={`${help.title} instructions`}
        aria-describedby={id}
      >
        ?
      </button>
      <span id={id} className="provider-help-popover" role="tooltip">
        <span className="provider-help-title">{help.title}</span>
        <ol>
          {help.steps.map((step) => (
            <li key={step}>{step}</li>
          ))}
        </ol>
      </span>
    </span>
  );
}

function draftFromSettings(app: AppSettings): Draft {
  return {
    minSeverity: app.notifications.minSeverity || "warn",
    email: {
      enabled: app.notifications.email.enabled,
      smtpHost: app.notifications.email.smtpHost,
      smtpPort: app.notifications.email.smtpPort || 587,
      smtpUsername: app.notifications.email.smtpUsername,
      smtpPassword: "",
      smtpSecurity: app.notifications.email.smtpSecurity || "starttls",
      from: app.notifications.email.from,
      toText: app.notifications.email.to.join("\n"),
    },
    slack: {
      enabled: app.notifications.slack.enabled,
      webhookUrl: "",
    },
    telegram: {
      enabled: app.notifications.telegram.enabled,
      botToken: "",
      chatId: app.notifications.telegram.chatId,
    },
  };
}

function buildUpdate(draft: Draft): Record<string, unknown> {
  const email: Record<string, unknown> = {
    enabled: draft.email.enabled,
    smtpHost: draft.email.smtpHost,
    smtpPort: draft.email.smtpPort,
    smtpUsername: draft.email.smtpUsername,
    smtpSecurity: draft.email.smtpSecurity,
    from: draft.email.from,
    to: splitRecipients(draft.email.toText),
  };
  if (draft.email.smtpPassword) email.smtpPassword = draft.email.smtpPassword;

  const slack: Record<string, unknown> = { enabled: draft.slack.enabled };
  if (draft.slack.webhookUrl) slack.webhookUrl = draft.slack.webhookUrl;

  const telegram: Record<string, unknown> = {
    enabled: draft.telegram.enabled,
    chatId: draft.telegram.chatId,
  };
  if (draft.telegram.botToken) telegram.botToken = draft.telegram.botToken;

  return {
    minSeverity: draft.minSeverity,
    email,
    slack,
    telegram,
  };
}

function splitRecipients(value: string): string[] {
  return value
    .split(/[\n,]+/)
    .map((s) => s.trim())
    .filter(Boolean);
}
