ALTER TABLE users
    ADD COLUMN IF NOT EXISTS is_superadmin BOOLEAN NOT NULL DEFAULT FALSE;

INSERT INTO app_settings (key, value)
VALUES ('app_enabled', 'true')
ON CONFLICT (key) DO NOTHING;
