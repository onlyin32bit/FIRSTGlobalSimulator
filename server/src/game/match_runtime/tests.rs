use super::*;

fn arena() -> ArenaConfig {
    crate::game::pack_loader::PackLoader::new("0.1.0")
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap()
        .arena
}

#[test]
fn applies_pack_mass_and_drivetrain_limits() {
    let mut arena = arena();
    arena.object_count = 1;
    let mut runtime = MatchRuntime::new("physics".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);

    let ball_body = runtime.rigid_body_set.get(runtime.objects[0].body).unwrap();
    assert!((ball_body.mass() - 0.062).abs() < 0.0001);

    let ball_handle = runtime.objects[0].body;
    let ball_body = runtime.rigid_body_set.get_mut(ball_handle).unwrap();
    ball_body.set_translation(vector![0.0, arena.ball.radius_m(), 0.0].into(), true);
    ball_body.set_linvel(vector![1.0, 0.0, 0.0].into(), true);
    runtime.apply_ball_rolling_resistance(1.0 / 60.0);
    let ball_speed = runtime.rigid_body_set[ball_handle].linvel().x;
    let expected_speed = 1.0 - arena.ball.rolling_resistance_mps2 / 60.0;
    assert!((ball_speed - expected_speed).abs() < 0.0001);

    runtime.add_player("player".into(), "Player".into(), "Team".into(), &arena);
    runtime.set_player_input("player", 0.0, 1.0, 1);
    // One second is long enough to observe acceleration while remaining
    // clear of the arena wall from the default spawn point.
    for _ in 0..60 {
        runtime.apply_player_drive(&arena);
        runtime.tick(1.0 / 60.0);
    }

    let player_body = runtime.players.get("player").unwrap().body;
    let robot_body = runtime.rigid_body_set.get(player_body).unwrap();
    let planar_speed = (robot_body.linvel().x.powi(2) + robot_body.linvel().z.powi(2)).sqrt();
    assert!((robot_body.mass() - arena.robot.mass_kg).abs() < 0.001);
    assert!(planar_speed > 0.5, "robot only reached {planar_speed} m/s");
    assert!(planar_speed <= arena.robot.max_speed_mps + 0.15);

    runtime.set_player_input("player", 0.5, 0.0, 2);
    for _ in 0..30 {
        runtime.apply_player_drive(&arena);
        runtime.tick(1.0 / 60.0);
    }
    let robot_body = runtime.rigid_body_set.get(player_body).unwrap();
    assert!(robot_body.angvel().y.abs() > 0.2);
    assert!(robot_body.angvel().y.abs() <= arena.robot.max_turn_rate_radps + 0.01);
}
