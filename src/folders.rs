// HermesRust — folders.rs
// Workspace folder APIs: list sub-directories (pure Rust) and the native
// folder picker (implemented at runtime through the OS dialog tools, so the
// binary keeps zero GUI dependencies).

use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Deserialize;
use serde_json::json;
use std::path::PathBuf;

use crate::config::expand_home;

#[derive(Deserialize)]
pub struct ListFolderQuery {
    pub path: Option<String>,
}

/// GET /api/list-folder?path=... — list sub-directories for quick access.
pub async fn list_folder(Query(q): Query<ListFolderQuery>) -> impl IntoResponse {
    let base = match q.path {
        Some(p) if !p.is_empty() => expand_home(&p),
        _ => dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")),
    };
    tokio::task::spawn_blocking(move || do_list_folder(base))
        .await
        .unwrap_or_else(|e| err(format!("internal error: {e}")))
}

fn do_list_folder(base: PathBuf) -> Result<axum::response::Response, (StatusCode, axum::Json<serde_json::Value>)> {
    let p = match base.canonicalize() {
        Ok(p) => p,
        Err(e) => return Err(err_json(format!("Invalid path: {e}"))),
    };
    if !p.is_dir() {
        return Err(err_json("Not a directory".into()));
    }

    let mut dirs: Vec<(String, String)> = Vec::new();
    let rd = match std::fs::read_dir(&p) {
        Ok(rd) => rd,
        Err(e) => return Err(err_json(format!("Cannot read directory: {e}"))),
    };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        // Symlinks to directories count as directories too.
        let is_dir = is_dir || entry.path().symlink_metadata().map(|m| m.is_dir()).unwrap_or(false);
        if is_dir {
            dirs.push((name, entry.path().to_string_lossy().to_string()));
        }
    }
    dirs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
    dirs.truncate(20);

    let parent = p.parent().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
    Ok(axum::Json(json!({
        "parent": parent,
        "current": p.to_string_lossy(),
        "dirs": dirs.into_iter().map(|(name, path)| json!({"name": name, "path": path})).collect::<Vec<_>>(),
    }))
    .into_response())
}

fn err_json(msg: String) -> (StatusCode, axum::Json<serde_json::Value>) {
    (StatusCode::BAD_REQUEST, axum::Json(json!({"error": msg})))
}
fn err(msg: String) -> Result<axum::response::Response, (StatusCode, axum::Json<serde_json::Value>)> {
    Err(err_json(msg))
}

/// POST /api/choose-folder — open the native folder picker dialog.
/// Implemented with runtime OS tooling (no GUI dependency at compile time):
///   Windows: PowerShell FolderBrowserDialog
///   macOS:   osascript choose folder
///   Linux:   zenity / kdialog
pub async fn choose_folder() -> impl IntoResponse {
    let folder = tokio::task::spawn_blocking(native_pick_folder)
        .await
        .unwrap_or_default();
    axum::Json(json!({ "folder": folder }))
}

fn native_pick_folder() -> String {
    #[cfg(target_os = "windows")]
    {
        let script = "Add-Type -AssemblyName System.Windows.Forms; \
                      $f = New-Object System.Windows.Forms.FolderBrowserDialog; \
                      if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { $f.SelectedPath } else { '' }";
        match std::process::Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", script])
            .output()
        {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            }
            _ => String::new(),
        }
    }
    #[cfg(target_os = "macos")]
    {
        match std::process::Command::new("osascript")
            .args(["-e", "POSIX path of (choose folder)"])
            .output()
        {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
            _ => String::new(),
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        for (bin, args) in [
            ("zenity", vec!["--file-selection", "--directory", "--title=Select workspace folder"]),
            ("kdialog", vec!["--getexistingdirectory", home.to_string_lossy().as_ref()]),
        ] {
            if let Ok(out) = std::process::Command::new(bin).args(&args).output() {
                if out.status.success() {
                    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !s.is_empty() {
                        return s;
                    }
                }
            }
        }
        String::new()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        String::new()
    }
}
