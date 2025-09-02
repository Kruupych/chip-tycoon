#![deny(warnings)]

//! AI utility evaluation stubs.

/// Trivial utility: higher is better.
pub fn utility(market_share: f32, margin: f32) -> f32 {
    (market_share * 0.7) + (margin * 0.3)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monotonic_increase() {
        assert!(utility(0.2, 0.1) < utility(0.3, 0.1));
        assert!(utility(0.2, 0.1) < utility(0.2, 0.2));
    }
}
