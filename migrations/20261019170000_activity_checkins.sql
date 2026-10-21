CREATE TABLE activity_checkins (
    id BIGSERIAL PRIMARY KEY,
    user_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    category TEXT NOT NULL
        CHECK (category IN (
            'nightclubs',
            'bars',
            'pubs',
            'movies',
            'concerts',
            'travel',
            'dining',
            'outdoors'
        )),
    place TEXT NOT NULL CHECK (char_length(place) BETWEEN 1 AND 100),
    note TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (note IS NULL OR char_length(note) <= 240)
);

CREATE INDEX activity_checkins_created_idx
    ON activity_checkins (created_at DESC, id DESC);

CREATE INDEX activity_checkins_category_created_idx
    ON activity_checkins (category, created_at DESC, id DESC);
