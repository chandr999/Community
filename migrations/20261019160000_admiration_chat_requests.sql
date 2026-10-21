CREATE TABLE profile_admirations (
    admirer_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    recipient_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (admirer_id, recipient_id),
    CHECK (admirer_id <> recipient_id)
);

CREATE INDEX profile_admirations_recipient_idx
    ON profile_admirations (recipient_id, created_at DESC);

CREATE TABLE personal_chat_requests (
    id BIGSERIAL PRIMARY KEY,
    sender_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    recipient_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'accepted', 'rejected')),
    conversation_id INT REFERENCES conversations(id) ON DELETE SET NULL,
    requested_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    responded_at TIMESTAMPTZ,
    CHECK (sender_id <> recipient_id),
    CHECK (status <> 'accepted' OR conversation_id IS NOT NULL)
);

CREATE INDEX personal_chat_requests_recipient_idx
    ON personal_chat_requests (recipient_id, requested_at DESC);

CREATE INDEX personal_chat_requests_sender_idx
    ON personal_chat_requests (sender_id, requested_at DESC);
