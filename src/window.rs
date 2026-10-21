// HermesRust — window.rs
// Opens the app window. No WebView engine is compiled in: the app reuses the
// browser already installed on the machine in "app mode" (a chrome-less,
// dedicated window with its own taskbar icon):
//   1. Microsoft Edge  --app=URL   (present on every Windows 10/11 machine)
//   2. Google Chrome   --app=URL
//   3. Chromium / Brave / Vivaldi --app=URL (Linux)
//   4. Default browser fallback
// The web UI itself is served by the embedded Rust HTTP server.

use std::path::PathBuf;
use std::process::Command;

pub fn open_app_window(url: &str, width: u32, height: u32) -> &'static str {
    let app_url = format!("--app={url}");
    let size_arg = format!("--window-size={width},{height}");

    // 1 & 2. Edge / Chrome — with their usual install locations.
    let mut candidates: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "windows")]
    {
        for var in ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"] {
            if let Ok(base) = std::env::var(var) {
                let base = PathBuf::from(base);
                candidates.push(base.join(r"Microsoft\Edge\Application\msedge.exe"));
                candidates.push(base.join(r"Google\Chrome\Application\chrome.exe"));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        candidates.push(home.join("Applications/Google Chrome.app/Contents/MacOS/Google Chrome"));
        candidates.push(PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"));
        candidates.push(home.join("Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"));
        candidates.push(PathBuf::from("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"));
        candidates.push(home.join("Applications/Chromium.app/Contents/MacOS/Chromium"));
        candidates.push(PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for bin in [
            "microsoft-edge",
            "microsoft-edge-stable",
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
            "brave-browser",
            "vivaldi",
        ] {
            candidates.push(PathBuf::from(bin));
        }
    }

    for path in &candidates {
        let mut cmd = Command::new(path);
        cmd.args([&app_url, "--new-window", &size_arg]);
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            // Avoid polluting the user's default profile with a new window? No —
            // using the default profile is what makes the app feel native.
            let _ = ();
        }
        if let Ok(mut child) = cmd.spawn() {
            let _ = child.wait(); // app-mode launchers exit immediately
            let _ = size_arg;
            return mode_name(path);
        }
    }

    // 4. Default browser fallback (like the original app without pywebview).
    if open::that(url).is_ok() {
        return "default browser";
    }
    "none"
}

fn mode_name(path: &PathBuf) -> &'static str {
    let s = path.to_string_lossy().to_lowercase();
    if s.contains("edge") {
        "Microsoft Edge (app mode)"
    } else if s.contains("chrom") || s.contains("brave") || s.contains("vivaldi") {
        "Chromium browser (app mode)"
    } else {
        "browser (app mode)"
    }
}
