-- Add migration script here
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    username TEXT UNIQUE NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE posts (
    id SERIAL PRIMARY KEY,
    user_id INT NOT NULL REFERENCES users(id),
    image_url TEXT NOT NULL,
    caption TEXT NOT NULL DEFAULT '',
    likes INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- sample data so the feed isn't empty (temporary, for development)
INSERT INTO users (username) VALUES ('alice'), ('ravi'), ('mina');

INSERT INTO posts (user_id, image_url, caption, likes) VALUES
    (1, 'https://picsum.photos/seed/a/600/600', 'Morning walk', 12),
    (2, 'https://picsum.photos/seed/b/600/600', 'Coffee time', 31),
    (3, 'https://picsum.photos/seed/c/600/600', 'Weekend trip', 7);