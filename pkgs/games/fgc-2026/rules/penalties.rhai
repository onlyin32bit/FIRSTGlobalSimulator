// Penalties for FGC 2026

fn on_robot_collision(robot_a, robot_b, force) {
    if force > 500.0 {
        // High-impact ramming penalty
        add_penalty(robot_a.team, "RAMMING", 10);
        log_event(robot_a.id, "Heavy collision with " + robot_b.id);
    }
}

fn on_robot_enter_zone(robot_id, zone_id) {
    // Example: Entering opponent's restricted zone
    let team = get_robot_team(robot_id);
    if zone_id == "red_restricted" && team != "red" {
        add_penalty(team, "ZONE_INTRUSION", 5);
    }
}
