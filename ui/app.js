/* HermesRust — app.js
   Front-end logic, rebuilt from the original single-file client.
   The PM orchestrator prompt is NOT built here anymore — the Rust backend
   builds it from the agent registry sent with each request (hermes.agents).
   Edit freely — refresh (F5) applies changes instantly. */

'use strict';

/* ═══════════════════ STATE ═══════════════════ */
const state = {
  sessions: [],
  messages: {},
  agents: [],
  skills: [],
  installedSkills: [],
  workspace: '',
  currentSessionId: null,
  isStreaming: false,
  abortController: null,
  completedTasks: 0,
  serverConfig: null,
  settings: { endpoint: '', apiKey: '', model: '' },
};

/* ═══════════════════ HELPERS ═══════════════════ */
const $ = (id) => document.getElementById(id);
const escapeHtml = (s) => { const d = document.createElement('div'); d.textContent = s ?? ''; return d.innerHTML; };
const api = () => (state.settings.endpoint || location.origin).replace(/\/$/, '');
const saveSettings = () => localStorage.setItem('hermes-settings', JSON.stringify(state.settings));
const uid = () => 'agent-' + Date.now() + '-' + Math.random().toString(36).slice(2, 6);

function toast(text, type = '') {
  const t = document.createElement('div');
  t.className = 'toast ' + type;
  t.textContent = text;
  $('toastContainer').appendChild(t);
  setTimeout(() => { t.style.opacity = '0'; t.style.transition = 'opacity .3s'; }, 2200);
  setTimeout(() => t.remove(), 2600);
}

function showStatus(type, text) {
  const bar = $('statusBar');
  bar.classList.add('visible');
  $('statusDot').className = 'status-dot ' + type;
  $('statusText').textContent = text;
}
function hideStatus() { setTimeout(() => $('statusBar').classList.remove('visible'), 1200); }

/* ═══════════════════ BOOT ═══════════════════ */
async function boot() {
  loadSettings();
  loadSessions();
  loadWorkspace();
  await loadServerConfig();
  await loadServerAgents();
  await loadServerSkills();
  renderAll();
  checkConnection();
}

async function loadServerConfig() {
  try {
    const r = await fetch(api() + '/api/config');
    state.serverConfig = await r.json();
    if (!state.settings.model) state.settings.model = state.serverConfig.model || '';
    if (!state.settings.endpoint) state.settings.endpoint = '';
    $('cfgPathNote').textContent = state.serverConfig.config_path || 'config/config.toml';
    $('footVersion').textContent = 'HermesRust v' + (state.serverConfig.version || '?');
    $('aboutVersion').textContent = 'v' + (state.serverConfig.version || '?');
    const up = state.serverConfig.uptime_secs || 0;
    $('aboutStats').textContent = `endpoint: ${state.serverConfig.endpoint || '(not set)'} · uptime ${up}s · agents ${state.serverConfig.orchestrator === false ? 'off' : 'on'}`;
  } catch (e) { console.warn('config unavailable', e); }
}

async function loadServerAgents() {
  // Server agents.json is the seed; local CRUD edits take precedence.
  if (localStorage.getItem('hermes-agents')) return;
  try {
    const r = await fetch(api() + '/api/agents');
    const d = await r.json();
    if (d.agents && d.agents.length) {
      state.agents = d.agents.map(a => ({ ...a, _status: 'idle', _progress: 0 }));
      saveAgents();
    }
  } catch (e) { console.warn('agents unavailable', e); }
  if (state.agents.length === 0) seedFallbackAgents();
}

async function loadServerSkills() {
  try {
    const r = await fetch(api() + '/api/skills');
    const d = await r.json();
    state.skills = Array.isArray(d) ? d : (d.skills || []);
  } catch (e) { console.warn('skills unavailable', e); }
  state.installedSkills = JSON.parse(localStorage.getItem('hermes-installed-skills') || '[]');
  const cats = [...new Set(state.skills.map(s => s.cat).filter(Boolean))].sort();
  for (const c of cats) {
    const o = document.createElement('option');
    o.value = c; o.textContent = c;
    $('skillCategory').appendChild(o);
  }
}

function seedFallbackAgents() {
  state.agents = [
    { id: 'agent-pm', name: 'Project Manager', abbr: 'PM', icon: '👔', color: '#1d4ed8', isMain: true, isBuiltin: true, description: 'Main orchestrator: decomposes tasks, dispatches the team, merges deliverables.', skills: ['Planning', 'Scheduling', 'Coordination'], systemPrompt: 'You are a senior Project Manager and team dispatcher.' },
    { id: 'agent-frontend', name: 'Frontend Engineer', abbr: 'FE', icon: '🖥️', color: '#06b6d4', isBuiltin: true, description: 'React/Vue/HTML/CSS/JS interfaces.', skills: ['React', 'TypeScript', 'CSS'], systemPrompt: 'You are a senior frontend engineer.' },
    { id: 'agent-backend', name: 'Backend Engineer', abbr: 'BE', icon: '⚙️', color: '#4f6ef7', isBuiltin: true, description: 'APIs, databases, microservices.', skills: ['APIs', 'Databases', 'Go'], systemPrompt: 'You are a senior backend engineer.' },
  ].map(a => ({ ...a, _status: 'idle', _progress: 0 }));
  saveAgents();
}

