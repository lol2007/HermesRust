# STRUCTURE.md — Project Map for Humans & AI Assistants

> **You (AI assistant) are reading the right file.** This document maps every
> file in HermesRust, explains what it does, and gives step-by-step recipes for
> the most common changes (new tab, new theme, new agent, new API endpoint…).
>
> **Golden rule:** `ui/` and `config/` are plain files. Changes there need
> **no recompile** — UI changes apply on refresh (F5), config changes apply
> automatically within 0.5 s. Only `src/*.rs` changes require `cargo build`.

---

## 1. Repository layout

```
HermesRust/
│
├── Cargo.toml                  # Rust manifest — deps, release profile (LTO, opt-level "z")
├── README.md                   # project readme (English)
├── STRUCTURE.md                # ← this file
├── LICENSE                     # MIT
├── .gitignore
│
├── src/                        # █████ BACKEND — 100% Rust ████
│   ├── main.rs                 # entry: CLI args (--port/--host/--no-window/--dir),
│   │                           #   banner, server start, window launch
│   ├── config.rs               # AppConfig (serde/TOML), defaults, env fallbacks,
│   │                           #   load_toml/write_toml, mtime(), resolve_base_dir()
│   ├── state.rs                # AppState (config+agents+skills in RwLocks),
│   │                           #   spawn_watcher(): 500 ms mtime poll → hot reload
│   ├── server.rs               # axum router: all routes, CORS middleware,
│   │                           #   static UI serving (no-store), /api/config PATCH
│   ├── proxy.rs                # POST /v1/chat/completions:
│   │                           #   orchestrator injection + provider call +
│   │                           #   SSE re-framing (find_event_boundary/sse_reframe)
│   ├── orchestrator.rs         # AgentDef struct, DEFAULT_AGENTS_JSON (embedded
│   │                           #   fallback), build_orchestrator_prompt() (PM prompt)
│   ├── folders.rs              # /api/list-folder (pure Rust) +
│   │                           #   /api/choose-folder (PowerShell/osascript/zenity)
│   └── window.rs               # open_app_window(): Edge → Chrome → Chromium →
│                               #   default browser, all with --app=<url>
│
├── ui/                         # █████ FRONTEND — plain files, live-served ████
│   ├── index.html              # full layout: sidebar, topbar, 4 views
│   │                           #   (chat / agents / skills / settings) + modals
│   ├── app.css                 # design system (CSS vars in :root, Apple-style)
│   └── app.js                  # all logic: state, SSE streaming, markdown renderer,
│                               #   agents CRUD, skill store, settings, workspace
│
├── config/                     # █████ LIVE CONFIG — hot-reloaded ≤ 0.5 s ████
│   ├── config.toml             # provider endpoint, api_key, model(s), port,
│   │                           #   orchestrator on/off, window size, ui_dir
│   ├── agents.json             # the 20-agent team (id/name/icon/color/skills/
│   │                           #   systemPrompt/isMain)
│   └── skills.json             # Skill Store catalog (name/desc/icon/cat)
│
├── tests/
│   ├── mock_upstream.py        # fake OpenAI-compatible provider (SSE + split frames)
│   └── run_tests.sh            # 20 functional checks (proxy, hot reload, UI serving)
│
├── scripts/
│   ├── build-windows.ps1       # local Windows build → hermes-rust.exe + zip
│   └── dev.sh                  # dev runner (--test runs the suite)
│
└── .github/workflows/
    └── release.yml             # tag push (v*) → Windows/Linux/macOS bundles
                                #   attached to the GitHub Release
```

---

## 2. Where things live (by topic)

| I want to change… | File | Recompile? |
|---|---|---|
| Colors, fonts, spacing, animations | `ui/app.css` (`:root` variables at top) | ❌ F5 |
| Layout, new tab/section, texts | `ui/index.html` + `ui/app.js` | ❌ F5 |
| Chat behavior, streaming, markdown | `ui/app.js` | ❌ F5 |
| Agents (names, prompts, colors) | `config/agents.json` | ❌ auto |
| Add an agent | `config/agents.json` (append one object) | ❌ auto |
| Skills catalog | `config/skills.json` | ❌ auto |
| Provider endpoint / API key / model | `config/config.toml` or in-app Settings | ❌ auto |
| Port, window size, bind host | `config/config.toml` (port needs app restart to rebind) | restart only |
| PM orchestration prompt | `src/orchestrator.rs` → `build_orchestrator_prompt()` | ✅ cargo build |
| API routes / backend endpoints | `src/server.rs` | ✅ cargo build |
| Proxy behavior (streaming, params) | `src/proxy.rs` | ✅ cargo build |
| Window behavior / size flags | `src/window.rs`, `config/config.toml` | ✅ / ❌ |
| Release pipeline | `.github/workflows/release.yml` | ❌ (CI) |

---

## 3. Recipes

### 3.1 Add a new agent (30 seconds, no recompile)

Append to `config/agents.json`:

