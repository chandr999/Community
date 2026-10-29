-- Rich posts: text-only posts, plus an optional file attachment.
ALTER TABLE posts DROP CONSTRAINT IF EXISTS posts_media_type_check;
ALTER TABLE posts ADD CONSTRAINT posts_media_type_check CHECK (media_type = ANY (ARRAY['image'::text, 'video'::text, 'text'::text]));
ALTER TABLE posts ADD COLUMN IF NOT EXISTS attachment_path TEXT;
ALTER TABLE posts ADD COLUMN IF NOT EXISTS attachment_name TEXT;
ALTER TABLE posts ADD COLUMN IF NOT EXISTS attachment_size BIGINT;