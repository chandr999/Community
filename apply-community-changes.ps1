# apply-community-changes.ps1
# Run from D:\Community:
#   powershell -ExecutionPolicy Bypass -File .\apply-community-changes.ps1
# It backs up src\main.rs and static\index.html first, checks every find-and-replace
# matches exactly once, and writes nothing if any check fails.

$ErrorActionPreference = 'Stop'
$root = (Get-Location).Path
$mainRs = Join-Path $root 'src\main.rs'
$indexHtml = Join-Path $root 'static\index.html'
$migDir = Join-Path $root 'migrations'
foreach ($p in @($mainRs, $indexHtml, $migDir)) {
  if (-not (Test-Path $p)) { throw "Run this from D:\Community. Missing: $p" }
}
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Edit-Once([string]$text, [string]$find, [string]$replace, [string]$label) {
  $count = [regex]::Matches($text, [regex]::Escape($find)).Count
  if ($count -ne 1) { throw "[$label] expected exactly 1 match, found $count. Nothing was changed." }
  return $text.Replace($find, $replace)
}

# ---------- 1. Migration: columns and table the Rust code already expects ----------
$migration = @'
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
'@
$migPath = Join-Path $migDir '20261027100000_community_pages.sql'

# ---------- 2. Rust: members list returns avatar and active flag ----------
$rs = [System.IO.File]::ReadAllText($mainRs, $utf8)
$rsDone = $rs.Contains('AS active FROM community_members cm')
if (-not $rsDone) {
  $structPattern = 'struct CommunityMember\s*\{\s*id: i32,\s*username: String,\s*is_owner: bool,\s*is_admin: bool,\s*enabled: bool,\s*\}'
  $structCount = [regex]::Matches($rs, $structPattern).Count
  if ($structCount -ne 1) { throw "[CommunityMember struct] expected exactly 1 match, found $structCount. Nothing was changed." }
  $structNew = @'
struct CommunityMember {
    id: i32,
    username: String,
    is_owner: bool,
    is_admin: bool,
    enabled: bool,
    avatar_url: Option<String>,
    active: bool,
}
'@
  $rs = [regex]::Replace($rs, $structPattern, $structNew.TrimEnd())
  $rs = Edit-Once $rs 'cm.is_enabled AS enabled FROM community_members cm' "cm.is_enabled AS enabled, u.avatar_url, COALESCE(u.account_enabled AND u.show_activity_status AND u.last_active_at > now() - interval '5 minutes', FALSE) AS active FROM community_members cm" 'members SQL'
}