function renderAll() {
  renderSessions();
  renderAgents();
  renderSkills();
  renderMessages();
  updateStats();
  updateModelUI();
}

/* ═══════════════════ PERSISTENCE ═══════════════════ */
function loadSettings() {
  try { const s = JSON.parse(localStorage.getItem('hermes-settings') || '{}'); Object.assign(state.settings, s); } catch (e) {}
}
function loadSessions() {
  try {
    const d = JSON.parse(localStorage.getItem('hermes-sessions') || '{}');
    state.sessions = d.sessions || [];
    state.messages = d.messages || {};
  } catch (e) {}
}
function saveSessions() {
  localStorage.setItem('hermes-sessions', JSON.stringify({ sessions: state.sessions, messages: state.messages }));
}
function saveAgents() { localStorage.setItem('hermes-agents', JSON.stringify(state.agents)); }
function loadWorkspace() { state.workspace = localStorage.getItem('hermes-workspace') || ''; }

/* ═══════════════════ NAVIGATION ═══════════════════ */
const VIEW_TITLES = { chat: 'Chat', agents: 'Agents', skills: 'Skill Store', settings: 'Settings' };
function showView(view) {
  document.querySelectorAll('.view').forEach(v => v.classList.remove('active'));
  $('view-' + view).classList.add('active');
  document.querySelectorAll('.nav-item').forEach(n => n.classList.toggle('active', n.dataset.view === view));
  $('viewTitle').textContent = VIEW_TITLES[view] || view;
  closeWorkspaceMenu();
}

/* ═══════════════════ SESSIONS ═══════════════════ */
function newChat() {
  const id = 'sess-' + Date.now();
  state.sessions.unshift({ id, title: 'New conversation', createdAt: new Date().toISOString(), updatedAt: new Date().toISOString() });
  state.messages[id] = [];
  state.currentSessionId = id;
  saveSessions(); renderSessions(); renderMessages();
}
function switchSession(id) {
  if (!state.messages[id]) state.messages[id] = [];
  state.currentSessionId = id;
  renderSessions(); renderMessages();
}
function deleteSession(id, ev) {
  ev.stopPropagation();
  state.sessions = state.sessions.filter(s => s.id !== id);
  delete state.messages[id];
  if (state.currentSessionId === id) state.currentSessionId = state.sessions[0]?.id || null;
  saveSessions(); renderSessions(); renderMessages();
}
function renderSessions() {
  const box = $('sessionsList');
  if (!state.sessions.length) {
    box.innerHTML = '<div class="sessions-empty">No session yet</div>';
    return;
  }
  box.innerHTML = state.sessions.map(s => `
    <button class="session-item ${s.id === state.currentSessionId ? 'active' : ''}" onclick="switchSession('${s.id}')">
      <span>💬</span><span class="s-title">${escapeHtml(s.title)}</span>
      <span class="session-del" onclick="deleteSession('${s.id}', event)">×</span>
    </button>`).join('');
}

function currentMessages() {
  if (!state.currentSessionId) newChat();
  return state.messages[state.currentSessionId];
}

function updateStats() {
  const totalMsgs = Object.values(state.messages).reduce((n, a) => n + a.length, 0);
  $('agStatDone').textContent = state.completedTasks;
  $('navAgentCount').textContent = state.agents.length;
  $('hintModel').textContent = state.settings.model ? 'model: ' + state.settings.model : '';
  $('hintWorkspace').textContent = state.workspace ? 'workspace: ' + state.workspace.replace(/^~\/?/, '~/…/') : '';
  $('workspaceLabel').textContent = state.workspace ? state.workspace.split(/[\\/]/).filter(Boolean).pop() || state.workspace : 'Set workspace';
  $('currentModel').textContent = state.settings.model || '—';
}

/* ═══════════════════ CHAT RENDERING ═══════════════════ */
function renderMessages() {
  const box = $('messagesContainer');
  const msgs = state.currentSessionId ? (state.messages[state.currentSessionId] || []) : [];
  if (!msgs.length) {
    box.innerHTML = '';
    $('welcomeScreen').style.display = 'flex';
    return;
  }
  $('welcomeScreen').style.display = 'none';
  box.innerHTML = msgs.map((m, i) => renderMessage(m, i)).join('');
  scrollToBottom();
}

