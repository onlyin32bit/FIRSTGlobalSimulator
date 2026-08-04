// Scoring rules for FGC 2026

fn on_phase_start(phase) {
    if phase == "TELEOP" {
        // MATCH begins when the announcer says "go."
        // 500 WILDFIRE are released onto the PLAYING FIELD SURFACE from beneath the EXTINGUISHER.
        let i = 0;
        while i < 500 {
            // Spawn the predefined "wildfire" object at the extinguisher output coordinates
            spawn_object("wildfire", [0.0, 0.5, 0.0]);
            i += 1;
        }
    }
}

// Triggered when a game piece enters a sensor zone defined in semantics.json
fn on_trigger_enter(trigger_id, entity_id) {
    if trigger_id == "blueSU" || trigger_id == "blueSUscore" {
        add_score("blue", "SU", 1);
    } else if trigger_id == "redSU" || trigger_id == "redSUscore" {
        add_score("red", "SU", 1);
    } else if trigger_id == "extinguisher" || trigger_id == "EXTscore" {
        // Global Alliance Element (Coopertition)
        // Per rules: when shooting balls into the extinguisher, points won't be accumulated.
        let is_shot = false;
        if type_of(entity_id) == "map" && entity_id.contains("is_shot") {
            is_shot = entity_id.is_shot;
        } else if type_of(entity_id) == "map" && entity_id.contains("shot_by_robot") {
            is_shot = entity_id.shot_by_robot;
        } else if type_of(entity_id) == "bool" {
            is_shot = entity_id;
        }
        if !is_shot {
            add_score("global", "EXT", 1);
        }
    }
}

// Triggered at the very end of the match to evaluate climbing states and apply multipliers
fn on_match_end(world_state) {
    // 1. Calculate CLIMB MULTIPLIER for each regional alliance
    let blue_multiplier = 1.0;
    let red_multiplier = 1.0;
    let total_zone3_robots = 0;

    for robot in world_state.robots {
        let inc = 0.0;
        if robot.climb_state == "CONTACT" { inc = 0.05; }
        else if robot.climb_state == "ZONE_1" { inc = 0.10; }
        else if robot.climb_state == "ZONE_2" { inc = 0.20; }
        else if robot.climb_state == "ZONE_3" { 
            inc = 0.30; 
            total_zone3_robots += 1;
        }

        if robot.team == "blue" { blue_multiplier += inc; }
        else if robot.team == "red" { red_multiplier += inc; }

        // 2. PARTNER CLIMB points
        if robot.is_supported_by_partner {
            add_score(robot.team, "PARTNER_CLIMB", 25);
        }
    }

    // Apply the multipliers to the SU (Suppression Unit) points
    apply_score_multiplier("blue", "SU", blue_multiplier);
    apply_score_multiplier("red", "SU", red_multiplier);

    // 3. COOPERTITION BONUS (Global Alliance)
    if total_zone3_robots >= 6 {
        add_score("global", "COOPERTITION", 40);
    } else if total_zone3_robots == 5 {
        add_score("global", "COOPERTITION", 25);
    } else if total_zone3_robots == 4 {
        add_score("global", "COOPERTITION", 10);
    }
}