```json
{
  "id": "agent-rust",
  "name": "Rust Engineer",
  "abbr": "RST",
  "description": "Rust, tokio, axum — systems programming",
  "icon": "🦀",
  "color": "#dea584",
  "skills": ["Rust", "tokio", "axum"],
  "systemPrompt": "You are a senior Rust engineer…",
  "isMain": false,
  "isBuiltin": false
}
```

Rules: `id` must be unique (`agent-*` by convention); exactly one agent has
`"isMain": true` (the PM orchestrator); save → applied within 0.5 s. The UI's
Agents view also supports full CRUD (stored in the browser's localStorage).

### 3.2 Add a new tab / view in the UI (4 steps, no recompile)

1. **`ui/index.html`** — add a nav button in `<nav class="sidebar-nav">`:
   `<button class="nav-item" data-view="myview" onclick="showView('myview')"><span class="nav-ico">🧩</span> My View</button>`
2. Still in `index.html` — add the section next to the other views inside `<main>`:
   `<section id="view-myview" class="view"> … </section>`
3. **`ui/app.js`** — add the title to `VIEW_TITLES = { …, myview: 'My View' }` and
   any logic you need (the `showView()` switch already handles any id).
4. **`ui/app.css`** — style it (reuse `.settings-card`, `.agents-grid`, etc.).

Refresh the window (F5 or the sidebar *Reload UI* link). Done.

### 3.3 Add a theme (no recompile)

All colors are CSS variables in `ui/app.css` → `:root`. For a dark theme,
duplicate the `:root` block under `html[data-theme="dark"] { … }`, set the
variables, then toggle `document.documentElement.dataset.theme` in `app.js`
(persist with `localStorage`). Put theme files in `ui/themes/*.css` and link
them from `index.html` — they are served live like everything else in `ui/`.

### 3.4 Change the orchestration behavior (recompile)

The PM system prompt is built in `src/orchestrator.rs` →
`build_orchestrator_prompt()`. It assembles: workflow rules, the `<!-- agent:ID -->`
marker contract, and the team roster. Edit there, then `cargo build --release`.
To disable orchestration entirely: `config.toml` → `orchestrator = false`
(raw single-agent chat, applied live).

### 3.5 Add a backend API endpoint (recompile)

1. Write the handler in `src/server.rs` (copy the shape of `skills()`):
   `async fn my_handler(State(app): State<Arc<AppState>>) -> impl IntoResponse { … }`
2. Register it: `.route("/api/my-endpoint", get(my_handler))`
3. Call it from `ui/app.js`: `fetch(api() + '/api/my-endpoint')`.
4. `cargo build --release`.

### 3.6 Change the app branding / window title

- Window/tab title: `<title>` in `ui/index.html`.
- Sidebar name: `.logo-name` block in `ui/index.html`.
- Startup banner: `src/main.rs`.
- Process name: `[[bin]] name` in `Cargo.toml` (recompile).

### 3.7 Point at a local model (Ollama etc.)

`config/config.toml`:

```toml
endpoint = "http://127.0.0.1:11434/v1"
api_key = ""                      # Ollama ignores it
model = "llama3.1"
```

### 3.8 Run tests

```bash
cargo build --release
python3 tests/mock_upstream.py &           # fake provider on :9999
./target/release/hermes-rust --no-window & # app on :8642
# point config at the mock: endpoint = "http://127.0.0.1:9999/v1"
bash tests/run_tests.sh
```

---

## 4. Data flow cheat-sheet

```
ui/app.js  sendMessage()
  └─ POST /v1/chat/completions
        body: { model, messages[], stream:true,
                hermes: { agents: [{id,name,description,systemPrompt,isMain}] } }
     └─ src/proxy.rs
          ├─ builds PM system prompt (hermes.agents if sent, else config/agents.json)
          ├─ strips `hermes` + unknown keys, forwards known OpenAI params
          ├─ POSTs to {endpoint}/chat/completions (Bearer from config.toml)
          └─ re-frames upstream SSE into complete events → browser
               └─ app.js parses `data:` chunks, renders markdown + agent badges
```

Hot-reload watcher: `src/state.rs` polls mtimes of `config/*` every 500 ms and
swaps the RwLock contents — requests always read the freshest values.

Static UI: `src/server.rs` `static_handler` serves **any** file under `ui/`
(`Cache-Control: no-store`) — that's why F5 is enough after edits.

---

## 5. Notes & gotchas

- **Config file gets rewritten by the Settings panel** — `PATCH /api/config`
  re-serializes `config/config.toml`, so manual comments in the file are lost
  when you save from the UI. Structure/keys are preserved.
- **Port change** in `config.toml` requires an app restart (the socket is bound
  once). Everything else is hot.
- The window is your browser in *app mode* — closing that window does not stop
  the server (multiple windows may share the browser process). Use
  **Quit** in the sidebar (POST `/api/quit`) or Ctrl+C in the terminal.
- `config/agents.json` is the **shared** team source; the UI seeds its
  localStorage from it once. To force a re-sync from the file, clear
  `hermes-agents` in localStorage (Settings → *Clear local data*).
- IDs: agent ids are used in `<!-- agent:ID -->` markers — keep them stable if
  you have old conversations.
