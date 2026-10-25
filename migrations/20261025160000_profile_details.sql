ALTER TABLE users
    ADD COLUMN IF NOT EXISTS date_of_birth DATE,
    ADD COLUMN IF NOT EXISTS birthday_visibility TEXT NOT NULL DEFAULT 'self',
    ADD COLUMN IF NOT EXISTS company_name TEXT,
    ADD COLUMN IF NOT EXISTS designation TEXT,
    ADD COLUMN IF NOT EXISTS work_visibility TEXT NOT NULL DEFAULT 'self',
    ADD COLUMN IF NOT EXISTS contact_email TEXT,
    ADD COLUMN IF NOT EXISTS email_visibility TEXT NOT NULL DEFAULT 'self',
    ADD COLUMN IF NOT EXISTS phone_visibility TEXT NOT NULL DEFAULT 'self',
    ADD COLUMN IF NOT EXISTS instagram_handle TEXT,
    ADD COLUMN IF NOT EXISTS facebook_handle TEXT,
    ADD COLUMN IF NOT EXISTS tiktok_handle TEXT,
    ADD COLUMN IF NOT EXISTS linkedin_handle TEXT,
    ADD COLUMN IF NOT EXISTS social_visibility TEXT NOT NULL DEFAULT 'self';
