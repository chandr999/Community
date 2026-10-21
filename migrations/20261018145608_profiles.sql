-- Add migration script here
ALTER TABLE users
    ADD COLUMN display_name TEXT,
    ADD COLUMN bio TEXT,
    ADD COLUMN age INT CHECK (age BETWEEN 13 AND 120),
    ADD COLUMN location TEXT,
    ADD COLUMN interests TEXT,
    ADD COLUMN relationship_status TEXT,
    ADD COLUMN avatar_url TEXT;