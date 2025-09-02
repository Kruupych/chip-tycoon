use criterion::{criterion_group, criterion_main, Criterion};

fn bench_ticks(c: &mut Criterion) {
    let dom = sim_core::World {
        macro_state: sim_core::MacroState {
            date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
            inflation_annual: 0.02,
            interest_rate: 0.05,
            fx_usd_index: 100.0,
        },
        tech_tree: vec![],
        companies: vec![sim_core::Company {
            name: "A".into(),
            cash_usd: rust_decimal::Decimal::new(5_000_000, 0),
            debt_usd: rust_decimal::Decimal::ZERO,
            ip_portfolio: vec![],
        }],
        segments: vec![sim_core::MarketSegment {
            name: "Seg".into(),
            base_demand_units: 1_000_000,
            price_elasticity: -1.2,
        }],
    };
    let mut world = sim_runtime::init_world(
        dom,
        sim_core::SimConfig {
            tick_days: 30,
            rng_seed: 42,
        },
    );
    c.bench_function("sim_tick", |b| {
        b.iter(|| {
            let _ = sim_runtime::run_months_in_place(&mut world, 1);
        })
    });
}

criterion_group!(benches, bench_ticks);
criterion_main!(benches);
