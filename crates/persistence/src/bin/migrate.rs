#![deny(warnings)]

use persistence::default_sqlite_url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = default_sqlite_url();
    // Ensure directory exists
    let path = url
        .strip_prefix("sqlite://")
        .or_else(|| url.strip_prefix("sqlite:"));
    if let Some(path) = path {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        // touch file to ensure it exists
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .append(true)
            .open(path)?;
    }
    let pool = persistence::init_db(url).await?;
    // Sanity: create a save entry
    let _id = persistence::create_save(&pool, "default", Some("initialized"))
        .await
        .unwrap_or(0);
    println!("DB migrated at {}", url);
    Ok(())
}
