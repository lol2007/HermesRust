// HermesRust — server.rs
// The embedded HTTP server (axum). Serves the live-editable `ui/` folder and
// exposes the same API surface as the original Python app — plus live config
// endpoints so the provider can be changed at runtime without a restart.

use axum::extract::{Request, State};
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use std::sync::Arc;

use crate::config::VERSION;
use crate::folders;
use crate::proxy;
use crate::state::AppState;

pub fn build_router(app: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/health", get(health))
        .route("/v1/health", get(health))
        .route("/v1/models", get(models))
        .route("/v1/chat/completions", post(proxy::chat_completions))
        .route("/api/agents", get(agents))
        .route("/api/skills", get(skills))
        .route("/api/config", get(get_config).patch(patch_config))
        .route("/api/choose-folder", post(folders::choose_folder))
        .route("/api/list-folder", get(folders::list_folder))
        .route("/api/quit", post(quit))
        .fallback(static_handler)
        .layer(middleware::from_fn(cors_middleware))
        .with_state(app)
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

async fn cors_middleware(req: Request, next: Next) -> Response {
    if req.method() == Method::OPTIONS {
        return cors_response(StatusCode::NO_CONTENT, axum::body::Body::empty());
    }
    let mut res = next.run(req).await;
    add_cors(res.headers_mut());
    // Never cache: every refresh of the window reflects on-disk edits.
    res.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    res
}

fn add_cors(h: &mut axum::http::HeaderMap) {
    h.insert("Access-Control-Allow-Origin", HeaderValue::from_static("*"));
    h.insert(
        "Access-Control-Allow-Methods",
        HeaderValue::from_static("GET, POST, PATCH, OPTIONS, DELETE"),
    );
    h.insert(
        "Access-Control-Allow-Headers",
        HeaderValue::from_static("Content-Type, Authorization, X-Hermes-Session-Id"),
    );
    h.insert(
        "Access-Control-Expose-Headers",
        HeaderValue::from_static("X-Hermes-Session-Id"),
    );
}

fn cors_response(status: StatusCode, body: axum::body::Body) -> Response {
    let mut res = Response::new(body);
    *res.status_mut() = status;
    add_cors(res.headers_mut());
    res
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn index_handler(State(app): State<Arc<AppState>>) -> Response {
    serve_file(&app, "index.html").await
}

/// Fallback: serve any static file from the live UI folder.
async fn static_handler(State(app): State<Arc<AppState>>, req: Request) -> Response {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return (StatusCode::METHOD_NOT_ALLOWED, "method not allowed").into_response();
    }
    let path = req
        .uri()
        .path()
        .trim_start_matches('/')
        .trim_start_matches("ui/");
    let path = if path.is_empty() { "index.html" } else { path };
    serve_file(&app, path).await
}

async fn serve_file(app: &AppState, rel: &str) -> Response {
    // Reject traversal attempts.
    if rel.split('/').any(|seg| seg == ".." || seg.is_empty() && rel != "") {
        return (StatusCode::BAD_REQUEST, "bad path").into_response();
    }
    let ui_dir = {
        let cfg = app.config.read().unwrap().resolved();
        crate::config::expand_home(&cfg.ui_dir)
    };
    let ui_dir = if ui_dir.is_absolute() {
        ui_dir
    } else {
        app.base_dir.join(ui_dir)
    };
    let full = ui_dir.join(rel);

    match tokio::fs::read(&full).await {
        Ok(bytes) => {
            let mime = mime_of(rel);
            let mut res = Response::new(axum::body::Body::from(bytes));
            res.headers_mut().insert(header::CONTENT_TYPE, mime);
            res
        }
        Err(_) => (
            StatusCode::NOT_FOUND,
            format!(
                "Not found: {}\nThe UI is served live from '{}' — check that the file exists.",
                rel,
                ui_dir.display()
            ),
        )
            .into_response(),
    }
}

fn mime_of(path: &str) -> HeaderValue {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let s = match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" | "md" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    };
    HeaderValue::from_static(s)
}

async fn health() -> impl IntoResponse {
    Json(json!({"status": "ok", "platform": "hermes-rust", "version": VERSION}))
}

async fn models(State(app): State<Arc<AppState>>) -> impl IntoResponse {
    let cfg = app.config.read().unwrap().resolved();
    let created = proxy::unix_now();
    let mut ids = vec![cfg.model.clone()];
    for m in &cfg.models {
        if !ids.contains(m) {
            ids.push(m.clone());
        }
    }
    Json(json!({
        "object": "list",
        "data": ids.into_iter().map(|id| json!({
            "id": id, "object": "model", "created": created, "owned_by": "hermes-rust"
        })).collect::<Vec<_>>(),
    }))
}

async fn agents(State(app): State<Arc<AppState>>) -> impl IntoResponse {
    let agents = app.agents.read().unwrap().clone();
    Json(json!({"agents": agents, "count": agents.len()}))
}

async fn skills(State(app): State<Arc<AppState>>) -> impl IntoResponse {
    let skills = app.skills.read().unwrap().clone();
    Json(skills)
}

/// Safe view of the config for the Settings panel (never returns the key).
async fn get_config(State(app): State<Arc<AppState>>) -> impl IntoResponse {
    let cfg = app.config.read().unwrap().resolved();
    Json(json!({
        "version": VERSION,
        "host": cfg.host,
        "port": cfg.port,
        "endpoint": cfg.endpoint,
        "has_api_key": !cfg.api_key.is_empty(),
        "model": cfg.model,
        "models": cfg.models,
        "orchestrator": cfg.orchestrator.unwrap_or(true),
        "open_window": cfg.open_window.unwrap_or(true),
        "uptime_secs": app.start.elapsed().as_secs(),
        "config_path": app.config_path.display().to_string(),
    }))
}

#[derive(serde::Deserialize)]
struct ConfigPatch {
    endpoint: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
    models: Option<Vec<String>>,
    orchestrator: Option<bool>,
}

/// PATCH /api/config — edit the provider live (rewrites config/config.toml;
/// the watcher reloads it instantly). No restart required.
async fn patch_config(
    State(app): State<Arc<AppState>>,
    Json(patch): Json<ConfigPatch>,
) -> Response {
    let mut cfg = app.config.read().unwrap().clone();
    if let Some(v) = patch.endpoint {
        cfg.endpoint = v.trim().to_string();
    }
    if let Some(v) = patch.api_key {
        cfg.api_key = v.trim().to_string();
    }
    if let Some(v) = patch.model {
        cfg.model = v.trim().to_string();
    }
    if let Some(v) = patch.models {
        cfg.models = v;
    }
    if let Some(v) = patch.orchestrator {
        cfg.orchestrator = Some(v);
    }
    match crate::config::write_toml(&app.config_path, &cfg) {
        Ok(_) => {
            *app.config.write().unwrap() = cfg.clone();
            let r = cfg.resolved();
            Json(json!({
                "ok": true,
                "saved_to": app.config_path.display().to_string(),
                "endpoint": r.endpoint,
                "model": r.model,
                "has_api_key": !r.api_key.is_empty(),
            }))
            .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "error": e})),
        )
            .into_response(),
    }
}

async fn quit() -> impl IntoResponse {
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        std::process::exit(0);
    });
    Json(json!({"ok": true, "bye": true}))
}
