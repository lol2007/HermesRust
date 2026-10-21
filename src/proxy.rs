// HermesRust — proxy.rs
// POST /v1/chat/completions — OpenAI-compatible endpoint.
//
// Responsibilities (all in Rust, replacing the Python aiohttp + AIAgent glue):
//   • Inject the PM orchestrator system prompt (built from the live agent
//     registry sent by the UI, or from config/agents.json).
//   • Forward the request to any OpenAI-compatible provider.
//   • Stream the upstream SSE answer back token-by-token (reframed on event
//     boundaries so the browser reader never sees split frames).

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::stream::StreamExt;
use futures_util::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::orchestrator::{build_orchestrator_prompt, AgentDef};
use crate::state::AppState;

/// Agent definition sent by the UI with each request (UI-side CRUD support).
#[derive(Debug, Deserialize)]
struct ClientAgent {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default, rename = "systemPrompt")]
    system_prompt: String,
    #[serde(default, rename = "isMain")]
    is_main: bool,
}

impl ClientAgent {
    fn to_def(&self) -> AgentDef {
        AgentDef {
            id: self.id.clone(),
            name: self.name.clone(),
            abbr: String::new(),
            description: self.description.clone(),
            icon: String::new(),
            color: String::new(),
            system_prompt: self.system_prompt.clone(),
            skills: vec![],
            is_main: self.is_main,
            is_builtin: false,
        }
    }
}

/// Keys we forward to the upstream provider (everything else is stripped).
const FORWARD_KEYS: &[&str] = &[
    "model",
    "messages",
    "stream",
    "temperature",
    "top_p",
    "max_tokens",
    "stop",
    "presence_penalty",
    "frequency_penalty",
    "tools",
    "tool_choice",
    "response_format",
    "user",
    "reasoning_effort",
];

pub async fn chat_completions(
    State(app): State<std::sync::Arc<AppState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let cfg = app.config.read().unwrap().resolved();

    if cfg.endpoint.is_empty() {
        return error_json(
            StatusCode::BAD_REQUEST,
            "No LLM endpoint configured. Edit config/config.toml (endpoint + api_key + model) — changes apply live, no restart needed.",
        );
    }

    let mut payload: Value = match serde_json::from_slice(&body) {
        Ok(Value::Object(v)) => Value::Object(v),
        _ => return error_json(StatusCode::BAD_REQUEST, "Invalid JSON body"),
    };

    // ---- orchestrator injection -------------------------------------------
    let session_id = headers
        .get("x-hermes-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("sess-{}", uuid::Uuid::new_v4().simple()));

    let orchestrator_on = cfg.orchestrator.unwrap_or(true);
    if orchestrator_on {
        let has_system = payload
            .get("messages")
            .and_then(|m| m.as_array())
            .map(|arr| arr.iter().any(|m| m.get("role").and_then(|r| r.as_str()) == Some("system")))
            .unwrap_or(false);

        if !has_system {
            // Priority 1: agent registry sent by the UI (supports live CRUD).
            let client_agents: Option<Vec<AgentDef>> = payload
                .get("hermes")
                .and_then(|h| h.get("agents"))
                .and_then(|a| a.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|a| serde_json::from_value::<ClientAgent>(a.clone()).ok())
                        .map(|ca| ca.to_def())
                        .collect::<Vec<_>>()
                })
                .filter(|v: &Vec<AgentDef>| !v.is_empty());

            // Priority 2: server-side registry (config/agents.json).
            let agents = client_agents.unwrap_or_else(|| app.agents.read().unwrap().clone());

            if let Some(prompt) = build_orchestrator_prompt(&agents) {
                let existing_messages = payload.get("messages").cloned();
                if let Some(obj) = payload.as_object_mut() {
                    obj.insert("messages".into(), inject_system(existing_messages, prompt));
                }
            }
        }
    }
    let _ = session_id; // reserved for future server-side session persistence

    // ---- build the clean upstream request ---------------------------------
    let mut upstream = serde_json::Map::new();
    if let Some(obj) = payload.as_object() {
        for key in FORWARD_KEYS {
            if let Some(v) = obj.get(*key) {
                upstream.insert((*key).to_string(), v.clone());
            }
        }
    }
    if !upstream.contains_key("model") {
        upstream.insert("model".into(), json!(cfg.model));
    }
    let upstream_body = Value::Object(upstream).to_string();

    // ---- call the provider --------------------------------------------------
    let client = app.http.clone();
    let mut req = client
        .post(cfg.chat_url())
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "text/event-stream, application/json")
        .body(upstream_body);
    if !cfg.api_key.is_empty() {
        req = req.bearer_auth(&cfg.api_key);
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            return error_json(
                StatusCode::BAD_GATEWAY,
                &format!("Cannot reach LLM endpoint {}: {e}", cfg.endpoint),
            )
        }
    };

    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let upstream_ct = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/json")
        .to_string();

    // Upstream error → forward verbatim so the UI shows the real cause.
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return (status, [("Content-Type", upstream_ct.as_str())], text).into_response();
    }

    let is_stream = upstream_ct.contains("text/event-stream")
        || payload.get("stream").and_then(|s| s.as_bool()).unwrap_or(false);

    if !is_stream {
        // Non-streaming: proxy the JSON body as-is.
        match resp.bytes().await {
            Ok(bytes) => (
                StatusCode::OK,
                [("Content-Type", upstream_ct.as_str())],
                bytes,
            )
                .into_response(),
            Err(e) => error_json(StatusCode::BAD_GATEWAY, &format!("Upstream read error: {e}")),
        }
    } else {
        // Streaming: reframe upstream SSE on event boundaries.
        let stream = resp.bytes_stream();
        let reframed = sse_reframe(stream);
        let mut builder = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-cache")
            .header("X-Accel-Buffering", "no")
            .header("X-Hermes-Session-Id", session_id_value(&headers));
        if let Some(hs) = builder.headers_mut() {
            hs.insert(
                header::CONNECTION,
                header::HeaderValue::from_static("keep-alive"),
            );
        }
        builder.body(Body::from_stream(reframed)).unwrap()
    }
}

