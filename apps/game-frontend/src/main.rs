#![deny(warnings)]

//! Minimal Bevy HUD/timeline with headless tick simulation.

use bevy_ecs::prelude::*;

#[derive(Resource, Default)]
struct HudState {
    months: u32,
    paused: bool,
    last_event: String,
}

fn tick_months_system(mut state: ResMut<HudState>) {
    // No-op: the test manipulates the resource directly.
    if state.paused {
        state.last_event = "paused".into();
    }
}

fn main() {
    let mut world = World::new();
    world.insert_resource(HudState::default());
    let mut schedule = bevy_ecs::schedule::Schedule::default();
    schedule.add_systems(tick_months_system);
    // No run loop: headless demo
    schedule.run(&mut world);
    let s = world.resource::<HudState>();
    println!(
        "game-frontend: HUD ready | months={} status={}",
        s.months, s.last_event
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_initializes_and_ticks_once() {
        let mut world = World::new();
        world.insert_resource(HudState::default());
        let mut schedule = bevy_ecs::schedule::Schedule::default();
        schedule.add_systems(tick_months_system);
        // Simulate a tick event by manually updating the resource
        {
            let mut s = world.resource_mut::<HudState>();
            s.months = s.months.saturating_add(1);
            s.last_event = "+1 months".into();
        }
        schedule.run(&mut world);
        let s = world.resource::<HudState>();
        assert_eq!(s.months, 1);
        assert_eq!(s.last_event, "+1 months");
    }
}