# ---------- 3. Page: JavaScript ----------
$newJs = @'
async function loadCommunityMembers(id, group, panel) {
      panel.replaceChildren(el('p', 'Loading members...', 'community-note'));
      const response = await fetch(`/api/communities/${id}/members`);
      if (!response.ok) {
        const message = response.status === 403
          ? 'Join this community to view its members.'
          : response.status === 404
            ? 'This community could not be found.'
            : 'The member list is temporarily unavailable. Please try again.';
        panel.replaceChildren(el('p', message, 'empty'));
        return;
      }
      const members = await response.json();
      const activeCount = members.filter(member => member.active).length;
      const section = el('section', undefined, 'community-member-list cm-members');
      section.append(el('h2', `Members \u00b7 ${members.length}` + (activeCount ? ` \u00b7 ${activeCount} active` : '')));
      if (group.is_owner || group.is_admin) {
        const addForm = el('form', undefined, 'cm-add-form');
        const username = el('input'); username.placeholder = 'Add someone by username'; username.maxLength = 30; username.required = true; username.autocomplete = 'off';
        const add = el('button', '+ Add member', 'cm-btn primary'); add.type = 'submit';
        const status = el('span', '', 'community-note');
        addForm.append(username, add, status);
        addForm.onsubmit = async event => {
          event.preventDefault(); add.disabled = true; status.textContent = '';
          const result = await post(`/api/communities/${id}/members`, { username: username.value.trim() });
          if (result.ok) loadCommunityMembers(id, group, panel);
          else { status.textContent = result.status === 404 ? 'No enabled account has that username.' : 'Could not add that member.'; add.disabled = false; }
        };
        section.append(addForm);
      }
      members.forEach(member => {
        const row = el('div', undefined, 'community-member-row cm-member-row');
        const who = el('div', undefined, 'cm-member-who');
        who.append(cmAvatar(member.avatar_url, member.active && member.enabled), profileLink(member.username, '@' + member.username));
        if (member.is_owner) who.append(el('span', 'Owner', 'cm-badge'));
        else if (member.is_admin) who.append(el('span', 'Admin', 'cm-badge'));
        if (!member.enabled) who.append(el('span', 'Access paused', 'cm-badge paused'));
        row.append(who);
        if (group.is_owner && !member.is_owner) {
          const adminButton = el('button', member.is_admin ? 'Remove admin' : 'Make admin', 'cm-btn'); adminButton.type = 'button';
          adminButton.onclick = async () => {
            const result = await fetch(`/api/communities/${id}/members/${member.id}/admin`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ is_admin: !member.is_admin }) });
            if (result.ok) loadCommunityMembers(id, group, panel); else alert('Could not update the community admin role.');
          };
          row.append(adminButton);
        }
        if ((group.is_owner || group.is_admin) && !member.is_owner && !member.is_admin) {
          const statusButton = el('button', member.enabled ? 'Pause access' : 'Enable access', 'cm-btn'); statusButton.type = 'button';
          statusButton.onclick = async () => {
            const result = await fetch(`/api/communities/${id}/members/${member.id}/status`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ enabled: !member.enabled }) });
            if (result.ok) loadCommunityMembers(id, group, panel); else alert('Could not update member access.');
          };
          const remove = el('button', 'Remove', 'cm-btn danger'); remove.type = 'button';
          remove.onclick = async () => {
            if (!window.confirm(`Remove @${member.username} from this community?`)) return;
            const result = await fetch(`/api/communities/${id}/members/${member.id}`, { method: 'DELETE' });
            if (result.ok) loadCommunityMembers(id, group, panel); else alert('Could not remove that member.');
          };
          row.append(statusButton, remove);
        }
        section.append(row);
      });
      panel.replaceChildren(section);
    }

    function cmAvatar(url, active) {
      const holder = el('span', undefined, 'cm-avatar');
      holder.append(avatarEl(url || null));
      if (active) { const dot = el('span', undefined, 'cm-dot'); dot.title = 'Active now'; holder.append(dot); }
      return holder;
    }

    function loadActiveCommunityMembers(id, anchor) {
      const strip = el('section', undefined, 'cm-active-strip');
      const label = el('span', 'Active now', 'cm-active-label');
      const list = el('div', undefined, 'cm-active-list');
      strip.append(label, list);
      anchor.after(strip);
      const refresh = async () => {
        try {
          const response = await fetch(`/api/communities/${id}/active`);
          if (!response.ok) return;
          const rows = await response.json();
          list.replaceChildren();
          label.textContent = rows.length ? `Active now \u00b7 ${rows.length}` : 'Active now';
          if (!rows.length) { list.append(el('span', 'No one is active right now.', 'community-note')); return; }
          rows.forEach(person => {
            const chip = el('a', undefined, 'cm-active-chip');
            chip.href = '#/u/' + encodeURIComponent(person.username); chip.title = '@' + person.username;
            chip.append(cmAvatar(person.avatar_url, true), el('span', '@' + person.username));
            list.append(chip);
          });
        } catch (error) { console.error('Could not load active members:', error); }
      };
      refresh();
      const timer = setInterval(() => { if (!strip.isConnected) { clearInterval(timer); return; } refresh(); }, 60000);
    }

    function cmSafeHtml(html) {
      const allowed = new Set(['B', 'STRONG', 'I', 'EM', 'U', 'S', 'BR', 'P', 'DIV', 'UL', 'OL', 'LI', 'A', 'BLOCKQUOTE']);
      const drop = new Set(['SCRIPT', 'STYLE', 'IFRAME', 'OBJECT', 'EMBED', 'TEMPLATE']);
      const doc = new DOMParser().parseFromString('<body>' + String(html || '') + '</body>', 'text/html');
      const clean = node => {
        Array.from(node.childNodes).forEach(child => {
          if (child.nodeType === 3) return;
          if (child.nodeType !== 1) { child.remove(); return; }
          if (drop.has(child.tagName)) { child.remove(); return; }
          clean(child);
          if (!allowed.has(child.tagName)) { child.replaceWith(...Array.from(child.childNodes)); return; }
          Array.from(child.attributes).forEach(attr => { if (!(child.tagName === 'A' && attr.name === 'href')) child.removeAttribute(attr.name); });
          if (child.tagName === 'A') {
            const href = child.getAttribute('href') || '';
            if (/^https?:\/\//i.test(href)) { child.setAttribute('target', '_blank'); child.setAttribute('rel', 'noopener noreferrer nofollow'); }
            else child.removeAttribute('href');
          }
        });
      };
      clean(doc.body);
      return doc.body.innerHTML;
    }

    function cmPlainToHtml(text) {
      const box = document.createElement('div');
      box.textContent = String(text || '');
      return box.innerHTML.replace(/\n/g, '<br>');
    }

    function cmRichEditor(placeholder, initialHtml) {
      const wrap = el('div', undefined, 'cm-rich');
      const bar = el('div', undefined, 'cm-rich-bar');
      const area = el('div', undefined, 'cm-rich-area');
      area.contentEditable = 'true';
      area.setAttribute('role', 'textbox'); area.setAttribute('aria-multiline', 'true');
      area.setAttribute('data-placeholder', placeholder);
      if (initialHtml) area.innerHTML = cmSafeHtml(initialHtml);
      [['B', 'bold', 'Bold'], ['I', 'italic', 'Italic'], ['U', 'underline', 'Underline'], ['List', 'insertUnorderedList', 'Bulleted list'], ['1.', 'insertOrderedList', 'Numbered list']].forEach(([text, command, title]) => {
        const button = el('button', text, 'cm-rich-btn'); button.type = 'button'; button.title = title; button.setAttribute('aria-label', title);
        button.onmousedown = event => event.preventDefault();
        button.onclick = () => { area.focus(); document.execCommand(command); };
        bar.append(button);
      });
      const linkButton = el('button', 'Link', 'cm-rich-btn'); linkButton.type = 'button'; linkButton.title = 'Add a link';
      linkButton.onmousedown = event => event.preventDefault();
      linkButton.onclick = () => {
        const selection = window.getSelection();
        const saved = selection.rangeCount ? selection.getRangeAt(0).cloneRange() : null;
        const url = window.prompt('Link address (starts with https://)');
        if (!url) return;
        if (!/^https?:\/\//i.test(url.trim())) { alert('Links must start with http:// or https://'); return; }
        area.focus();
        if (saved) { selection.removeAllRanges(); selection.addRange(saved); }
        document.execCommand('createLink', false, url.trim());
      };
      bar.append(linkButton);
      wrap.append(bar, area);
      return { wrap, area, html: () => cmSafeHtml(area.innerHTML).trim(), blank: () => !area.textContent.trim() };
    }

    function cmPostCard(id, group, panel, topic) {
      const reload = () => loadCommunityDiscussion(id, group, panel);
      const card = el('article', undefined, 'community-post cm-post-card');
      const head = el('header', undefined, 'community-post-head');
      const author = el('div', undefined, 'community-post-author');
      author.append(profileLink(topic.username, '@' + topic.username), el('span', new Date(topic.created_at).toLocaleString() + (topic.edited ? ' (edited)' : ''), 'community-meta'));
      head.append(avatarEl(topic.avatar_url), author);
      const content = el('div', undefined, 'community-post-content');
      const bodyBox = el('div', undefined, 'cm-post-body' + (topic.is_html ? '' : ' cm-plain'));
      if (topic.is_html) bodyBox.innerHTML = cmSafeHtml(topic.body); else bodyBox.textContent = topic.body;
      content.append(el('h3', topic.title), bodyBox);

      const actions = el('div', undefined, 'cm-post-actions');
      const sidePanel = el('div', undefined, 'cm-post-panel'); sidePanel.hidden = true;
      const replies = el('div', undefined, 'community-comments'); replies.hidden = true;
      const closePanel = () => { sidePanel.hidden = true; sidePanel.replaceChildren(); };

      const repliesButton = el('button', `${topic.comment_count} ${Number(topic.comment_count) === 1 ? 'reply' : 'replies'}`, 'cm-btn'); repliesButton.type = 'button';
      repliesButton.onclick = async () => {
        replies.hidden = !replies.hidden;
        if (replies.hidden || replies.dataset.loaded) return;
        const result = await fetch(`/api/communities/${id}/posts/${topic.id}/comments`);
        if (!result.ok) { replies.append(el('p', 'Replies could not be loaded.', 'community-note')); return; }
        const rows = await result.json();
        if (!rows.length) replies.append(el('p', 'No replies yet.', 'community-note'));
        rows.forEach(comment => {
          const row = el('div', undefined, 'community-comment');
          row.append(el('span', '@' + comment.username + ': ' + comment.body));
          if (comment.can_delete) {
            const del = el('button', 'Delete reply', 'cm-btn danger'); del.type = 'button';
            del.onclick = async () => {
              if (!window.confirm('Delete this reply?')) return;
              const outcome = await fetch(`/api/communities/${id}/posts/${topic.id}/comments/${comment.id}`, { method: 'DELETE' });
              if (outcome.ok) reload();
            };
            row.append(del);
          }
          replies.append(row);
        });
        replies.dataset.loaded = '1';
      };
      actions.append(repliesButton);

      const share = el('button', 'Share', 'cm-btn'); share.type = 'button';
      share.onclick = async () => {
        const link = location.origin + location.pathname + '#/community/' + id;
        const text = topic.title + ' - ' + group.name;
        try {
          if (navigator.share) { await navigator.share({ title: topic.title, text, url: link }); return; }
          await navigator.clipboard.writeText(text + '\n' + link);
          share.textContent = 'Link copied';
          setTimeout(() => { share.textContent = 'Share'; }, 1800);
        } catch (error) { if (!error || error.name !== 'AbortError') window.prompt('Copy this link', link); }
      };
      actions.append(share);

      if (topic.can_edit) {
        const edit = el('button', 'Edit', 'cm-btn'); edit.type = 'button';
        edit.onclick = () => {
          sidePanel.hidden = false; sidePanel.replaceChildren();
          const titleInput = el('input'); titleInput.value = topic.title; titleInput.maxLength = 120;
          const editor = cmRichEditor('Edit your message', topic.is_html ? topic.body : cmPlainToHtml(topic.body));
          const note = el('span', '', 'community-note');
          const save = el('button', 'Save changes', 'cm-btn primary'); save.type = 'button';
          const cancel = el('button', 'Cancel', 'cm-btn'); cancel.type = 'button';
          cancel.onclick = closePanel;
          save.onclick = async () => {
            if (!titleInput.value.trim() || editor.blank()) { note.textContent = 'Add a title and a message.'; return; }
            const body = editor.html();
            if (body.length > 20000) { note.textContent = 'That message is too long.'; return; }
            save.disabled = true;
            const result = await fetch(`/api/communities/${id}/posts/${topic.id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ title: titleInput.value, body }) });
            if (result.ok) reload(); else { note.textContent = 'Could not save your changes.'; save.disabled = false; }
          };
          const buttons = el('div', undefined, 'cm-row'); buttons.append(save, cancel, note);
          sidePanel.append(titleInput, editor.wrap, buttons);
        };
        actions.append(edit);
      }

      if (topic.can_delete) {
        const del = el('button', 'Delete', 'cm-btn danger'); del.type = 'button';
        del.onclick = async () => {
          if (!window.confirm('Delete this discussion and its replies?')) return;
          const result = await fetch(`/api/communities/${id}/posts/${topic.id}`, { method: 'DELETE' });
          if (result.ok) reload(); else alert('Could not delete this discussion.');
        };
        actions.append(del);
      }

      if (!topic.can_edit) {
        const report = el('button', 'Report', 'cm-btn'); report.type = 'button';
        report.onclick = () => {
          sidePanel.hidden = false; sidePanel.replaceChildren();
          const reason = el('select');
          [['spam', 'Spam'], ['harassment', 'Harassment'], ['hate', 'Hate speech'], ['nudity', 'Nudity'], ['violence', 'Violence'], ['scam', 'Scam or fraud'], ['other', 'Something else']].forEach(([value, text]) => { const option = el('option', text); option.value = value; reason.append(option); });
          const details = el('textarea'); details.placeholder = 'Add details (optional)'; details.maxLength = 500; details.rows = 3;
          const note = el('span', '', 'community-note');
          const send = el('button', 'Send report', 'cm-btn primary'); send.type = 'button';
          const cancel = el('button', 'Cancel', 'cm-btn'); cancel.type = 'button';
          cancel.onclick = closePanel;
          send.onclick = async () => {
            send.disabled = true;
            const result = await post(`/api/communities/${id}/posts/${topic.id}/report`, { reason: reason.value, details: details.value });
            if (result.ok) { note.textContent = 'Thanks, your report was recorded.'; return; }
            note.textContent = result.status === 409 ? 'You already reported this post.' : 'Could not send your report.';
            send.disabled = false;
          };
          const buttons = el('div', undefined, 'cm-row'); buttons.append(send, cancel, note);
          sidePanel.append(el('strong', 'Report this discussion'), reason, details, buttons);
        };
        actions.append(report);
      }

      const replyForm = el('form', undefined, 'community-toolbar cm-reply-form');
      const input = el('input'); input.placeholder = 'Write a reply'; input.maxLength = 2000; input.required = true;
      const send = el('button', 'Reply', 'cm-btn primary'); send.type = 'submit';
      replyForm.append(input, send);
      replyForm.onsubmit = async event => {
        event.preventDefault(); send.disabled = true;
        const result = await post(`/api/communities/${id}/posts/${topic.id}/comments`, { body: input.value });
        if (result.ok) reload(); else { alert('Could not add your reply.'); send.disabled = false; }
      };

      card.append(head, content, actions, sidePanel, replies, replyForm);
      return card;
    }

    async function loadCommunityDiscussion(id, group, panel) {
      panel.replaceChildren();
      const composer = el('section', undefined, 'community-composer cm-composer');
      composer.append(el('h2', 'Start a discussion'));
      const form = el('form', undefined, 'community-discussion-form');
      const title = el('input'); title.placeholder = 'Give your topic a clear title'; title.maxLength = 120; title.required = true;
      const editor = cmRichEditor('Ask a question, share advice, or start a conversation...');
      const submit = el('button', 'Share with community', 'cm-btn primary'); submit.type = 'submit';
      const status = el('p', '', 'community-note');
      form.append(title, editor.wrap, submit, status);
      composer.append(form); panel.append(composer);
      form.onsubmit = async event => {
        event.preventDefault();
        if (editor.blank()) { status.textContent = 'Write a message first.'; return; }
        const body = editor.html();
        if (body.length > 20000) { status.textContent = 'That message is too long.'; return; }
        submit.disabled = true;
        const result = await post(`/api/communities/${id}/posts`, { title: title.value, body });
        if (result.ok) loadCommunityDiscussion(id, group, panel);
        else { status.textContent = 'Could not post. Check your title and message.'; submit.disabled = false; }
      };
      const response = await fetch(`/api/communities/${id}/posts`);
      if (!response.ok) { panel.append(el('p', 'Discussions could not be loaded.', 'empty')); return; }
      const posts = await response.json();
      const postsBox = el('section', undefined, 'community-posts');
      if (!posts.length) postsBox.append(el('p', 'No discussions yet. Be the first to start a conversation.', 'empty'));
      posts.forEach(topic => postsBox.append(cmPostCard(id, group, panel, topic)));
      panel.append(postsBox);
    }

    
'@

# ---------- 4. Page: CSS ----------
$newCss = @'

    /* --- Community page upgrades --- */
    .cm-btn { font: inherit; font-size: .85rem; padding: .4rem .95rem; border-radius: 999px; border: 1px solid rgba(128,128,128,.4); background: transparent; color: inherit; cursor: pointer; }
    .cm-btn:hover { background: rgba(128,128,128,.14); }
    .cm-btn:disabled { opacity: .55; cursor: default; }
    .cm-btn.primary { background: #2563eb; border-color: #2563eb; color: #fff; }
    .cm-btn.primary:hover { background: #1d4ed8; }
    .cm-btn.danger { color: #dc2626; border-color: rgba(220,38,38,.45); }
    .cm-btn.danger:hover { background: rgba(220,38,38,.1); }
    .cm-avatar { position: relative; display: inline-flex; flex: 0 0 auto; }
    .cm-dot { position: absolute; right: -1px; bottom: -1px; width: 11px; height: 11px; border-radius: 50%; background: #22c55e; box-shadow: 0 0 0 2px rgba(255,255,255,.95); }
    .cm-active-strip { display: flex; align-items: center; gap: .9rem; flex-wrap: wrap; padding: .65rem 1rem; margin: .75rem 0; border: 1px solid rgba(128,128,128,.25); border-radius: 14px; background: rgba(34,197,94,.07); }
    .cm-active-label { font-size: .8rem; font-weight: 600; white-space: nowrap; }
    .cm-active-list { display: flex; gap: .5rem; flex-wrap: wrap; }
    .cm-active-chip { display: inline-flex; align-items: center; gap: .4rem; padding: .2rem .65rem .2rem .25rem; border-radius: 999px; background: rgba(128,128,128,.12); color: inherit; text-decoration: none; font-size: .82rem; }
    .cm-active-chip:hover { background: rgba(128,128,128,.22); }
    .cm-active-chip .cm-avatar > :first-child { width: 28px; height: 28px; }
    .cm-rich { border: 1px solid rgba(128,128,128,.4); border-radius: 12px; overflow: hidden; }
    .cm-rich-bar { display: flex; gap: .25rem; flex-wrap: wrap; padding: .35rem; border-bottom: 1px solid rgba(128,128,128,.25); background: rgba(128,128,128,.07); }
    .cm-rich-btn { font: inherit; font-size: .8rem; min-width: 2rem; padding: .25rem .55rem; border: 0; border-radius: 8px; background: transparent; color: inherit; cursor: pointer; }
    .cm-rich-btn:hover { background: rgba(128,128,128,.2); }
    .cm-rich-area { min-height: 110px; max-height: 340px; overflow: auto; padding: .7rem .85rem; outline: none; line-height: 1.5; }
    .cm-rich-area:empty::before { content: attr(data-placeholder); opacity: .55; pointer-events: none; }
    .cm-rich-area ul, .cm-rich-area ol, .cm-post-body ul, .cm-post-body ol { margin: .4rem 0 .4rem 1.4rem; padding: 0; }
    .cm-rich-area a, .cm-post-body a { color: #2563eb; text-decoration: underline; }
    .cm-post-card { border: 1px solid rgba(128,128,128,.28); border-radius: 16px; padding: 1rem 1.15rem; margin-bottom: 1rem; background: rgba(128,128,128,.05); }
    .cm-post-body { line-height: 1.55; overflow-wrap: anywhere; }
    .cm-post-body.cm-plain { white-space: pre-wrap; }
    .cm-post-actions { display: flex; gap: .5rem; flex-wrap: wrap; margin-top: .8rem; }
    .cm-post-panel { display: grid; gap: .6rem; margin-top: .8rem; padding: .8rem; border: 1px solid rgba(128,128,128,.25); border-radius: 12px; }
    .cm-post-panel input, .cm-post-panel select, .cm-post-panel textarea { font: inherit; width: 100%; box-sizing: border-box; padding: .5rem .65rem; border-radius: 10px; border: 1px solid rgba(128,128,128,.4); background: transparent; color: inherit; }
    .cm-row { display: flex; gap: .5rem; align-items: center; flex-wrap: wrap; }
    .cm-add-form { display: flex; gap: .5rem; align-items: center; flex-wrap: wrap; margin: .75rem 0 1rem; padding: .7rem; border: 1px dashed rgba(128,128,128,.45); border-radius: 14px; }
    .cm-add-form input { flex: 1 1 200px; font: inherit; padding: .5rem .8rem; border-radius: 999px; border: 1px solid rgba(128,128,128,.4); background: transparent; color: inherit; }
    .cm-member-row { display: flex; align-items: center; gap: .6rem; flex-wrap: wrap; padding: .6rem .2rem; border-bottom: 1px solid rgba(128,128,128,.18); }
    .cm-member-who { display: flex; align-items: center; gap: .55rem; flex: 1 1 200px; min-width: 0; }
    .cm-badge { font-size: .7rem; padding: .1rem .5rem; border-radius: 999px; background: rgba(37,99,235,.15); }
    .cm-badge.paused { background: rgba(220,38,38,.15); }
    .cm-reply-form { margin-top: .8rem; }
'@

# ---------- 5. Page: apply edits ----------
$html = [System.IO.File]::ReadAllText($indexHtml, $utf8)
$htmlDone = $html.Contains('function cmSafeHtml')
if (-not $htmlDone) {
  $startMarker = 'async function loadCommunityMembers(id, group, panel) {'
  $endMarker = 'function showHome() {'
  $startCount = [regex]::Matches($html, [regex]::Escape($startMarker)).Count
  $endCount = [regex]::Matches($html, [regex]::Escape($endMarker)).Count
  if ($startCount -ne 1 -or $endCount -ne 1) { throw "[community block] markers found $startCount/$endCount times, expected 1/1. Nothing was changed." }
  $s = $html.IndexOf($startMarker)
  $e = $html.IndexOf($endMarker)
  if ($e -le $s) { throw '[community block] end marker comes before start marker. Nothing was changed.' }
  $html = $html.Substring(0, $s) + $newJs + $html.Substring($e)

  $html = Edit-Once $html "const edit = el('button', 'Edit details');" "const edit = el('button', 'Edit');" 'Edit button'
  $html = Edit-Once $html "const imageButton = el('button', group.image_url ? 'Change image' : 'Add image');" "const imageButton = el('button', 'Image');" 'Image button'
  $html = Edit-Once $html "const remove = el('button', 'Delete community');" "const remove = el('button', 'Delete');" 'Delete button'
  $html = Edit-Once $html "const memberAction = el('button', 'Manage members'); memberAction.onclick = async () => showCommunityTab('Members');" "const memberAction = el('button', 'Manage'); memberAction.onclick = () => { activateTab('Members'); tabBar.scrollIntoView({ behavior: 'smooth', block: 'start' }); };" 'Manage button'
  $html = Edit-Once $html 'bannerContent.append(copy, actions); banner.append(bannerContent); main.append(banner);' 'bannerContent.append(copy, actions); banner.append(bannerContent); main.append(banner); if (group.is_member && !group.is_disabled) loadActiveCommunityMembers(id, banner);' 'Active strip'

  $styleCount = [regex]::Matches($html, '</style>').Count
  if ($styleCount -lt 1) { throw '[CSS] no </style> tag found. Nothing was changed.' }
  $at = $html.IndexOf('</style>')
  $html = $html.Substring(0, $at) + $newCss + "`n  " + $html.Substring($at)
}

# ---------- 6. Write everything (only reached if every check passed) ----------
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$backupDir = Join-Path $root ("backups\community-" + $stamp)
New-Item -ItemType Directory -Force -Path $backupDir | Out-Null
Copy-Item $mainRs (Join-Path $backupDir 'main.rs')
Copy-Item $indexHtml (Join-Path $backupDir 'index.html')

if (-not (Test-Path $migPath)) { [System.IO.File]::WriteAllText($migPath, $migration, $utf8); Write-Host "Created $migPath" } else { Write-Host 'Migration file already exists, left as is.' }
if (-not $rsDone) { [System.IO.File]::WriteAllText($mainRs, $rs, $utf8); Write-Host 'Patched src\main.rs' } else { Write-Host 'src\main.rs already patched.' }
if (-not $htmlDone) { [System.IO.File]::WriteAllText($indexHtml, $html, $utf8); Write-Host 'Patched static\index.html' } else { Write-Host 'static\index.html already patched.' }
Write-Host "Backups saved in $backupDir"
Write-Host ''
Write-Host 'Next: sqlx migrate run, then cargo run, then open http://127.0.0.1:3000/ and hard refresh (Ctrl+F5).'
