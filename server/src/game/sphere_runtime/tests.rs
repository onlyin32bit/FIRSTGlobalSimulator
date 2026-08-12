use super::*;

fn arena() -> ArenaConfig {
    crate::game::pack_loader::PackLoader::new("0.1.0")
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap()
        .arena
}

#[test]
fn simulates_pack_count_and_keeps_balls_in_bounds() {
    let arena = arena();
    let mut runtime = SphereRuntime::new("test".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    for _ in 0..180 {
        runtime.tick(1.0 / 60.0);
    }
    let positions = runtime.field_object_positions();
    assert_eq!(positions.count as usize, arena.object_count);
    assert!(
        positions
            .active_mask
            .iter()
            .map(|mask| mask.count_ones() as usize)
            .sum::<usize>()
            == arena.object_count
    );
    assert!(
        positions
            .quantized_positions
            .chunks_exact(3)
            .all(|position| position.iter().all(|value| *value <= u16::MAX))
    );
}

#[test]
fn pack_guard_rail_footprint_bounds_the_authoritative_arena() {
    let mut arena = arena();
    arena.object_count = 1;
    arena.gravity_scale = 0.0;
    arena.ramp.enabled = false;
    let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut runtime = SphereRuntime::new("field-boundary".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    runtime.balls[0].position = [10.0, arena.ball.radius_m(), 10.0];
    runtime.balls[0].active = true;

    runtime.tick(1.0 / 60.0);

    let radius = arena.ball.radius_m();
    assert!(runtime.balls[0].position[0] <= pack.field_definition.boundary.max[0] - radius);
    assert!(runtime.balls[0].position[2] <= pack.field_definition.boundary.max[2] - radius);
}

#[test]
fn pack_spawn_supports_the_robot_on_the_authored_riser_surface() {
    let arena = arena();
    let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut runtime = SphereRuntime::new("pack-spawn".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    runtime.add_player(
        "red-driver".into(),
        "Driver".into(),
        "red".into(),
        Some("red-driver-1"),
        &arena,
    );

    let player = &runtime.player_snapshots()[0];
    assert!(
        (player.y - (pack.field_definition.floor_height_m + arena.robot.height_m * 0.5)).abs()
            < 1.0e-5
    );
    assert!(player.y > arena.robot.height_m * 0.5);
}

#[test]
fn scores_only_matching_alliance_outtakes_and_reverses_on_exit() {
    let mut arena = arena();
    arena.object_count = 1;
    arena.gravity_scale = 0.0;
    arena.ramp.enabled = false;
    let field = FieldDefinition {
        colliders: Vec::new(),
        anchors: BTreeMap::new(),
        triggers: Vec::new(),
        scoring_targets: vec![FieldScoringTarget {
            id: "blue-suppression-unit".into(),
            kind: "suppression-unit".into(),
            alliance: Some("blue".into()),
            points: 1,
            enabled: true,
            requires_robot_outtake: true,
            min: [-1.0, 0.0, -1.0],
            max: [1.0, 1.0, 1.0],
            retention: Some(crate::game::pack_loader::ScoringRetentionConfig {
                min: [-1.0, 0.0, -2.0],
                max: [1.0, 1.0, 1.0],
                open_top: true,
            }),
        }],
        floor_height_m: 0.0,
        boundary: FieldBoundary::default(),
    };
    let mut runtime = SphereRuntime::new("semantic".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &field);
    runtime.set_scoring_enabled(true);
    runtime.balls[0].position = [0.0, 0.5, 0.0];
    runtime.balls[0].active = true;
    runtime.tick(1.0 / 60.0);
    assert_eq!(
        runtime.score_state.blue_score, 0,
        "unlaunched balls cannot score"
    );

    runtime.balls[0].last_outtake_alliance = Some("red".into());
    runtime.tick(1.0 / 60.0);
    assert_eq!(
        runtime.score_state.blue_score, 0,
        "opposing alliance cannot score"
    );

    runtime.balls[0].last_outtake_alliance = Some("blue".into());
    runtime.tick(1.0 / 60.0);
    assert_eq!(runtime.score_state.blue_score, 1);
    assert!(runtime.balls[0].active, "scored balls stay physical");

    runtime.balls[0].position[2] = -1.5;
    runtime.tick(1.0 / 60.0);
    assert_eq!(
        runtime.score_state.blue_score, 1,
        "a ball remains scored after settling deeper into its hopper"
    );

    runtime.balls[0].position[1] = 2.0;
    runtime.tick(1.0 / 60.0);
    assert_eq!(
        runtime.score_state.blue_score, 0,
        "score is removed when the ball exits"
    );
}

#[test]
fn ext_dispenser_releases_a_deterministic_four_second_fountain() {
    let mut arena = arena();
    arena.object_count = 12;
    arena.ramp.enabled = false;
    let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut runtime = SphereRuntime::new("fountain".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    assert!(runtime.balls.iter().all(|ball| !ball.active));
    assert!(
        runtime.ball_spawn[1] < 1.0,
        "temporary semantic offset should lower the dispenser"
    );
    assert!(runtime.ball_spawn[1] > pack.field_definition.floor_height_m);

    runtime.add_player(
        "driver".into(),
        "Driver".into(),
        "red".into(),
        Some("red-driver-1"),
        &arena,
    );
    // Players may enter during the lobby countdown, but the dispenser
    // remains closed until the authoritative match-start transition.
    runtime.tick(1.0 / 60.0);
    assert!(runtime.balls.iter().all(|ball| !ball.active));
    runtime.begin_match();
    for _ in 0..60 {
        runtime.tick(1.0 / 60.0);
    }
    let released_after_one_second = runtime.balls.iter().filter(|ball| ball.active).count();
    assert!(released_after_one_second > 0 && released_after_one_second < arena.object_count);
    assert!(
        runtime
            .balls
            .iter()
            .any(|ball| ball.active && ball.position[2] > runtime.ball_spawn[2])
    );

    for _ in 0..181 {
        runtime.tick(1.0 / 60.0);
    }
    assert!(runtime.balls.iter().all(|ball| ball.active));
}

#[test]
fn robot_drive_is_bounded_and_wakes_contacts() {
    let mut arena = arena();
    arena.object_count = 32;
    let mut runtime = SphereRuntime::new("test".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
    runtime.set_player_input("p", 0.25, 1.0, 0.0, 0.0, 1);
    for _ in 0..120 {
        runtime.apply_player_drive(&arena, 1.0 / 60.0);
        runtime.tick(1.0 / 60.0);
    }
    let player = &runtime.player_snapshots()[0];
    let speed = (player.velocity_x.powi(2) + player.velocity_z.powi(2)).sqrt();
    assert!(speed <= arena.robot.max_speed_mps + 0.25);
    assert!(player.heading_deg.abs() > 1.0);
    assert!((player.y - arena.robot.height_m * 0.5).abs() < 1.0e-6);
    assert_eq!(player.velocity_y, 0.0);
}

#[test]
fn rotated_robot_uses_rotated_field_contact_geometry() {
    let arena = arena();
    let collider = FieldCollider {
        id: "thin-panel".into(),
        min: [-0.025, 0.0, -1.0],
        max: [0.025, 0.5, 1.0],
        center: [0.0, 0.25, 0.0],
        half_extents: [0.025, 0.25, 1.0],
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let contact = robot_field_obb_contact(
        [0.30, 0.25, 0.0],
        std::f32::consts::FRAC_PI_4,
        [
            arena.robot.width_m * 0.5,
            arena.robot.height_m * 0.5,
            arena.robot.length_m * 0.5,
        ],
        &collider,
    );
    assert!(
        contact.is_some(),
        "a 45° chassis corner should contact the thin panel"
    );
    let (normal, penetration) = contact.unwrap();
    assert!(normal[0] > 0.9);
    assert!(penetration > 0.0);
    let (x_extent, z_extent) = robot_planar_extents(&arena.robot, std::f32::consts::FRAC_PI_4);
    assert!((x_extent - 0.3535534).abs() < 1.0e-4);
    assert!((z_extent - 0.3535534).abs() < 1.0e-4);
}

#[test]
fn thin_panel_recovery_keeps_the_approach_side() {
    let panel = FieldCollider {
        id: "su-polycarbonate".into(),
        min: [-0.025, 0.0, -1.0],
        max: [0.025, 2.0, 1.0],
        center: [0.0, 1.0, 0.0],
        half_extents: [0.025, 1.0, 1.0],
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let (axis, sign, _) = inside_obb_exit_face([-0.10, 1.0, 0.0], [0.01, 0.0, 0.0], &panel);
    assert_eq!(axis, 0);
    assert!(sign < 0.0, "a ball entering from the left stays left");
}

#[test]
fn balls_cannot_lift_the_carpet_supported_robot() {
    let mut arena = arena();
    arena.object_count = 8;
    let mut runtime = SphereRuntime::new("grounded-robot".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
    let robot_position = runtime.players["p"].position;
    for (index, ball) in runtime.balls.iter_mut().enumerate() {
        ball.position = [
            robot_position[0] + (index as f32 - 3.5) * 0.04,
            arena.ball.radius_m(),
            robot_position[2],
        ];
        ball.velocity = [0.0; 3];
    }
    for _ in 0..120 {
        runtime.tick(1.0 / 60.0);
    }
    let player = &runtime.player_snapshots()[0];
    assert!((player.y - arena.robot.height_m * 0.5).abs() < 1.0e-6);
    assert_eq!(player.velocity_y, 0.0);
}

#[test]
fn penetration_correction_does_not_create_launch_velocity() {
    let mut arena = arena();
    arena.object_count = 2;
    arena.gravity_scale = 0.0;
    arena.ramp.enabled = false;
    let mut runtime = SphereRuntime::new("split-impulse".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    let radius = arena.ball.radius_m();
    runtime.balls[0].position = [-radius * 0.2, 1.0, 0.0];
    runtime.balls[1].position = [radius * 0.2, 1.0, 0.0];
    runtime.balls[0].velocity = [0.0; 3];
    runtime.balls[1].velocity = [0.0; 3];

    runtime.tick(1.0 / 60.0);

    assert!(length_sq(runtime.balls[0].velocity) < 1.0e-8);
    assert!(length_sq(runtime.balls[1].velocity) < 1.0e-8);
    assert!(
        runtime.balls[1].position[0] - runtime.balls[0].position[0]
            >= arena.ball.diameter_m - 0.001
    );
}

#[test]
fn drivetrain_stalls_against_field_wall() {
    let mut arena = arena();
    arena.object_count = 0;
    arena.ramp.enabled = false;
    let mut runtime = SphereRuntime::new("wall-stall".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
    let wall_limit = -SphereRuntime::FIELD_HALF_EXTENT + arena.robot.length_m * 0.5;
    let player = runtime.players.get_mut("p").unwrap();
    player.position = [0.0, arena.robot.height_m * 0.5, wall_limit];
    player.yaw = 0.0;
    runtime.set_player_input("p", 0.0, 1.0, 0.0, 0.0, 1);

    for _ in 0..180 {
        runtime.apply_player_drive(&arena, 1.0 / 60.0);
        runtime.tick(1.0 / 60.0);
    }

    let player = runtime.players.get("p").unwrap();
    assert!((player.position[2] - wall_limit).abs() < 1.0e-5);
    assert!(player.velocity[2].abs() < 1.0e-5);
}

#[test]
fn drivetrain_slides_along_wall_after_shallow_impact() {
    let mut arena = arena();
    arena.object_count = 0;
    arena.ramp.enabled = false;
    let mut runtime = SphereRuntime::new("wall-slide".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);

    // Drive almost into the negative-Z perimeter wall. Forward has a
    // small X component, which should carry the robot along the wall
    // rather than being erased by virtual lateral wheel scrub.
    let yaw = 0.20;
    let (_, z_extent) = robot_planar_extents(&arena.robot, yaw);
    let wall_limit = -SphereRuntime::FIELD_HALF_EXTENT + z_extent;
    let player = runtime.players.get_mut("p").unwrap();
    player.position = [0.0, arena.robot.height_m * 0.5, wall_limit];
    player.yaw = yaw;
    runtime.set_player_input("p", 0.0, 1.0, 0.0, 0.0, 1);

    for _ in 0..180 {
        runtime.apply_player_drive(&arena, 1.0 / 60.0);
        runtime.tick(1.0 / 60.0);
    }

    let player = runtime.players.get("p").unwrap();
    assert!(player.position[2] >= wall_limit - 1.0e-4);
    assert!(
        player.position[0].abs() > 0.35,
        "robot did not slide along wall: {:?}",
        player.position
    );
    assert!(
        player.velocity[0].abs() > 0.1,
        "robot lost its wall-parallel velocity: {:?}",
        player.velocity
    );
}

#[test]
fn trapped_ball_stays_bounded_and_pushes_back_on_robot() {
    let mut arena = arena();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    arena.robot.intake_enabled = false;
    let mut runtime = SphereRuntime::new("trapped-ball".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
    let ball_limit = -SphereRuntime::FIELD_HALF_EXTENT + arena.ball.radius_m();
    let robot_start = ball_limit + arena.ball.radius_m() + arena.robot.length_m * 0.5;
    let player = runtime.players.get_mut("p").unwrap();
    player.position = [0.0, arena.robot.height_m * 0.5, robot_start];
    player.yaw = 0.0;
    runtime.balls[0].position = [0.0, arena.ball.radius_m(), ball_limit];
    runtime.balls[0].velocity = [0.0; 3];
    runtime.set_player_input("p", 0.0, 1.0, 0.0, 0.0, 1);

    let mut maximum_ball_speed = 0.0_f32;
    for _ in 0..240 {
        runtime.apply_player_drive(&arena, 1.0 / 60.0);
        runtime.tick(1.0 / 60.0);
        maximum_ball_speed = maximum_ball_speed.max(length_sq(runtime.balls[0].velocity).sqrt());
    }

    let player = runtime.players.get("p").unwrap();
    assert!(runtime.balls[0].position[2] >= ball_limit - 1.0e-5);
    assert!(
        maximum_ball_speed < 2.0,
        "ball reached {maximum_ball_speed:.3} m/s"
    );
    assert!(
        player.position[2] >= robot_start - 0.01,
        "robot tunneled into trapped ball: z={}",
        player.position[2]
    );
    assert!(player.velocity[2].abs() < 0.25);
}

#[test]
fn overlapping_balls_separate_and_rebound() {
    let mut arena = arena();
    arena.object_count = 2;
    let mut runtime = SphereRuntime::new("contacts".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    let radius = arena.ball.radius_m();
    runtime.balls[0].position = [-radius * 0.8, 1.0, 0.0];
    runtime.balls[1].position = [radius * 0.8, 1.0, 0.0];
    runtime.balls[0].velocity = [1.0, 0.0, 0.0];
    runtime.balls[1].velocity = [-1.0, 0.0, 0.0];
    runtime.tick(1.0 / 60.0);
    let separation = (runtime.balls[1].position[0] - runtime.balls[0].position[0]).abs();
    assert!(separation >= arena.ball.diameter_m - 0.001);
    assert!(runtime.balls[0].velocity[0] < 0.0);
    assert!(runtime.balls[1].velocity[0] > 0.0);
}

#[test]
fn harder_ball_impacts_use_less_restitution() {
    fn rebound_ratio(mut arena: ArenaConfig, speed: f32) -> f32 {
        arena.object_count = 2;
        arena.gravity_scale = 0.0;
        arena.ball.drag_coefficient = 0.0;
        arena.ball.linear_damping = 0.0;
        arena.ball.angular_damping = 0.0;
        arena.ramp.enabled = false;
        let mut runtime = SphereRuntime::new("restitution".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        let radius = arena.ball.radius_m();
        let dt = 1.0 / 60.0;
        runtime.balls[0].position = [-radius - speed * dt, 1.0, 0.0];
        runtime.balls[1].position = [radius + speed * dt, 1.0, 0.0];
        runtime.balls[0].velocity = [speed, 0.0, 0.0];
        runtime.balls[1].velocity = [-speed, 0.0, 0.0];
        runtime.tick(dt as f64);
        (-runtime.balls[0].velocity[0] / speed).max(0.0)
    }

    let arena = arena();
    let gentle = rebound_ratio(arena.clone(), 0.25);
    let hard = rebound_ratio(arena, 2.0);
    assert!(gentle > hard, "gentle={gentle:.3}, hard={hard:.3}");
    assert!(gentle <= 1.0 && hard >= 0.0);
}

#[test]
fn carpet_friction_converts_sliding_to_spin() {
    let arena = arena();
    let radius = arena.ball.radius_m();
    let mut ball = Ball {
        position: [0.0, radius, 0.0],
        previous_position: [0.0, radius, 0.0],
        velocity: [2.0, -1.0, 0.0],
        pre_solve_velocity: [2.0, -1.0, 0.0],
        angular_velocity: [0.0; 3],
        quiet_ticks: 0,
        sleeping: false,
        grounded: true,
        on_ramp: false,
        active: true,
        release_at_seconds: 0.0,
        released: true,
        last_outtake_alliance: None,
    };
    resolve_sphere_surface_velocity(
        &mut ball,
        [0.0, 1.0, 0.0],
        [0.0; 3],
        &arena.floor.restitution_curve,
        arena.floor.static_friction,
        arena.floor.dynamic_friction,
        radius,
        arena.ball.mass_kg,
        arena.ball.inertia_factor,
        0.0,
        arena.solver.restitution_velocity_threshold_mps,
    );

    let contact_velocity = add(
        ball.velocity,
        cross(ball.angular_velocity, [0.0, -radius, 0.0]),
    );
    assert!(ball.velocity[0] < 2.0);
    assert!(ball.angular_velocity[2] < 0.0);
    assert!(contact_velocity[0].abs() < 0.01);
}

#[test]
fn quadratic_air_drag_is_observable_at_high_speed() {
    let mut drag_arena = arena();
    drag_arena.object_count = 1;
    drag_arena.gravity_scale = 0.0;
    drag_arena.ramp.enabled = false;
    let mut vacuum_arena = drag_arena.clone();
    vacuum_arena.ball.drag_coefficient = 0.0;
    let mut with_drag = SphereRuntime::new("drag".into(), "fgc-2026".into(), 0);
    let mut without_drag = SphereRuntime::new("vacuum".into(), "fgc-2026".into(), 0);
    with_drag.create_test_arena(&drag_arena);
    without_drag.create_test_arena(&vacuum_arena);
    with_drag.balls[0].position = [0.0, 2.0, 0.0];
    without_drag.balls[0].position = [0.0, 2.0, 0.0];
    with_drag.balls[0].velocity = [10.0, 0.0, 0.0];
    without_drag.balls[0].velocity = [10.0, 0.0, 0.0];

    with_drag.integrate(&drag_arena, 1.0 / 60.0);
    without_drag.integrate(&vacuum_arena, 1.0 / 60.0);
    assert!(with_drag.balls[0].velocity[0] < without_drag.balls[0].velocity[0]);
}

#[test]
fn powered_intake_roller_drives_a_contacting_ball() {
    fn roller_velocity(mut arena: ArenaConfig, power: f32) -> f32 {
        arena.object_count = 1;
        arena.ramp.enabled = false;
        let mut runtime = SphereRuntime::new("intake".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
        let player = runtime.players.get_mut("p").unwrap();
        player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        player.yaw = 0.0;
        player.intake_power = power;
        runtime.balls[0].position = [
            0.0,
            arena.ball.radius_m(),
            -arena.robot.intake_forward_offset_m - 0.03,
        ];
        runtime.balls[0].velocity = [0.0; 3];
        runtime.balls[0].pre_solve_velocity = [0.0; 3];
        runtime.apply_contact_velocities(&arena, 1.0 / 60.0);
        runtime.balls[0].velocity[2]
    }

    let arena = arena();
    let idle = roller_velocity(arena.clone(), 0.0);
    let powered = roller_velocity(arena, 1.0);
    assert!(idle.abs() < 0.001);
    assert!(
        powered > 0.1,
        "floor ball should be pulled into the hopper, powered velocity={powered:.3}"
    );
}

#[test]
fn ramp_contact_rolls_downhill() {
    let mut arena = arena();
    arena.object_count = 1;
    let mut runtime = SphereRuntime::new("ramp".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    let angle = arena.ramp.angle_deg.to_radians();
    let start_z = arena.ramp.start_z + arena.ramp.length_m * 0.65;
    runtime.balls[0].position = [
        arena.ramp.center_x,
        (arena.ball.radius_m() + (start_z - arena.ramp.start_z) * angle.sin()) / angle.cos(),
        start_z,
    ];
    runtime.balls[0].velocity = [0.0; 3];
    for _ in 0..30 {
        runtime.tick(1.0 / 60.0);
    }
    assert!(
        runtime.balls[0].position[2] < start_z - 0.05,
        "ball z={} did not roll down from {start_z}",
        runtime.balls[0].position[2]
    );
}

#[test]
fn powered_intake_captures_a_ball_in_the_roller_mouth() {
    let mut arena = arena();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    let mut runtime = SphereRuntime::new("capture".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
    let player = runtime.players.get_mut("p").unwrap();
    player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
    player.yaw = 0.0;
    player.intake_power = 1.0;
    runtime.balls[0].position = [
        0.0,
        arena.ball.radius_m(),
        -arena.robot.intake_forward_offset_m,
    ];
    runtime.balls[0].velocity = [0.0; 3];
    runtime.balls[0].pre_solve_velocity = [0.0; 3];
    for _ in 0..20 {
        runtime.tick(1.0 / 60.0);
    }
    assert!(
        !runtime.balls[0].active,
        "ball should be captured into the hopper"
    );
    assert_eq!(runtime.players["p"].stored.len(), 1);
    assert_eq!(runtime.players["p"].stored[0], 0);
    assert!(
        runtime
            .drain_semantic_events()
            .iter()
            .any(|event| event.kind == "intake")
    );
}

#[test]
fn driving_with_intake_captures_a_floor_ball() {
    let mut arena = arena();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    let mut runtime = SphereRuntime::new("drive-intake".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
    runtime.set_player_input("p", 0.0, 1.0, 1.0, 0.0, 1);
    let player = runtime.players.get_mut("p").unwrap();
    player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
    player.yaw = 0.0;
    runtime.balls[0].position = [0.0, arena.ball.radius_m(), -0.6];
    runtime.balls[0].velocity = [0.0; 3];
    runtime.balls[0].pre_solve_velocity = [0.0; 3];
    for _ in 0..180 {
        runtime.apply_player_drive(&arena, 1.0 / 60.0);
        runtime.tick(1.0 / 60.0);
        if runtime.players["p"].stored.len() == 1 {
            break;
        }
    }
    assert_eq!(
        runtime.players["p"].stored.len(),
        1,
        "driving into a floor ball with intake should capture it"
    );
    assert_eq!(runtime.players["p"].stored[0], 0);
    assert!(
        runtime
            .drain_semantic_events()
            .iter()
            .any(|event| event.kind == "intake")
    );
}

#[test]
fn zero_capacity_mech_override_blocks_intake() {
    let mut arena = arena();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    let mut runtime = SphereRuntime::new("blocked".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
    let player = runtime.players.get_mut("p").unwrap();
    player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
    player.yaw = 0.0;
    player.intake_power = 1.0;
    player.mech = MechSpec {
        capacity: Some(0),
        ..MechSpec::default()
    };
    runtime.balls[0].position = [
        0.0,
        arena.ball.radius_m(),
        -arena.robot.intake_forward_offset_m,
    ];
    runtime.balls[0].velocity = [0.0; 3];
    runtime.balls[0].pre_solve_velocity = [0.0; 3];
    for _ in 0..20 {
        runtime.tick(1.0 / 60.0);
    }
    assert!(
        runtime.balls[0].active,
        "ball stays free when hopper capacity is zero"
    );
    assert!(runtime.players["p"].stored.is_empty());
}

#[test]
fn outtake_launches_a_stored_ball_through_the_wide_flywheel() {
    let mut arena = arena();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    let mut runtime = SphereRuntime::new("launch".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
    let player = runtime.players.get_mut("p").unwrap();
    player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
    player.yaw = 0.0;
    player.outtake_power = 1.0;
    player.stored.push_back(0);
    for _ in 0..25 {
        runtime.tick(1.0 / 60.0);
    }
    let ball = &runtime.balls[0];
    assert!(ball.active, "launched ball should be active");
    assert!(
        runtime.players["p"].stored.is_empty(),
        "hopper should drain"
    );
    assert!(
        ball.velocity[1] > 0.5,
        "upward launch velocity was {}",
        ball.velocity[1]
    );
    assert!(
        ball.velocity[2] < 0.0,
        "forward launch at yaw 0 was {}",
        ball.velocity[2]
    );
    assert!(
        ball.position[1] > arena.robot.height_m + arena.ball.radius_m(),
        "launch height was {}",
        ball.position[1]
    );
    assert!(
        runtime
            .drain_semantic_events()
            .iter()
            .any(|event| event.kind == "outtake")
    );
}

#[test]
fn replay_is_deterministic() {
    let mut arena = arena();
    arena.object_count = 128;
    let mut left = SphereRuntime::new("left".into(), "fgc-2026".into(), 42);
    let mut right = SphereRuntime::new("right".into(), "fgc-2026".into(), 42);
    left.create_test_arena(&arena);
    right.create_test_arena(&arena);
    for _ in 0..240 {
        left.tick(1.0 / 60.0);
        right.tick(1.0 / 60.0);
    }
    assert_eq!(
        left.field_object_positions(),
        right.field_object_positions()
    );
}
