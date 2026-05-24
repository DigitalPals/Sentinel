//! Outbound alert notifications: SMTP email, Slack incoming webhooks, Telegram
//! Bot API messages and browser Web Push.

use std::time::Duration;

use anyhow::{anyhow, bail, Context};
use atomic_web_push::{
    engine::general_purpose::URL_SAFE_NO_PAD, ContentEncoding, ReqwestWebPushClient,
    SubscriptionInfo, Urgency, VapidKeyGenerator, VapidSignatureBuilder, WebPushClient,
    WebPushMessageBuilder,
};
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;

use crate::db;
use crate::model::Alert;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NotificationSettings {
    pub min_severity: String,
    pub email: EmailNotificationSettings,
    pub slack: SlackNotificationSettings,
    pub telegram: TelegramNotificationSettings,
    pub push: PushNotificationSettings,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            min_severity: "warn".to_string(),
            email: EmailNotificationSettings::default(),
            slack: SlackNotificationSettings::default(),
            telegram: TelegramNotificationSettings::default(),
            push: PushNotificationSettings::default(),
        }
    }
}

impl NotificationSettings {
    pub fn public(&self) -> NotificationSettingsPublic {
        NotificationSettingsPublic {
            min_severity: normalize_min_severity(&self.min_severity),
            email: EmailNotificationPublic {
                enabled: self.email.enabled,
                smtp_host: self.email.smtp_host.clone(),
                smtp_port: self.email.smtp_port,
                smtp_username: self.email.smtp_username.clone(),
                has_password: !self.email.smtp_password.is_empty(),
                smtp_security: normalize_smtp_security(&self.email.smtp_security),
                from: self.email.from.clone(),
                to: self.email.to.clone(),
            },
            slack: SlackNotificationPublic {
                enabled: self.slack.enabled,
                has_webhook_url: !self.slack.webhook_url.is_empty(),
            },
            telegram: TelegramNotificationPublic {
                enabled: self.telegram.enabled,
                has_bot_token: !self.telegram.bot_token.is_empty(),
                chat_id: self.telegram.chat_id.clone(),
            },
            push: PushNotificationPublic {
                enabled: self.push.enabled,
                configured: !self.push.vapid_private_key.is_empty(),
                public_key: push_public_key(&self.push).ok(),
                vapid_subject: self.push.vapid_subject.clone(),
            },
        }
    }

    pub fn apply_update(&mut self, update: NotificationSettingsUpdate) {
        if let Some(v) = update.min_severity {
            self.min_severity = normalize_min_severity(&v);
        }
        if let Some(v) = update.email {
            self.email.apply_update(v);
        }
        if let Some(v) = update.slack {
            self.slack.apply_update(v);
        }
        if let Some(v) = update.telegram {
            self.telegram.apply_update(v);
        }
        if let Some(v) = update.push {
            self.push.apply_update(v);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EmailNotificationSettings {
    pub enabled: bool,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub smtp_password: String,
    pub smtp_security: String,
    pub from: String,
    pub to: Vec<String>,
}

impl Default for EmailNotificationSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            smtp_host: String::new(),
            smtp_port: 587,
            smtp_username: String::new(),
            smtp_password: String::new(),
            smtp_security: "starttls".to_string(),
            from: String::new(),
            to: Vec::new(),
        }
    }
}

