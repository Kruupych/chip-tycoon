#![deny(warnings)]

//! Persistence layer: DB schema and snapshots (stubs).

/// Returns the default SQLite URL used for local saves.
pub fn default_sqlite_url() -> &'static str {
    "sqlite://./saves/main.db"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn url_is_sqlite() {
        assert!(default_sqlite_url().starts_with("sqlite://"));
    }
}
