// HermesRust — main.rs
// Hermes Agent Desktop, rewritten in pure Rust.
//
//   • One tiny static binary (~5 MB) — no Python, no Node, no runtime.
//   • Launches in well under 100 ms.
//   • Window = browser in app mode (Edge/Chrome), UI served live from disk.
//   • config/ + ui/ are plain files: edit them at any time, changes are
//     picked up live (config) or on the next refresh (UI).

mod config;
mod folders;
mod orchestrator;
mod proxy;
mod server;
mod state;
mod window;

use config::VERSION;
use state::AppState;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

struct Args {
    port: Option<u16>,
    host: Option<String>,
    no_window: bool,
    base_dir: Option<PathBuf>,
    help: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        port: None,
        host: None,
        no_window: false,
        base_dir: None,
        help: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--port" | "-p" => a.port = it.next().and_then(|v| v.parse().ok()),
            "--host" => a.host = it.next(),
            "--no-window" => a.no_window = true,
            "--dir" | "-d" => a.base_dir = it.next().map(PathBuf::from),
            "--help" | "-h" => a.help = true,
            _ => {}
        }
    }
    a
}

fn print_help() {
    println!(
        "HermesRust v{VERSION} — Hermes Agent Desktop rewritten in Rust

USAGE:
    hermes-rust [OPTIONS]

OPTIONS:
    -p, --port <PORT>     Override the port (default: 8642)
        --host <HOST>     Bind address (default: 127.0.0.1)
        --no-window       Do not open the app window (server only)
    -d, --dir <PATH>      Base directory containing config/ and ui/
    -h, --help            Show this help

LIVE EDITING:
    config/config.toml    Provider, model, port… — hot-reloaded (~0.5 s)
    config/agents.json    The 20-agent team — hot-reloaded
    config/skills.json    Skill Store catalog — hot-reloaded
    ui/                   The entire front-end — refresh to apply"
    );
}

#[tokio::main]
async fn main() {
    let args = parse_args();
    if args.help {
        print_help();
        return;
    }

    let boot = Instant::now();

    // Resolve the base dir (config/ + ui/), honoring the CLI override first.
    let cli_dir = args.base_dir.clone();
    let base_dir = config::resolve_base_dir(cli_dir);
    let app = AppState::new(base_dir.clone());
    app.spawn_watcher();

    // CLI overrides beat config values.
    {
        let mut cfg = app.config.write().unwrap();
        if let Some(p) = args.port {
            cfg.port = p;
        }
        if let Some(h) = &args.host {
            cfg.host = h.clone();
        }
        if args.no_window {
            cfg.open_window = Some(false);
        }
    }
    let cfg = app.config.read().unwrap().resolved();

    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[hermes] cannot bind {addr}: {e}");
            std::process::exit(1);
        }
    };
    let url = format!("http://{addr}");

    println!();
    println!("  ┌─────────────────────────────────────────────────────┐");
    println!("  │  HermesRust v{VERSION}                                    │");
    println!("  │  Hermes Agent Desktop — rewritten in pure Rust      │");
    println!("  ├─────────────────────────────────────────────────────┤");
    println!("  │  UI & API : {url:<38}│");
    println!("  │  Provider : {:<41}│", truncate(&cfg.endpoint, 41));
    println!("  │  Model    : {:<41}│", truncate(&cfg.model, 41));
    println!("  │  Agents   : {:<41}│", format!("{} loaded", app.agents.read().unwrap().len()));
    println!("  │  Files    : {:<41}│", truncate(&base_dir.display().to_string(), 41));
    println!("  └─────────────────────────────────────────────────────┘");
    println!("  Live-edit config/ and ui/ — no restart needed.");
    println!("  Stop: Ctrl+C  (or POST /api/quit)");
    println!();

    // Open the window after the server is listening.
    if cfg.open_window.unwrap_or(true) {
        let url_c = url.clone();
        let (w, h) = (cfg.window_width, cfg.window_height);
        std::thread::spawn(move || {
            let mode = window::open_app_window(&url_c, w, h);
            if mode != "none" {
                eprintln!("[hermes] window opened via {mode}");
            } else {
                eprintln!("[hermes] could not open any browser — open {url_c} manually");
            }
        });
    }

    let router = server::build_router(Arc::clone(&app));
    let ready_ms = boot.elapsed().as_millis();
    eprintln!("[hermes] ready in {ready_ms} ms");

    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("[hermes] shutting down");
    };
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown)
        .await
        .unwrap();
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max.saturating_sub(3)).collect();
    format!("{cut}...")
}
