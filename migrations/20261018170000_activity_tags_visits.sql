CREATE TABLE post_tags (
    post_id INT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    user_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (post_id, user_id)
);

CREATE INDEX post_tags_user_id_idx ON post_tags (user_id, created_at DESC);

CREATE TABLE activity_notifications (
    id BIGSERIAL PRIMARY KEY,
    recipient_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    actor_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    post_id INT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    comment_id INT REFERENCES comments(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('comment_mention', 'photo_tag')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK ((kind = 'comment_mention' AND comment_id IS NOT NULL)
        OR (kind = 'photo_tag' AND comment_id IS NULL))
);

CREATE INDEX activity_notifications_recipient_idx
    ON activity_notifications (recipient_id, created_at DESC, id DESC);

CREATE TABLE profile_visits (
    profile_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    visitor_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    last_visited_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (profile_id, visitor_id),
    CHECK (profile_id <> visitor_id)
);

CREATE INDEX profile_visits_recent_idx
    ON profile_visits (profile_id, last_visited_at DESC);
