# apply-final-changes.ps1
# Run from D:\Community:
#   powershell -ExecutionPolicy Bypass -File .\apply-final-changes.ps1
# Then:  sqlx migrate run   and   cargo run
#
# What it does
#   1. Online indicator (green glowing dot) on avatars and username links across the app
#   2. Chats show profile pictures (with online dot) instead of name initials
#   3. Home post box: one text field + photo/video, file, GIF, sticker, emoji, tag, drag-and-drop
#   4. Follow / Following button only changes label and color (no "Following @user" text)
#   5. Right side of the home page gets Daily spark, Community pulse and Take a breather panels
#   6. Chat window moved up so it no longer covers the feedback button
# Files changed: static\index.html, src\main.rs, plus one new migration.
# Backups go to backups\final-<timestamp>. Nothing is written if any check fails.

$ErrorActionPreference = 'Stop'
$root = (Get-Location).Path
$indexHtml = Join-Path $root 'static\index.html'
$mainRs = Join-Path $root 'src\main.rs'
$migDir = Join-Path $root 'migrations'
foreach ($p in @($indexHtml, $mainRs, $migDir)) {
  if (-not (Test-Path $p)) { throw "Run this from D:\Community. Missing: $p" }
}
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Edit-Once([string]$text, [string]$find, [string]$replace, [string]$label) {
  $count = [regex]::Matches($text, [regex]::Escape($find)).Count
  if ($count -ne 1) { throw "[$label] expected exactly 1 match, found $count. Nothing was changed." }
  return $text.Replace($find, $replace)
}

function Edit-IfPresent([string]$text, [string]$find, [string]$replace, [string]$label) {
  $count = [regex]::Matches($text, [regex]::Escape($find)).Count
  if ($count -eq 0) { Write-Host "  (skipped optional step: $label)"; return $text }
  if ($count -ne 1) { throw "[$label] expected 0 or 1 match, found $count. Nothing was changed." }
  return $text.Replace($find, $replace)
}

function Get-Edits([string]$text, [int]$expected, [string]$name) {
  $found = [regex]::Matches($text, '(?s)@@FIND\r?\n(.*?)\r?\n@@REPLACE\r?\n(.*?)\r?\n@@END')
  if ($found.Count -ne $expected) { throw "[$name] expected $expected edits, parsed $($found.Count). Nothing was changed." }
  $list = New-Object System.Collections.ArrayList
  foreach ($m in $found) { [void]$list.Add(@($m.Groups[1].Value, $m.Groups[2].Value)) }
  return ,$list
}

# =====================================================================
# 1. Database migration
# =====================================================================
$migration = @'
-- Rich posts: text-only posts, plus an optional file attachment.
ALTER TABLE posts DROP CONSTRAINT IF EXISTS posts_media_type_check;
ALTER TABLE posts ADD CONSTRAINT posts_media_type_check CHECK (media_type = ANY (ARRAY['image'::text, 'video'::text, 'text'::text]));
ALTER TABLE posts ADD COLUMN IF NOT EXISTS attachment_path TEXT;
ALTER TABLE posts ADD COLUMN IF NOT EXISTS attachment_name TEXT;
ALTER TABLE posts ADD COLUMN IF NOT EXISTS attachment_size BIGINT;
'@
$migPath = Join-Path $migDir '20261028100000_post_rich_content.sql'

# =====================================================================
# 2. Rust: new helpers, presence endpoint, attachment download, new create_post
# =====================================================================
$rustHelpers = @'
const TEXT_POST_URL: &str = "/uploads/text-only";
const MAX_POST_TEXT_CHARS: usize = 2000;
const MAX_ATTACHMENT_BYTES: usize = 20 * 1024 * 1024;
const MAX_REMOTE_MEDIA_BYTES: usize = 25 * 1024 * 1024;

fn clean_attachment_name(raw: &str) -> String {
    let base = raw.rsplit(|c: char| c == '/' || c == '\\').next().unwrap_or("");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == ' ' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned: String = cleaned.trim().trim_matches('.').chars().take(80).collect();
    if cleaned.is_empty() {
        "attachment".to_string()
    } else {
        cleaned
    }
}

fn attachment_extension(name: &str, bytes: &[u8]) -> Option<&'static str> {
    let ext = name.rsplit('.').next()?.to_ascii_lowercase();
    let zip = bytes.starts_with(b"PK\x03\x04");
    let ole = bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]);
    let text = std::str::from_utf8(bytes).is_ok();
    match ext.as_str() {
        "pdf" if bytes.starts_with(b"%PDF") => Some("pdf"),
        "docx" if zip => Some("docx"),
        "xlsx" if zip => Some("xlsx"),
        "pptx" if zip => Some("pptx"),
        "zip" if zip => Some("zip"),
        "doc" if ole => Some("doc"),
        "xls" if ole => Some("xls"),
        "ppt" if ole => Some("ppt"),
        "rtf" if bytes.starts_with(b"{\\rtf") => Some("rtf"),
        "txt" if text => Some("txt"),
        "csv" if text => Some("csv"),
        "md" if text => Some("md"),
        _ => None,
    }
}

async fn fetch_remote_post_media(
    value: &str,
) -> Result<(Vec<u8>, &'static str, &'static str), StatusCode> {
    if value.len() > 2048 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let parsed = reqwest::Url::parse(value).map_err(|_| StatusCode::BAD_REQUEST)?;
    let host = parsed.host_str().unwrap_or_default().to_string();
    if parsed.scheme() != "https"
        || !(host == "giphy.com"
            || host.ends_with(".giphy.com")
            || host == "upload.wikimedia.org"
            || host == "thumb.wikimedia.org"
            || host == "commons.wikimedia.org")
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Community/1.0")
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let response = client.get(parsed).send().await.map_err(|e| {
        eprintln!("remote post media fetch failed: {e}");
        StatusCode::BAD_GATEWAY
    })?;
    if !response.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }
    if response
        .content_length()
        .map(|n| n as usize > MAX_REMOTE_MEDIA_BYTES)
        .unwrap_or(false)
    {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let bytes = response.bytes().await.map_err(|e| {
        eprintln!("remote post media read failed: {e}");
        StatusCode::BAD_GATEWAY
    })?;
    if bytes.len() > MAX_REMOTE_MEDIA_BYTES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    if let Some(ext) = image_extension(&bytes) {
        Ok((bytes.to_vec(), ext, "image"))
    } else if let Some(ext) = video_extension(&bytes) {
        Ok((bytes.to_vec(), ext, "video"))
    } else {
        Err(StatusCode::UNSUPPORTED_MEDIA_TYPE)
    }
}

async fn presence_lookup(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<(String, Option<String>, bool)>>, StatusCode> {
    let me: Option<i32> = current_user_id(&pool, &headers).await.ok();
    let names: Vec<String> = params
        .get("users")
        .map(|raw| {
            raw.split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty() && s.len() <= 50)
                .take(100)
                .collect()
        })
        .unwrap_or_default();
    if names.is_empty() {
        return Ok(Json(Vec::new()));
    }
    let rows: Vec<(String, Option<String>, bool)> = sqlx::query_as(
        r#"
        SELECT u.username,
               CASE WHEN u.profile_visibility = 'public'
                      OR u.id = $2
                      OR EXISTS (SELECT 1 FROM follows f
                                 WHERE f.follower_id = $2 AND f.followee_id = u.id)
                    THEN u.avatar_url END AS avatar_url,
               COALESCE(u.account_enabled AND u.show_activity_status
                        AND u.last_active_at > now() - interval '2 minutes', FALSE) AS online
        FROM users u
        WHERE lower(u.username) = ANY($1) AND u.account_enabled
        "#,
    )
    .bind(&names)
    .bind(me)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("presence lookup failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows))
}

async fn read_post_attachment(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(post_id): Path<i32>,
) -> Result<Response, StatusCode> {
    let _user_id = current_user_id(&pool, &headers).await?;
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT attachment_path, attachment_name FROM posts WHERE id = $1 AND attachment_path IS NOT NULL",
    )
    .bind(post_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        eprintln!("post attachment lookup failed: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let (path, name) = match row {
        Some((Some(path), name)) => (path, name.unwrap_or_else(|| "attachment".to_string())),
        _ => return Err(StatusCode::NOT_FOUND),
    };
    let safe =
        !path.is_empty() && !path.contains('/') && !path.contains('\\') && !path.contains("..");
    if !safe {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let bytes = tokio::fs::read(format!("private_uploads/{path}"))
        .await
        .map_err(|e| {
            eprintln!("could not read post attachment: {e}");
            StatusCode::NOT_FOUND
        })?;
    let content_type = match path.rsplit('.').next() {
        Some("pdf") => "application/pdf",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Some("pptx") => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        Some("zip") => "application/zip",
        Some("doc") => "application/msword",
        Some("xls") => "application/vnd.ms-excel",
        Some("ppt") => "application/vnd.ms-powerpoint",
        Some("rtf") => "application/rtf",
        Some("txt") | Some("md") => "text/plain; charset=utf-8",
        Some("csv") => "text/csv; charset=utf-8",
        _ => "application/octet-stream",
    };
    let disposition = format!("attachment; filename=\"{}\"", name.replace('"', "_"));
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (header::CONTENT_DISPOSITION, disposition),
            (header::CACHE_CONTROL, "private, no-store".to_string()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
        ],
        bytes,
    )
        .into_response())
}