function renderMessage(msg, index) {
  const who = msg.role === 'user' ? 'user' : 'assistant';
  const avatar = who === 'user' ? '👤' : 'H';
  const body = msg.role === 'user' ? escapeHtml(msg.content).replace(/\n/g, '<br>') : renderMarkdown(msg.content, false);
  return `<div class="message-row ${who}" id="msg-${index}">
    <div class="message-avatar">${avatar}</div>
    <div class="message-bubble"><div class="md">${body}</div></div>
  </div>`;
}

function scrollToBottom() {
  const c = $('messagesContainer');
  c.scrollTop = c.scrollHeight;
}

/* ── Markdown renderer (ported & simplified from the original) ── */
function renderMarkdown(text, isStreaming) {
  if (!text) return '';
  let t = text;

  // 1. Extract agent routing markers → badges (lookup by id, name, or raw id).
  t = t.replace(/<!--\s*agent:([\S]+?)\s*-->/g, (m, id) => {
    const ag = state.agents.find(a => a.id === id) || state.agents.find(a => a.name === id);
    const name = ag ? `${ag.icon || '🤖'} ${ag.name}` : `🤖 ${id}`;
    const color = ag ? ag.color : '#4f6ef7';
    return `\u0001AGENT|${color}|${name}\u0001`;
  });

  // 2. Extract fenced code blocks before escaping.
  const blocks = [];
  t = t.replace(/```(\w*)\n?([\s\S]*?)(?:```|$)/g, (m, lang, code) => {
    blocks.push({ lang, code });
    return `\u0001BLOCK${blocks.length - 1}\u0001`;
  });

  t = escapeHtml(t);

  // 3. Tables (ported parseTable logic).
  t = t.split(/\n/).reduce((acc, line, i, arr) => {
    const prev = acc.lines[acc.lines.length - 1];
    const isRow = line.includes('|') && line.trim() !== '';
    const isSep = /^\s*\|?\s*:?-{2,}.*\|\s*$/.test(line);
    if (isRow && !isSep && prev && prev.includes('|') && !acc.inTable) acc.inTable = true;
    acc.lines.push(line);
    return acc;
  }, { lines: [], inTable: false });
  t = t.lines.join('\n');
  t = parseTable(t);

  // 4. Block-level.
  t = t.replace(/^#### (.*)$/gm, '<h4>$1</h4>')
       .replace(/^### (.*)$/gm, '<h3>$1</h3>')
       .replace(/^## (.*)$/gm, '<h2>$1</h2>')
       .replace(/^# (.*)$/gm, '<h1>$1</h1>')
       .replace(/^&gt; (.*)$/gm, '<blockquote>$1</blockquote>')
       .replace(/^---+$/gm, '<hr>');
  // lists
  t = t.replace(/(?:^|\n)((?:\s*[-*] .*(?:\n|$))+)/g, (m, list) => {
    const items = list.trim().split(/\n/).map(l => '<li>' + l.replace(/^\s*[-*] /, '') + '</li>').join('');
    return '\n<ul>' + items + '</ul>';
  });
  t = t.replace(/(?:^|\n)((?:\s*\d+\. .*(?:\n|$))+)/g, (m, list) => {
    const items = list.trim().split(/\n/).map(l => '<li>' + l.replace(/^\s*\d+\. /, '') + '</li>').join('');
    return '\n<ol>' + items + '</ol>';
  });
  // paragraphs (single newlines → <br> outside blocks)
  t = t.split(/\n{2,}/).map(para => {
    const s = para.trim();
    if (!s) return '';
    if (/^<(h\d|ul|ol|blockquote|pre|hr|table|\u0001)/.test(s) || /^\u0001/.test(s)) return s;
    return '<p>' + s.replace(/\n/g, '<br>') + '</p>';
  }).join('\n');

  // 5. Inline.
  t = inlineFormat(t);

  // 6. Re-insert code blocks.
  t = t.replace(/\u0001BLOCK(\d+)\u0001/g, (m, i) => {
    const b = blocks[+i];
    const lang = b.lang || 'text';
    const id = 'code' + i + '-' + Math.floor(Math.random() * 1e6);
    return `<div class="code-head"><span>${escapeHtml(lang)}</span><button class="copy-btn" onclick="copyCode(this, '${id}')">Copy</button></div><pre><code id="${id}">${escapeHtml(b.code.replace(/\n$/, ''))}</code></pre>`;
  });

  // 7. Re-insert agent badges.
  t = t.replace(/\u0001AGENT\|([^|]+)\|([^\u0001]+)\u0001/g,
    (m, color, name) => `<div class="agent-badge"><span class="ab-ico" style="filter:none">${''}</span><span style="width:8px;height:8px;border-radius:50%;background:${color};display:inline-block"></span>${escapeHtml(name.trim())}</div>`);

  return t;
}

function inlineFormat(t) {
  t = t.replace(/`([^`\n]+)`/g, '<code class="inline">$1</code>');
  t = t.replace(/\*\*([^*\n]+)\*\*/g, '<strong>$1</strong>');
  t = t.replace(/(^|[\s(])\*([^*\n]+)\*(?=[\s).,!?:;]|$)/g, '$1<em>$2</em>');
  t = t.replace(/\[([^\]]+)\]\((https?:[^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>');
  return t;
}

function parseTable(t) {
  const r = t.split('\n').reduce((acc, line) => {
    const isSep = /^\s*\|?[\s:|-]+\|?\s*$/.test(line) && line.includes('-');
    const isRow = line.includes('|') && line.trim() !== '' && !isSep;
    if (isSep && acc.rows.length) {
      acc.rows.push('SEP');
    } else if (isRow) {
      acc.rows.push(line.split('|').map(c => c.trim()).filter((c, i, a) => !(c === '' && (i === 0 || i === a.length - 1)) || a.length <= 2));
    } else if (acc.rows.length) {
      acc.out.push(buildTable(acc.rows)); acc.rows = []; acc.out.push(line);
    } else {
      acc.out.push(line);
    }
    return acc;
  }, { rows: [], out: [] });
  if (r.rows.length) r.out.push(buildTable(r.rows));
  return r.out.join('\n');
}

function buildTable(rows) {
  if (!rows.length) return '';
  const header = rows[0];
  const bodyRows = rows.slice(1).filter(r => r !== 'SEP');
  if (!bodyRows.length && rows.length > 1) return rows.map(r => Array.isArray(r) ? r.join(' | ') : '').join('\n');
  const th = '<tr>' + header.map(c => '<th>' + c + '</th>').join('') + '</tr>';
  const tb = bodyRows.map(r => '<tr>' + r.map(c => '<td>' + c + '</td>').join('') + '</tr>').join('');
  return '<table>' + th + tb + '</table>';
}

function copyCode(btn, id) {
  const code = document.getElementById(id);
  if (!code) return;
  navigator.clipboard.writeText(code.textContent).then(() => {
    btn.textContent = 'Copied!';
    setTimeout(() => (btn.textContent = 'Copy'), 1400);
  });
}

/* ═══════════════════ SEND / STREAM ═══════════════════ */
function handleKeyDown(e) {
  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); }
}
function autoResize(el) { el.style.height = 'auto'; el.style.height = Math.min(el.scrollHeight, 120) + 'px'; }
function quickPrompt(text) { $('userInput').value = text; sendMessage(); }

async function sendMessage() {
  const input = $('userInput');
  const text = input.value.trim();
  if (!text || state.isStreaming) return;

  if (!state.workspace) { showWorkspacePrompt(); return; }
  if (!state.currentSessionId) newChat();

  const sessionId = state.currentSessionId;
  const msgs = currentMessages();
  msgs.push({ role: 'user', content: text });
  input.value = ''; input.style.height = 'auto';

  if (msgs.length === 1) {
    const s = state.sessions.find(s => s.id === sessionId);
    if (s) { s.title = text.slice(0, 42) + (text.length > 42 ? '…' : ''); s.updatedAt = new Date().toISOString(); }
  }
  saveSessions(); renderSessions(); updateStats();

  $('welcomeScreen').style.display = 'none';
  const container = $('messagesContainer');
  container.insertAdjacentHTML('beforeend', renderMessage(msgs[msgs.length - 1], msgs.length - 1));

  msgs.push({ role: 'assistant', content: '' });
  const ai = msgs.length - 1;
  container.insertAdjacentHTML('beforeend',
    `<div class="message-row assistant" id="msg-${ai}"><div class="message-avatar">H</div><div class="message-bubble"><div class="md"><span class="stream-cursor"></span></div></div></div>`);
  scrollToBottom();

  state.isStreaming = true; updateSendButton();
  showStatus('connecting', 'Connecting…');
  const ac = new AbortController();
  state.abortController = ac;

  try {
    const apiMsgs = msgs.slice(0, -1).map(m => ({ role: m.role, content: m.content }));
    const body = {
      model: state.settings.model || state.serverConfig?.model || 'hermes-rust',
      messages: apiMsgs,
      stream: true,
      hermes: {
        agents: state.agents.map(a => ({
          id: a.id, name: a.name, description: a.description || '',
          systemPrompt: a.systemPrompt || '', isMain: !!a.isMain,
        })),
      },
    };
    const headers = { 'Content-Type': 'application/json', 'X-Hermes-Session-Id': sessionId };
    if (state.settings.apiKey) headers['Authorization'] = 'Bearer ' + state.settings.apiKey;

    const resp = await fetch(api() + '/v1/chat/completions', {
      method: 'POST', headers, body: JSON.stringify(body), signal: ac.signal,
    });
    if (!resp.ok) {
      const errText = await resp.text();
      throw new Error(`API ${resp.status}: ${errText.slice(0, 300)}`);
    }
    showStatus('connected', 'Team is working…');

    const reader = resp.body.getReader();
    const decoder = new TextDecoder();
    let full = '', buf = '';
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buf += decoder.decode(value, { stream: true });
      const lines = buf.split('\n'); buf = lines.pop();
      for (const line of lines) {
        if (!line.startsWith('data: ')) continue;
        const data = line.slice(6).trim();
        if (data === '[DONE]') continue;
        try {
          const chunk = JSON.parse(data);
          const delta = chunk.choices?.[0]?.delta;
          if (delta?.content) {
            full += delta.content;
            updateAssistantMsg(ai, full);
          }
        } catch (e) {}
      }
    }

    const agentTags = [...full.matchAll(/<!--\s*agent:([\S]+?)\s*-->/g)].map(m => m[1]);
    const uniqueAgents = [...new Set(agentTags)];
    if (uniqueAgents.length > 0) {
      msgs[ai].agentIds = uniqueAgents;
      msgs[ai].agentId = uniqueAgents[0];
    }
    msgs[ai].content = full;
    saveSessions(); updateStats();
    resetAllAgentStatuses();
    state.completedTasks += Math.max(1, uniqueAgents.length);
    showStatus('connected', 'Done');
  } catch (err) {
    if (err.name === 'AbortError') {
      msgs[ai].content = msgs[ai].content || '(stopped)';
    } else {
      msgs[ai].content = `**Error:** ${escapeHtml(err.message)}`;
      showStatus('error', err.message.slice(0, 80));
    }
    saveSessions();
  }

  state.isStreaming = false; state.abortController = null;
  updateSendButton(); hideStatus();
  const el = $('msg-' + ai);
  if (el) el.querySelector('.message-bubble').innerHTML = '<div class="md">' + renderMarkdown(msgs[ai].content, false) + '</div>';
  scrollToBottom();
}

function updateAssistantMsg(i, content) {
  const el = $('msg-' + i);
  if (el) {
    el.querySelector('.message-bubble').innerHTML =
      '<div class="md">' + renderMarkdown(content, true) + '</div><span class="stream-cursor"></span>';
    scrollToBottom();
  }
  const tags = [...content.matchAll(/<!--\s*agent:([\S]+?)\s*-->/g)].map(m => m[1]);
  if (tags.length > 0) {
    resetAllAgentStatuses();
    const lastAgent = tags[tags.length - 1];
    const ag = state.agents.find(a => a.id === lastAgent || a.name === lastAgent);
    if (ag) {
      setAgentWorking(ag.id, Math.min(90, Math.round((content.length / 400) * 10)));
      showStatus('connected', `${ag.name} is working…`);
    }
    const pm = state.agents.find(a => a.isMain);
    if (pm && (!ag || pm.id !== ag.id)) setAgentWorking(pm.id, Math.min(95, tags.length * 18));
  }
}

function stopStreaming() { if (state.abortController) state.abortController.abort(); }
function updateSendButton() {
  const btn = $('sendBtn');
  if (state.isStreaming) {
    btn.className = 'send-btn stop-btn'; btn.onclick = stopStreaming;
    btn.innerHTML = '<svg viewBox="0 0 16 16" fill="none"><rect x="4" y="4" width="8" height="8" rx="1" fill="currentColor"/></svg>';
    btn.title = 'Stop';
  } else {
    btn.className = 'send-btn'; btn.onclick = sendMessage;
    btn.innerHTML = '<svg viewBox="0 0 16 16" fill="none"><path d="M3 13l10-5L3 3v4l6 1-6 1v4z" fill="currentColor"/></svg>';
    btn.title = 'Send';
  }
}

/* ═══════════════════ AGENTS DASHBOARD ═══════════════════ */
function setAgentWorking(agentId, progress) {
  const a = state.agents.find(x => x.id === agentId);
  if (a) { a._status = 'working'; a._progress = progress || 0; }
}
function resetAllAgentStatuses() {
  state.agents.forEach(a => { a._status = 'idle'; a._progress = 0; });
}

function renderAgents() {
  const total = state.agents.length;
  const working = state.agents.filter(a => a._status === 'working');
  const avg = working.length ? Math.round(working.reduce((s, a) => s + (a._progress || 0), 0) / working.length) : 0;
  $('agStatTotal').textContent = total;
  $('agStatWorking').textContent = working.length;
  $('agStatProgress').textContent = working.length ? avg + '%' : '—';
  $('agStatDone').textContent = state.completedTasks;
  $('navAgentCount').textContent = total;

  $('agentsGrid').innerHTML = state.agents.map(a => {
    const abbr = escapeHtml(a.abbr || (a.name || 'A').slice(0, 2).toUpperCase());
    const icon = a.icon ? `<span style="font-size:17px">${a.icon}</span>` : abbr;
    const skills = (a.skills || []).slice(0, 3).map(s => `<span class="agent-skill-tag">${escapeHtml(s)}</span>`).join('');
    const status = a._status || 'idle';
    const progress = a._progress || 0;
    const label = status === 'working' ? `Working ${progress}%` : status === 'waiting' ? 'Waiting' : 'Idle';
    const dot = status === 'working' ? 'working' : status === 'waiting' ? 'waiting' : '';
    const bar = status === 'working' ? `<div class="agent-progress-bar"><div class="agent-progress-fill" style="width:${progress}%"></div></div>` : '';
    return `
    <div class="agent-card" onclick="openAgentEditor('${a.id}')">
      <div class="agent-card-top">
        <div class="agent-avatar" style="background:${a.color || '#4f6ef7'}">${icon}</div>
        <div>
          <div class="agent-card-name">${escapeHtml(a.name)}</div>
          <div class="agent-card-desc">${escapeHtml(a.description || '')}</div>
        </div>
      </div>
      <div class="agent-status"><span class="agent-status-dot ${dot}"></span><span>${label}</span>${bar}</div>
      <div class="agent-skills-row">${skills}</div>
      <div class="agent-card-footer">
        <span>${escapeHtml(a.model || state.settings.model || 'default model')}</span>
        ${a.isMain ? '<span class="agent-card-badge main">MAIN · PM</span>' : a.isBuiltin ? '<span class="agent-card-badge">Built-in</span>' : ''}
      </div>
    </div>`;
  }).join('');
}

let editingAgentId = null;
function openAgentEditor(id) {
  editingAgentId = id || null;
  if (id) {
    const a = state.agents.find(x => x.id === id);
    if (!a) return;
    $('agentEditorTitle').textContent = 'Edit agent';
    $('agentName').value = a.name || '';
    $('agentAbbr').value = a.abbr || '';
    $('agentIcon').value = a.icon || '';
    $('agentColor').value = a.color || '#4f6ef7';
    $('agentDesc').value = a.description || '';
    $('agentPrompt').value = a.systemPrompt || '';
    $('agentSkills').value = (a.skills || []).join(', ');
    $('agentIsMain').checked = !!a.isMain;
    $('agentDeleteBtn').style.display = a.isBuiltin ? 'none' : 'inline-block';
  } else {
    $('agentEditorTitle').textContent = 'New agent';
    $('agentName').value = ''; $('agentAbbr').value = '';
    $('agentIcon').value = '🤖'; $('agentColor').value = '#4f6ef7';
    $('agentDesc').value = ''; $('agentPrompt').value = '';
    $('agentSkills').value = ''; $('agentIsMain').checked = false;
    $('agentDeleteBtn').style.display = 'none';
  }
  $('agentEditorOverlay').classList.add('visible');
}
function closeAgentEditor() { $('agentEditorOverlay').classList.remove('visible'); editingAgentId = null; }
function saveAgent() {
  const name = $('agentName').value.trim();
  if (!name) return toast('Name is required', 'err');
  const data = {
    name,
    abbr: $('agentAbbr').value.trim(),
    icon: $('agentIcon').value.trim() || '🤖',
    color: $('agentColor').value,
    description: $('agentDesc').value.trim(),
    systemPrompt: $('agentPrompt').value,
    skills: $('agentSkills').value.split(',').map(s => s.trim()).filter(Boolean),
    isMain: $('agentIsMain').checked,
    updatedAt: new Date().toISOString(),
  };
  if (data.isMain) state.agents.forEach(a => (a.isMain = false));
  if (editingAgentId) {
    const idx = state.agents.findIndex(a => a.id === editingAgentId);
    if (idx >= 0) Object.assign(state.agents[idx], data);
  } else {
    state.agents.push({ ...data, id: uid(), isBuiltin: false, createdAt: new Date().toISOString(), _status: 'idle', _progress: 0 });
  }
  saveAgents(); renderAgents(); closeAgentEditor();
  toast('Agent saved — applied live', 'ok');
}
function deleteAgent() {
  if (!editingAgentId) return;
  state.agents = state.agents.filter(a => a.id !== editingAgentId);
  if (!state.agents.some(a => a.isMain) && state.agents.length) state.agents[0].isMain = true;
  saveAgents(); renderAgents(); closeAgentEditor();
  toast('Agent deleted');
}

/* ═══════════════════ SKILL STORE ═══════════════════ */
function renderSkills() {
  const q = ($('skillSearch')?.value || '').toLowerCase();
  const cat = $('skillCategory')?.value || '';
  const list = state.skills.filter(s =>
    (!cat || s.cat === cat) &&
    (!q || (s.name || '').toLowerCase().includes(q) || (s.desc || '').toLowerCase().includes(q))
  );
  $('skillsGrid').innerHTML = list.length ? list.map(s => {
    const installed = state.installedSkills.includes(s.name);
    return `
    <div class="skill-card">
      <div class="skill-top">
        <div class="skill-ico">${s.icon || '⚡'}</div>
        <div class="skill-name">${escapeHtml(s.name)}</div>
      </div>
      <div class="skill-desc">${escapeHtml(s.desc || '')}</div>
      <div class="skill-foot">
        <span class="skill-cat">${escapeHtml(s.cat || 'general')}</span>
        <button class="skill-install ${installed ? 'installed' : ''}" onclick="installSkill('${escapeHtml(s.name)}', this)">
          ${installed ? 'Installed ✓' : '+ Install'}
        </button>
      </div>
    </div>`;
  }).join('') : '<div class="skills-empty">No skills match your search.</div>';
}

function installSkill(name, btn) {
  if (state.installedSkills.includes(name)) return;
  btn.classList.add('loading');
  setTimeout(() => {
    state.installedSkills.push(name);
    localStorage.setItem('hermes-installed-skills', JSON.stringify(state.installedSkills));
    renderSkills();
    toast(`Skill "${name}" installed — mention it in chat to use it`, 'ok');
  }, 450);
}

/* ═══════════════════ SETTINGS ═══════════════════ */
function fillSettings() {
  const c = state.serverConfig;
  if (!c) return;
  $('setEndpoint').value = c.endpoint || '';
  $('setModel').value = c.model || '';
  $('setModels').value = (c.models || []).join(', ');
  $('setApiKey').value = '';
  $('setApiKey').placeholder = c.has_api_key ? '•••••••• (a key is set — leave empty to keep)' : 'sk-…';
  $('setLocalEndpoint').value = state.settings.endpoint || '';
}

async function saveServerConfig() {
  const patch = {};
  const ep = $('setEndpoint').value.trim();
  const model = $('setModel').value.trim();
  const models = $('setModels').value.split(',').map(s => s.trim()).filter(Boolean);
  const key = $('setApiKey').value.trim();
  if (ep) patch.endpoint = ep;
  if (model) patch.model = model;
  if (models.length) patch.models = models;
  if (key) patch.api_key = key;
  try {
    const r = await fetch(api() + '/api/config', {
      method: 'PATCH', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(patch),
    });
    const d = await r.json();
    if (!d.ok) throw new Error(d.error || 'save failed');
    state.serverConfig = { ...state.serverConfig, ...d, endpoint: d.endpoint, model: d.model, has_api_key: d.has_api_key };
    if (!state.settings.model) { state.settings.model = d.model; saveSettings(); }
    fillSettings(); updateModelUI(); updateStats();
    toast('Saved — applied live on the server', 'ok');
  } catch (e) {
    toast('Save failed: ' + e.message, 'err');
  }
}

function saveLocalOverride() {
  const v = $('setLocalEndpoint').value.trim();
  state.settings.endpoint = v;
  saveSettings();
  toast(v ? 'UI will use ' + v : 'UI will use this server', 'ok');
  setTimeout(() => location.reload(), 600);
}

function clearLocalData() {
  if (!confirm('Delete all local data (sessions, agents, settings, workspace)?')) return;
  ['hermes-settings', 'hermes-sessions', 'hermes-agents', 'hermes-workspace', 'hermes-workspaces', 'hermes-installed-skills'].forEach(k => localStorage.removeItem(k));
  location.reload();
}

async function checkConnection() {
  try {
    const r = await fetch(api() + '/health', { signal: AbortSignal.timeout(3000) });
    if (r.ok) {
      $('connDot').className = 'conn-dot online';
      $('connText').textContent = 'Server online';
      fillSettings();
      try {
        const mr = await fetch(api() + '/v1/models');
        const md = await mr.json();
        const ids = (md.data || []).map(m => m.id).filter(Boolean);
        if (state.serverConfig && ids.length) state.serverConfig.models = ids;
        updateModelUI();
      } catch (e) {}
      return;
    }
    throw new Error('bad status');
  } catch (e) {
    $('connDot').className = 'conn-dot offline';
    $('connText').textContent = 'Server unreachable';
  }
}

/* ═══════════════════ MODEL SWITCHER ═══════════════════ */
function updateModelUI() {
  updateStats();
  renderModelDropdown();
}
function toggleModelDropdown(e) {
  e.stopPropagation();
  $('modelDropdown').classList.toggle('visible');
  renderModelDropdown();
}
function closeModelDropdown() { $('modelDropdown').classList.remove('visible'); }
document.addEventListener('click', (e) => {
  if (!e.target.closest('.model-switch')) closeModelDropdown();
  if (!e.target.closest('#workspaceMenu') && !e.target.closest('.workspace-btn')) closeWorkspaceMenu();
});

function renderModelDropdown() {
  const models = state.serverConfig?.models || [state.settings.model].filter(Boolean);
  const current = state.settings.model || models[0] || '';
  $('modelDropdown').innerHTML =
    models.map(m => `
      <button class="model-option ${m === current ? 'active' : ''}" onclick="selectModel('${escapeHtml(m)}')">
        <span>${escapeHtml(m)}</span>${m === current ? '<span>✓</span>' : ''}
      </button>`).join('') +
    `<div class="model-add-row">
      <input id="customModelInput" placeholder="custom model id…" onkeydown="if(event.key==='Enter'){event.preventDefault();addCustomModel();}">
      <button class="primary-btn" style="padding:6px 12px" onclick="addCustomModel()">Add</button>
    </div>`;
}
function selectModel(name) {
  state.settings.model = name;
  saveSettings(); updateStats(); closeModelDropdown();
  toast('Model switched to ' + name);
}
function addCustomModel() {
  const v = ($('customModelInput')?.value || '').trim();
  if (!v) return;
  if (state.serverConfig && !state.serverConfig.models.includes(v)) state.serverConfig.models.push(v);
  selectModel(v);
}

/* ═══════════════════ WORKSPACE ═══════════════════ */
function toggleWorkspaceMenu(e) {
  e.stopPropagation();
  const menu = $('workspaceMenu');
  if (menu.classList.contains('visible')) return closeWorkspaceMenu();
  const rect = e.currentTarget.getBoundingClientRect();
  menu.style.top = rect.bottom + 8 + 'px';
  menu.style.left = Math.max(10, rect.right - 340) + 'px';
  menu.classList.add('visible');
  browseWorkspace(state.workspace || '~');
}
function closeWorkspaceMenu() { $('workspaceMenu').classList.remove('visible'); }

async function browseWorkspace(path) {
  try {
    const r = await fetch(api() + '/api/list-folder?path=' + encodeURIComponent(path));
    const d = await r.json();
    if (d.error) return toast(d.error, 'err');
    $('wsCurrent').textContent = d.current;
    $('wsCurrent').dataset.path = d.current;
    $('wsDirs').innerHTML = d.dirs.length
      ? d.dirs.map(x => `<button class="ws-dir" onclick="browseWorkspace('${escapeHtml(x.path).replace(/'/g, "\\'")}')"><span class="ico">📁</span><span>${escapeHtml(x.name)}</span></button>`).join('')
      : '<div class="ws-empty">No sub-folder here.</div>';
  } catch (e) { toast('Browse failed: ' + e.message, 'err'); }
}
function wsGoUp() {
  const cur = $('wsCurrent').dataset.path || '/';
  const parent = cur.replace(/[\\/][^\\/]+[\\/]?$/, '') || '/';
  browseWorkspace(parent);
}
function wsSelectCurrent() {
  selectWorkspace($('wsCurrent').dataset.path || '');
  closeWorkspaceMenu();
}
function selectWorkspace(path) {
  state.workspace = path;
  localStorage.setItem('hermes-workspace', path);
  const recents = JSON.parse(localStorage.getItem('hermes-workspaces') || '[]').filter(p => p !== path);
  recents.unshift(path);
  localStorage.setItem('hermes-workspaces', JSON.stringify(recents.slice(0, 5)));
  updateStats();
  toast('Workspace: ' + path, 'ok');
}

