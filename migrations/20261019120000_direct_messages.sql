CREATE TABLE conversations (
    id SERIAL PRIMARY KEY,
    title TEXT,
    is_group BOOLEAN NOT NULL DEFAULT FALSE,
    created_by INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (title IS NULL OR char_length(title) <= 60)
);

CREATE TABLE conversation_members (
    conversation_id INT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    user_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (conversation_id, user_id)
);

CREATE INDEX conversation_members_user_idx
    ON conversation_members (user_id, conversation_id);

CREATE TABLE direct_messages (
    id BIGSERIAL PRIMARY KEY,
    conversation_id INT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    sender_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body TEXT NOT NULL DEFAULT '',
    image_path TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (char_length(body) <= 2000),
    CHECK (body <> '' OR image_path IS NOT NULL)
);

CREATE INDEX direct_messages_conversation_idx
    ON direct_messages (conversation_id, id DESC);
