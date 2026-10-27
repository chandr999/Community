# apply-chat-friends-community.ps1
# Run from D:\Community:
#   powershell -ExecutionPolicy Bypass -File .\apply-chat-friends-community.ps1
# Changes static\index.html only (no Rust or database changes needed):
#   1. Chats open in a floating window (tabs, search, people search, group chat) instead of a page
#   2. Friends become colorful circles; Favorites move to your profile, above Interests
#   3. Community page gets a search field (top left) with Open / Join buttons
# It backs up index.html first, checks every find-and-replace matches exactly once,
# and writes nothing if any check fails.

$ErrorActionPreference = 'Stop'
$root = (Get-Location).Path
$indexHtml = Join-Path $root 'static\index.html'
if (-not (Test-Path $indexHtml)) { throw "Run this from D:\Community. Missing: $indexHtml" }
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Edit-Once([string]$text, [string]$find, [string]$replace, [string]$label) {
  $count = [regex]::Matches($text, [regex]::Escape($find)).Count
  if ($count -ne 1) { throw "[$label] expected exactly 1 match, found $count. Nothing was changed." }
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
# 1. Floating chat window: new global code (placed where showInbox used to be)
# =====================================================================
$chatGlobals = @'
var chatWindowOpen = false;
    var chatOpenBusy = false;
    var lastPageHash = (location.hash && location.hash !== '#/inbox') ? location.hash : '#/';

    function stopChatPolling() {
      if (inboxPoll !== null) { clearInterval(inboxPoll); inboxPoll = null; }
    }

    function cwPalette() {
      const read = node => getComputedStyle(node).backgroundColor;
      let bg = read(document.body);
      if (!bg || bg === 'rgba(0, 0, 0, 0)' || bg === 'transparent') bg = read(document.documentElement);
      const fg = getComputedStyle(document.body).color;
      if (!bg || bg === 'rgba(0, 0, 0, 0)' || bg === 'transparent') {
        const parts = (fg.match(/\d+/g) || [0, 0, 0]).map(Number);
        bg = (parts[0] + parts[1] + parts[2] > 450) ? '#1b1b1f' : '#ffffff';
      }
      return { bg, fg };
    }

    function ensureChatWindow() {
      let win = document.getElementById('chatWindow');
      if (win) return win;
      win = document.createElement('section');
      win.id = 'chatWindow'; win.className = 'cw'; win.hidden = true;
      win.setAttribute('role', 'dialog'); win.setAttribute('aria-label', 'Chats');
      const head = document.createElement('header'); head.className = 'cw-head';
      const title = document.createElement('strong'); title.textContent = 'Chats';
      const actions = document.createElement('span'); actions.className = 'cw-head-actions';
      const minimize = document.createElement('button'); minimize.type = 'button'; minimize.className = 'cw-icon';
      minimize.title = 'Minimize'; minimize.setAttribute('aria-label', 'Minimize chats'); minimize.textContent = '\u2013';
      const close = document.createElement('button'); close.type = 'button'; close.className = 'cw-icon';
      close.title = 'Close'; close.setAttribute('aria-label', 'Close chats'); close.textContent = '\u00d7';
      minimize.onclick = event => { event.stopPropagation(); win.classList.toggle('cw-minimized'); };
      close.onclick = event => { event.stopPropagation(); closeChatWindow(); };
      head.onclick = () => win.classList.toggle('cw-minimized');
      actions.append(minimize, close); head.append(title, actions);
      const body = document.createElement('div'); body.className = 'cw-body';
      win.append(head, body);
      const palette = cwPalette();
      win.style.background = palette.bg; win.style.color = palette.fg;
      document.body.append(win);
      return win;
    }

    async function openChatWindow(intent) {
      if (chatOpenBusy) return;
      chatOpenBusy = true;
      try {
        const win = ensureChatWindow();
        win.hidden = false; win.classList.remove('cw-minimized');
        chatWindowOpen = true;
        const body = win.querySelector('.cw-body');
        stopChatPolling();
        body.replaceChildren();
        await renderChatWindow(body, intent === 'chat');
      } finally { chatOpenBusy = false; }
    }

    function closeChatWindow() {
      chatWindowOpen = false;
      stopChatPolling();
      const win = document.getElementById('chatWindow');
      if (win) win.hidden = true;
    }

    // Anything that used to navigate to the Chats page now opens the floating window.
    async function showInbox() {
      const viaRoute = location.hash === '#/inbox';
      await openChatWindow(activeConversationId !== null ? 'chat' : 'list');
      if (viaRoute && location.hash === '#/inbox') {
        history.replaceState(null, '', lastPageHash || '#/');
        route();
      }
    }

    document.addEventListener('click', event => {
      const link = event.target && event.target.closest ? event.target.closest('a[href="#/inbox"]') : null;
      if (!link) return;
      event.preventDefault();
      const win = document.getElementById('chatWindow');
      const isOpen = chatWindowOpen && win && !win.hidden;
      if (isOpen && link.classList.contains('inbox-link')) {
        if (win.classList.contains('cw-minimized')) win.classList.remove('cw-minimized'); else closeChatWindow();
      } else if (isOpen) {
        win.classList.remove('cw-minimized');
      } else {
        openChatWindow('list');
      }
    });
    window.addEventListener('hashchange', () => { if (location.hash !== '#/inbox') lastPageHash = location.hash || '#/'; });
    setInterval(() => { if (!loggedIn && chatWindowOpen) closeChatWindow(); }, 1500);

    
'@

# Edits applied inside the original chat function (renamed renderChatWindow). Chat features stay as they were.
$chatEditsText = @'
@@FIND
async function showInbox() {
@@REPLACE
async function renderChatWindow(view, openChat) {
@@END
@@FIND
document.getElementById('homeView').hidden = true;
@@REPLACE

@@END
@@FIND
document.getElementById('profileView').hidden = true;
@@REPLACE

@@END
@@FIND
document.getElementById('settingsView').hidden = true;
@@REPLACE

@@END
@@FIND
document.getElementById('userSearchView').hidden = true;
@@REPLACE

@@END
@@FIND
document.getElementById('signupView').hidden = true;
@@REPLACE

@@END
@@FIND
const view = document.getElementById('inboxView');
@@REPLACE

@@END
@@FIND
view.hidden = false;
@@REPLACE

@@END
@@FIND
view.textContent = '';
@@REPLACE

@@END
@@FIND
stopInboxPolling();
@@REPLACE
stopChatPolling();
@@END
@@FIND
view.append(el('h1', 'Chats', 'inbox-title'));
@@REPLACE

@@END
@@FIND
view.append(requestsPanel);
@@REPLACE
requestsPanel.classList.add('cw-requests'); view.append(requestsPanel);
@@END
@@FIND
requestsList.replaceChildren();
@@REPLACE
requestsList.replaceChildren(); requestsPanel.hidden = false;
@@END
@@FIND
requestsList.append(el('p', 'No personal chat requests yet.', 'connection-empty')); return;
@@REPLACE
requestsPanel.hidden = true; return;
@@END
@@FIND
activeConversationId = data.conversation_id; location.hash = '#/inbox'; await showInbox();
@@REPLACE
activeConversationId = data.conversation_id; await showInbox();
@@END
@@FIND
const layout = el('div', undefined, 'inbox-layout');
@@REPLACE
const layout = el('div', undefined, 'inbox-layout cw-layout');
@@END
@@FIND
[['all', 'All'], ['unread', 'Unread'], ['groups', 'Groups'], ['community', 'Community']]
@@REPLACE
[['unread', 'Unread'], ['all', 'All'], ['groups', 'Groups'], ['community', 'Community'], ['personal', 'Personal']]
@@END
@@FIND
let inboxFilter = 'all';
@@REPLACE
let inboxFilter = 'all';
      let chatTextFilter = '';
      let communityRenderToken = 0;
@@END
@@FIND
createForm.onsubmit = async event => {
@@REPLACE
const followChips = el('div', undefined, 'cw-follow-chips');
      createForm.insertBefore(followChips, selectedRecipients);
      async function loadFollowChips() {
        followChips.replaceChildren();
        const followed = await loadFollowed();
        if (!followed.length) return;
        followChips.append(el('span', 'People you follow', 'hint'));
        followed.slice(0, 12).forEach(person => {
          const chip = el('button', '@' + person.username, 'cw-chip'); chip.type = 'button';
          chip.onclick = () => addRecipient(person.username);
          followChips.append(chip);
        });
      }
      const baseNewClick = newButton.onclick;
      newButton.onclick = () => { baseNewClick(); if (!createForm.hidden) loadFollowChips(); };
      createForm.onsubmit = async event => {
@@END
@@FIND
sidebar.append(sidebarHead, createForm, messageFilters, conversationList);
@@REPLACE
const searchBox = el('div', undefined, 'cw-search');
      const searchInput = el('input'); searchInput.type = 'search'; searchInput.autocomplete = 'off';
      searchInput.placeholder = 'Search chats or people'; searchInput.setAttribute('aria-label', 'Search chats, people, or followers');
      searchBox.append(searchInput);
      const peopleBox = el('div', undefined, 'cw-people'); peopleBox.hidden = true;
      let followedCache = null; let peopleRequest = 0;
      async function loadFollowed() {
        if (followedCache) return followedCache;
        try { const r = await fetch('/api/me/friends'); followedCache = r.ok ? await r.json() : []; } catch { followedCache = []; }
        return followedCache;
      }
      async function startChatWith(username) {
        const response = await post('/api/conversations', { usernames: [username], title: '' });
        if (!response.ok) { alert(response.status === 403 ? 'This person is not accepting messages from you.' : 'Could not start that chat.'); return; }
        const result = await response.json();
        activeConversationId = result.id;
        searchInput.value = ''; chatTextFilter = ''; peopleBox.hidden = true; peopleBox.replaceChildren();
        chat.hidden = false; chat.classList.add('chat-window');
        await loadConversations(); await loadMessages(activeConversationId);
      }
      searchInput.oninput = async () => {
        chatTextFilter = searchInput.value.trim().toLowerCase();
        renderConversationItems();
        const query = searchInput.value.trim(); const request = ++peopleRequest;
        if (query.length < 2) { peopleBox.hidden = true; peopleBox.replaceChildren(); return; }
        peopleBox.hidden = false; peopleBox.replaceChildren(el('p', 'Searching people...', 'cw-note'));
        const followed = await loadFollowed();
        const lowered = query.toLowerCase();
        const people = followed
          .filter(p => p.username.toLowerCase().includes(lowered) || (p.display_name || '').toLowerCase().includes(lowered))
          .map(p => ({ username: p.username, display_name: p.display_name, avatar_url: p.avatar_url, following: true }));
        try {
          const response = await fetch('/api/users/search?q=' + encodeURIComponent(query));
          if (request !== peopleRequest) return;
          if (response.ok) (await response.json()).forEach(u => {
            if (!people.some(p => p.username === u.username)) people.push({ username: u.username, display_name: u.display_name, avatar_url: u.avatar_url, following: false });
          });
        } catch { /* keep the followed matches */ }
        if (request !== peopleRequest) return;
        peopleBox.replaceChildren(el('h3', 'People', 'cw-people-title'));
        if (!people.length) { peopleBox.append(el('p', 'No people found.', 'cw-note')); return; }
        people.slice(0, 8).forEach(person => {
          const row = el('div', undefined, 'cw-person');
          const copy = el('span', undefined, 'cw-person-copy');
          copy.append(el('strong', '@' + person.username), el('span', person.following ? 'Following' : (person.display_name || ''), 'cw-person-sub'));
          const chatButton = el('button', 'Chat', 'cw-btn primary'); chatButton.type = 'button';
          chatButton.onclick = () => startChatWith(person.username);
          const groupButton = el('button', '+ Group', 'cw-btn'); groupButton.type = 'button'; groupButton.title = 'Add to a new group chat';
          groupButton.onclick = () => {
            createForm.hidden = false;
            if (!groupMode) groupModeButton.onclick();
            addRecipient(person.username);
            loadFollowChips();
          };
          row.append(avatarEl(person.avatar_url), copy, chatButton, groupButton);
          peopleBox.append(row);
        });
      };
      sidebar.append(sidebarHead, messageFilters, searchBox, createForm, peopleBox, conversationList);
@@END
@@FIND
inboxPoll = setInterval(async () => {
@@REPLACE
if (openChat && activeConversationId !== null && cachedConversations.some(item => item.id === activeConversationId)) {
        chat.hidden = false; chat.classList.add('chat-window');
        await loadMessages(activeConversationId);
      }
      inboxPoll = setInterval(async () => {
        if (!loggedIn || !chatWindowOpen) return;
@@END
@@FIND
const visibleConversations = cachedConversations.filter(conversation => {
@@REPLACE
const visibleConversations = cachedConversations.filter(conversation => {
          if (chatTextFilter && !(((conversation.title || '') + ' ' + conversation.members.join(' ') + ' ' + (conversation.latest_message || '')).toLowerCase().includes(chatTextFilter))) return false;
@@END
@@FIND
if (inboxFilter === 'groups') return conversation.is_group;
@@REPLACE
if (inboxFilter === 'personal') return !conversation.is_group;
          if (inboxFilter === 'groups') return conversation.is_group;
@@END
@@FIND
function renderConversationItems() {
@@REPLACE
function renderConversationItems() {
        const token = ++communityRenderToken;
        renderConversationItemsBase();
        if (inboxFilter === 'community') appendCommunityShortcuts(token);
      }

      async function appendCommunityShortcuts(token) {
        let communities = [];
        try { const response = await fetch('/api/communities'); if (response.ok) communities = await response.json(); } catch { return; }
        if (token !== communityRenderToken || inboxFilter !== 'community') return;
        const joined = communities.filter(group => group.is_member && !group.is_disabled);
        conversationList.append(el('h3', 'Your communities', 'cw-people-title'));
        if (!joined.length) { conversationList.append(el('p', 'Join a community from the Community page to see it here.', 'activity-empty')); return; }
        joined.forEach(group => {
          const item = el('button', undefined, 'conversation-item'); item.type = 'button';
          const mark = el('div', group.name.slice(0, 1).toUpperCase(), 'activity-icon');
          const copy = el('span', undefined, 'conversation-copy');
          copy.append(el('span', group.name, 'conversation-name'), el('span', group.member_count + ' members \u00b7 open discussion', 'conversation-preview'));
          item.append(mark, copy);
          item.onclick = () => { closeChatWindow(); location.hash = '#/community/' + group.id; };
          conversationList.append(item);
        });
      }

      function renderConversationItemsBase() {
@@END
'@

# =====================================================================
# 2. Friends circles + favorites on the profile
# =====================================================================
$friendsJs = @'
function friendHue(name) {
      let hue = 0;
      for (const ch of String(name)) hue = (hue * 31 + ch.charCodeAt(0)) % 360;
      return hue;
    }

    function friendCircle(person, withActions) {
      const card = el('article', undefined, 'fc-person');
      const hue = friendHue(person.username);
      card.style.setProperty('--fc-a', 'hsl(' + hue + ', 85%, 62%)');
      card.style.setProperty('--fc-b', 'hsl(' + ((hue + 55) % 360) + ', 90%, 72%)');
      const ring = el('a', undefined, 'fc-circle');
      ring.href = '#/u/' + encodeURIComponent(person.username);
      ring.title = '@' + person.username;
      ring.append(avatarEl(person.avatar_url));
      const name = profileLink(person.username, '@' + person.username);
      if (name.classList) name.classList.add('fc-name');
      card.append(ring, name);
      if (person.display_name) card.append(el('span', person.display_name, 'fc-sub'));
      if (!withActions) return card;
      const actions = el('div', undefined, 'fc-actions');
      const favorite = el('button', person.is_favorite ? '\u2665 Favorited' : '\u2661 Favorite', 'fc-btn' + (person.is_favorite ? ' on' : ''));
      favorite.type = 'button';
      favorite.title = person.is_favorite ? 'Remove from favorites' : 'Add to favorites';
      favorite.onclick = async () => {
        const r = await fetch('/api/users/' + encodeURIComponent(person.username) + '/favorite', { method: person.is_favorite ? 'DELETE' : 'POST' });
        if (r.ok) showFriends();
      };
      const unfollow = el('button', 'Unfollow', 'fc-btn'); unfollow.type = 'button';
      unfollow.onclick = async () => {
        if (!window.confirm('Unfollow @' + person.username + '?')) return;
        const r = await fetch('/api/users/' + encodeURIComponent(person.username) + '/follow', { method: 'DELETE' });
        if (r.ok) showFriends();
      };
      actions.append(favorite, unfollow); card.append(actions);
      return card;
    }

    async function showFriends() {
      stopInboxPolling(); hideSocialViews();
      const view = document.getElementById('friendsView'); view.hidden = false; view.replaceChildren();
      view.append(el('h1', 'Friends', 'inbox-title'));
      if (!loggedIn) { view.append(el('p', 'Log in to see the people you follow.', 'empty')); return; }
      const response = await fetch('/api/me/friends');
      if (!response.ok) { view.append(el('p', 'Friends could not be loaded. Please try again.', 'empty')); return; }
      const people = await response.json();
      const following = el('section', undefined, 'fc-section');
      following.append(el('h2', 'Following \u00b7 ' + people.length));
      if (!people.length) following.append(el('p', 'People you follow will appear here.', 'empty'));
      const grid = el('div', undefined, 'fc-grid');
      people.forEach(person => grid.append(friendCircle(person, true)));
      following.append(grid);
      if (currentUsername) {
        const hint = el('p', undefined, 'fc-hint');
        hint.append('Your favorites appear on your profile, above Interests. ', profileLink(currentUsername, 'View my profile'));
        following.append(hint);
      }
      view.append(following);
    }

    async function fillProfileFavorites(box) {
      try {
        const response = await fetch('/api/me/friends');
        if (!response.ok) return;
        const favorites = (await response.json()).filter(person => person.is_favorite);
        box.append(el('h2', 'Favorites'));
        if (!favorites.length) {
          box.append(el('p', 'Mark people as favorites from the Friends page for quick access.', 'empty'));
        } else {
          const row = el('div', undefined, 'fc-grid fc-compact');
          favorites.forEach(person => row.append(friendCircle(person, false)));
          box.append(row);
        }
        box.hidden = false;
      } catch (error) { console.error('Could not load favorites:', error); }
    }

    
'@

# =====================================================================
# 3. Community search (top left of the Community page)
# =====================================================================
$communitySearchJs = @'
function communitySearchBox(communities) {
      const box = el('div', undefined, 'cs-search');
      const input = el('input'); input.type = 'search'; input.autocomplete = 'off';
      input.placeholder = 'Search communities to join'; input.setAttribute('aria-label', 'Search communities');
      const results = el('div', undefined, 'cs-results'); results.hidden = true;
      const go = hash => { if (location.hash === hash) route(); else location.hash = hash; };
      const render = () => {
        const query = input.value.trim().toLowerCase();
        results.replaceChildren();
        if (!query) { results.hidden = true; return; }
        results.hidden = false;
        const matches = communities.filter(group => (group.name + ' ' + (group.description || '')).toLowerCase().includes(query)).slice(0, 8);
        if (!matches.length) { results.append(el('p', 'No community matches that search.', 'community-note')); return; }
        matches.forEach(group => {
          const row = el('div', undefined, 'cs-row');
          const info = el('div', undefined, 'cs-info');
          info.append(el('strong', group.name), el('span', group.member_count + ' members', 'cs-sub'));
          const action = el('button', undefined, 'cs-btn'); action.type = 'button';
          if (group.is_disabled) { action.textContent = 'Access paused'; action.disabled = true; }
          else if (group.is_member) { action.textContent = 'Open'; action.onclick = () => go('#/community/' + group.id); }
          else {
            action.textContent = 'Join'; action.className = 'cs-btn primary';
            action.onclick = async () => {
              action.disabled = true;
              const response = await post('/api/communities/' + group.id + '/join');
              if (response.ok) go('#/community/' + group.id);
              else { alert('Could not join this community. Please try again.'); action.disabled = false; }
            };
          }
          row.append(info, action); results.append(row);
        });
      };
      input.oninput = render;
      input.onkeydown = event => { if (event.key === 'Escape') { input.value = ''; render(); } };
      box.append(input, results);
      return box;
    }

    
'@

$pageEditsText = @'
@@FIND
function stopInboxPolling() {
@@REPLACE
function stopInboxPolling() {
      if (chatWindowOpen) return; // the floating chat window keeps refreshing while you browse
@@END
@@FIND
function communityWorkspace(view, communities, activeId) {
@@REPLACE
%%COMMUNITY_SEARCH_JS%%function communityWorkspace(view, communities, activeId) {
@@END
@@FIND
sidebarHead.append(create); sidebar.append(sidebarHead);
@@REPLACE
sidebar.append(communitySearchBox(communities)); sidebarHead.append(create); sidebar.append(sidebarHead);
@@END
@@FIND
if (pr.interests) {
@@REPLACE
if (currentUsername && pr.username === currentUsername) {
        const favoritesBox = el('section', undefined, 'profile-about-card profile-favorites'); favoritesBox.hidden = true;
        view.append(favoritesBox); fillProfileFavorites(favoritesBox);
      }
      if (pr.interests) {
@@END
'@

# =====================================================================
# 4. CSS
# =====================================================================
$newCss = @'

    /* --- Floating chat window --- */
    .cw { position: fixed; right: 18px; bottom: 18px; width: min(780px, calc(100vw - 24px)); height: min(600px, calc(100vh - 36px)); z-index: 1000; display: flex; flex-direction: column; border-radius: 18px; overflow: hidden; border: 1px solid rgba(128,128,128,.35); box-shadow: 0 18px 50px rgba(0,0,0,.3); }
    .cw[hidden] { display: none !important; }
    .cw.cw-minimized { height: auto; width: min(320px, calc(100vw - 24px)); }
    .cw.cw-minimized .cw-body { display: none; }
    .cw-head { display: flex; align-items: center; justify-content: space-between; padding: .6rem .9rem; background: linear-gradient(90deg, #6366f1, #ec4899); color: #fff; cursor: pointer; flex: 0 0 auto; }
    .cw-head-actions { display: inline-flex; gap: .3rem; }
    .cw-icon { font: inherit; font-size: 1.1rem; line-height: 1; width: 1.9rem; height: 1.9rem; border: 0; border-radius: 50%; background: rgba(255,255,255,.22); color: #fff; cursor: pointer; }
    .cw-icon:hover { background: rgba(255,255,255,.4); }
    .cw .cw-body { flex: 1 1 auto; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }
    .cw .cw-layout { display: flex !important; flex: 1 1 auto; min-height: 0; gap: 0 !important; margin: 0 !important; width: 100% !important; max-width: none !important; }
    .cw .conversation-sidebar { flex: 0 0 290px; display: flex !important; flex-direction: column; min-height: 0; overflow: hidden; padding: .6rem; box-sizing: border-box; border-right: 1px solid rgba(128,128,128,.25); max-width: none !important; width: auto !important; height: auto !important; }
    .cw .conversation-list { flex: 1 1 auto; min-height: 0; overflow: auto; max-height: none !important; }
    .cw .conversation-sidebar-head { display: flex; align-items: center; justify-content: space-between; gap: .5rem; }
    .cw .conversation-sidebar-head h2 { font-size: 1rem; margin: 0; }
    .cw .cw-layout:has(.chat-pane[hidden]) .conversation-sidebar { flex: 1 1 auto; border-right: 0; }
    .cw .chat-pane, .cw .chat-pane.chat-window { position: static !important; inset: auto !important; transform: none !important; width: auto !important; height: auto !important; max-width: none !important; max-height: none !important; margin: 0 !important; border: 0 !important; border-radius: 0 !important; box-shadow: none !important; flex: 1 1 auto; min-width: 0; min-height: 0; display: flex; flex-direction: column; }
    .cw .chat-pane[hidden] { display: none !important; }
    .cw .message-list { flex: 1 1 auto; min-height: 0; overflow: auto; max-height: none !important; height: auto !important; }
    .cw .message-composer { flex-wrap: wrap; }
    .cw .message-composer input[type="text"] { flex: 1 1 120px; min-width: 0; }
    .cw .message-media { max-width: 100%; }
    .cw .message-filters { display: flex; flex-wrap: wrap; gap: .3rem; margin: .4rem 0; }
    .cw .cw-requests { margin: 0 !important; padding: .4rem .8rem !important; max-height: 130px; overflow: auto; border-bottom: 1px solid rgba(128,128,128,.25); flex: 0 0 auto; }
    .cw .cw-search { margin: .3rem 0 .5rem; }
    .cw .cw-search input { width: 100%; box-sizing: border-box; font: inherit; padding: .5rem .85rem; border-radius: 999px; border: 1px solid rgba(128,128,128,.4); background: transparent; color: inherit; }
    .cw .cw-people { margin: 0 0 .5rem; padding: .4rem; border: 1px solid rgba(128,128,128,.25); border-radius: 12px; max-height: 230px; overflow: auto; flex: 0 0 auto; }
    .cw .cw-people-title { margin: .3rem .3rem; font-size: .8rem; opacity: .75; }
    .cw .cw-person { display: flex; align-items: center; gap: .5rem; padding: .3rem; }
    .cw .cw-person > :first-child { width: 32px !important; height: 32px !important; flex: 0 0 32px; }
    .cw .cw-person-copy { display: flex; flex-direction: column; flex: 1 1 auto; min-width: 0; }
    .cw .cw-person-sub, .cw .cw-note { font-size: .75rem; opacity: .7; }
    .cw .cw-btn { font: inherit; font-size: .78rem; padding: .25rem .65rem; border-radius: 999px; border: 1px solid rgba(128,128,128,.4); background: transparent; color: inherit; cursor: pointer; }
    .cw .cw-btn.primary { background: #6366f1; border-color: #6366f1; color: #fff; }
    .cw .cw-follow-chips { display: flex; flex-wrap: wrap; align-items: center; gap: .3rem; margin: .4rem 0; }
    .cw .cw-chip { font: inherit; font-size: .75rem; padding: .15rem .55rem; border-radius: 999px; border: 1px solid rgba(99,102,241,.5); background: rgba(99,102,241,.1); color: inherit; cursor: pointer; }
    @media (max-width: 640px) {
      .cw { right: 0; bottom: 0; width: 100vw; height: 100vh; border-radius: 0; }
      .cw .conversation-sidebar { flex-basis: 100%; border-right: 0; }
      .cw .cw-layout:has(.chat-pane:not([hidden])) .conversation-sidebar { display: none !important; }
    }

    /* --- Friends circles and profile favorites --- */
    .fc-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 1.5rem 1rem; margin: 1rem 0; }
    .fc-person { display: flex; flex-direction: column; align-items: center; text-align: center; gap: .35rem; }
    .fc-circle { display: block; width: 100px; height: 100px; border-radius: 50%; padding: 4px; box-sizing: border-box; background: linear-gradient(135deg, var(--fc-a), var(--fc-b)); box-shadow: 0 6px 16px rgba(0,0,0,.18); transition: transform .15s; }
    .fc-circle:hover { transform: translateY(-3px) scale(1.05); }
    .fc-circle > * { width: 100% !important; height: 100% !important; border-radius: 50% !important; object-fit: cover; box-sizing: border-box; border: 3px solid rgba(255,255,255,.92); display: flex; align-items: center; justify-content: center; }
    .fc-name { font-weight: 600; font-size: .9rem; overflow-wrap: anywhere; }
    .fc-sub { font-size: .75rem; opacity: .7; }
    .fc-actions { display: flex; flex-wrap: wrap; gap: .3rem; justify-content: center; }
    .fc-btn { font: inherit; font-size: .72rem; padding: .2rem .65rem; border-radius: 999px; border: 1px solid rgba(128,128,128,.4); background: transparent; color: inherit; cursor: pointer; }
    .fc-btn:hover { background: rgba(128,128,128,.14); }
    .fc-btn.on { background: linear-gradient(135deg, var(--fc-a), var(--fc-b)); color: #fff; border-color: transparent; }
    .fc-hint { font-size: .85rem; opacity: .8; }
    .fc-compact { grid-template-columns: repeat(auto-fill, minmax(92px, 1fr)); }
    .fc-compact .fc-circle { width: 72px; height: 72px; }

    /* --- Community search --- */
    .cs-search { margin-bottom: .9rem; }
    .cs-search input { width: 100%; box-sizing: border-box; font: inherit; padding: .55rem .9rem; border-radius: 999px; border: 1px solid rgba(128,128,128,.4); background: transparent; color: inherit; }
    .cs-results { margin-top: .4rem; border: 1px solid rgba(128,128,128,.25); border-radius: 12px; padding: .3rem .5rem; }
    .cs-row { display: flex; align-items: center; gap: .6rem; padding: .45rem .1rem; border-bottom: 1px solid rgba(128,128,128,.18); }
    .cs-row:last-child { border-bottom: 0; }
    .cs-info { display: flex; flex-direction: column; flex: 1 1 auto; min-width: 0; }
    .cs-sub { font-size: .75rem; opacity: .7; }
    .cs-btn { font: inherit; font-size: .8rem; padding: .3rem .8rem; border-radius: 999px; border: 1px solid rgba(128,128,128,.4); background: transparent; color: inherit; cursor: pointer; }
    .cs-btn.primary { background: #2563eb; border-color: #2563eb; color: #fff; }
    .cs-btn:disabled { opacity: .55; cursor: default; }
'@

# =====================================================================
# Apply everything in memory first
# =====================================================================
$html = [System.IO.File]::ReadAllText($indexHtml, $utf8)
if ($html.Contains('function ensureChatWindow')) {
  Write-Host 'static\index.html already contains these changes. Nothing to do.'
  return
}

foreach ($name in @('stopChatPolling', 'cwPalette', 'ensureChatWindow', 'openChatWindow', 'closeChatWindow', 'renderChatWindow', 'friendHue', 'friendCircle', 'fillProfileFavorites', 'communitySearchBox', 'chatWindowOpen', 'chatOpenBusy', 'lastPageHash')) {
  if ([regex]::IsMatch($html, '\b(function|var|let|const)\s+' + $name + '\b')) { throw "[name conflict] '$name' already exists in index.html. Nothing was changed." }
}

$chatEdits = Get-Edits $chatEditsText 24 'chat edits'
$pageEdits = Get-Edits $pageEditsText 4 'page edits'

# 4 small edits spread across the page
foreach ($pair in $pageEdits) {
  $replacement = $pair[1].Replace('%%COMMUNITY_SEARCH_JS%%', $communitySearchJs)
  $html = Edit-Once $html $pair[0] $replacement ('page: ' + $pair[0])
}

# Chat function: pull it out, edit it, put it back behind the new window code
$chatStart = 'async function showInbox() {'
$chatEnd = '// ---------- feed ----------'
$startCount = [regex]::Matches($html, [regex]::Escape($chatStart)).Count
$endCount = [regex]::Matches($html, [regex]::Escape($chatEnd)).Count
if ($startCount -ne 1 -or $endCount -ne 1) { throw "[chat function] markers found $startCount/$endCount times, expected 1/1. Nothing was changed." }
$s = $html.IndexOf($chatStart, [System.StringComparison]::Ordinal)
$e = $html.IndexOf($chatEnd, [System.StringComparison]::Ordinal)
if ($e -le $s) { throw '[chat function] end marker comes before start marker. Nothing was changed.' }
$chatFn = $html.Substring($s, $e - $s)
foreach ($pair in $chatEdits) { $chatFn = Edit-Once $chatFn $pair[0] $pair[1] ('chat: ' + $pair[0]) }
$html = $html.Substring(0, $s) + $chatGlobals + $chatFn + $html.Substring($e)

# Friends page: replace the old function with circles
$friendsStart = 'async function showFriends() {'
$friendsEnd = 'function communitySideItem(group, activeId, openCommunity) {'
$fsCount = [regex]::Matches($html, [regex]::Escape($friendsStart)).Count
$feCount = [regex]::Matches($html, [regex]::Escape($friendsEnd)).Count
if ($fsCount -ne 1 -or $feCount -ne 1) { throw "[friends page] markers found $fsCount/$feCount times, expected 1/1. Nothing was changed." }
$fs = $html.IndexOf($friendsStart, [System.StringComparison]::Ordinal)
$fe = $html.IndexOf($friendsEnd, [System.StringComparison]::Ordinal)
if ($fe -le $fs) { throw '[friends page] end marker comes before start marker. Nothing was changed.' }
$html = $html.Substring(0, $fs) + $friendsJs + $html.Substring($fe)

# CSS
if ([regex]::Matches($html, '</style>').Count -lt 1) { throw '[CSS] no </style> tag found. Nothing was changed.' }
$at = $html.IndexOf('</style>', [System.StringComparison]::Ordinal)
$html = $html.Substring(0, $at) + $newCss + "`n  " + $html.Substring($at)

# =====================================================================
# Write (only reached if every check passed)
# =====================================================================
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$backupDir = Join-Path $root ('backups\chat-friends-' + $stamp)
New-Item -ItemType Directory -Force -Path $backupDir | Out-Null
Copy-Item $indexHtml (Join-Path $backupDir 'index.html')
[System.IO.File]::WriteAllText($indexHtml, $html, $utf8)
Write-Host 'Patched static\index.html'
Write-Host "Backup saved in $backupDir"
Write-Host ''
Write-Host 'Next: hard refresh the app in your browser (Ctrl+F5). If the page looks unchanged, stop and restart cargo run first.'