impl EmailNotificationSettings {
    fn apply_update(&mut self, update: EmailNotificationUpdate) {
        if let Some(v) = update.enabled {
            self.enabled = v;
        }
        if let Some(v) = update.smtp_host {
            self.smtp_host = v.trim().to_string();
        }
        if let Some(v) = update.smtp_port {
            self.smtp_port = v.max(1);
        }
        if let Some(v) = update.smtp_username {
            self.smtp_username = v.trim().to_string();
        }
        if let Some(v) = update.smtp_password {
            self.smtp_password = v;
        }
        if let Some(v) = update.smtp_security {
            self.smtp_security = normalize_smtp_security(&v);
        }
        if let Some(v) = update.from {
            self.from = v.trim().to_string();
        }
        if let Some(v) = update.to {
            self.to = v
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct SlackNotificationSettings {
    pub enabled: bool,
    pub webhook_url: String,
}

impl SlackNotificationSettings {
    fn apply_update(&mut self, update: SlackNotificationUpdate) {
        if let Some(v) = update.enabled {
            self.enabled = v;
        }
        if let Some(v) = update.webhook_url {
            self.webhook_url = v.trim().to_string();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TelegramNotificationSettings {
    pub enabled: bool,
    pub bot_token: String,
    pub chat_id: String,
}

impl TelegramNotificationSettings {
    fn apply_update(&mut self, update: TelegramNotificationUpdate) {
        if let Some(v) = update.enabled {
            self.enabled = v;
        }
        if let Some(v) = update.bot_token {
            self.bot_token = v.trim().to_string();
        }
        if let Some(v) = update.chat_id {
            self.chat_id = v.trim().to_string();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PushNotificationSettings {
    pub enabled: bool,
    pub vapid_subject: String,
    pub vapid_private_key: String,
}

impl Default for PushNotificationSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            vapid_subject: "mailto:admin@localhost".to_string(),
            vapid_private_key: String::new(),
        }
    }
}

impl PushNotificationSettings {
    fn apply_update(&mut self, update: PushNotificationUpdate) {
        if let Some(v) = update.enabled {
            self.enabled = v;
        }
        if let Some(v) = update.vapid_subject {
            let v = v.trim();
            self.vapid_subject = if v.is_empty() {
                PushNotificationSettings::default().vapid_subject
            } else {
                v.to_string()
            };
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationSettingsPublic {
    pub min_severity: String,
    pub email: EmailNotificationPublic,
    pub slack: SlackNotificationPublic,
    pub telegram: TelegramNotificationPublic,
    pub push: PushNotificationPublic,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailNotificationPublic {
    pub enabled: bool,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_username: String,
    pub has_password: bool,
    pub smtp_security: String,
    pub from: String,
    pub to: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlackNotificationPublic {
    pub enabled: bool,
    pub has_webhook_url: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelegramNotificationPublic {
    pub enabled: bool,
    pub has_bot_token: bool,
    pub chat_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushNotificationPublic {
    pub enabled: bool,
    pub configured: bool,
    pub public_key: Option<String>,
    pub vapid_subject: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct NotificationSettingsUpdate {
    pub min_severity: Option<String>,
    pub email: Option<EmailNotificationUpdate>,
    pub slack: Option<SlackNotificationUpdate>,
    pub telegram: Option<TelegramNotificationUpdate>,
    pub push: Option<PushNotificationUpdate>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct EmailNotificationUpdate {
    pub enabled: Option<bool>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_security: Option<String>,
    pub from: Option<String>,
    pub to: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct SlackNotificationUpdate {
    pub enabled: Option<bool>,
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TelegramNotificationUpdate {
    pub enabled: Option<bool>,
    pub bot_token: Option<String>,
    pub chat_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct PushNotificationUpdate {
    pub enabled: Option<bool>,
    pub vapid_subject: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationChannel {
    Email,
    Slack,
    Telegram,
    Push,
}

pub fn ensure_push_vapid(settings: &mut NotificationSettings) -> anyhow::Result<()> {
    if settings.push.vapid_private_key.trim().is_empty() {
        let key = VapidKeyGenerator::new().context("generating browser push VAPID key")?;
        settings.push.vapid_private_key = key.secret_key_base64();
    }
    Ok(())
}

pub fn push_public_key(settings: &PushNotificationSettings) -> anyhow::Result<String> {
    if settings.vapid_private_key.trim().is_empty() {
        bail!("browser push VAPID key is not configured");
    }
    let key = VapidKeyGenerator::from_base64(&settings.vapid_private_key)
        .context("loading browser push VAPID key")?;
    Ok(key.public_key_base64())
}

pub async fn send_alert_notifications(
    pool: &PgPool,
    settings: &NotificationSettings,
    alerts: &[Alert],
) -> anyhow::Result<()> {
    let selected: Vec<Alert> = alerts
        .iter()
        .filter(|a| severity_allowed(&a.sev, &settings.min_severity))
        .cloned()
        .collect();
    if selected.is_empty() {
        return Ok(());
    }

    let mut errors = Vec::new();
    if settings.email.enabled {
        if let Err(e) = send_email(&settings.email, &selected).await {
            errors.push(format!("email: {e:#}"));
        }
    }
    if settings.slack.enabled {
        if let Err(e) = send_slack(&settings.slack, &selected).await {
            errors.push(format!("Slack: {e:#}"));
        }
    }
    if settings.telegram.enabled {
        if let Err(e) = send_telegram(&settings.telegram, &selected).await {
            errors.push(format!("Telegram: {e:#}"));
        }
    }
    if settings.push.enabled {
        if let Err(e) = send_push(pool, &settings.push, &selected).await {
            errors.push(format!("browser push: {e:#}"));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        bail!(errors.join("; "))
    }
}

pub async fn send_test_notification(
    pool: &PgPool,
    settings: &NotificationSettings,
    channel: &NotificationChannel,
) -> anyhow::Result<()> {
    let alerts = vec![Alert {
        id: "sentinel:test:notification".to_string(),
        sev: "info".to_string(),
        status: "open".to_string(),
        title: "Sentinel test notification".to_string(),
        desc: "This confirms Sentinel can send notifications through this channel.".to_string(),
        source: "Sentinel".to_string(),
        host: "backend".to_string(),
        target: "notifications".to_string(),
        age_min: 0,
        occurrences: 1,
        assignee: None,
        rule: "manual notification test".to_string(),
    }];

    match channel {
        NotificationChannel::Email => send_email(&settings.email, &alerts).await,
        NotificationChannel::Slack => send_slack(&settings.slack, &alerts).await,
        NotificationChannel::Telegram => send_telegram(&settings.telegram, &alerts).await,
        NotificationChannel::Push => send_push(pool, &settings.push, &alerts).await,
    }
}

async fn send_email(settings: &EmailNotificationSettings, alerts: &[Alert]) -> anyhow::Result<()> {
    if settings.smtp_host.is_empty() {
        bail!("SMTP host is required");
    }
    if settings.from.is_empty() {
        bail!("email sender is required");
    }
    if settings.to.is_empty() {
        bail!("at least one email recipient is required");
    }

    let mut builder = Message::builder()
        .from(parse_mailbox(&settings.from).context("invalid email sender")?)
        .subject(notification_subject(alerts));
    for recipient in &settings.to {
        builder = builder.to(parse_mailbox(recipient)
            .with_context(|| format!("invalid email recipient '{recipient}'"))?);
    }
    let message = builder
        .body(format_plain(alerts))
        .context("building email message")?;

    let mut transport = match normalize_smtp_security(&settings.smtp_security).as_str() {
        "none" => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&settings.smtp_host),
        "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(&settings.smtp_host)
            .context("building SMTP TLS transport")?,
        _ => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&settings.smtp_host)
            .context("building SMTP STARTTLS transport")?,
    };
    transport = transport.port(settings.smtp_port);
    if !settings.smtp_username.is_empty() {
        transport = transport.credentials(Credentials::new(
            settings.smtp_username.clone(),
            settings.smtp_password.clone(),
        ));
    }
    let mailer = transport.build();
    mailer.send(message).await.context("sending SMTP message")?;
    Ok(())
}

async fn send_slack(settings: &SlackNotificationSettings, alerts: &[Alert]) -> anyhow::Result<()> {
    if settings.webhook_url.is_empty() {
        bail!("Slack webhook URL is required");
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("building Slack HTTP client")?;
    let res = client
        .post(&settings.webhook_url)
        .json(&json!({ "text": format_slack(alerts) }))
        .send()
        .await
        .context("sending Slack webhook")?;
    if !res.status().is_success() {
        bail!("Slack webhook returned HTTP {}", res.status());
    }
    Ok(())
}

async fn send_telegram(
    settings: &TelegramNotificationSettings,
    alerts: &[Alert],
) -> anyhow::Result<()> {
    if settings.bot_token.is_empty() {
        bail!("Telegram bot token is required");
    }
    if settings.chat_id.is_empty() {
        bail!("Telegram chat ID is required");
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("building Telegram HTTP client")?;
    let url = format!(
        "https://api.telegram.org/bot{}/sendMessage",
        settings.bot_token
    );
    let res = client
        .post(url)
        .json(&json!({
            "chat_id": settings.chat_id,
            "text": format_plain(alerts),
            "disable_web_page_preview": true,
        }))
        .send()
        .await
        .context("sending Telegram message")?;
    if !res.status().is_success() {
        let status = res.status();
        let body = res.text().await.unwrap_or_default();
        bail!("Telegram API returned HTTP {status}: {body}");
    }
    Ok(())
}

async fn send_push(
    pool: &PgPool,
    settings: &PushNotificationSettings,
    alerts: &[Alert],
) -> anyhow::Result<()> {
    if settings.vapid_private_key.trim().is_empty() {
        bail!("browser push VAPID key is not configured");
    }
    let subscriptions = db::get_push_subscriptions(pool).await?;
    if subscriptions.is_empty() {
        bail!("no browser push subscriptions are registered");
    }

    let payload = format_push_payload(alerts);
    let client = ReqwestWebPushClient::new();
    let mut errors = Vec::new();
    for sub in subscriptions {
        let info = SubscriptionInfo::new(sub.endpoint, sub.p256dh, sub.auth);
        let mut sig_builder =
            VapidSignatureBuilder::from_base64(&settings.vapid_private_key, URL_SAFE_NO_PAD, &info)
                .context("building browser push VAPID signature")?;
        if !settings.vapid_subject.trim().is_empty() {
            sig_builder.add_claim("sub", settings.vapid_subject.clone());
        }
        let mut builder = WebPushMessageBuilder::new(&info);
        builder.set_payload(ContentEncoding::Aes128Gcm, payload.as_bytes());
        builder.set_vapid_signature(sig_builder.build()?);
        builder.set_ttl(3600);
        builder.set_urgency(Urgency::High);
        if let Err(e) = client.send(builder.build()?).await {
            errors.push(format!("{e:#}"));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        bail!(errors.join("; "))
    }
}

fn parse_mailbox(value: &str) -> anyhow::Result<Mailbox> {
    value.parse().map_err(|e| anyhow!("{e}"))
}

fn notification_subject(alerts: &[Alert]) -> String {
    match alerts {
        [a] => format!("Sentinel {} alert: {}", a.sev.to_uppercase(), a.title),
        _ => format!("Sentinel: {} new alerts", alerts.len()),
    }
}

fn format_plain(alerts: &[Alert]) -> String {
    let mut out = String::new();
    out.push_str(&notification_subject(alerts));
    out.push_str("\n\n");
    for (idx, a) in alerts.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        out.push_str(&format!(
            "[{}] {}\nSource: {} / {}\nTarget: {}\nRule: {}\nOccurrences: {}\n{}\n",
            a.sev.to_uppercase(),
            a.title,
            a.source,
            a.host,
            a.target,
            a.rule,
            a.occurrences,
            a.desc
        ));
    }
    out
}

fn format_slack(alerts: &[Alert]) -> String {
    let mut out = String::new();
    out.push_str(&format!("*{}*\n", notification_subject(alerts)));
    for a in alerts {
        out.push_str(&format!(
            "\n*{}* `{}`\n{} / {} / {}\nRule: `{}`\nOccurrences: {}\n{}",
            a.title,
            a.sev.to_uppercase(),
            a.source,
            a.host,
            a.target,
            a.rule,
            a.occurrences,
            a.desc
        ));
    }
    out
}

fn format_push_payload(alerts: &[Alert]) -> String {
    let body = match alerts {
        [a] => format!("{} / {} / {}", a.source, a.host, a.target),
        _ => format!("{} alerts need attention", alerts.len()),
    };
    json!({
        "title": notification_subject(alerts),
        "body": body,
        "url": "/alerts",
        "tag": "sentinel-alerts",
        "icon": "/pwa-icon.svg",
        "badge": "/pwa-icon.svg",
        "data": {
            "url": "/alerts",
        }
    })
    .to_string()
}

fn normalize_min_severity(value: &str) -> String {
    match value {
        "crit" => "crit".to_string(),
        _ => "warn".to_string(),
    }
}

fn normalize_smtp_security(value: &str) -> String {
    match value {
        "none" => "none".to_string(),
        "tls" => "tls".to_string(),
        _ => "starttls".to_string(),
    }
}

fn severity_rank(sev: &str) -> u8 {
    match sev {
        "crit" => 0,
        "warn" => 1,
        _ => 2,
    }
}

fn severity_allowed(sev: &str, min: &str) -> bool {
    severity_rank(sev) <= severity_rank(&normalize_min_severity(min))
}