fn session_id_value(headers: &HeaderMap) -> String {
    headers
        .get("x-hermes-session-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("no-session")
        .to_string()
}

/// Prepend a system message (keeping the conversation intact).
fn inject_system(messages: Option<Value>, system_prompt: String) -> Value {
    let mut arr = match messages {
        Some(Value::Array(a)) => a,
        _ => vec![],
    };
    arr.insert(0, json!({"role": "system", "content": system_prompt}));
    Value::Array(arr)
}

/// Re-chunk an upstream byte stream into complete SSE events.
/// Guarantees the browser never receives a frame split mid-event.
fn sse_reframe<S>(upstream: S) -> impl Stream<Item = Result<axum::body::Bytes, axum::Error>>
where
    S: Stream<Item = Result<axum::body::Bytes, reqwest::Error>> + Unpin,
{
    futures_util::stream::unfold(
        (upstream, Vec::<u8>::new(), false),
        |(mut stream, mut buf, mut done)| async move {
            loop {
                if let Some((end, skip)) = find_event_boundary(&buf) {
                    let event: Vec<u8> = buf.drain(..end).collect();
                    let _ = skip;
                    return Some((Ok(axum::body::Bytes::from(event)), (stream, buf, done)));
                }
                if done {
                    if buf.is_empty() {
                        return None;
                    }
                    let rest = axum::body::Bytes::from(std::mem::take(&mut buf));
                    return Some((Ok(rest), (stream, buf, done)));
                }
                match stream.next().await {
                    Some(Ok(chunk)) => buf.extend_from_slice(&chunk),
                    Some(Err(e)) => {
                        return Some((Err(axum::Error::new(e)), (stream, buf, done)));
                    }
                    None => {
                        done = true;
                    }
                }
            }
        },
    )
}

/// Find the end of the next complete SSE event: "\n\n" or "\r\n\r\n".
/// Returns (end_index_exclusive, bytes_to_skip).
fn find_event_boundary(buf: &[u8]) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut i = 0;
    while i < buf.len() {
        if buf[i] == b'\n' {
            if i + 1 < buf.len() && buf[i + 1] == b'\n' {
                let cand = (i + 2, 2);
                if best.map(|b| cand.0 < b.0).unwrap_or(true) {
                    best = Some(cand);
                }
                i += 2;
                continue;
            }
            if i + 3 < buf.len() && &buf[i..i + 4] == b"\r\n\r\n" {
                let cand = (i + 4, 4);
                if best.map(|b| cand.0 < b.0).unwrap_or(true) {
                    best = Some(cand);
                }
                i += 4;
                continue;
            }
        }
        i += 1;
    }
    best
}

fn error_json(status: StatusCode, message: &str) -> Response {
    let body = json!({
        "error": {"message": message, "type": "hermes_rust_error", "code": status.as_u16()}
    });
    (status, axum::Json(body)).into_response()
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
