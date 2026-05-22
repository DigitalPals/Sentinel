-- First-run onboarding flag.
--
-- The onboarding wizard sets this to true once initial setup has been
-- completed or skipped. It stays false on a fresh database, which is what the
-- frontend uses to decide whether to show the wizard.
INSERT INTO settings (key, value) VALUES ('onboarding_done', 'false'::jsonb)
ON CONFLICT (key) DO NOTHING;