'@

$rustCreatePost = @'
async fn create_post(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<StatusCode, StatusCode> {
    let user_id = current_user_id(&pool, &headers).await?;
    let account_enabled: bool =
        sqlx::query_scalar("SELECT account_enabled FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&pool)
            .await
            .map_err(|e| {
                eprintln!("post account status lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
    if !account_enabled {
        return Err(StatusCode::FORBIDDEN);
    }

    let mut caption = String::new();
    let mut media: Option<(Vec<u8>, &'static str, &'static str)> = None;
    let mut remote_media_url: Option<String> = None;
    let mut attachment: Option<(Vec<u8>, String, &'static str)> = None;
    let mut tags_input = String::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?
    {
        let name = field.name().map(|s| s.to_string());
        match name.as_deref() {
            Some("caption") => {
                caption = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            }
            Some("image") => {
                let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                if bytes.len() > 50 * 1024 * 1024 {
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
                let uploaded_media = if let Some(ext) = image_extension(&bytes) {
                    (bytes.to_vec(), ext, "image")
                } else if let Some(ext) = video_extension(&bytes) {
                    (bytes.to_vec(), ext, "video")
                } else {
                    return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
                };
                media = Some(uploaded_media);
            }
            Some("remote_media_url") => {
                remote_media_url = Some(field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?);
            }
            Some("attachment") => {
                let file_name = field.file_name().map(|s| s.to_string()).unwrap_or_default();
                let bytes = field.bytes().await.map_err(|_| StatusCode::BAD_REQUEST)?;
                if bytes.len() > MAX_ATTACHMENT_BYTES {
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
                let display_name = clean_attachment_name(&file_name);
                let ext = attachment_extension(&display_name, &bytes)
                    .ok_or(StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
                attachment = Some((bytes.to_vec(), display_name, ext));
            }
            Some("tags") => {
                tags_input = field.text().await.map_err(|_| StatusCode::BAD_REQUEST)?;
            }
            _ => {}
        }
    }

    if media.is_none() {
        if let Some(value) = remote_media_url.as_deref() {
            media = Some(fetch_remote_post_media(value).await?);
        }
    }
    let caption = caption.trim();
    if caption.chars().count() > MAX_POST_TEXT_CHARS {
        return Err(StatusCode::BAD_REQUEST);
    }
    if caption.is_empty() && media.is_none() && attachment.is_none() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let tags = tagged_usernames(&tags_input)?;
    let tagged_users: Vec<(i32,)> = if tags.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as("SELECT id FROM users WHERE username = ANY($1)")
            .bind(&tags)
            .fetch_all(&pool)
            .await
            .map_err(|e| {
                eprintln!("post tag lookup failed: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
    };
    if tagged_users.len() != tags.len() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let (url, media_type): (String, &'static str) = if let Some((bytes, ext, kind)) = media {
        (save_image(&bytes, ext).await?, kind)
    } else {
        (TEXT_POST_URL.to_string(), "text")
    };
    let has_media_file = media_type != "text";

    let mut saved_attachment: Option<(String, String, i64)> = None;
    if let Some((bytes, display_name, ext)) = attachment {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let stored = format!("post-{user_id}-{stamp}.{ext}");
        let saved: Result<(), std::io::Error> = async {
            tokio::fs::create_dir_all("private_uploads").await?;
            tokio::fs::write(format!("private_uploads/{stored}"), &bytes).await
        }
        .await;
        if let Err(e) = saved {
            eprintln!("post attachment save failed: {e}");
            if has_media_file {
                remove_upload(&url).await;
            }
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        saved_attachment = Some((stored, display_name, bytes.len() as i64));
    }
    let attachment_path: Option<String> = saved_attachment.as_ref().map(|a| a.0.clone());
    let attachment_name: Option<String> = saved_attachment.as_ref().map(|a| a.1.clone());
    let attachment_size: Option<i64> = saved_attachment.as_ref().map(|a| a.2);

    let result: Result<(), sqlx::Error> = async {
        let mut tx = pool.begin().await?;
        let (post_id,): (i32,) = sqlx::query_as(
            "INSERT INTO posts (user_id, image_url, media_type, caption, attachment_path, attachment_name, attachment_size)
             VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
        )
        .bind(user_id)
        .bind(&url)
        .bind(media_type)
        .bind(caption)
        .bind(&attachment_path)
        .bind(&attachment_name)
        .bind(attachment_size)
        .fetch_one(&mut *tx)
        .await?;

        for (tagged_user_id,) in &tagged_users {
            sqlx::query("INSERT INTO post_tags (post_id, user_id) VALUES ($1, $2)")
                .bind(post_id)
                .bind(tagged_user_id)
                .execute(&mut *tx)
                .await?;
            if *tagged_user_id != user_id {
                sqlx::query(
                    "INSERT INTO activity_notifications (recipient_id, actor_id, post_id, kind)
                     VALUES ($1, $2, $3, 'photo_tag')",
                )
                .bind(tagged_user_id)
                .bind(user_id)
                .bind(post_id)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await
    }
    .await;
    if let Err(e) = result {
        if has_media_file {
            remove_upload(&url).await;
        }
        if let Some(stored) = &attachment_path {
            let _ = tokio::fs::remove_file(format!("private_uploads/{stored}")).await;
        }
        eprintln!("create post failed: {e}");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    Ok(StatusCode::CREATED)
}
'@

$rustPostFields = @'

    #[sqlx(default)]
    attachment_name: Option<String>,
    #[sqlx(default)]
    attachment_size: Option<i64>,
'@

# =====================================================================
# 3. Page JavaScript: presence, composer, feed helpers, right-side widgets
# =====================================================================
$richBlock = @'
// ---------- online presence ----------
    var presenceCache = {};
    var presenceTimer = null;

    function presenceKey(name) {
      return String(name || '').replace(/^@/, '').trim().toLowerCase();
    }

    function presenceInfo(key) {
      return (typeof presenceCache === 'object' && presenceCache) ? presenceCache[key] : undefined;
    }

    function schedulePresence() {
      if (presenceTimer) return;
      presenceTimer = setTimeout(() => { presenceTimer = null; presenceRefresh(false); }, 350);
    }

    function presenceDot(username) {
      const dot = document.createElement('span');
      dot.className = 'pdot';
      dot.dataset.pu = presenceKey(username);
      dot.title = 'Online now';
      dot.setAttribute('aria-hidden', 'true');
      const info = presenceInfo(dot.dataset.pu);
      if (info && info.online) dot.classList.add('on');
      schedulePresence();
      return dot;
    }

    function swapPresenceAvatar(wrap, url) {
      const old = wrap.querySelector('.avatar');
      if (!old || old.tagName === 'IMG') return;
      const img = avatarEl(url);
      wrap.replaceChild(img, old);
    }

    // A profile picture with a green dot when that person is online.
    function presenceAvatar(username, url, size) {
      const key = presenceKey(username);
      const wrap = document.createElement('span');
      wrap.className = 'pav' + (size ? ' ' + size : '');
      wrap.dataset.pa = key;
      const avatar = avatarEl(url || null);
      if (!url) { avatar.textContent = (key[0] || '?').toUpperCase(); avatar.classList.add('pav-initial'); }
      wrap.append(avatar, presenceDot(username));
      const info = presenceInfo(key);
      if (!url && info && info.avatar) swapPresenceAvatar(wrap, info.avatar);
      return wrap;
    }

    function groupAvatarStack(names) {
      const stack = document.createElement('span');
      stack.className = 'pav-stack';
      const shown = names.slice(0, 2);
      if (!shown.length) { stack.append(el('div', 'G', 'activity-icon')); return stack; }
      shown.forEach(name => stack.append(presenceAvatar(name, null, 'chat')));
      return stack;
    }

    // Every profile link whose text is the username gets an online dot automatically.
    function presenceTagLink(a, username, label) {
      const key = presenceKey(username);
      if (!key || presenceKey(label) !== key) return;
      a.dataset.pu = key;
      const info = presenceInfo(key);
      if (info && info.online) a.dataset.online = '1';
      schedulePresence();
    }

    function presenceApply() {
      document.querySelectorAll('[data-pu]').forEach(node => {
        const info = presenceCache[node.dataset.pu];
        const online = Boolean(info && info.online);
        if (node.classList.contains('pdot')) node.classList.toggle('on', online);
        else if (online) node.dataset.online = '1';
        else delete node.dataset.online;
      });
      document.querySelectorAll('.pav[data-pa]').forEach(wrap => {
        const info = presenceCache[wrap.dataset.pa];
        if (info && info.avatar) swapPresenceAvatar(wrap, info.avatar);
      });
    }

    async function presenceRefresh(force) {
      const names = new Set();
      document.querySelectorAll('[data-pu]').forEach(node => { if (node.dataset.pu) names.add(node.dataset.pu); });
      document.querySelectorAll('.pav[data-pa]').forEach(node => { if (node.dataset.pa) names.add(node.dataset.pa); });
      const now = Date.now();
      const stale = Array.from(names).filter(name => force || !presenceCache[name] || now - presenceCache[name].at > 20000);
      for (let i = 0; i < stale.length; i += 80) {
        const batch = stale.slice(i, i + 80);
        try {
          const response = await fetch('/api/presence?users=' + encodeURIComponent(batch.join(',')));
          if (!response.ok) continue;
          const rows = await response.json();
          const seen = new Set();
          rows.forEach(row => {
            const key = presenceKey(row[0]);
            seen.add(key);
            presenceCache[key] = { online: Boolean(row[2]), avatar: row[1] || null, at: Date.now() };
          });
          batch.forEach(key => { if (!seen.has(key)) presenceCache[key] = { online: false, avatar: null, at: Date.now() }; });
        } catch (error) { /* try again on the next cycle */ }
      }
      presenceApply();
    }

    if (document.body) {
      new MutationObserver(() => schedulePresence()).observe(document.body, { childList: true, subtree: true });
    }
    setInterval(() => presenceRefresh(true), 25000);

    // ---------- post helpers ----------
    function pcFormatBytes(n) {
      const size = Number(n) || 0;
      if (size >= 1024 * 1024) return (size / (1024 * 1024)).toFixed(1) + ' MB';
      if (size >= 1024) return Math.round(size / 1024) + ' KB';
      return size + ' B';
    }

    function pcHue(name) {
      let hue = 0;
      for (const ch of String(name)) hue = (hue * 31 + ch.charCodeAt(0)) % 360;
      return hue;
    }

    function pcTextStage(p) {
      const stage = el('div', undefined, 'photo text-post');
      const hue = pcHue(p.user);
      stage.style.setProperty('--tp-a', 'hsl(' + hue + ', 70%, 55%)');
      stage.style.setProperty('--tp-b', 'hsl(' + ((hue + 60) % 360) + ', 75%, 62%)');
      const raw = p.caption && p.caption.trim() ? p.caption : (p.attachment_name ? 'Shared a file' : '');
      stage.append(el('p', raw, 'text-post-body' + (raw.length > 160 ? ' long' : '')));
      return stage;
    }

    function pcAttachmentChip(p) {
      const link = el('a', undefined, 'post-attachment');
      link.href = '/api/posts/' + p.id + '/attachment';
      link.setAttribute('download', p.attachment_name);
      link.rel = 'noopener';
      link.append(
        el('span', '\u{1F4CE}', 'post-attachment-icon'),
        el('span', p.attachment_name, 'post-attachment-name'),
        el('span', pcFormatBytes(p.attachment_size), 'post-attachment-size'),
      );
      return link;
    }

    // ---------- rich post box ----------
    function renderComposer() {
      const box = document.getElementById('composer');
      box.textContent = '';
      box.classList.remove('pc', 'dragging');
      if (!loggedIn) return;
      box.classList.add('pc');

      const MAX_TEXT = 2000;
      const MAX_TOTAL = 50 * 1024 * 1024;
      const MAX_FILE = 20 * 1024 * 1024;
      const state = { media: null, mediaUrl: null, remote: null, file: null };
      let busy = false;
      let pickerKind = 'gifs';

      const top = el('div', undefined, 'pc-top');
      const text = el('textarea', undefined, 'pc-text');
      text.id = 'pcText'; text.rows = 2; text.maxLength = MAX_TEXT;
      text.placeholder = 'What\u2019s on your mind?';
      text.setAttribute('aria-label', 'Write a post');
      top.append(presenceAvatar(currentUsername || '', null, 'chat'), text);
      const fillMine = () => {
        if (!currentUsername) return false;
        top.replaceChild(presenceAvatar(currentUsername, null, 'chat'), top.firstElementChild);
        text.placeholder = 'What\u2019s on your mind, @' + currentUsername + '?';
        document.dispatchEvent(new Event('spark-refresh'));
        return true;
      };
      if (!fillMine()) {
        let tries = 0;
        const waiter = setInterval(() => { if (!box.isConnected || fillMine() || ++tries > 30) clearInterval(waiter); }, 150);
      }

      const previews = el('div', undefined, 'pc-previews'); previews.hidden = true;
      const tagRow = el('div', undefined, 'pc-tagrow'); tagRow.hidden = true;
      const tagInput = el('input'); tagInput.type = 'text'; tagInput.placeholder = 'Tag people (usernames, comma-separated, up to 10)';
      tagRow.append(tagInput);
      const tray = el('div', undefined, 'pc-tray'); tray.hidden = true;
      const picker = el('section', undefined, 'pc-picker'); picker.hidden = true;
      const pickerHead = el('div', undefined, 'pc-picker-head');
      const pickerSearch = el('input'); pickerSearch.type = 'search'; pickerSearch.placeholder = 'Search GIFs'; pickerSearch.setAttribute('aria-label', 'Search GIFs or stickers');
      const pickerSearchBtn = el('button', 'Search', 'pc-mini'); pickerSearchBtn.type = 'button';
      pickerHead.append(pickerSearch, pickerSearchBtn);
      const pickerStatus = el('p', 'Search online GIFs and stickers.', 'pc-picker-status');
      const pickerResults = el('div', undefined, 'pc-picker-results');
      picker.append(pickerHead, pickerStatus, pickerResults);

      const photoInput = el('input'); photoInput.type = 'file'; photoInput.hidden = true;
      photoInput.accept = 'image/jpeg,image/png,image/webp,image/gif,video/mp4,video/webm';
      const fileInput = el('input'); fileInput.type = 'file'; fileInput.hidden = true;
      fileInput.accept = '.pdf,.doc,.docx,.xls,.xlsx,.ppt,.pptx,.txt,.csv,.md,.rtf,.zip';

      const bar = el('div', undefined, 'pc-bar');
      const tools = el('div', undefined, 'pc-tools');
      const makeTool = (label, title) => { const b = el('button', label, 'pc-tool'); b.type = 'button'; b.title = title; b.setAttribute('aria-label', title); return b; };
      const photoBtn = makeTool('\u{1F4F7} Photo/Video', 'Add a photo or video');
      const fileBtn = makeTool('\u{1F4CE} File', 'Attach a file');
      const gifBtn = makeTool('GIF', 'Add a GIF');
      const stickerBtn = makeTool('\u{1F31F} Sticker', 'Add a sticker');
      const emojiBtn = makeTool('\u{1F60A}', 'Add an emoji');
      const tagBtn = makeTool('@ Tag', 'Tag people');
      tools.append(photoBtn, fileBtn, gifBtn, stickerBtn, emojiBtn, tagBtn);
      const right = el('div', undefined, 'pc-right');
      const counter = el('span', '0/' + MAX_TEXT, 'pc-counter');
      const submit = el('button', 'Post', 'pc-submit'); submit.type = 'button'; submit.disabled = true;
      right.append(counter, submit);
      bar.append(tools, right);
      const msg = el('p', undefined, 'pc-msg'); msg.setAttribute('role', 'status');

      ['\u{1F600}', '\u{1F602}', '\u{1F970}', '\u{1F60D}', '\u{1F60A}', '\u{1F609}', '\u{1F62D}', '\u{1F60E}', '\u{1F917}', '\u2764\uFE0F', '\u{1F44D}', '\u{1F389}', '\u{1F64F}', '\u{1F525}', '\u2728', '\u{1F4AC}', '\u{1F308}', '\u{1F33F}', '\u{1F355}', '\u{1F3B6}', '\u{1F4AA}', '\u{1F914}', '\u{1F973}', '\u{1F31F}'].forEach(emoji => {
        const pick = el('button', emoji, 'pc-emoji'); pick.type = 'button';
        pick.onclick = () => {
          const start = text.selectionStart ?? text.value.length; const end = text.selectionEnd ?? start;
          text.value = text.value.slice(0, start) + emoji + text.value.slice(end);
          text.focus(); text.selectionStart = text.selectionEnd = start + emoji.length; autosize();
        };
        tray.append(pick);
      });

      function hasContent() { return text.value.trim().length > 0 || Boolean(state.media || state.remote || state.file); }
      function updatePostState() { counter.textContent = text.value.length + '/' + MAX_TEXT; submit.disabled = busy || !hasContent(); }
      function autosize() { text.style.height = 'auto'; text.style.height = Math.min(text.scrollHeight, 260) + 'px'; updatePostState(); }
      function removeButton(onClick) { const b = el('button', '\u00d7', 'pc-remove'); b.type = 'button'; b.setAttribute('aria-label', 'Remove'); b.onclick = onClick; return b; }

      function renderPreviews() {
        previews.replaceChildren();
        if (state.media && state.mediaUrl) {
          const chip = el('div', undefined, 'pc-preview');
          const isVideo = state.media.type.startsWith('video/');
          const thumb = el(isVideo ? 'video' : 'img');
          thumb.src = state.mediaUrl;
          if (isVideo) { thumb.muted = true; thumb.preload = 'metadata'; } else { thumb.alt = 'Selected photo'; }
          chip.append(thumb, removeButton(() => clearMedia()));
          previews.append(chip);
        }
        if (state.remote) {
          const chip = el('div', undefined, 'pc-preview');
          const thumb = el('img'); thumb.src = state.remote.preview; thumb.alt = state.remote.label;
          chip.append(thumb, el('span', state.remote.label, 'pc-badge'), removeButton(() => { state.remote = null; renderPreviews(); }));
          previews.append(chip);
        }
        if (state.file) {
          const chip = el('div', undefined, 'pc-file');
          chip.append(el('span', '\u{1F4CE}', 'pc-file-icon'), el('span', state.file.name, 'pc-file-name'), el('span', pcFormatBytes(state.file.size), 'pc-file-size'), removeButton(() => { state.file = null; renderPreviews(); }));
          previews.append(chip);
        }
        previews.hidden = !previews.childElementCount;
        updatePostState();
      }
      function clearMedia() { if (state.mediaUrl) URL.revokeObjectURL(state.mediaUrl); state.media = null; state.mediaUrl = null; renderPreviews(); }
      function setMedia(file) {
        if (file.size > MAX_TOTAL) { msg.textContent = 'Photos and videos can be up to 50 MB.'; return; }
        if (state.mediaUrl) URL.revokeObjectURL(state.mediaUrl);
        state.media = file; state.mediaUrl = URL.createObjectURL(file); state.remote = null; msg.textContent = ''; renderPreviews();
      }
      function setRemote(url, preview, label) {
        if (state.mediaUrl) URL.revokeObjectURL(state.mediaUrl);
        state.media = null; state.mediaUrl = null; state.remote = { url, preview, label }; msg.textContent = ''; renderPreviews();
      }
      function setFile(file) {
        if (file.size > MAX_FILE) { msg.textContent = 'Files can be up to 20 MB.'; return; }
        state.file = file; msg.textContent = ''; renderPreviews();
      }
      function takeFiles(files) {
        Array.from(files).forEach(file => { if (/^(image|video)\//.test(file.type)) setMedia(file); else setFile(file); });
      }
      function reset() {
        text.value = ''; tagInput.value = ''; tagRow.hidden = true; tray.hidden = true; picker.hidden = true;
        if (state.mediaUrl) URL.revokeObjectURL(state.mediaUrl);
        state.media = null; state.mediaUrl = null; state.remote = null; state.file = null;
        photoInput.value = ''; fileInput.value = ''; msg.textContent = ''; text.style.height = 'auto'; renderPreviews();
      }

      async function searchMedia() {
        const query = pickerSearch.value.trim();
        if (!query) { pickerStatus.textContent = 'Enter a search term.'; return; }
        pickerSearchBtn.disabled = true; pickerStatus.textContent = 'Searching...'; pickerResults.replaceChildren();
        try {
          const response = await fetch('/api/chat-media/search?q=' + encodeURIComponent(query) + '&kind=' + pickerKind);
          if (response.status === 401) throw new Error('Please log in again to search.');
          if (response.status === 503) throw new Error('Online media search is temporarily unavailable.');
          if (!response.ok) throw new Error('Search failed (status ' + response.status + ').');
          const results = await response.json();
          if (!results.length) { pickerStatus.textContent = 'No results found.'; return; }
          pickerStatus.textContent = 'Choose one to add to your post.';
          results.forEach(result => {
            const pick = el('button'); pick.type = 'button'; pick.title = result.title || 'Add to post';
            const img = el('img'); img.src = result.preview_url; img.alt = result.title || (pickerKind === 'stickers' ? 'Sticker' : 'GIF'); img.loading = 'lazy';
            pick.append(img);
            pick.onclick = () => { setRemote(result.media_url, result.preview_url || result.media_url, pickerKind === 'stickers' ? 'Sticker' : 'GIF'); picker.hidden = true; };
            const tile = el('div', undefined, 'pc-tile'); tile.append(pick); pickerResults.append(tile);
          });
        } catch (error) { pickerStatus.textContent = error.message; }
        finally { pickerSearchBtn.disabled = false; }
      }
      async function openPicker(kind) {
        if (!picker.hidden && pickerKind === kind) { picker.hidden = true; return; }
        const changed = pickerKind !== kind;
        pickerKind = kind; pickerSearch.placeholder = kind === 'stickers' ? 'Search stickers' : 'Search GIFs';
        picker.hidden = false; tray.hidden = true;
        if (changed) pickerResults.replaceChildren();
        if (changed || !pickerResults.childElementCount) {
          if (!pickerSearch.value.trim()) pickerSearch.value = 'trending';
          await searchMedia();
        }
      }

      photoBtn.onclick = () => photoInput.click();
      fileBtn.onclick = () => fileInput.click();
      gifBtn.onclick = () => openPicker('gifs');
      stickerBtn.onclick = () => openPicker('stickers');
      emojiBtn.onclick = () => { picker.hidden = true; tray.hidden = !tray.hidden; };
      tagBtn.onclick = () => { tagRow.hidden = !tagRow.hidden; if (!tagRow.hidden) tagInput.focus(); };
      photoInput.onchange = () => { if (photoInput.files && photoInput.files[0]) setMedia(photoInput.files[0]); photoInput.value = ''; };
      fileInput.onchange = () => { if (fileInput.files && fileInput.files[0]) setFile(fileInput.files[0]); fileInput.value = ''; };
      pickerSearchBtn.onclick = searchMedia;
      pickerSearch.onkeydown = event => { if (event.key === 'Enter') { event.preventDefault(); searchMedia(); } };
      text.oninput = autosize;
      text.onpaste = event => { const files = event.clipboardData && event.clipboardData.files; if (files && files.length) { event.preventDefault(); takeFiles(files); } };
      text.onkeydown = event => { if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') { event.preventDefault(); if (!submit.disabled) submit.click(); } };
      box.ondragover = event => { event.preventDefault(); box.classList.add('dragging'); };
      box.ondragleave = event => { if (!box.contains(event.relatedTarget)) box.classList.remove('dragging'); };
      box.ondrop = event => { event.preventDefault(); box.classList.remove('dragging'); if (event.dataTransfer && event.dataTransfer.files && event.dataTransfer.files.length) takeFiles(event.dataTransfer.files); };

      submit.onclick = async () => {
        if (!hasContent()) return;
        if (state.media && state.file && state.media.size + state.file.size > MAX_TOTAL) { msg.textContent = 'Photo/video and file together must stay under 50 MB.'; return; }
        const form = new FormData();
        form.append('caption', text.value);
        form.append('tags', tagRow.hidden ? '' : tagInput.value);
        if (state.media) form.append('image', state.media);
        else if (state.remote) form.append('remote_media_url', state.remote.url);
        if (state.file) form.append('attachment', state.file);
        busy = true; updatePostState(); msg.textContent = 'Posting...';
        let response;
        try { response = await fetch('/api/posts', { method: 'POST', body: form }); }
        catch (error) { busy = false; msg.textContent = 'Could not reach the server. Check your connection.'; updatePostState(); return; }
        busy = false;
        if (response.ok) { reset(); loadFeed(); loadActivity(); return; }
        if (response.status === 413) msg.textContent = 'Too large: photos and videos up to 50 MB, files up to 20 MB.';
        else if (response.status === 415) msg.textContent = 'That file type is not supported.';
        else if (response.status === 400) msg.textContent = 'Check your text (max 2000 characters), the tagged usernames (up to 10 existing users), and your attachments.';
        else if (response.status === 401) msg.textContent = 'Please log in again.';
        else if (response.status === 403) msg.textContent = 'Your account cannot post right now.';
        else if (response.status === 502) msg.textContent = 'That GIF or sticker could not be fetched. Try another one.';
        else msg.textContent = 'Something went wrong. Please try again.';
        updatePostState();
      };

      box.append(top, previews, tagRow, tray, picker, photoInput, fileInput, bar, msg);
      updatePostState();
    }

    // ---------- right side of the home page ----------
    var SPARK_PROMPTS = [
      'What is one tiny thing that made you smile today?',
      'Describe your day in exactly five words.',
      'Share a song that is stuck in your head right now.',
      'What is something you learned this week?',
      'Post a photo of the view from where you are.',
      'What is the best meal you have had recently?',
      'Tell us about a place you would love to visit.',
      'What is a small win you are proud of?',
      'Who made your week better, and how?',
      'Show us something that is on your desk right now.',
      'What would your perfect Sunday look like?',
      'Share a quote that keeps you going.',
      'What is a hobby you would like to pick up?',
      'Name a movie you could watch again and again.',
      'What is one thing you are grateful for today?',
      'Share your favorite way to unwind.',
      'If you could have dinner with anyone, who would it be?',
      'What is the kindest thing someone did for you lately?',
      'Post a throwback: where were you a year ago?',
      'What is your current favorite emoji, and why?',
      'Tell a joke that always makes you laugh.',
      'What is on your reading list or watch list?',
      'What is one goal you are working toward this month?',
      'Share something beautiful you noticed today.',
    ];

    function buildSparkPanel() {
      const panel = el('section', undefined, 'activity-panel spark-panel');
      panel.id = 'sparkPanel'; panel.setAttribute('aria-label', 'Daily spark');
      panel.append(el('h2', '\u2728 Daily spark', 'activity-heading'));
      const greet = el('p', undefined, 'spark-greet');
      const prompt = el('p', undefined, 'spark-text');
      const actions = el('div', undefined, 'spark-actions');
      const use = el('button', 'Use this prompt', 'spark-btn primary'); use.type = 'button';
      const shuffle = el('button', 'Shuffle', 'spark-btn'); shuffle.type = 'button';
      actions.append(use, shuffle);
      let index = Math.floor(Date.now() / 86400000) % SPARK_PROMPTS.length;
      const paint = () => {
        const hour = new Date().getHours();
        const part = hour < 5 ? 'Late night' : hour < 12 ? 'Good morning' : hour < 18 ? 'Good afternoon' : 'Good evening';
        greet.textContent = part + (currentUsername ? ', @' + currentUsername : '') + '!';
        prompt.textContent = SPARK_PROMPTS[index];
      };
      paint();
      document.addEventListener('spark-refresh', paint);
      use.onclick = () => {
        const field = document.getElementById('pcText');
        if (!field) { use.textContent = 'Log in to post'; setTimeout(() => { use.textContent = 'Use this prompt'; }, 2000); return; }
        field.value = SPARK_PROMPTS[index] + ' ';
        field.dispatchEvent(new Event('input'));
        field.focus();
        field.scrollIntoView({ behavior: 'smooth', block: 'center' });
      };
      shuffle.onclick = () => { index = (index + 1 + Math.floor(Math.random() * (SPARK_PROMPTS.length - 1))) % SPARK_PROMPTS.length; paint(); };
      const timer = setInterval(() => { if (!panel.isConnected) { clearInterval(timer); return; } paint(); }, 60000);
      panel.append(greet, prompt, actions);
      return panel;
    }

    function buildPulsePanel() {
      const panel = el('section', undefined, 'activity-panel pulse-panel');
      panel.id = 'pulsePanel'; panel.setAttribute('aria-label', 'Community pulse');
      panel.append(el('h2', '\u{1F4C8} Community pulse', 'activity-heading'));
      const body = el('div', undefined, 'pulse-body');
      body.append(el('p', 'Reading the room...', 'news-note'));
      panel.append(body);
      const STOP = new Set('this that with have from your about just like what when they them then than into over very been were will would could should their there here more some much also only even after before because while where which who whom whose does done make made'.split(' '));
      async function refresh() {
        try {
          const [feedResponse, activeResponse] = await Promise.all([fetch('/api/feed'), fetch('/api/users/active')]);
          const posts = feedResponse.ok ? await feedResponse.json() : [];
          const active = activeResponse.ok ? await activeResponse.json() : [];
          const counts = { image: 0, video: 0, text: 0 };
          const words = {}; const posters = {};
          let best = null;
          posts.forEach(p => {
            counts[p.media_type] = (counts[p.media_type] || 0) + 1;
            posters[p.user] = (posters[p.user] || 0) + 1;
            if (!best || p.likes > best.likes) best = p;
            String(p.caption || '').toLowerCase().split(/[^a-z0-9#_]+/).forEach(word => {
              if (word.length < 4 || STOP.has(word)) return;
              words[word] = (words[word] || 0) + (word.startsWith('#') ? 3 : 1);
            });
          });
          const topWords = Object.entries(words).sort((a, b) => b[1] - a[1]).slice(0, 6);
          const topPoster = Object.entries(posters).sort((a, b) => b[1] - a[1])[0];
          body.replaceChildren();
          const online = el('div', undefined, 'pulse-online');
          online.append(el('span', undefined, 'pulse-online-dot'), el('strong', String(active.length)), el('span', active.length === 1 ? ' person online now' : ' people online now'));
          body.append(online);
          const grid = el('div', undefined, 'pulse-grid');
          [['Posts', posts.length], ['Photos', counts.image || 0], ['Videos', counts.video || 0], ['Thoughts', counts.text || 0]].forEach(([label, value]) => {
            const cell = el('div', undefined, 'pulse-cell'); cell.append(el('strong', String(value)), el('span', label)); grid.append(cell);
          });
          body.append(grid);
          if (best && best.likes > 0) {
            const row = el('p', undefined, 'pulse-line');
            row.append('Most loved: ', profileLink(best.user, '@' + best.user), ' (' + likesText(best.likes) + ')');
            body.append(row);
          }
          if (topPoster) {
            const row = el('p', undefined, 'pulse-line');
            row.append('Top voice: ', profileLink(topPoster[0], '@' + topPoster[0]), ' (' + topPoster[1] + (topPoster[1] === 1 ? ' post' : ' posts') + ')');
            body.append(row);
          }
          if (topWords.length) {
            const chips = el('div', undefined, 'pulse-chips');
            topWords.forEach(([word]) => chips.append(el('span', word, 'pulse-chip')));
            body.append(el('p', 'Trending words', 'pulse-sub'), chips);
          }
        } catch (error) { body.replaceChildren(el('p', 'The pulse is resting. Check back soon.', 'news-note')); }
      }
      refresh();
      const timer = setInterval(() => {
        if (!panel.isConnected) { clearInterval(timer); return; }
        const home = document.getElementById('homeView');
        if (home && !home.hidden) refresh();
      }, 90000);
      return panel;
    }

    function buildBreathPanel() {
      const panel = el('section', undefined, 'activity-panel breath-panel');
      panel.id = 'breathPanel'; panel.setAttribute('aria-label', 'Take a breather');
      panel.append(el('h2', '\u{1F33F} Take a breather', 'activity-heading'));
      const orb = el('div', undefined, 'breath-orb');
      const label = el('span', 'Ready?', 'breath-label');
      orb.append(label);
      const note = el('p', 'A 10-second breathing loop. In for 4, hold for 2, out for 4.', 'news-note');
      const toggle = el('button', 'Start', 'spark-btn primary'); toggle.type = 'button';
      let timer = null; let tick = 0;
      const phases = () => { const t = tick % 10; return t < 4 ? 'Breathe in' : t < 6 ? 'Hold' : 'Breathe out'; };
      const stop = () => { clearInterval(timer); timer = null; panel.classList.remove('breathing'); label.textContent = 'Ready?'; toggle.textContent = 'Start'; };
      toggle.onclick = () => {
        if (timer) { stop(); return; }
        tick = 0; panel.classList.add('breathing'); toggle.textContent = 'Stop'; label.textContent = phases();
        timer = setInterval(() => {
          if (!panel.isConnected) { stop(); return; }
          tick += 1; label.textContent = phases();
        }, 1000);
      };
      panel.append(orb, note, toggle);
      return panel;
    }

    function homeAsideInit() {
      const aside = document.querySelector('.home-aside');
      if (!aside || aside.querySelector('#sparkPanel')) return;
      aside.append(buildSparkPanel(), buildPulsePanel(), buildBreathPanel());
    }
    setTimeout(homeAsideInit, 0);

    
'@

# =====================================================================
# 4. Page edits (each find must match exactly once)
# =====================================================================
$requiredEditsText = @'
@@FIND
a.href = '#/u/' + encodeURIComponent(username.replace(/^@/, ''));
@@REPLACE
a.href = '#/u/' + encodeURIComponent(username.replace(/^@/, '')); presenceTagLink(a, username, label);
@@END
@@FIND
user.append(avatarEl(p.avatar_url), author);
@@REPLACE
user.append(presenceAvatar(p.user, p.avatar_url), author);
@@END
@@FIND
const followButton = el('button', p.followed_by_me ? 'Unfollow' : 'Follow', 'post-follow-button');
@@REPLACE
const followButton = el('button', p.followed_by_me ? 'Following' : 'Follow', 'post-follow-button' + (p.followed_by_me ? ' is-following' : ''));
@@END
@@FIND
followButton.textContent = p.followed_by_me ? 'Unfollow' : 'Follow';
@@REPLACE
followButton.textContent = p.followed_by_me ? 'Following' : 'Follow';
@@END
@@FIND
followStatus.textContent = p.followed_by_me ? 'Following @' + p.user : 'Unfollowed @' + p.user;
@@REPLACE
followButton.classList.toggle('is-following', p.followed_by_me);
@@END
@@FIND
followButton.textContent = state ? 'Unfollow' : 'Follow';
@@REPLACE
followButton.textContent = state ? 'Following' : 'Follow'; followButton.classList.toggle('is-following', state);
@@END
@@FIND
if (p.media_type === 'video') {
@@REPLACE
if (p.media_type === 'text') {
          media = pcTextStage(p);
        } else if (p.media_type === 'video') {
@@END
@@FIND
meta.append(userLink(p.user), document.createTextNode(' ' + p.caption));
@@REPLACE
meta.append(userLink(p.user), document.createTextNode(p.media_type === 'text' ? '' : ' ' + p.caption));
        if (p.attachment_name) meta.append(pcAttachmentChip(p));
@@END
@@FIND
if (item.media_type === 'video') {
@@REPLACE
if (item.media_type === 'text') {
          /* text-only post: the text below is the content */
        } else if (item.media_type === 'video') {
@@END
@@FIND
head.append(avatarWithMood(pr.avatar_url, pr.mood, true));
@@REPLACE
const profileAvatar = avatarWithMood(pr.avatar_url, pr.mood, true);
      profileAvatar.append(presenceDot(pr.username));
      head.append(profileAvatar);
@@END
@@FIND
const avatar = el('div', conversation.is_group ? 'G' : (conversation.members.find(name => name !== currentUsername) || '?')[0].toUpperCase(), 'activity-icon');
@@REPLACE
const otherNames = conversation.members.filter(name => name !== currentUsername);
          const avatar = conversation.is_group ? groupAvatarStack(otherNames) : presenceAvatar(otherNames[0] || '?', null, 'chat');
@@END
@@FIND
bubble.append(el('span', message.sender, 'message-sender'));
@@REPLACE
const senderRow = el('div', undefined, 'message-sender-row');
            senderRow.append(presenceAvatar(message.sender, null, 'xs'), el('span', message.sender, 'message-sender'));
            bubble.append(senderRow);
@@END
'@

# Steps that belong to earlier patches: applied when present, skipped otherwise
$optionalEditsText = @'
@@FIND
ring.append(avatarEl(person.avatar_url));
@@REPLACE
ring.append(presenceAvatar(person.username, person.avatar_url));
@@END
@@FIND
head.append(avatarEl(topic.avatar_url), author);
@@REPLACE
head.append(presenceAvatar(topic.username, topic.avatar_url), author);
@@END
@@FIND
row.append(avatarEl(person.avatar_url), copy, chatButton, groupButton);
@@REPLACE
row.append(presenceAvatar(person.username, person.avatar_url), copy, chatButton, groupButton);
@@END
'@

# =====================================================================
# 5. CSS
# =====================================================================
$newCss = @'

    /* --- Online presence --- */
    .pav { position: relative; display: inline-flex; flex: 0 0 auto; }
    .pav .avatar.pav-initial { display: grid; place-items: center; background: #f0ebff; color: #7655e8; font-weight: 750; font-size: 13px; }
    .pav.chat .avatar { width: 40px; height: 40px; }
    .pav.xs .avatar { width: 22px; height: 22px; font-size: 10px; }
    .pdot { position: absolute; right: -1px; bottom: -1px; width: 11px; height: 11px; box-sizing: content-box; border: 2px solid #fff; border-radius: 50%; background: #22c55e; opacity: 0; transform: scale(.4); transition: opacity .2s, transform .2s; pointer-events: none; }
    .pdot.on { opacity: 1; transform: scale(1); animation: pdot-glow 2.4s ease-out infinite; }
    .pav.xs .pdot { width: 7px; height: 7px; border-width: 1.5px; }
    .pav-stack { display: inline-flex; flex: 0 0 auto; }
    .pav-stack .pav + .pav { margin-left: -14px; }
    .pav-stack .pav .avatar { border: 2px solid #fff; box-sizing: border-box; }
    .pav-stack .pdot { display: none; }
    .avatar-with-mood > .pdot { top: 8px; right: 8px; bottom: auto; width: 16px; height: 16px; }
    .fc-circle > .pav { display: block !important; position: relative; width: 100% !important; height: 100% !important; border: 0 !important; border-radius: 50% !important; }
    .fc-circle .pav > .avatar { width: 100% !important; height: 100% !important; border-radius: 50% !important; border: 3px solid rgba(255,255,255,.92); box-sizing: border-box; object-fit: cover; }
    .fc-circle .pdot { right: 6px; bottom: 6px; width: 15px; height: 15px; }
    a.ulink[data-online="1"]::after { content: ""; display: inline-block; width: 7px; height: 7px; margin-left: 5px; border-radius: 50%; background: #22c55e; vertical-align: middle; animation: pdot-glow 2.4s ease-out infinite; }
    .post-author a.ulink::after, .fc-person a.ulink::after, .community-post-author a.ulink::after, .cw-person a.ulink::after { display: none; }
    .fc-circle .pdot.on::after, .avatar-with-mood > .pdot.on::after { content: "\2726"; position: absolute; top: -10px; right: -8px; color: #fbbf24; font-size: 11px; line-height: 1; animation: pdot-twinkle 2s ease-in-out infinite; }
    @keyframes pdot-glow { 0% { box-shadow: 0 0 0 0 rgba(34,197,94,.55); } 70% { box-shadow: 0 0 0 7px rgba(34,197,94,0); } 100% { box-shadow: 0 0 0 0 rgba(34,197,94,0); } }
    @keyframes pdot-twinkle { 0%, 100% { opacity: .35; transform: scale(.8) rotate(0deg); } 50% { opacity: 1; transform: scale(1.2) rotate(25deg); } }
    .conversation-item .pav .avatar, .conversation-item .pav-stack .avatar { width: 40px; height: 40px; }
    .message-sender-row { display: flex; align-items: center; gap: 6px; margin-bottom: 3px; }
    .card .meta a.ulink::after { display: none; }
    .cw [hidden] { display: none !important; }

    /* --- Chat window clears the feedback button --- */
    .cw { bottom: 74px; height: min(600px, calc(100vh - 96px)); z-index: 9100; }
    @media (max-width: 640px) { .cw { bottom: 0; height: 100vh; } }

    /* --- Follow button --- */
    .post-follow-button { transition: background .18s, color .18s, border-color .18s; }
    .post-follow-button.is-following { color: #1f7a4d; background: #e8f7ee; border-color: #8fd0ad; }
    .post-follow-button.is-following:hover { color: #a33; background: #fdeeee; border-color: #e3a3a3; }

    /* --- Post box --- */
    #composer.pc { gap: 12px; position: relative; }
    #composer.pc.dragging { outline: 2px dashed var(--violet); outline-offset: -6px; background: #f8f5ff; }
    #composer .pc-top { display: flex; align-items: flex-start; gap: 12px; }
    #composer .pc-top .pav { margin-top: 3px; }
    #composer .pc-text { flex: 1 1 auto; min-width: 0; min-height: 52px; max-height: 260px; padding: 11px 13px; border: 1px solid var(--line); border-radius: 14px; font: inherit; font-size: 14px; line-height: 1.5; resize: none; background: #fcfbfe; }
    #composer .pc-text:focus { outline: 2px solid #cdbff7; background: #fff; }
    #composer .pc-previews { display: flex; flex-wrap: wrap; gap: 10px; }
    #composer .pc-preview { position: relative; width: 150px; height: 110px; overflow: hidden; border-radius: 12px; border: 1px solid var(--line); background: #f0edf5; }
    #composer .pc-preview img, #composer .pc-preview video { width: 100%; height: 100%; object-fit: cover; display: block; }
    #composer .pc-badge { position: absolute; left: 6px; bottom: 6px; padding: 2px 7px; border-radius: 999px; background: #201c2ed9; color: #fff; font-size: 10px; }
    #composer .pc-file { display: inline-flex; align-items: center; gap: 8px; max-width: 100%; padding: 8px 10px; border: 1px solid var(--line); border-radius: 12px; background: #fcfbfe; font-size: 12px; }
    #composer .pc-file-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 200px; font-weight: 600; }
    #composer .pc-file-size { color: var(--muted); }
    #composer .pc-remove { position: static; align-self: center; padding: 0; width: 22px; height: 22px; border-radius: 50%; border: 0; background: #201c2ecc; color: #fff; font-size: 14px; line-height: 1; cursor: pointer; }
    #composer .pc-preview .pc-remove { position: absolute; top: 6px; right: 6px; }
    #composer .pc-tagrow input { width: 100%; box-sizing: border-box; padding: 9px 12px; border: 1px solid var(--line); border-radius: 10px; font: inherit; font-size: 12px; }
    #composer .pc-tray { display: flex; flex-wrap: wrap; gap: 4px; padding: 8px; border: 1px solid var(--line); border-radius: 12px; background: #fcfbfe; }
    #composer .pc-tray[hidden], #composer .pc-picker[hidden], #composer .pc-previews[hidden], #composer .pc-tagrow[hidden] { display: none; }
    #composer .pc-emoji { align-self: auto; padding: 4px 6px; border: 0; border-radius: 8px; background: transparent; color: inherit; font-size: 20px; line-height: 1; cursor: pointer; }
    #composer .pc-emoji:hover { background: #efe9fb; }
    #composer .pc-picker { display: grid; gap: 8px; padding: 10px; border: 1px solid var(--line); border-radius: 12px; background: #fcfbfe; }
    #composer .pc-picker-head { display: flex; gap: 6px; }
    #composer .pc-picker-head input { flex: 1 1 auto; min-width: 0; padding: 8px 11px; border: 1px solid var(--line); border-radius: 999px; font: inherit; font-size: 12px; }
    #composer .pc-picker-status { margin: 0; color: var(--muted); font-size: 11px; }
    #composer .pc-picker-results { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 6px; max-height: 240px; overflow: auto; }
    #composer .pc-tile button { display: block; width: 100%; padding: 0; border: 0; border-radius: 8px; overflow: hidden; background: #ede9f6; cursor: pointer; align-self: auto; }
    #composer .pc-tile img { width: 100%; height: 84px; object-fit: cover; display: block; }
    #composer .pc-bar { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap; padding-top: 10px; border-top: 1px solid var(--line); }
    #composer .pc-tools { display: flex; flex-wrap: wrap; gap: 6px; }
    #composer .pc-tool { align-self: auto; padding: 6px 12px; border: 1px solid var(--line); border-radius: 999px; background: #fff; color: #4e4a5b; font-size: 12px; cursor: pointer; transition: background .15s, border-color .15s; }
    #composer .pc-tool:hover { background: #f4f1fb; border-color: #cdbff7; }
    #composer .pc-mini { align-self: auto; padding: 7px 14px; font-size: 12px; border-radius: 999px; }
    #composer .pc-right { display: flex; align-items: center; gap: 12px; }
    #composer .pc-counter { color: var(--muted); font-size: 11px; }
    #composer .pc-submit { align-self: auto; padding: 8px 24px; border-radius: 999px; font-weight: 700; }
    #composer .pc-submit:disabled { opacity: .45; cursor: default; }
    #composer .pc-msg { min-height: 1em; margin: 0; color: #a33; font-size: 12px; }

    /* --- Text posts and attachments in the feed --- */
    .card .photo.text-post { display: flex; align-items: center; justify-content: center; min-height: 240px; padding: 34px 76px 34px 30px; box-sizing: border-box; background: linear-gradient(135deg, var(--tp-a, #7655e8), var(--tp-b, #e879a8)); color: #fff; text-align: center; }
    .text-post-body { margin: 0; font-family: var(--serif); font-size: 26px; font-weight: 500; line-height: 1.35; white-space: pre-wrap; overflow-wrap: anywhere; text-shadow: 0 1px 8px rgba(0,0,0,.18); }
    .text-post-body.long { font-size: 18px; text-align: left; }
    .post-attachment { display: inline-flex; align-items: center; gap: 8px; max-width: 100%; margin-top: 8px; padding: 8px 12px; border: 1px solid var(--line); border-radius: 12px; background: #fcfbfe; color: var(--ink); text-decoration: none; font-size: 12px; }
    .post-attachment:hover { background: #f4f1fb; border-color: #cdbff7; }
    .post-attachment-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 240px; font-weight: 600; }
    .post-attachment-size { color: var(--muted); }

    /* --- Right-side panels --- */
    .spark-panel .spark-greet { margin: 0 0 4px; color: var(--muted); font-size: 11px; }
    .spark-panel .spark-text { margin: 0 0 12px; font-family: var(--serif); font-size: 18px; line-height: 1.35; }
    .spark-actions { display: flex; gap: 8px; flex-wrap: wrap; }
    .spark-btn { padding: 7px 14px; border: 1px solid var(--line); border-radius: 999px; background: #fff; color: var(--ink); font-size: 12px; cursor: pointer; }
    .spark-btn.primary { color: #fff; background: var(--violet); border-color: var(--violet); }
    .pulse-online { display: flex; align-items: center; gap: 6px; margin-bottom: 10px; font-size: 12px; }
    .pulse-online strong { font-size: 18px; }
    .pulse-online-dot { width: 9px; height: 9px; border-radius: 50%; background: #22c55e; animation: pdot-glow 2.4s ease-out infinite; }
    .pulse-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 6px; margin-bottom: 10px; }
    .pulse-cell { display: grid; justify-items: center; padding: 8px 2px; border-radius: 12px; background: #f6f2ff; }
    .pulse-cell strong { font-size: 16px; color: #5a3fc0; }
    .pulse-cell span { color: var(--muted); font-size: 9px; }
    .pulse-line { margin: 4px 0; font-size: 11px; color: #4e4a5b; }
    .pulse-sub { margin: 10px 0 5px; color: var(--muted); font-size: 10px; letter-spacing: .06em; text-transform: uppercase; }
    .pulse-chips { display: flex; flex-wrap: wrap; gap: 5px; }
    .pulse-chip { padding: 3px 9px; border-radius: 999px; background: linear-gradient(135deg, #efe7ff, #dff3ea); color: #453679; font-size: 11px; }
    .breath-panel { text-align: center; }
    .breath-orb { display: grid; place-items: center; width: 110px; height: 110px; margin: 6px auto 10px; border-radius: 50%; background: radial-gradient(circle at 35% 30%, #d9f2e6, #a6d8c3 55%, #7fb9a6); box-shadow: 0 8px 22px #347e7240; transition: transform 1s ease-in-out; }
    .breath-label { color: #1f5e50; font-size: 12px; font-weight: 700; }
    .breath-panel.breathing .breath-orb { animation: breath-loop 10s ease-in-out infinite; }
    @keyframes breath-loop { 0% { transform: scale(.7); } 40% { transform: scale(1.15); } 60% { transform: scale(1.15); } 100% { transform: scale(.7); } }
'@

# =====================================================================
# Apply everything in memory first
# =====================================================================
$html = [System.IO.File]::ReadAllText($indexHtml, $utf8)
$rs = [System.IO.File]::ReadAllText($mainRs, $utf8)

if ($html.Contains('function presenceAvatar') -or $rs.Contains('async fn presence_lookup')) {
  Write-Host 'These changes appear to be applied already. Nothing to do.'
  return
}

# Name-conflict guard
foreach ($name in @('presenceKey', 'schedulePresence', 'presenceDot', 'presenceAvatar', 'presenceTagLink', 'presenceApply', 'presenceRefresh', 'swapPresenceAvatar', 'groupAvatarStack', 'pcFormatBytes', 'pcHue', 'pcTextStage', 'pcAttachmentChip', 'buildSparkPanel', 'buildPulsePanel', 'buildBreathPanel', 'homeAsideInit', 'SPARK_PROMPTS', 'presenceCache')) {
  if ([regex]::IsMatch($html, '\b(function|var|let|const)\s+' + $name + '\b')) { throw "[name conflict] '$name' already exists in index.html. Nothing was changed." }
}
foreach ($name in @('presence_lookup', 'read_post_attachment', 'clean_attachment_name', 'attachment_extension', 'fetch_remote_post_media', 'TEXT_POST_URL', 'MAX_POST_TEXT_CHARS', 'MAX_ATTACHMENT_BYTES', 'MAX_REMOTE_MEDIA_BYTES')) {
  if ([regex]::IsMatch($rs, '\b(fn|const)\s+' + $name + '\b')) { throw "[name conflict] '$name' already exists in main.rs. Nothing was changed." }
}

# ---- index.html ----
$required = Get-Edits $requiredEditsText 12 'required edits'
foreach ($pair in $required) { $html = Edit-Once $html $pair[0] $pair[1] ('page: ' + $pair[0]) }
$optional = Get-Edits $optionalEditsText 3 'optional edits'
foreach ($pair in $optional) { $html = Edit-IfPresent $html $pair[0] $pair[1] ('page: ' + $pair[0]) }

$cStart = 'function renderComposer() {'
$cEnd = 'async function renderAuth() {'
$csCount = [regex]::Matches($html, [regex]::Escape($cStart)).Count
$ceCount = [regex]::Matches($html, [regex]::Escape($cEnd)).Count
if ($csCount -ne 1 -or $ceCount -ne 1) { throw "[composer] markers found $csCount/$ceCount times, expected 1/1. Nothing was changed." }
$s = $html.IndexOf($cStart, [System.StringComparison]::Ordinal)
$e = $html.IndexOf($cEnd, [System.StringComparison]::Ordinal)
if ($e -le $s) { throw '[composer] end marker comes before start marker. Nothing was changed.' }
$html = $html.Substring(0, $s) + $richBlock + $html.Substring($e)

if ([regex]::Matches($html, '</style>').Count -lt 1) { throw '[CSS] no </style> tag found. Nothing was changed.' }
$at = $html.IndexOf('</style>', [System.StringComparison]::Ordinal)
$html = $html.Substring(0, $at) + $newCss + "`n  " + $html.Substring($at)

# ---- main.rs ----
# Post struct gets the attachment fields
$structPattern = '(struct Post \{[^}]*?tags: Vec<String>,)(\s*\})'
if ([regex]::Matches($rs, $structPattern).Count -ne 1) { throw '[Post struct] expected exactly 1 match. Nothing was changed.' }
$rs = [regex]::Replace($rs, $structPattern, { param($m) $m.Groups[1].Value + $rustPostFields.TrimEnd() + $m.Groups[2].Value })

# feed query returns them
$feedStart = $rs.IndexOf('async fn feed(', [System.StringComparison]::Ordinal)
if ($feedStart -lt 0 -or [regex]::Matches($rs, [regex]::Escape('async fn feed(')).Count -ne 1) { throw '[feed] could not find async fn feed( exactly once. Nothing was changed.' }
$feedEndMatch = [regex]::Match($rs.Substring($feedStart), '(?m)^\}')
if (-not $feedEndMatch.Success) { throw '[feed] could not find the end of the function. Nothing was changed.' }
$feedEnd = $feedStart + $feedEndMatch.Index + 1
$feedFn = $rs.Substring($feedStart, $feedEnd - $feedStart)
$feedFn = Edit-Once $feedFn 'p.caption,' "p.caption,`n               p.attachment_name,`n               p.attachment_size," 'feed select list'
$rs = $rs.Substring(0, $feedStart) + $feedFn + $rs.Substring($feedEnd)

# create_post replaced
$cpStart = $rs.IndexOf('async fn create_post(', [System.StringComparison]::Ordinal)
if ($cpStart -lt 0 -or [regex]::Matches($rs, [regex]::Escape('async fn create_post(')).Count -ne 1) { throw '[create_post] could not find async fn create_post( exactly once. Nothing was changed.' }
$cpEndMatch = [regex]::Match($rs.Substring($cpStart), '(?m)^\}')
if (-not $cpEndMatch.Success) { throw '[create_post] could not find the end of the function. Nothing was changed.' }
$cpEnd = $cpStart + $cpEndMatch.Index + 1
$rs = $rs.Substring(0, $cpStart) + $rustCreatePost + "`n`n" + $rustHelpers.TrimEnd() + $rs.Substring($cpEnd)

# routes for the two new endpoints
$rs = Edit-Once $rs '.route("/api/users/active", get(active_users))' ('.route("/api/users/active", get(active_users))' + "`n        " + '.route("/api/presence", get(presence_lookup))' + "`n        " + '.route("/api/posts/{id}/attachment", get(read_post_attachment))') 'routes'

# =====================================================================
# Write (only reached if every check passed)
# =====================================================================
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$backupDir = Join-Path $root ('backups\final-' + $stamp)
New-Item -ItemType Directory -Force -Path $backupDir | Out-Null
Copy-Item $indexHtml (Join-Path $backupDir 'index.html')
Copy-Item $mainRs (Join-Path $backupDir 'main.rs')
if (-not (Test-Path $migPath)) { [System.IO.File]::WriteAllText($migPath, $migration, $utf8); Write-Host "Created $migPath" } else { Write-Host 'Migration file already exists, left as is.' }
[System.IO.File]::WriteAllText($mainRs, $rs, $utf8); Write-Host 'Patched src\main.rs'
[System.IO.File]::WriteAllText($indexHtml, $html, $utf8); Write-Host 'Patched static\index.html'
Write-Host "Backups saved in $backupDir"
Write-Host ''
Write-Host 'Next steps:'
Write-Host '  1. sqlx migrate run'
Write-Host '  2. cargo run   (if it prints a compile error, paste it to me)'
Write-Host '  3. Open the app and hard refresh (Ctrl+F5)'
