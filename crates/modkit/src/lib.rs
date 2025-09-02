#![deny(warnings)]

//! Modding API based on Rhai (stub).

use rhai::Engine;

/// Returns a fresh Rhai engine with default configuration.
pub fn new_engine() -> Engine {
    Engine::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn engine_runs_script() {
        let engine = new_engine();
        let result: i64 = engine.eval("40 + 2").unwrap();
        assert_eq!(result, 42);
    }
}
