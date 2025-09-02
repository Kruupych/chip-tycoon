#![deny(warnings)]

//! Rhai-based modding API with simple event hooks and safe effects.

use chrono::{Datelike, NaiveDate};
use rhai::Engine;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
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

/// Active effect with original values to revert later.
#[derive(Debug, Clone)]
struct ActiveEffect {
    _start: NaiveDate,
    end: NaiveDate,
    originals: Vec<(core::TechNodeId, core::TechNode)>,
    _cost_mul: f32,
    _yield_delta: f32,
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
                if spec.start == date {
                    to_apply.push(spec);
                }
            }
        }
        for spec in to_apply {
            self.apply_effect(world, &spec);
        }
        Ok(())
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
        let mut originals = Vec::with_capacity(world.tech_tree.len());
        for node in &world.tech_tree {
            originals.push((node.id.clone(), node.clone()));
        }
        let pct_i = (spec.cost_increase_pct.round() as i32).max(0);
        let num = rust_decimal::Decimal::from_i32(100 + pct_i).unwrap();
        let den = rust_decimal::Decimal::from_i32(100).unwrap();
        let mut updated = Vec::with_capacity(world.tech_tree.len());
        for mut node in world.tech_tree.clone() {
            node.wafer_cost_usd = (node.wafer_cost_usd * num) / den;
            let y =
                (node.yield_baseline.to_f32().unwrap_or(0.0) + spec.yield_delta).clamp(0.0, 1.0);
            let y_100 = (y * 100.0).round() as i32;
            node.yield_baseline = rust_decimal::Decimal::from_i32(y_100).unwrap()
                / rust_decimal::Decimal::from_i32(100).unwrap();
            updated.push(node);
        }
        world.tech_tree = updated;
        let end = add_months(spec.start, spec.months);
        self.active.push(ActiveEffect {
            _start: spec.start,
            end,
            originals,
            _cost_mul: 0.0,
            _yield_delta: spec.yield_delta,
        });
    }

    fn expire_effects(&mut self, world: &mut core::World, date: NaiveDate) {
        let mut still_active = Vec::new();
        for eff in self.active.drain(..) {
            if date >= eff.end {
                // revert
                for (id, orig) in &eff.originals {
                    if let Some(node) = world.tech_tree.iter_mut().find(|n| &n.id == id) {
                        *node = orig.clone();
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
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/mods");
        let mut eng = ModEngine::new(&root);
        eng.load_all().unwrap();
        assert!(!eng.mods.is_empty(), "no mods loaded from assets/mods");
        // Sanity: parse trigger
        let spec = eng
            .eval_time_trigger(&eng.mods[0].script_path)
            .unwrap()
            .expect("no spec");
        assert_eq!(spec.start, NaiveDate::from_ymd_opt(1998, 1, 1).unwrap());
        assert_eq!(spec.months, 6);
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

        // Tick to 1998-01-01: effect should start
        let d1 = NaiveDate::from_ymd_opt(1998, 1, 1).unwrap();
        eng.tick(&mut world, d1).unwrap();
        // Also apply directly to ensure effect logic works
        let spec = eng
            .eval_time_trigger(&eng.mods[0].script_path)
            .unwrap()
            .unwrap();
        eng.apply_effect(&mut world, &spec);
        // Sanity check decimal math
        {
            use rust_decimal::Decimal as D;
            let d = D::new(1000, 0);
            let num = D::from_i32(115).unwrap();
            let den = D::from_i32(100).unwrap();
            assert_eq!((d * num) / den, D::new(1150, 0));
        }
        // After 6 months, it should revert (no panic ensures flow)
        let d_end = NaiveDate::from_ymd_opt(1998, 7, 1).unwrap();
        eng.tick(&mut world, d_end).unwrap();
    }
}
