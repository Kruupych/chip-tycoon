use std::process::Command;

fn main() {
    // Try to embed git SHA
    let sha = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=GIT_SHA={}", sha);
    // Build date in ISO-like format from git (fallback to UNIX epoch seconds)
    let date = Command::new("git")
        .args(["show", "-s", "--format=%ci", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
                Ok(d) => format!("{}", d.as_secs()),
                Err(_) => "unknown".into(),
            }
        });
    println!("cargo:rustc-env=BUILD_DATE={}", date);
    println!("cargo:rerun-if-changed=.git/HEAD");
}