async function openNativeFolderPicker() {
  try {
    const r = await fetch(api() + '/api/choose-folder', { method: 'POST' });
    const d = await r.json();
    if (d.folder) {
      selectWorkspace(d.folder);
      closeWorkspaceMenu();
      skipWorkspacePrompt();
    } else {
      toast('Native picker unavailable — type a path instead');
    }
  } catch (e) {
    toast('Picker failed: ' + e.message, 'err');
  }
}

function showWorkspacePrompt() { $('workspacePrompt').classList.add('visible'); }
function skipWorkspacePrompt() {
  $('workspacePrompt').classList.remove('visible');
  if (!state.workspace) { state.workspace = '~'; updateStats(); }
}
async function chooseWorkspaceFromPrompt() {
  const v = $('workspaceInput').value.trim();
  if (!v) return toast('Enter a path or click Browse', 'err');
  selectWorkspace(v);
  skipWorkspacePrompt();
}

/* ═══════════════════ QUIT ═══════════════════ */
async function quitApp() {
  if (!confirm('Stop the HermesRust server and close the app?')) return;
  try { await fetch(api() + '/api/quit', { method: 'POST' }); } catch (e) {}
  document.body.innerHTML = '<div style="display:flex;height:100vh;align-items:center;justify-content:center;font-family:sans-serif;color:#6e6e73;flex-direction:column;gap:8px"><b style="color:#1d1d1f;font-size:18px">HermesRust stopped</b><span>You can close this window.</span></div>';
}

boot();
