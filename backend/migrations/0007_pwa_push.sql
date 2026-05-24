-- Cybex Sentinel — browser push notification support.
--
-- Browser push subscriptions are stored server-side and used as another alert
-- delivery channel. VAPID signing material lives in the existing notification
-- settings JSON; the private key is never exposed through public settings.

CREATE TABLE IF NOT EXISTS push_subscriptions (
    endpoint TEXT PRIMARY KEY,
    p256dh TEXT NOT NULL,
    auth TEXT NOT NULL,
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

UPDATE settings
SET value = jsonb_set(
    value,
    '{push}',
    '{
        "enabled": false,
        "vapidSubject": "mailto:admin@localhost",
        "vapidPrivateKey": ""
    }'::jsonb,
    true
)
WHERE key = 'notifications'
  AND NOT (value ? 'push');
