#![deny(warnings)]

//! ECS runtime for the simulation (stubbed without full Bevy dependency for Phase 0).

use bevy_ecs::prelude::*;

/// Minimal world initialization to prove wiring and enable later systems.
pub fn init_world() -> World {
    World::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn world_creates() {
        let _world = init_world();
    }
}
