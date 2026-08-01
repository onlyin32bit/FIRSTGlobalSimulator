// Robot constraints and mechanics for FGC 2026

fn validate_build(robot_spec) {
    // Validate maximum dimensions (e.g. 50cm x 50cm x 50cm starting size)
    if robot_spec.width > 0.5 || robot_spec.length > 0.5 || robot_spec.height > 0.5 {
        return false;
    }
    
    // Max mass 15kg
    if robot_spec.mass > 15.0 {
        return false;
    }
    
    return true;
}

// Called every tick to calculate physics modifiers (like battery drain)
fn calculate_physics_modifiers(robot_id, state) {
    let modifiers = #{
        speed_multiplier: 1.0,
        intake_disabled: false
    };

    // Brownout calculation: If voltage < 9.0V, disable motors
    if state.battery_voltage < 9.0 {
        modifiers.speed_multiplier = 0.0;
        modifiers.intake_disabled = true;
    }

    return modifiers;
}
