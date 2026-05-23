// Notifications — outbound channels for alert delivery. Placeholder until the
// Email/Slack integrations land.
import { Card } from "../../components";

export default function NotificationsSection() {
  return (
    <Card title="Notifications" sub="outbound channels for alert delivery">
      <div style={{ padding: "20px 18px", display: "flex", flexDirection: "column", gap: 8 }}>
        <div style={{ fontSize: 13, fontWeight: 600 }}>
          Email and Slack integrations are coming soon.
        </div>
        <div className="set-note">
          Outbound channels will let Sentinel push warning and critical alerts to your team
          — planned: SMTP email, Slack incoming webhooks, and generic webhooks.
        </div>
      </div>
    </Card>
  );
}
