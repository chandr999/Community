CREATE TABLE stories (
    id SERIAL PRIMARY KEY,
    user_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('text', 'image', 'video')),
    body TEXT NOT NULL DEFAULT '',
    media_url TEXT,
    bg_color TEXT NOT NULL DEFAULT '#6a54c6',
    mood TEXT CHECK (mood IS NULL OR mood IN (
        'Bored', 'Romantic', 'Angry', 'Sad', 'Happy', 'Calm',
        'Excited', 'Playful', 'Lonely', 'Grateful', 'Stressed'
    )),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT now() + interval '24 hours'
);
CREATE INDEX stories_user_active_idx ON stories (user_id, expires_at);

CREATE TABLE story_views (
    story_id INT NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    viewer_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    viewed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (story_id, viewer_id)
);
