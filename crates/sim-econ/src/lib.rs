#![deny(warnings)]

//! Economic models: demand and pricing helpers.

use rust_decimal::Decimal;

/// Compute a trivial price as cost plus a margin.
pub fn cost_plus(unit_cost: Decimal, margin: Decimal) -> Decimal {
    unit_cost * (Decimal::ONE + margin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_cost_plus() {
        let cost = Decimal::new(100, 2); // 1.00
        let margin = Decimal::new(50, 2); // 0.50
        assert_eq!(cost_plus(cost, margin), Decimal::new(150, 2));
    }
}
