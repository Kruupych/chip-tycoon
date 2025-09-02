#![deny(warnings)]

//! Rhai-based modding API with simple event hooks and safe effects.

use chrono::{Datelike, NaiveDate};
use rhai::Engine;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use serde::Deserialize;
use sim_core as core;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use thiserror::Error;
use tracing::info;

/// Metadata for a mod package.
#[derive(Debug, Clone, Deserialize)]
pub struct ModMeta {
    pub id: String,
    pub name: String,
    pub version: String,
    pub engine_schema_version: u32,
    pub compat: Option<String>,
    pub hooks: Option<Vec<String>>, // e.g., ["time_trigger"]
}

#[derive(Debug, Error)]
pub enum ModError {
    #[error("invalid metadata: {0}")]
    InvalidMeta(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("rhai error: {0}")]
    Rhai(String),
}

impl From<std::io::Error> for ModError {
    fn from(e: std::io::Error) -> Self {
        ModError::Io(e.to_string())
    }
}

impl From<rhai::EvalAltResult> for ModError {
    fn from(e: rhai::EvalAltResult) -> Self {
        ModError::Rhai(e.to_string())
    }
}
impl From<rhai::ParseError> for ModError {
    fn from(e: rhai::ParseError) -> Self {
        ModError::Rhai(e.to_string())
    }
}

/// Effect specification returned by scripts.
#[derive(Debug, Clone, Default)]
pub struct EffectSpec {
    pub start: NaiveDate,
    pub months: u32,
    pub cost_increase_pct: f32,
    pub yield_delta: f32,
}

/// Loaded mod with metadata and script path.
#[derive(Debug, Clone)]
pub struct LoadedMod {
    pub meta: ModMeta,
    pub dir: PathBuf,
    pub script_path: PathBuf,
    pub script_mtime: SystemTime,
}

#[derive(Debug, Clone)]
struct Patch {
    index: usize,
    old_cost: Decimal,
    old_yield: Decimal,
}

/// Active effect with patches to revert later.
#[derive(Debug, Clone)]
struct ActiveEffect {
    start: NaiveDate,
    end: NaiveDate,
    patches: Vec<Patch>,
}

/// Mod engine: loads mods and applies effects when triggers fire.
pub struct ModEngine {
    root: PathBuf,
    engine: Engine,
    mods: Vec<LoadedMod>,
    active: Vec<ActiveEffect>,
}

