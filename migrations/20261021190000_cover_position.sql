ALTER TABLE users
    ADD COLUMN IF NOT EXISTS cover_position_x INT NOT NULL DEFAULT 50
        CHECK (cover_position_x BETWEEN 0 AND 100),
    ADD COLUMN IF NOT EXISTS cover_position_y INT NOT NULL DEFAULT 50
        CHECK (cover_position_y BETWEEN 0 AND 100),
    ADD COLUMN IF NOT EXISTS cover_zoom INT NOT NULL DEFAULT 100
        CHECK (cover_zoom BETWEEN 100 AND 200);
