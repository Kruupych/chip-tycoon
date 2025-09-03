//! Embedded YAML assets for Windows-safe runtime (no filesystem reads).
//! Provides stable names looked up by `get_yaml(name)`.

#[inline]
pub fn get_yaml(name: &str) -> &'static str {
    match name {
        // Scenarios
        "campaign_1990s" => include_str!("../../../assets/scenarios/campaign_1990s.yaml"),
        "tutorial_24m" => include_str!("../../../assets/scenarios/tutorial_24m.yaml"),
        // Data
        "markets_1990s" => include_str!("../../../assets/data/markets_1990s.yaml"),
        "tech_era_1990s" => include_str!("../../../assets/data/tech_era_1990s.yaml"),
        "difficulty" => include_str!("../../../assets/scenarios/difficulty.yaml"),
        // Events
        "events_1990s" => include_str!("../../../assets/events/campaign_1990s.yaml"),
        // AI defaults (already embedded in sim-ai; exposed here for completeness)
        "ai_defaults" => include_str!("../../../assets/data/ai_defaults.yaml"),
        other => panic!("unknown embedded yaml: {other}"),
    }
}

