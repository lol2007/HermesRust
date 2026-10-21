# HermesRust — local Windows build script
# Produces hermes-rust.exe and a portable folder next to the repo.
# Requires: https://rustup.rs (Rust stable, MSVC toolchain) + Visual Studio Build Tools.

$ErrorActionPreference = "Stop"
Write-Host "HermesRust — building Windows release..." -ForegroundColor Cyan

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

$dist = "release-dist"
$pkg = "$dist/HermesRust"
New-Item -ItemType Directory -Force -Path "$pkg/config", "$pkg/ui" | Out-Null

Copy-Item "target/release/hermes-rust.exe" "$pkg/"
Copy-Item "config/*" "$pkg/config/" -Recurse -Force
Copy-Item "ui/*" "$pkg/ui/" -Recurse -Force
Copy-Item "README.md", "STRUCTURE.md", "LICENSE" "$pkg/" -ErrorAction SilentlyContinue

Compress-Archive -Path "$pkg" -DestinationPath "$dist/HermesRust-portable-windows.zip" -Force
Write-Host ""
Write-Host "Done." -ForegroundColor Green
Write-Host "  Binary : $pkg/hermes-rust.exe"
Write-Host "  Zip    : $dist/HermesRust-portable-windows.zip"
Write-Host ""
Write-Host "Run it:  cd $pkg ; .\hermes-rust.exe"
Write-Host "The app window opens automatically (Edge/Chrome app mode)."
