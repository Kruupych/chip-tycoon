use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rust_decimal::Decimal;

fn build_world(n_companies: usize) -> sim_core::World {
    let segments = vec![sim_core::MarketSegment {
        name: "Desktop".into(),
        base_demand_units: 500_000,
        price_elasticity: -1.8,
    }];
    let tech = vec![sim_core::TechNode {
        id: sim_core::TechNodeId("N600".into()),
        year_available: 1990,
        density_mtr_per_mm2: Decimal::new(1, 0),
        freq_ghz_baseline: Decimal::new(1, 0),
        leakage_index: Decimal::new(1, 0),
        yield_baseline: Decimal::new(9, 1),
        wafer_cost_usd: Decimal::new(1000, 0),
        mask_set_cost_usd: Decimal::new(2_500_000, 2),
        dependencies: vec![],
    }];
    let mut companies = Vec::with_capacity(n_companies);
    for i in 0..n_companies {
        companies.push(sim_core::Company {
            name: format!("C{i}"),
            cash_usd: Decimal::new(5_000_000, 0),
            debt_usd: Decimal::ZERO,
            ip_portfolio: vec![],
        });
    }
    sim_core::World {
        macro_state: sim_core::MacroState {
            date: chrono::NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
            inflation_annual: 0.02,
            interest_rate: 0.05,
            fx_usd_index: 100.0,
        },
        tech_tree: tech,
        companies,
        segments,
    }
}

fn bench_quick(c: &mut Criterion) {
    let world = build_world(10);
    let w0 = sim_runtime::init_world(
        world,
        sim_core::SimConfig {
            tick_days: 30,
            rng_seed: 42,
        },
    );
    c.bench_function("sim 10 companies x 40y", |b| {
        b.iter(|| {
            let w = sim_runtime::clone_world_state(&w0);
            let _ = black_box(sim_runtime::run_months(w, 480));
        })
    });
}

criterion_group!(benches, bench_quick);
criterion_main!(benches);
