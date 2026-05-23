-- Cybex Sentinel — outbound alert notifications.
--
-- Stores SMTP email, Slack incoming webhook and Telegram Bot API delivery
-- settings in the existing JSON-backed application settings table. Secrets are
-- retained server-side and masked in the Settings API response.

INSERT INTO settings (key, value) VALUES
    ('notifications', '{
        "minSeverity": "warn",
        "email": {
            "enabled": false,
            "smtpHost": "",
            "smtpPort": 587,
            "smtpUsername": "",
            "smtpPassword": "",
            "smtpSecurity": "starttls",
            "from": "",
            "to": []
        },
        "slack": {
            "enabled": false,
            "webhookUrl": ""
        },
        "telegram": {
            "enabled": false,
            "botToken": "",
            "chatId": ""
        }
    }'::jsonb)
ON CONFLICT (key) DO NOTHING;
