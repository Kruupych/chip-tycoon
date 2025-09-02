fn main() {
    // Ensure a default icon exists so tauri-build doesn't fail locally
    let icon_path = std::path::Path::new("icons/icon.png");
    if !icon_path.exists() {
        let png_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIW2P8z/C/HwAFgwJ/l9Wl5QAAAABJRU5ErkJggg==";
        if let Ok(bytes) = base64::decode(png_b64) {
            let _ = std::fs::create_dir_all("icons");
            let _ = std::fs::write(icon_path, bytes);
        }
    }
    // Preserve tauri build
    tauri_build::build();
    // Embed git sha and build date similarly to CLI
    let sha = std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=GIT_SHA={}", sha);
    let date = std::process::Command::new("git")
        .args(["show", "-s", "--format=%ci", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "".into());
    println!("cargo:rustc-env=BUILD_DATE={}", date);
    println!("cargo:rerun-if-changed=.git/HEAD");
}
