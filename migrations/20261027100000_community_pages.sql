-- Community pages: rich-text flag, edited marker, and post reports.
ALTER TABLE community_posts ADD COLUMN IF NOT EXISTS is_html BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE community_posts ADD COLUMN IF NOT EXISTS edited_at TIMESTAMPTZ;

CREATE TABLE IF NOT EXISTS community_post_reports (
  id BIGSERIAL PRIMARY KEY,
  community_id INTEGER NOT NULL REFERENCES communities(id) ON DELETE CASCADE,
  post_id BIGINT NOT NULL REFERENCES community_posts(id) ON DELETE CASCADE,
  reporter_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  reason TEXT NOT NULL,
  details TEXT NOT NULL DEFAULT '',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (post_id, reporter_id)
);