ALTER TABLE users
    ADD COLUMN last_active_at TIMESTAMPTZ;

CREATE INDEX users_active_idx
    ON users (last_active_at DESC)
    WHERE last_active_at IS NOT NULL;

CREATE TABLE user_favorites (
    user_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    favorite_user_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, favorite_user_id),
    CHECK (user_id <> favorite_user_id)
);

CREATE INDEX user_favorites_target_idx
    ON user_favorites (favorite_user_id, created_at DESC);

CREATE TABLE post_reports (
    id BIGSERIAL PRIMARY KEY,
    reporter_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    post_id INT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    reason TEXT NOT NULL CHECK (reason IN ('spam', 'harassment', 'hate', 'nudity', 'violence', 'scam', 'other')),
    details TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (reporter_id, post_id),
    CHECK (char_length(details) <= 500)
);

CREATE INDEX post_reports_post_idx
    ON post_reports (post_id, created_at DESC);
