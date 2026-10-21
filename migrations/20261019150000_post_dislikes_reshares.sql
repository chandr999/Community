CREATE TABLE post_dislikes (
    user_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    post_id INT NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, post_id)
);

CREATE INDEX post_dislikes_post_idx ON post_dislikes (post_id);

ALTER TABLE posts
    ADD COLUMN reshared_from_id INT REFERENCES posts(id) ON DELETE CASCADE;

CREATE INDEX posts_reshared_from_idx ON posts (reshared_from_id);

CREATE UNIQUE INDEX posts_one_reshare_per_user_idx
    ON posts (user_id, reshared_from_id)
    WHERE reshared_from_id IS NOT NULL;
