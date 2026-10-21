// HermesRust — state.rs
// Shared application state + the live hot-reload watcher.
//
// Everything the user (or an AI assistant) may want to tweak lives in plain
// files next to the binary:
//   config/config.toml  — provider, model, port…
//   config/agents.json  — the 20-agent team (add/edit/remove agents live)
//   config/skills.json  — the Skill Store catalog
//   ui/*                — the whole front-end (HTML/CSS/JS)
// A background thread polls mtimes every 500 ms and reloads on change.

use crate::config::{load_toml, AppConfig};
use crate::orchestrator::{load_agents_file, AgentDef, DEFAULT_AGENTS_JSON};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

pub struct AppState {
    pub config: RwLock<AppConfig>,
    pub agents: RwLock<Vec<AgentDef>>,
    pub skills: RwLock<serde_json::Value>,
    pub base_dir: PathBuf,
    pub config_path: PathBuf,
    pub agents_path: PathBuf,
    pub skills_path: PathBuf,
    pub http: reqwest::Client,
    pub start: Instant,
    pub reload_count: Arc<AtomicU64>,
}

impl AppState {
    pub fn new(base_dir: PathBuf) -> Arc<Self> {
        let config_path = base_dir.join("config/config.toml");
        let agents_path = base_dir.join("config/agents.json");
        let skills_path = base_dir.join("config/skills.json");

        let cfg = load_toml(&config_path).unwrap_or_default();
        let agents = load_agents_file(&agents_path).unwrap_or_else(|e| {
            eprintln!("[hermes] agents.json: {e} — using embedded default team");
            serde_json::from_str(DEFAULT_AGENTS_JSON).expect("embedded agents must parse")
        });
        let skills = load_skills_file(&skills_path);

        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(15))
            .pool_idle_timeout(Duration::from_secs(90))
            .user_agent(concat!("HermesRust/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build HTTP client");

        Arc::new(AppState {
            config: RwLock::new(cfg),
            agents: RwLock::new(agents),
            skills: RwLock::new(skills),
            base_dir,
            config_path,
            agents_path,
            skills_path,
            http,
            start: Instant::now(),
            reload_count: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Spawn the mtime watcher: reloads config/agents/skills within 500 ms.
    pub fn spawn_watcher(self: &Arc<Self>) {
        let st = Arc::clone(self);
        std::thread::spawn(move || {
            let mut mt_cfg = crate::config::mtime(&st.config_path);
            let mut mt_ag = crate::config::mtime(&st.agents_path);
            let mut mt_sk = crate::config::mtime(&st.skills_path);
            loop {
                std::thread::sleep(Duration::from_millis(500));
                if crate::config::mtime(&st.config_path) != mt_cfg {
                    mt_cfg = crate::config::mtime(&st.config_path);
                    match load_toml(&st.config_path) {
                        Ok(c) => {
                            *st.config.write().unwrap() = c;
                            st.bump("config.toml");
                        }
                        Err(e) => eprintln!("[hermes] reload config failed: {e}"),
                    }
                }
                if crate::config::mtime(&st.agents_path) != mt_ag {
                    mt_ag = crate::config::mtime(&st.agents_path);
                    match load_agents_file(&st.agents_path) {
                        Ok(a) => {
                            *st.agents.write().unwrap() = a;
                            st.bump("agents.json");
                        }
                        Err(e) => eprintln!("[hermes] reload agents failed: {e}"),
                    }
                }
                if crate::config::mtime(&st.skills_path) != mt_sk {
                    mt_sk = crate::config::mtime(&st.skills_path);
                    *st.skills.write().unwrap() = load_skills_file(&st.skills_path);
                    st.bump("skills.json");
                }
            }
        });
    }

    fn bump(&self, what: &str) {
        self.reload_count.fetch_add(1, Ordering::Relaxed);
        eprintln!(
            "[hermes] live-reloaded {} (reload #{})",
            what,
            self.reload_count.load(Ordering::Relaxed)
        );
    }
}

fn load_skills_file(path: &PathBuf) -> serde_json::Value {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            eprintln!("[hermes] skills.json invalid: {e}");
            serde_json::json!([])
        }),
        Err(_) => serde_json::json!([]),
    }
}
