// HermesRust — config.rs
// Application configuration with live hot-reload.
// The file `config/config.toml` is watched on disk: any edit (by hand or by
// an AI agent) is picked up within ~500ms, without restarting the app.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    /// OpenAI-compatible base URL of the upstream LLM provider.
    /// Example: https://dashscope.aliyuncs.com/compatible-mode/v1
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    /// Extra models shown in the in-app model switcher.
    #[serde(default)]
    pub models: Vec<String>,
    /// Inject the multi-agent Project Manager system prompt.
    /// None = automatic (enabled when agents are defined).
    #[serde(default)]
    pub orchestrator: Option<bool>,
    /// Relative or absolute path of the UI folder (live-editable).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub ui_dir: String,
    /// Open the app window on startup (Edge/Chrome app-mode, else browser).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_window: Option<bool>,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub window_width: u32,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub window_height: u32,
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

impl AppConfig {
    /// Fill empty fields with defaults, then environment-variable fallbacks
    /// (mirrors the behaviour of the original Python app).
    pub fn resolved(&self) -> AppConfig {
        let mut c = self.clone();
        if c.host.is_empty() {
            c.host = "127.0.0.1".into();
        }
        if c.port == 0 {
            c.port = 8642;
        }
        if c.endpoint.is_empty() {
            c.endpoint = std::env::var("BASE_URL").unwrap_or_default();
        }
        if c.api_key.is_empty() {
            for k in ["HERMES_API_KEY", "DASHSCOPE_API_KEY", "OPENAI_API_KEY", "ANTHROPIC_API_KEY"] {
                if let Ok(v) = std::env::var(k) {
                    if !v.is_empty() {
                        c.api_key = v;
                        break;
                    }
                }
            }
        }
        if c.model.is_empty() {
            c.model = std::env::var("MODEL").unwrap_or_else(|_| "hermes-rust".into());
        }
        if c.models.is_empty() {
            c.models = vec![c.model.clone()];
        } else if !c.models.iter().any(|m| m == &c.model) {
            c.models.insert(0, c.model.clone());
        }
        if c.ui_dir.is_empty() {
            c.ui_dir = "ui".into();
        }
        if c.window_width == 0 {
            c.window_width = 1280;
        }
        if c.window_height == 0 {
            c.window_height = 860;
        }
        c
    }

    /// URL of the upstream /chat/completions endpoint.
    pub fn chat_url(&self) -> String {
        let base = self.endpoint.trim_end_matches('/');
        if base.ends_with("/chat/completions") {
            base.to_string()
        } else {
            format!("{base}/chat/completions")
        }
    }

    /// URL of the upstream /models endpoint.
    #[allow(dead_code)]
    pub fn models_url(&self) -> String {
        let base = self.endpoint.trim_end_matches('/');
        if base.ends_with("/chat/completions") {
            base.replace("/chat/completions", "/models")
        } else if base.ends_with("/models") {
            base.to_string()
        } else {
            format!("{base}/models")
        }
    }
}

pub fn load_toml(path: &Path) -> Result<AppConfig, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    toml::from_str(&text).map_err(|e| format!("invalid TOML in {}: {e}", path.display()))
}

pub fn write_toml(path: &Path, cfg: &AppConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    // Write through a temp file so the mtime watcher sees a complete file.
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, &text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Resolve the base directory that contains `config/` and `ui/`.
/// Priority: CLI override > folder next to the executable > current dir.
pub fn resolve_base_dir(cli_override: Option<PathBuf>) -> PathBuf {
    if let Some(p) = cli_override {
        return p;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if parent.join("config").is_dir() || parent.join("ui").is_dir() {
                return parent.to_path_buf();
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn expand_home(p: &str) -> PathBuf {
    if p == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    }
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(p)
}
