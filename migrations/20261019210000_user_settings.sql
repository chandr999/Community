ALTER TABLE users
    ADD COLUMN profile_visibility TEXT NOT NULL DEFAULT 'public'
        CHECK (profile_visibility IN ('public', 'followers', 'private')),
    ADD COLUMN message_privacy TEXT NOT NULL DEFAULT 'everyone'
        CHECK (message_privacy IN ('everyone', 'following', 'nobody')),
    ADD COLUMN show_activity_status BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN keep_profile_visitors BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN notify_mentions BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN account_enabled BOOLEAN NOT NULL DEFAULT TRUE;

CREATE TABLE user_blocks (
    blocker_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    blocked_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (blocker_id, blocked_id),
    CHECK (blocker_id <> blocked_id)
);
