ALTER TABLE activity_checkins
    ADD COLUMN IF NOT EXISTS latitude DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS longitude DOUBLE PRECISION;

ALTER TABLE activity_checkins
    DROP CONSTRAINT IF EXISTS activity_checkins_category_check,
    DROP CONSTRAINT IF EXISTS activity_checkins_coordinates_check,
    ADD CONSTRAINT activity_checkins_category_check
        CHECK (category IN (
            'nightclubs',
            'bars',
            'pubs',
            'restaurants',
            'hotels',
            'movies',
            'concerts',
            'travel',
            'dining',
            'outdoors'
        )),
    ADD CONSTRAINT activity_checkins_coordinates_check
        CHECK (
            (latitude IS NULL AND longitude IS NULL)
            OR (
                latitude BETWEEN -90 AND 90
                AND longitude BETWEEN -180 AND 180
            )
        );

CREATE TABLE IF NOT EXISTS live_locations (
    user_id INT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    latitude DOUBLE PRECISION NOT NULL CHECK (latitude BETWEEN -90 AND 90),
    longitude DOUBLE PRECISION NOT NULL CHECK (longitude BETWEEN -180 AND 180),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS live_locations_updated_idx
    ON live_locations (updated_at DESC);
