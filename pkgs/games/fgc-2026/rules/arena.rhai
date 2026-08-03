// Runtime arena configuration for the FGC 2026 test match.
// This is intentionally data-driven: the Rust runtime reads this function
// and does not hard-code the object count, shape, or motion parameters.
fn arena_config() {
    #{
        object_id: "wildfire",
        object_count: 24,
        object_radius: 0.09,
        spawn_radius: 4.5,
        spawn_height: 1.8,
        restitution: 0.72,
        gravity_scale: 1.0,
        color: "#f97316"
    }
}
