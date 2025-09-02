fn main() {
    // Ensure default icons exist so tauri-build doesn't fail (Windows requires .ico)
    let _ = std::fs::create_dir_all("icons");
    let png_path = std::path::Path::new("icons/icon.png");
    if !png_path.exists() {
        // 1x1 white PNG
        let png_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIW2P8z/C/HwAFgwJ/l9Wl5QAAAABJRU5ErkJggg==";
        if let Ok(bytes) = base64::decode(png_b64) {
            let _ = std::fs::write(png_path, bytes);
        }
    }
    let ico_path = std::path::Path::new("icons/icon.ico");
    if !ico_path.exists() {
        // Generate a simple 32x32 blue square ICO
        let mut data = vec![0u8; 32 * 32 * 4];
        for px in data.chunks_mut(4) {
            px[0] = 0x33; // B
            px[1] = 0x66; // G
            px[2] = 0xCC; // R
            px[3] = 0xFF; // A
        }
        let image = ico::IconImage::from_rgba_data(32, 32, data);
        let mut dir = ico::IconDir::new(ico::ResourceType::Icon);
        dir.add_entry(ico::IconDirEntry::encode(&image).expect("encode ico"));
        let mut file = std::fs::File::create(ico_path).expect("create ico");
        dir.write(&mut file).expect("write ico");
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
