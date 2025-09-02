# Chip Tycoon — Quickstart

Requirements
- Rust stable (1.75+), rustfmt, clippy.
- UI: Node.js + pnpm; on Linux, WebKitGTK/GTK3 (e.g., libwebkit2gtk-4.1-dev, libgtk-3-dev), libayatana-appindicator3, libssl-dev.
- Windows: MSVC toolchain and WebView2 Runtime; Visual Studio Build Tools for Tauri.

Run Desktop UI
- `just release-ui` then launch installer/bundle under `apps/mgmt-ui/src-tauri/target/release/bundle`.
- Or dev mode: `just run-ui`.

Run CLI
- `just release-cli` then `./target/release/cli --version`.
- Campaign: `./target/release/cli --campaign 1990s`.
- Export report: `./target/release/cli --campaign 1990s --export-campaign telemetry/campaign.json`.

Tutorial & Export
- In UI, go to Campaign page → Restart 1990s; follow Mission HUD.
- Use “Export Report” to write JSON/Parquet (dry-run; does not change state).

Troubleshooting
- If UI doesn’t start on Linux, ensure WebKitGTK and dependencies are installed.
- For Windows, ensure MSVC and WebView2 are present.

Windows UI Build

Run these from Windows PowerShell:

```
Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass
./scripts/windows/install-tauri-prereqs.ps1
./scripts/windows/build-ui.ps1
```