impl ModEngine {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            engine: Engine::new(),
            mods: vec![],
            active: vec![],
        }
    }

    pub fn load_all(&mut self) -> Result<(), ModError> {
        let entries = fs::read_dir(&self.root)?;
        self.mods.clear();
        for ent in entries {
            let ent = ent?;
            if !ent.file_type()?.is_dir() {
                continue;
            }
            let dir = ent.path();
            let meta_path = dir.join("metadata.yaml");
            let script_path = dir.join("script.rhai");
            if !meta_path.exists() || !script_path.exists() {
                continue;
            }
            let meta_text = fs::read_to_string(&meta_path)?;
            let meta: ModMeta = serde_yaml::from_str(&meta_text)
                .map_err(|e| ModError::InvalidMeta(e.to_string()))?;
            let mtime = fs::metadata(&script_path)?
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH);
            self.mods.push(LoadedMod {
                meta,
                dir,
                script_path,
                script_mtime: mtime,
            });
        }
        Ok(())
    }

    #[allow(unused)]
    pub fn reload_if_changed(&mut self) -> Result<(), ModError> {
        for m in &mut self.mods {
            let mtime = fs::metadata(&m.script_path)?
                .modified()
                .unwrap_or(SystemTime::UNIX_EPOCH);
            if mtime > m.script_mtime {
                info!("Reloading mod: {}", m.meta.id);
                m.script_mtime = mtime;
            }
        }
        Ok(())
    }

    /// Progress simulation date and apply or expire effects.
    pub fn tick(&mut self, world: &mut core::World, date: NaiveDate) -> Result<(), ModError> {
        self.expire_effects(world, date);
        let mut to_apply: Vec<EffectSpec> = Vec::new();
        for m in &self.mods {
            if let Some(spec) = self.eval_time_trigger_with_meta(m)? {
                let end = add_months(spec.start, spec.months);
                if spec.start == date && !self.is_effect_active(spec.start, end) {
                    to_apply.push(spec);
                }
            }
        }
        for spec in to_apply {
            self.apply_effect(world, &spec);
        }
        Ok(())
    }

    fn is_effect_active(&self, start: NaiveDate, end: NaiveDate) -> bool {
        self.active.iter().any(|e| e.start == start && e.end == end)
    }

    pub(crate) fn eval_time_trigger(
        &self,
        script_path: &Path,
    ) -> Result<Option<EffectSpec>, ModError> {
        let script = fs::read_to_string(script_path).unwrap_or_default();
        // Expect a function time_trigger() -> map with keys
        let ast = self.engine.compile(&script).map_err(ModError::from)?;
        let scope = &mut rhai::Scope::new();
        let result = self
            .engine
            .eval_ast_with_scope::<rhai::Dynamic>(scope, &ast);
        match result {
            Ok(val) => {
                if !val.is_map() {
                    return Ok(None);
                }
                let map = val.cast::<rhai::Map>();
                let start_s = map
                    .get("start")
                    .and_then(|v| v.clone().try_cast::<String>());
                let months = map
                    .get("months")
                    .and_then(|v| v.clone().try_cast::<i64>())
                    .unwrap_or(0);
                let cost_pct = map
                    .get("cost_pct")
                    .and_then(|v| v.clone().try_cast::<f32>())
                    .unwrap_or(0.0);
                let yield_delta = map
                    .get("yield_delta")
                    .and_then(|v| v.clone().try_cast::<f32>())
                    .unwrap_or(0.0);
                if let Some(start_s) = start_s {
                    let start = NaiveDate::parse_from_str(&start_s, "%Y-%m-%d")
                        .map_err(|e| ModError::InvalidMeta(e.to_string()))?;
                    return Ok(Some(EffectSpec {
                        start,
                        months: months as u32,
                        cost_increase_pct: cost_pct,
                        yield_delta,
                    }));
                }
                Ok(None)
            }
            Err(_) => Ok(None),
        }
    }

    pub(crate) fn eval_time_trigger_with_meta(
        &self,
        m: &LoadedMod,
    ) -> Result<Option<EffectSpec>, ModError> {
        if let Some(spec) = self.eval_time_trigger(&m.script_path)? {
            return Ok(Some(spec));
        }
        // Fallback: parse from metadata.yaml if present
        #[derive(Deserialize)]
        struct TimeEffect {
            start: String,
            months: u32,
            cost_pct: f32,
            yield_delta: f32,
        }
        #[derive(Deserialize)]
        struct MetaFile {
            #[serde(default)]
            time_effect: Option<TimeEffect>,
        }
        let meta_path = m.dir.join("metadata.yaml");
        let text = fs::read_to_string(meta_path)?;
        let mf: MetaFile =
            serde_yaml::from_str(&text).map_err(|e| ModError::InvalidMeta(e.to_string()))?;
        if let Some(t) = mf.time_effect {
            let start = NaiveDate::parse_from_str(&t.start, "%Y-%m-%d")
                .map_err(|e| ModError::InvalidMeta(e.to_string()))?;
            Ok(Some(EffectSpec {
                start,
                months: t.months,
                cost_increase_pct: t.cost_pct,
                yield_delta: t.yield_delta,
            }))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn apply_effect(&mut self, world: &mut core::World, spec: &EffectSpec) {
        let mul =
            cost_multiplier(Decimal::from_f32(spec.cost_increase_pct).unwrap_or(Decimal::ZERO));
        let mut patches = Vec::with_capacity(world.tech_tree.len());
        for (i, node) in world.tech_tree.iter_mut().enumerate() {
            let old_cost = node.wafer_cost_usd;
            let old_yield = node.yield_baseline;
            let new_cost = (old_cost * mul).round_dp(0);
            let y = (old_yield.to_f32().unwrap_or(0.0) + spec.yield_delta).clamp(0.0, 1.0);
            let y_100 = (y * 100.0).round() as i32;
            node.wafer_cost_usd = new_cost;
            node.yield_baseline =
                Decimal::from_i32(y_100).unwrap() / Decimal::from_i32(100).unwrap();
            patches.push(Patch {
                index: i,
                old_cost,
                old_yield,
            });
        }
        let end = add_months(spec.start, spec.months);
        self.active.push(ActiveEffect {
            start: spec.start,
            end,
            patches,
        });
    }

    fn expire_effects(&mut self, world: &mut core::World, date: NaiveDate) {
        let mut still_active = Vec::new();
        for eff in self.active.drain(..) {
            if date >= eff.end {
                for p in &eff.patches {
                    if let Some(n) = world.tech_tree.get_mut(p.index) {
                        n.wafer_cost_usd = p.old_cost;
                        n.yield_baseline = p.old_yield;
                    }
                }
                info!("Effect expired at {}", date);
            } else {
                still_active.push(eff);
            }
        }
        self.active = still_active;
    }
}

fn add_months(start: NaiveDate, months: u32) -> NaiveDate {
    let mut y = start.year();
    let mut m = start.month() as i32 + months as i32;
    y += (m - 1) / 12;
    m = (m - 1) % 12 + 1;
    let m_u = u32::try_from(m).unwrap_or(1);
    NaiveDate::from_ymd_opt(y, m_u, start.day()).unwrap_or(start)
}

/// Returns a fresh Rhai engine with default configuration.
pub fn new_engine() -> Engine {
    Engine::new()
}

/// Convert cost percentage/fraction to multiplier.
/// If |x| <= 1 it's treated as a fraction (0.15 => +15%), else as a percent (15 => +15%).
pub fn cost_multiplier(x: Decimal) -> Decimal {
    let abs = x.abs();
    if abs <= Decimal::ONE {
        Decimal::ONE + x
    } else {
        Decimal::ONE + x / Decimal::from_i32(100).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn engine_runs_script() {
        let engine = new_engine();
        let result: i64 = engine.eval("40 + 2").unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn example_mod_applies_and_expires() {
        // Base world
        let mut world = core::World {
            macro_state: core::MacroState {
                date: NaiveDate::from_ymd_opt(1997, 12, 1).unwrap(),
                inflation_annual: 0.0,
                interest_rate: 0.0,
                fx_usd_index: 100.0,
            },
            tech_tree: vec![core::TechNode {
                id: core::TechNodeId("N90".to_string()),
                year_available: 1990,
                density_mtr_per_mm2: Decimal::new(1, 0),
                freq_ghz_baseline: Decimal::new(1, 0),
                leakage_index: Decimal::new(1, 0),
                yield_baseline: Decimal::new(90, 2),
                wafer_cost_usd: Decimal::new(1000, 0),
                mask_set_cost_usd: Decimal::new(5000, 0),
                dependencies: vec![],
            }],
            companies: vec![],
            segments: vec![],
        };
        assert_eq!(world.tech_tree.len(), 1);

        // Scenario A: cost_pct = 15, yield_delta=-0.02, months=6
        let mut eng = ModEngine::new(".");
        let spec_a = EffectSpec {
            start: NaiveDate::from_ymd_opt(1998, 1, 1).unwrap(),
            months: 6,
            cost_increase_pct: 15.0,
            yield_delta: -0.02,
        };
        eng.apply_effect(&mut world, &spec_a);
        let node = &world.tech_tree[0];
        assert_eq!(node.wafer_cost_usd, Decimal::new(1150, 0));
        assert_eq!(node.yield_baseline, Decimal::new(88, 2));
        // Apply again via tick on same date -> no accumulation
        eng.tick(&mut world, spec_a.start).unwrap();
        let node = &world.tech_tree[0];
        assert_eq!(node.wafer_cost_usd, Decimal::new(1150, 0));
        // Expire after 6 months
        let end = NaiveDate::from_ymd_opt(1998, 7, 1).unwrap();
        eng.tick(&mut world, end).unwrap();
        let node = &world.tech_tree[0];
        assert_eq!(node.wafer_cost_usd, Decimal::new(1000, 0));
        assert_eq!(node.yield_baseline, Decimal::new(90, 2));

        // Scenario B: cost_pct = 0.15 (fraction)
        let mut world2 = world.clone();
        let mut eng2 = ModEngine::new(".");
        let spec_b = EffectSpec {
            start: NaiveDate::from_ymd_opt(1999, 1, 1).unwrap(),
            months: 6,
            cost_increase_pct: 0.15,
            yield_delta: -0.02,
        };
        eng2.apply_effect(&mut world2, &spec_b);
        let node = &world2.tech_tree[0];
        assert_eq!(node.wafer_cost_usd, Decimal::new(1150, 0));
        assert_eq!(node.yield_baseline, Decimal::new(88, 2));
        let end_b = NaiveDate::from_ymd_opt(1999, 7, 1).unwrap();
        eng2.tick(&mut world2, end_b).unwrap();
        let node = &world2.tech_tree[0];
        assert_eq!(node.wafer_cost_usd, Decimal::new(1000, 0));
        assert_eq!(node.yield_baseline, Decimal::new(90, 2));
    }

    #[test]
    fn test_cost_multiplier() {
        use rust_decimal::Decimal as D;
        assert_eq!(super::cost_multiplier(D::new(15, 0)), D::new(115, 2)); // 1.15
        assert_eq!(super::cost_multiplier(D::new(15, 2)), D::new(115, 2)); // 0.15 -> 1.15
        assert_eq!(super::cost_multiplier(D::new(-10, 0)), D::new(90, 2)); // 0.9
        assert_eq!(super::cost_multiplier(D::new(-10, 2)), D::new(90, 2)); // 0.9
    }
}
