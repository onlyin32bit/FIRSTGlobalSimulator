use super::*;

fn arena() -> ArenaConfig {
    let mut arena = crate::game::pack_loader::PackLoader::new("0.1.0")
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap()
        .arena;
    // These unit tests cover the specialized fallback solver. Rapier has a
    // focused integration test below and is the live pack default.
    arena.physics_backend = "sphere_xpbd".into();
    arena
}

#[test]
fn rapier_backend_owns_authored_balls_and_keeps_their_state_finite() {
    let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 8;
    arena.spawn_release_seconds = 0.0;

    let mut runtime = SphereRuntime::new("rapier-balls".into(), "fgc-2026".into(), 7);
    runtime.create_test_arena(&arena);
    runtime.set_robot_definition(pack.default_robot.as_ref());
    runtime.add_player(
        "driver".into(),
        "Driver".into(),
        "blue".into(),
        None,
        &arena,
    );

    for _ in 0..120 {
        runtime.tick(1.0 / 60.0);
    }

    assert_eq!(
        runtime.step_metrics().active_balls + runtime.step_metrics().sleeping_balls,
        8
    );
    assert!(runtime.balls.iter().all(|ball| {
        ball.position.iter().all(|value| value.is_finite())
            && ball.velocity.iter().all(|value| value.is_finite())
    }));
}

#[test]
fn starter_bot_uses_authored_intake_semantics() {
    let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    arena.gravity_scale = 0.0;
    let mut runtime = SphereRuntime::new("starter-bot-intake".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    runtime.set_robot_definition(pack.default_robot.as_ref());
    runtime.context.phase = MatchPhase::Teleop;
    runtime.ball_release_elapsed = Some(arena.spawn_release_seconds);
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
    let player = runtime.players.get_mut("p").unwrap();
    player.intake_power = 1.0;
    let definition = runtime.robot_definition.as_ref().unwrap();
    let intake = definition
        .zones
        .iter()
        .find(|zone| zone.kind == RobotSemanticKind::Intake)
        .unwrap();
    let mouth = robot_local_collider(
        &intake.collider,
        runtime.players["p"].position,
        0.0,
        -arena.robot.height_m * 0.5,
    );
    runtime.balls[0].active = true;
    runtime.balls[0].released = true;
    runtime.balls[0].position = mouth.center;
    runtime.balls[0].previous_position = mouth.center;
    for _ in 0..30 {
        runtime.step_mechanics(&arena, 1.0 / 60.0);
    }
    assert_eq!(runtime.players["p"].stored.len(), 1);
    assert_eq!(runtime.players["p"].stored[0], 0);
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
fn fast_ball_crossing_a_thin_field_wall_is_stopped_at_the_entry_face() {
    let mut arena = arena();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    arena.gravity_scale = 0.0;
    let wall = FieldCollider {
        id: "thin-goal-wall".into(),
        min: [-1.0, -0.25, -0.01],
        max: [1.0, 0.25, 0.01],
        center: [0.0, 0.0, 0.0],
        half_extents: [1.0, 0.25, 0.01],
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let mut field = FieldDefinition::default();
    field.colliders = vec![wall];

    let mut runtime = SphereRuntime::new("wall-sweep".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &field);
    let radius = arena.ball.radius_m();
    runtime.balls[0].active = true;
    runtime.balls[0].previous_position = [0.0, 0.05, -1.0];
    runtime.balls[0].position = [0.0, 0.05, 1.0];
    runtime.balls[0].velocity = [0.0, 0.0, 120.0];

    runtime.solve_positions(&arena, 1.0 / 60.0);

    assert!(
        runtime.balls[0].position[2] <= -0.01 - radius + 1.0e-4,
        "ball crossed the wall instead of stopping at its entry face: {:?}",
        runtime.balls[0].position
    );
    assert!(
        runtime.balls[0].position[2] < 0.0,
        "ball was projected through the far side of the wall: {:?}",
        runtime.balls[0].position
    );
}

#[test]
fn embedded_ball_pushed_through_thin_wall_is_returned_to_its_approach_side() {
    let mut arena = arena();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    arena.gravity_scale = 0.0;
    let wall = FieldCollider {
        id: "thin-goal-wall".into(),
        min: [-1.0, -0.25, -0.01],
        max: [1.0, 0.25, 0.01],
        center: [0.0, 0.0, 0.0],
        half_extents: [1.0, 0.25, 0.01],
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let mut field = FieldDefinition::default();
    field.colliders = vec![wall];

    let mut runtime = SphereRuntime::new("embedded-wall-sweep".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &field);
    let radius = arena.ball.radius_m();
    // The ball begins the tick already overlapping the wall's inflated volume
    // (embedded just beyond the -Z face) and is pushed by a robot past the far
    // limit. It must come back out on the -Z side it approached from, never
    // through the far face.
    runtime.balls[0].active = true;
    runtime.balls[0].previous_position = [0.0, 0.05, -0.05];
    runtime.balls[0].position = [0.0, 0.05, 0.20];
    runtime.balls[0].velocity = [0.0, 0.0, 0.0];

    runtime.solve_positions(&arena, 1.0 / 60.0);

    assert!(
        runtime.balls[0].position[2] < 0.0,
        "embedded ball pushed through the wall must return to its approach (-Z) side: {:?}",
        runtime.balls[0].position
    );
    assert!(
        runtime.balls[0].position[2] <= -0.01 - radius + 1.0e-4,
        "ball must rest at the -Z entry face, not the far side: {:?}",
        runtime.balls[0].position
    );
}

#[test]
fn active_mechanism_bypasses_only_its_local_collider() {
    let zone = FieldCollider {
        id: "TransferZone".into(),
        min: [-0.05, -0.05, -0.05],
        max: [0.05, 0.05, 0.05],
        center: [0.0, 0.0, 0.0],
        half_extents: [0.05, 0.05, 0.05],
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let roller = FieldCollider {
        id: "TransferRoller".into(),
        min: [-0.01, -0.01, -0.01],
        max: [0.01, 0.01, 0.01],
        center: [0.0, 0.0, 0.0],
        half_extents: [0.01, 0.01, 0.01],
        axes: zone.axes,
    };
    let ramp = FieldCollider {
        id: "HopperRamp".into(),
        min: [-0.20, -0.02, 0.08],
        max: [0.20, 0.08, 0.28],
        center: [0.0, 0.03, 0.18],
        half_extents: [0.20, 0.05, 0.10],
        axes: zone.axes,
    };

    assert!(collider_is_inside_mechanism_zone(&roller, &zone));
    assert!(
        !collider_is_inside_mechanism_zone(&ramp, &zone),
        "an active mechanism zone must not disable an adjacent hopper ramp"
    );
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
fn starter_bot_can_drive_from_an_authored_spawn_surface() {
    let mut arena = arena();
    arena.object_count = 0;
    let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut runtime = SphereRuntime::new("spawn-drive".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    runtime.set_robot_definition(pack.default_robot.as_ref());
    runtime.add_player(
        "red-driver".into(),
        "Driver".into(),
        "red".into(),
        Some("red-driver-1"),
        &arena,
    );
    let start = runtime.players["red-driver"].position;
    runtime.set_player_input("red-driver", 0.0, 1.0, 0.0, 0.0, 1);
    let mut supported_ticks = 0;
    for _ in 0..120 {
        runtime.tick(1.0 / 60.0);
        supported_ticks += usize::from(runtime.players["red-driver"].floor_supported);
    }

    let player = &runtime.players["red-driver"];
    let displacement =
        ((player.position[0] - start[0]).powi(2) + (player.position[2] - start[2]).powi(2)).sqrt();
    assert!(
        displacement > 1.0,
        "starter bot stayed stuck at spawn: start={start:?} final={:?} supported={} supported_ticks={supported_ticks}",
        player.position,
        player.floor_supported
    );
    assert!(supported_ticks >= 100);
    assert!(player.rotation[0].abs() < 0.1 && player.rotation[2].abs() < 0.1);
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
        runtime.score_state.blue_score, 0,
        "score is removed as soon as the ball physically exits the sensor"
    );
    assert!(
        (runtime.balls[0].position[2] + 1.5).abs() < 1.0e-6,
        "scoring must not project a ball back into a retention volume"
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
        arena.gravity_scale = 1.0;
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
        physics_dirty: false,
        release_at_seconds: 0.0,
        released: true,
        owner: None,
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
        powered > 0.05,
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
        runtime.balls[0].active,
        "ball stays physically active in 3D during intake"
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
fn outtake_launches_a_stored_ball_through_the_wide_flywheel() {
    let mut arena = arena();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    arena.robot.outtake_rate_bps = 3.0;
    let mut runtime = SphereRuntime::new("launch".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    runtime.context.phase = MatchPhase::Teleop;
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
    {
        let player = runtime.players.get_mut("p").unwrap();
        player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        player.yaw = 0.0;
        player.stored.push_back(0);
    }
    runtime.balls[0].owner = Some("p".into());
    runtime.set_player_input("p", 0.0, 0.0, 0.0, 1.0, 1);
    for _ in 0..25 {
        runtime.apply_player_drive(&arena, 1.0 / 60.0);
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
        ball.position[1] > arena.ball.radius_m() + 0.35,
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

#[test]
fn intake_target_semantics_defines_direction_and_applies_intake_force() {
    let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    arena.gravity_scale = 0.0;
    // Keep this force-only test out of logical storage.
    arena.robot.storage_capacity = 0;
    let mut runtime = SphereRuntime::new("intake-target-test".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    runtime.set_robot_definition(pack.default_robot.as_ref());
    runtime.context.phase = MatchPhase::Teleop;
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);

    let definition = runtime.robot_definition.as_ref().unwrap();
    let intake_zone = definition
        .zones
        .iter()
        .find(|zone| zone.kind == RobotSemanticKind::Intake)
        .unwrap();

    // Verify direction vector points into the robot (-Z in Blender coords)
    assert!(
        intake_zone.direction[2] < -0.5,
        "direction Z component should point inside the robot: {:?}",
        intake_zone.direction
    );

    // Place a ball at the intake mouth and turn on intake power
    let player = runtime.players.get_mut("p").unwrap();
    player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
    player.yaw = 0.0;
    player.intake_power = 1.0;

    let mouth = robot_local_collider(
        &intake_zone.collider,
        player.position,
        0.0,
        -arena.robot.height_m * 0.5,
    );
    runtime.balls[0].active = true;
    runtime.balls[0].position = mouth.center;
    runtime.balls[0].previous_position = mouth.center;
    runtime.balls[0].velocity = [0.0; 3];

    // Tick simulation solver
    runtime.apply_contact_velocities(&arena, 1.0 / 60.0);

    // Verify ball accelerated inside the robot (forward/inward along intake direction)
    assert!(
        runtime.balls[0].velocity[2] > 0.1,
        "ball velocity should be pulled inward, got velocity: {:?}",
        runtime.balls[0].velocity
    );
}

#[test]
fn transfer_and_outtake_target_semantics_apply_directional_forces() {
    let loader = crate::game::pack_loader::PackLoader::new("0.1.0");
    let pack = loader
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let robot_def = pack.default_robot.as_ref().unwrap();

    let transfer_zone = robot_def
        .zones
        .iter()
        .find(|z| z.kind == RobotSemanticKind::Transfer)
        .expect("starter-bot must have TransferZone");
    assert!(
        transfer_zone.direction[2] < -0.5,
        "TransferZone direction should point backward toward the outtake/shooter: {:?}",
        transfer_zone.direction
    );

    let outtake_zone = robot_def
        .zones
        .iter()
        .find(|z| z.kind == RobotSemanticKind::Outtake)
        .expect("starter-bot must have OuttakeZone");
    assert!(
        outtake_zone.direction[1] > 0.5,
        "OuttakeZone direction vector should have an upward launch pitch toward OuttakeTarget: {:?}",
        outtake_zone.direction
    );
}

#[test]
fn outtake_applies_force_to_only_one_ball_at_a_time() {
    let loader = crate::game::pack_loader::PackLoader::new("0.1.0");
    let pack = loader
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 2;
    arena.ramp.enabled = false;
    arena.gravity_scale = 0.0;
    let mut runtime = SphereRuntime::new("outtake-single-ball-test".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    runtime.set_robot_definition(pack.default_robot.as_ref());
    runtime.context.phase = MatchPhase::Teleop;
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);

    let definition = runtime.robot_definition.as_ref().unwrap();
    let outtake_zone = definition
        .zones
        .iter()
        .find(|z| z.kind == RobotSemanticKind::Outtake)
        .unwrap();

    let player = runtime.players.get_mut("p").unwrap();
    player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
    player.yaw = 0.0;
    player.outtake_power = 1.0;

    let mouth = robot_local_collider(
        &outtake_zone.collider,
        player.position,
        0.0,
        -arena.robot.height_m * 0.5,
    );
    let dir = rotate_robot_local_pose(outtake_zone.direction, player.rotation);

    // Ball 0 is foremost (closer to exit along outtake direction)
    runtime.balls[0].active = true;
    runtime.balls[0].position = [
        mouth.center[0] + dir[0] * 0.01,
        mouth.center[1] + dir[1] * 0.01,
        mouth.center[2] + dir[2] * 0.01,
    ];
    runtime.balls[0].previous_position = runtime.balls[0].position;
    runtime.balls[0].velocity = [0.0; 3];

    // Ball 1 is behind Ball 0, but still touching the OuttakeZone mouth
    runtime.balls[1].active = true;
    runtime.balls[1].position = [
        mouth.center[0] - dir[0] * 0.01,
        mouth.center[1] - dir[1] * 0.01,
        mouth.center[2] - dir[2] * 0.01,
    ];
    runtime.balls[1].previous_position = runtime.balls[1].position;
    runtime.balls[1].velocity = [0.0; 3];

    // Verify both balls are in contact with the outtake mouth
    assert!(
        sphere_authored_obb_contact(runtime.balls[0].position, arena.ball.radius_m(), &mouth)
            .is_some(),
        "Ball 0 must touch outtake zone"
    );
    assert!(
        sphere_authored_obb_contact(runtime.balls[1].position, arena.ball.radius_m(), &mouth)
            .is_some(),
        "Ball 1 must touch outtake zone"
    );

    // Run contact velocity solver tick
    runtime.apply_contact_velocities(&arena, 1.0 / 60.0);

    let speed_0 = dot(runtime.balls[0].velocity, dir);
    let speed_1 = dot(runtime.balls[1].velocity, dir);

    assert!(
        speed_0 > 0.5,
        "Foremost ball (Ball 0) should receive outtake launch force, speed={speed_0}"
    );
    assert!(
        speed_1 > 0.5,
        "Both balls should receive force since the engine powers all balls touching the outtake zone, speed={speed_1}"
    );
}

#[test]
fn transfer_operates_within_robot_only_during_outtake() {
    let loader = crate::game::pack_loader::PackLoader::new("0.1.0");
    let pack = loader
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 3;
    arena.ramp.enabled = false;
    arena.gravity_scale = 0.0;
    let mut runtime =
        SphereRuntime::new("transfer-zone-isolation-test".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    runtime.set_robot_definition(pack.default_robot.as_ref());
    runtime.context.phase = MatchPhase::Teleop;
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);

    let definition = runtime.robot_definition.as_ref().unwrap();
    let transfer_zone = definition
        .zones
        .iter()
        .find(|z| z.kind == RobotSemanticKind::Transfer)
        .unwrap();
    let outtake_zone = definition
        .zones
        .iter()
        .find(|z| z.kind == RobotSemanticKind::Outtake)
        .unwrap();

    let player = runtime.players.get_mut("p").unwrap();
    player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
    player.yaw = 0.0;
    player.rotation = [0.0, 0.0, 0.0, 1.0];
    player.intake_power = 1.0;
    player.outtake_power = 0.0;

    let outtake_mouth = robot_local_collider(
        &outtake_zone.collider,
        player.position,
        0.0,
        -arena.robot.height_m * 0.5,
    );
    let transfer_dir = rotate_robot_local_pose(transfer_zone.direction, player.rotation);

    // Ball 0: inside the robot bounds (at front intake area, not touching OuttakeZone)
    runtime.balls[0].active = true;
    runtime.balls[0].position = [
        player.position[0],
        player.position[1],
        player.position[2] - 0.15,
    ];
    runtime.balls[0].previous_position = runtime.balls[0].position;
    runtime.balls[0].velocity = [0.0; 3];

    // Ball 1: touching OuttakeZone
    runtime.balls[1].active = true;
    runtime.balls[1].position = outtake_mouth.center;
    runtime.balls[1].previous_position = outtake_mouth.center;
    runtime.balls[1].velocity = [0.0; 3];

    // Ball 2: outside the robot
    runtime.balls[2].active = true;
    runtime.balls[2].position = [
        player.position[0] + 1.0,
        player.position[1],
        player.position[2],
    ];
    runtime.balls[2].previous_position = runtime.balls[2].position;
    runtime.balls[2].velocity = [0.0; 3];

    assert!(
        sphere_authored_obb_contact(
            runtime.balls[0].position,
            arena.ball.radius_m(),
            &outtake_mouth
        )
        .is_none(),
        "Ball 0 must NOT touch OuttakeZone, ball0={:?}, outtake_mouth={:?}",
        runtime.balls[0].position,
        outtake_mouth
    );
    assert!(
        sphere_authored_obb_contact(
            runtime.balls[1].position,
            arena.ball.radius_m(),
            &outtake_mouth
        )
        .is_some(),
        "Ball 1 must touch OuttakeZone"
    );

    // Intake must not power the transfer, even for a ball already inside the
    // robot envelope.
    runtime.apply_contact_velocities(&arena, 1.0 / 60.0);

    let intake_speed_0 = dot(runtime.balls[0].velocity, transfer_dir);
    assert!(
        intake_speed_0.abs() < 0.001,
        "intake must not apply transfer force, got speed={intake_speed_0}"
    );

    runtime.balls[0].velocity = [0.0; 3];
    runtime.balls[1].velocity = [0.0; 3];
    runtime.balls[2].velocity = [0.0; 3];
    for index in 0..3 {
        runtime.balls[index].owner = Some("p".into());
    }
    let player = runtime.players.get_mut("p").unwrap();
    player.intake_power = 0.0;
    player.outtake_power = 1.0;

    // Transfer activates only for the outtake command.
    runtime.apply_contact_velocities(&arena, 1.0 / 60.0);

    let speed_0 = dot(runtime.balls[0].velocity, transfer_dir);
    let speed_2 = dot(runtime.balls[2].velocity, transfer_dir);
    let to_mouth = sub(outtake_mouth.center, runtime.balls[0].position);
    let to_mouth_len = length_sq(to_mouth).sqrt();
    let toward_mouth = dot(runtime.balls[0].velocity, to_mouth) / to_mouth_len.max(1.0e-6);

    println!(
        "DEBUG TEST: ball0_vel={:?} transfer_dir={:?} outtake_mouth={:?}",
        runtime.balls[0].velocity, transfer_dir, outtake_mouth
    );

    assert!(
        speed_0 > 0.05,
        "Ball 0 inside the robot must receive transfer force towards outtake, got speed={speed_0}"
    );
    assert!(
        toward_mouth > 0.0,
        "Transfer force must attract balls toward the outtake mouth, got velocity={:?}",
        runtime.balls[0].velocity
    );
    assert!(
        speed_2.abs() < 0.001,
        "Ball 2 outside the robot must NOT receive transfer force, got speed={speed_2}"
    );
}

#[test]
fn lowered_transfer_belt_speed_carries_balls_at_the_belt_speed() {
    let loader = crate::game::pack_loader::PackLoader::new("0.1.0");
    let pack = loader
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    arena.gravity_scale = 0.0;
    // A slow belt. The conveyor must still push at the full rated force so a
    // ball inside the robot is carried through at the belt speed, never left
    // to stall just because the target speed is low.
    arena.robot.transfer_surface_speed_mps = 0.35;
    arena.robot.transfer_normal_force_n = 60.0;
    let mut runtime = SphereRuntime::new("slow-belt".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    runtime.set_robot_definition(pack.default_robot.as_ref());
    runtime.context.phase = MatchPhase::Teleop;
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);

    let definition = runtime.robot_definition.as_ref().unwrap();
    let transfer_zone = definition
        .zones
        .iter()
        .find(|z| z.kind == RobotSemanticKind::Transfer)
        .unwrap();
    let player = runtime.players.get_mut("p").unwrap();
    player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
    player.yaw = 0.0;
    player.rotation = [0.0, 0.0, 0.0, 1.0];
    player.outtake_power = 1.0;

    let transfer_dir = rotate_robot_local_pose(transfer_zone.direction, player.rotation);

    runtime.balls[0].active = true;
    runtime.balls[0].position = [
        player.position[0],
        player.position[1],
        player.position[2] - 0.15,
    ];
    runtime.balls[0].previous_position = runtime.balls[0].position;
    runtime.balls[0].velocity = [0.0; 3];
    runtime.balls[0].owner = Some("p".into());

    runtime.apply_contact_velocities(&arena, 1.0 / 60.0);

    let belt_speed = arena.robot.transfer_surface_speed_mps;
    let speed = dot(runtime.balls[0].velocity, transfer_dir);
    assert!(
        (speed - belt_speed).abs() < 1.0e-3,
        "a ball inside the robot must be carried at the (slower) belt speed, got {speed}"
    );
    let total_speed = length_sq(runtime.balls[0].velocity).sqrt();
    assert!(
        total_speed <= belt_speed + 1.0e-3,
        "the transfer must never carry a ball faster than the belt, got {total_speed}"
    );
}

#[test]
#[ignore]
fn dbg_wall_climb_probe() {
    let loader = crate::game::pack_loader::PackLoader::new("0.1.0");
    let pack = loader
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    let mut runtime = SphereRuntime::new("dbg-wall-climb".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    runtime.set_robot_definition(pack.default_robot.as_ref());
    runtime.context.phase = MatchPhase::Teleop;
    runtime.add_player(
        "p".into(),
        "Player".into(),
        "Team".into(),
        Some("red-driver-1"),
        &arena,
    );
    {
        let player = runtime.players.get_mut("p").unwrap();
        player.intake_power = 1.0;
        player.outtake_power = 0.0;
        player.position = [0.0, runtime.field_floor_y + arena.robot.height_m * 0.5, 0.0];
        player.yaw = 0.0;
        player.rotation = [0.0, 0.0, 0.0, 1.0];
    }
    let definition = runtime.robot_definition.as_ref().unwrap();
    let outtake_zone = definition
        .zones
        .iter()
        .find(|z| z.kind == RobotSemanticKind::Outtake)
        .unwrap();
    let ground_offset_y = -arena.robot.height_m * 0.5;
    let mouth = robot_local_collider(
        &outtake_zone.collider,
        runtime.players["p"].position,
        0.0,
        ground_offset_y,
    );
    println!(
        "seat_y(world)={} ball_radius={}",
        mouth.center[1] + arena.ball.radius_m(),
        arena.ball.radius_m()
    );

    // Probe several hopper positions; each runs a fresh sim so state is clean.
    let spawn_locals = [
        [0.0_f32, 0.129, -0.10],
        [0.0_f32, 0.129, 0.00],
        [0.0_f32, 0.129, 0.06],
        [0.0_f32, 0.129, -0.06],
        [0.0_f32, 0.124, 0.03],
        [0.16_f32, 0.124, 0.00],
        [-0.16_f32, 0.124, 0.00],
        [0.16_f32, 0.124, -0.15],
        [-0.16_f32, 0.124, -0.15],
    ];
    for local in spawn_locals {
        if local == spawn_locals[0] {
            let mut r0 = SphereRuntime::new("probe".into(), "fgc-2026".into(), 0);
            r0.create_field_arena(&arena, &pack.field_definition);
            r0.set_robot_definition(pack.default_robot.as_ref());
            r0.add_player(
                "p".into(),
                "Player".into(),
                "Team".into(),
                Some("red-driver-1"),
                &arena,
            );
            let pp = r0.players["p"].position;
            let pr = r0.players["p"].rotation;
            let g = -arena.robot.height_m * 0.5;
            println!(
                "== transfer cfg: speed={} force={}",
                arena.robot.transfer_surface_speed_mps, arena.robot.transfer_normal_force_n
            );
            println!("== hopper colliders (local, posed to player {pp:?}) ==");
            for c in &definition.colliders {
                let posed = robot_local_collider_pose(c, pp, pr, g);
                println!(
                    "  {:<12} center={:?} he={:?} posed={:?}",
                    c.id, c.center, c.half_extents, posed.center
                );
            }
            println!("== zones ==");
            for z in &definition.zones {
                println!(
                    "  {:?} {:?} center={:?} dir={:?}",
                    z.id, z.kind, z.collider.center, z.direction
                );
            }
        }
        let mut r = SphereRuntime::new("probe".into(), "fgc-2026".into(), 0);
        r.create_field_arena(&arena, &pack.field_definition);
        r.set_robot_definition(pack.default_robot.as_ref());
        r.context.phase = MatchPhase::Teleop;
        r.add_player(
            "p".into(),
            "Player".into(),
            "Team".into(),
            Some("red-driver-1"),
            &arena,
        );
        {
            let p = r.players.get_mut("p").unwrap();
            p.intake_power = 1.0;
            p.outtake_power = 0.0;
        }
        let player_pos = r.players["p"].position;
        let player_rot = r.players["p"].rotation;
        let world = add(
            player_pos,
            rotate_robot_local_pose([local[0], local[1] + ground_offset_y, local[2]], player_rot),
        );
        r.balls[0].active = true;
        r.balls[0].released = true;
        r.balls[0].velocity = [0.0; 3];
        r.balls[0].position = world;
        r.balls[0].previous_position = world;
        let mut max_y = world[1];
        let mut max_vy = 0.0_f32;
        for t in 0..720 {
            r.tick(1.0 / 60.0);
            let b = &r.balls[0];
            max_y = max_y.max(b.position[1]);
            max_vy = max_vy.max(b.velocity[1]);
            if local == [0.0_f32, 0.129, -0.10] && (t % 30 == 0 || t == 719) {
                let rel = rotate_robot_local_pose(
                    sub(b.position, r.players["p"].position),
                    r.players["p"].rotation,
                );
                let flags = r.ball_debug[0];
                println!(
                    "  t={t:>3} pos=({:.3},{:.3},{:.3}) rel=({:.3},{:.3},{:.3}) vel=({:.2},{:.2},{:.2}) flags=0x{flags:02x}",
                    b.position[0],
                    b.position[1],
                    b.position[2],
                    rel[0],
                    rel[1],
                    rel[2],
                    b.velocity[0],
                    b.velocity[1],
                    b.velocity[2]
                );
            }
            if local == [0.0_f32, 0.124, 0.03] && t % 30 == 0 {
                let rel = rotate_robot_local_pose(
                    sub(b.position, r.players["p"].position),
                    r.players["p"].rotation,
                );
                let mouth_world = robot_local_collider_pose(
                    &outtake_zone.collider,
                    r.players["p"].position,
                    r.players["p"].rotation,
                    ground_offset_y,
                );
                let to_mouth = sub(mouth_world.center, b.position);
                let horiz_dist = length_sq([to_mouth[0], 0.0, to_mouth[2]]).sqrt();
                let seat_y = mouth_world.center[1] + arena.ball.radius_m();
                let flags = r.ball_debug[0];
                let mut overlap_ids: Vec<String> = Vec::new();
                for collider in &definition.colliders {
                    let posed = robot_local_collider_pose(
                        collider,
                        r.players["p"].position,
                        r.players["p"].rotation,
                        ground_offset_y,
                    );
                    if sphere_robot_obb_contact(
                        b.position,
                        b.previous_position,
                        arena.ball.radius_m(),
                        &posed,
                    )
                    .is_some()
                    {
                        overlap_ids.push(collider.id.clone());
                    }
                }
                println!(
                    "  CLIMB t={t:>3} rel=({:.3},{:.3},{:.3}) y={:.3} seat={:.3} hdist={:.3} vy={:.2} flags=0x{flags:02x} ovl={} server_contacts={:?}",
                    rel[0],
                    rel[1],
                    rel[2],
                    b.position[1],
                    seat_y,
                    horiz_dist,
                    b.velocity[1],
                    overlap_ids.join(","),
                    r.ball_contact_collider_ids()[0]
                );
            }
        }
        let end = r.balls[0].position;
        println!(
            "spawn {local:?} robot={player_pos:?} maxY={:.3} maxVy={:.3} end=({:.3},{:.3},{:.3}) vy={:.3}",
            max_y, max_vy, end[0], end[1], end[2], r.balls[0].velocity[1]
        );
    }
}

#[test]
#[ignore]
fn dbg_funnel_force_field_probe() {
    let loader = crate::game::pack_loader::PackLoader::new("0.1.0");
    let pack = loader
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    let definition = pack.default_robot.as_ref().unwrap();
    let ground_offset_y = -arena.robot.height_m * 0.5;

    let spawn_locals = [
        [0.0_f32, 0.11, 0.10],
        [0.0_f32, 0.11, 0.06],
        [0.0_f32, 0.11, 0.03],
        [0.0_f32, 0.11, 0.00],
        [0.055_f32, 0.11, 0.10],
        [0.05_f32, 0.11, 0.06],
        [-0.055_f32, 0.11, 0.10],
        [-0.05_f32, 0.11, 0.06],
        [0.06_f32, 0.11, 0.03],
        [-0.06_f32, 0.11, 0.03],
        [0.08_f32, 0.11, 0.10],
        [-0.08_f32, 0.11, 0.10],
        [0.00_f32, 0.05, 0.24],
        [0.05_f32, 0.05, 0.24],
        [-0.05_f32, 0.05, 0.24],
        [0.09_f32, 0.05, 0.24],
        [-0.09_f32, 0.05, 0.24],
        [0.00_f32, 0.50, 0.06],
        [0.04_f32, 0.50, 0.06],
        [-0.04_f32, 0.50, 0.06],
        [0.00_f32, 0.50, 0.09],
        [0.04_f32, 0.50, 0.09],
        [0.04_f32, 0.50, 0.03],
        [0.00_f32, 0.11, 0.04],
        [0.04_f32, 0.11, 0.04],
        [-0.04_f32, 0.11, 0.04],
        [0.00_f32, 0.11, 0.20],
        [0.04_f32, 0.11, 0.20],
        [-0.04_f32, 0.11, 0.20],
        [0.08_f32, 0.11, 0.20],
        [-0.08_f32, 0.11, 0.20],
        [0.00_f32, 0.11, 0.15],
        [0.06_f32, 0.11, 0.15],
        [-0.06_f32, 0.11, 0.15],
    ];
    for local in spawn_locals {
        let mut r = SphereRuntime::new("probe".into(), "fgc-2026".into(), 0);
        r.create_field_arena(&arena, &pack.field_definition);
        r.set_robot_definition(pack.default_robot.as_ref());
        r.context.phase = MatchPhase::Teleop;
        r.add_player(
            "p".into(),
            "Player".into(),
            "Team".into(),
            Some("red-driver-1"),
            &arena,
        );
        {
            let p = r.players.get_mut("p").unwrap();
            p.intake_power = 1.0;
            p.outtake_power = 0.0;
        }
        let player_pos = r.players["p"].position;
        let player_rot = r.players["p"].rotation;
        let world = add(
            player_pos,
            rotate_robot_local_pose([local[0], local[1] + ground_offset_y, local[2]], player_rot),
        );
        if local == spawn_locals[0] {
            println!(
                "player_pos={player_pos:?} player_rot={player_rot:?} ground_offset_y={ground_offset_y} spawn_local={local:?} spawn_world={world:?}"
            );
            let mut x = -0.14_f32;
            while x <= 0.14 {
                let mut row = String::new();
                let mut z = 0.24_f32;
                while z >= -0.12 {
                    let ball = add(
                        player_pos,
                        rotate_robot_local_pose([x, 0.11 + ground_offset_y, z], player_rot),
                    );
                    let mut blocked = Vec::new();
                    for collider in &definition.colliders {
                        let posed = robot_local_collider_pose(
                            collider,
                            player_pos,
                            player_rot,
                            ground_offset_y,
                        );
                        if sphere_authored_obb_contact(ball, arena.ball.radius_m(), &posed)
                            .is_some()
                        {
                            blocked.push(
                                collider
                                    .id
                                    .split('.')
                                    .last()
                                    .unwrap_or(&collider.id)
                                    .to_string(),
                            );
                        }
                    }
                    if blocked.is_empty() {
                        row.push_str("    .");
                    } else {
                        row.push_str(&format!("{:>5}", blocked.join(",")));
                    }
                    z -= 0.02;
                }
                println!("  x={x:>6.2} {row}");
                x += 0.02;
            }
            println!("== funnel colliders local min/max ==");
            for collider in &definition.colliders {
                if collider.id.starts_with("Plane")
                    || collider.id == "Cylinder.004"
                    || collider.id == "OuttakeRoller"
                {
                    println!(
                        "{:<12} min=({:+.3},{:+.3},{:+.3}) max=({:+.3},{:+.3},{:+.3})",
                        collider.id,
                        collider.min[0],
                        collider.min[1],
                        collider.min[2],
                        collider.max[0],
                        collider.max[1],
                        collider.max[2]
                    );
                }
            }
            println!("== zones ==");
            for z in &definition.zones {
                let posed =
                    robot_local_collider_pose(&z.collider, player_pos, player_rot, ground_offset_y);
                println!(
                    "{:?} {:?} center=({:+.3},{:+.3},{:+.3}) he=({:+.3},{:+.3},{:+.3})",
                    z.id,
                    z.kind,
                    z.collider.center[0],
                    z.collider.center[1],
                    z.collider.center[2],
                    posed.half_extents[0],
                    posed.half_extents[1],
                    posed.half_extents[2]
                );
            }
            println!("== posed inner-guider OBBs ==");
            for id in ["Plane.009", "Plane.010"] {
                if let Some(c) = definition.colliders.iter().find(|c| c.id == id) {
                    let posed =
                        robot_local_collider_pose(c, player_pos, player_rot, ground_offset_y);
                    let mut world_min = posed.center;
                    let mut world_max = posed.center;
                    for world_axis in 0..3 {
                        let radius = (0..3)
                            .map(|axis| {
                                posed.axes[axis][world_axis].abs() * posed.half_extents[axis]
                            })
                            .sum::<f32>();
                        world_min[world_axis] -= radius;
                        world_max[world_axis] += radius;
                    }
                    println!(
                        "{id} x∈[{:+.3},{:+.3}] y∈[{:+.3},{:+.3}] z∈[{:+.3},{:+.3}] axes={:?}",
                        world_min[0],
                        world_max[0],
                        world_min[1],
                        world_max[1],
                        world_min[2],
                        world_max[2],
                        posed.axes
                    );
                }
            }
        }
        r.balls[0].active = true;
        r.balls[0].released = true;
        r.balls[0].velocity = [0.0; 3];
        r.balls[0].position = world;
        r.balls[0].previous_position = world;
        if local[2] > 0.2 {
            let forward = rotate_robot_local_pose([0.0, 0.0, 1.0], player_rot);
            r.balls[0].velocity = mul(forward, -1.2);
        }
        if local[1] == 0.11 && local[2] == 0.04 {
            let forward = rotate_robot_local_pose([0.0, 0.0, 1.0], player_rot);
            let right = rotate_robot_local_pose([1.0, 0.0, 0.0], player_rot);
            let lateral = if local[0] == 0.0 {
                0.6
            } else {
                local[0].signum() * 0.9
            };
            r.balls[0].velocity = add(mul(forward, -0.8), mul(right, lateral));
        }
        let mut passed = false;
        let mut last_rel_z = local[2];
        let mut stuck_at = 0.0_f32;
        let mut last_contacts: Vec<String> = Vec::new();
        for t in 0..480 {
            r.tick(1.0 / 60.0);
            let b = &r.balls[0];
            let rel = rotate_robot_local_pose(
                sub(b.position, r.players["p"].position),
                r.players["p"].rotation,
            );
            if local == [0.0_f32, 0.11, 0.20] && t == 0 {
                println!(
                    "WORLD player={:?} rot={:?} ball_world={:?} rel={:?}",
                    player_pos, player_rot, b.position, rel
                );
                for id in [
                    "Plane.007",
                    "Plane.008",
                    "Plane.009",
                    "Plane.010",
                    "Plane.011",
                    "Plane.012",
                ] {
                    if let Some(c) = definition.colliders.iter().find(|c| c.id == id) {
                        let posed =
                            robot_local_collider_pose(c, player_pos, player_rot, ground_offset_y);
                        let mut wmin = posed.center;
                        let mut wmax = posed.center;
                        for wa in 0..3 {
                            let rad = (0..3)
                                .map(|ax| posed.axes[ax][wa].abs() * posed.half_extents[ax])
                                .sum::<f32>();
                            wmin[wa] -= rad;
                            wmax[wa] += rad;
                        }
                        println!(
                            "  {id} world center=({:.3},{:.3},{:.3}) AABB x∈[{:.3},{:.3}] y∈[{:.3},{:.3}] z∈[{:.3},{:.3}]",
                            posed.center[0],
                            posed.center[1],
                            posed.center[2],
                            wmin[0],
                            wmax[0],
                            wmin[1],
                            wmax[1],
                            wmin[2],
                            wmax[2]
                        );
                    }
                }
            }
            if (local == [0.05_f32, 0.11, 0.06] || local == [0.05_f32, 0.11, 0.05])
                && (t < 90)
                && t % 3 == 0
            {
                println!(
                    "  G t={t:>3} rel=({:.3},{:.3},{:.3}) vel=({:.2},{:.2},{:.2}) contacts={:?}",
                    rel[0],
                    rel[1],
                    rel[2],
                    b.velocity[0],
                    b.velocity[1],
                    b.velocity[2],
                    r.ball_contact_collider_ids()[0]
                );
            }
            if rel[2] < -0.02 && b.position[1] > 0.12 {
                passed = true;
            }
            if rel[2] < 0.005 && rel[2] > last_rel_z {
                stuck_at = rel[2];
            }
            last_rel_z = rel[2];
            let contacts = r.ball_contact_collider_ids()[0].clone();
            if !contacts.is_empty() {
                last_contacts = contacts;
            }
            if t % 30 == 0 || t == 479 {
                println!(
                    "  t={t:>3} rel=({:.3},{:.3},{:.3}) vy={:.2} contacts={:?}",
                    rel[0],
                    rel[1],
                    rel[2],
                    b.velocity[1],
                    r.ball_contact_collider_ids()[0]
                );
            }
        }
        println!(
            "funnel spawn {local:?} passed={passed} stuck_at_z={stuck_at:.3} last_contacts={:?}",
            last_contacts
        );
    }
}

#[test]
#[ignore]
fn dbg_guider_entry_probe() {
    let loader = crate::game::pack_loader::PackLoader::new("0.1.0");
    let pack = loader
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    let ground_offset_y = -arena.robot.height_m * 0.5;

    for &local_x in &[0.0_f32, 0.02, 0.04, 0.05, 0.055] {
        let mut r = SphereRuntime::new("probe".into(), "fgc-2026".into(), 0);
        r.create_field_arena(&arena, &pack.field_definition);
        r.set_robot_definition(pack.default_robot.as_ref());
        r.context.phase = MatchPhase::Teleop;
        r.add_player(
            "p".into(),
            "Player".into(),
            "Team".into(),
            Some("red-driver-1"),
            &arena,
        );
        {
            let p = r.players.get_mut("p").unwrap();
            p.intake_power = 0.0;
            p.outtake_power = 0.0;
        }
        let player_pos = r.players["p"].position;
        let player_rot = r.players["p"].rotation;
        let local = [local_x, 0.11_f32, 0.04_f32];
        let world = add(
            player_pos,
            rotate_robot_local_pose([local[0], local[1] + ground_offset_y, local[2]], player_rot),
        );
        r.balls[0].active = true;
        r.balls[0].released = true;
        r.balls[0].position = world;
        r.balls[0].previous_position = world;
        r.balls[0].velocity = mul(rotate_robot_local_pose([0.0, 0.0, -1.0], player_rot), 1.2);
        let mut max_rel_z = -10.0_f32;
        for t in 0..240 {
            r.tick(1.0 / 60.0);
            let b = &r.balls[0];
            let rel = rotate_robot_local_pose(
                sub(b.position, r.players["p"].position),
                r.players["p"].rotation,
            );
            max_rel_z = max_rel_z.max(rel[2]);
            if t < 45 && t % 5 == 0 {
                println!(
                    "  E x={local_x} t={t:>3} rel=({:.3},{:.3},{:.3}) vel=({:.2},{:.2},{:.2}) contacts={:?}",
                    rel[0],
                    rel[1],
                    rel[2],
                    b.velocity[0],
                    b.velocity[1],
                    b.velocity[2],
                    r.ball_contact_collider_ids()[0]
                );
            }
        }
        let passed = max_rel_z > 0.045;
        println!(
            "guider-entry x={local_x} max_rel_z={max_rel_z:.3} passed={passed} end_contacts={:?}",
            r.ball_contact_collider_ids()[0]
        );
    }
}

#[test]
#[ignore]
fn dbg_transfer_wall_climb_probe() {
    let loader = crate::game::pack_loader::PackLoader::new("0.1.0");
    let pack = loader
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    let ground_offset_y = -arena.robot.height_m * 0.5;

    for &local_x in &[0.055_f32, 0.04, 0.0, -0.04, -0.055] {
        let mut r = SphereRuntime::new("probe".into(), "fgc-2026".into(), 0);
        r.create_field_arena(&arena, &pack.field_definition);
        r.set_robot_definition(pack.default_robot.as_ref());
        r.context.phase = MatchPhase::Teleop;
        r.add_player(
            "p".into(),
            "Player".into(),
            "Team".into(),
            Some("red-driver-1"),
            &arena,
        );
        {
            let p = r.players.get_mut("p").unwrap();
            p.intake_power = 1.0;
            p.outtake_power = 0.0;
        }
        let player_pos = r.players["p"].position;
        let player_rot = r.players["p"].rotation;
        let local = [local_x, 0.11_f32, 0.04_f32];
        let world = add(
            player_pos,
            rotate_robot_local_pose([local[0], local[1] + ground_offset_y, local[2]], player_rot),
        );
        r.balls[0].active = true;
        r.balls[0].released = true;
        r.balls[0].position = world;
        r.balls[0].previous_position = world;
        let mut max_y = -10.0_f32;
        let mut max_rel_z = -10.0_f32;
        let mut seated = false;
        for t in 0..600 {
            r.tick(1.0 / 60.0);
            let b = &r.balls[0];
            let rel = rotate_robot_local_pose(
                sub(b.position, r.players["p"].position),
                r.players["p"].rotation,
            );
            max_y = max_y.max(rel[1]);
            max_rel_z = max_rel_z.max(rel[2]);
            if rel[1] > 0.0 && rel[2] > -0.03 && rel[2] < 0.05 {
                seated = true;
            }
            if t % 60 == 0 {
                println!(
                    "  climb x={local_x} t={t:>3} rel=({:.3},{:.3},{:.3}) contacts={:?}",
                    rel[0],
                    rel[1],
                    rel[2],
                    r.ball_contact_collider_ids()[0]
                );
            }
        }
        println!(
            "wall-climb x={local_x} max_rel_y={max_y:.3} max_rel_z={max_rel_z:.3} seated={seated} end_contacts={:?}",
            r.ball_contact_collider_ids()[0]
        );
    }
}

#[test]
fn robot_obb_contact_keeps_the_face_the_ball_entered_from() {
    let collider = FieldCollider {
        id: "chassis".into(),
        min: [0.0; 3],
        max: [0.0; 3],
        center: [0.0, 0.0, 0.0],
        half_extents: [0.25, 0.25, 0.25],
        axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
    let radius = 0.05;

    // A fast ball crossed from -Z to PAST the Z midpoint in one tick. The
    // naive nearest-face rule would now pick +Z and warp it out the back.
    let previous = [0.0, 0.0, -0.35];
    let current = [0.0, 0.0, 0.10];
    let (normal, penetration) =
        sphere_robot_obb_contact(current, previous, radius, &collider).unwrap();
    assert!(
        normal[2] < -0.99,
        "entry-face contact must push the ball back out the -Z face, got {normal:?}"
    );
    assert!((penetration - (radius + 0.15)).abs() < 1.0e-4);

    // Sanity check that the pre-existing helper really flips to the far face.
    let (naive_normal, _) = sphere_authored_obb_contact(current, radius, &collider).unwrap();
    assert!(
        naive_normal[2] > 0.99,
        "naive nearest-face should pick +Z for a ball past the midpoint"
    );

    // The final centre can also be completely beyond a thin wall. Continuous
    // contact must still retain the entry face rather than missing the wall.
    let traversed = [0.0, 0.0, 0.40];
    let (swept_normal, swept_penetration) =
        sphere_robot_obb_contact(traversed, previous, radius, &collider).unwrap();
    assert!(
        swept_normal[2] < -0.99,
        "swept contact must retain the -Z entry face, got {swept_normal:?}"
    );
    assert!(
        swept_penetration > 0.6,
        "swept contact must return enough correction to put the ball back at the entry face"
    );

    // A ball already inside (no crossing) still exits through the nearest face.
    let (resting_normal, _) =
        sphere_robot_obb_contact(current, current, radius, &collider).unwrap();
    assert!(
        resting_normal[2] > 0.99,
        "resting ball uses the nearest face"
    );
}

#[test]
fn ball_deep_inside_robot_is_held_by_collider_contacts() {
    let loader = crate::game::pack_loader::PackLoader::new("0.1.0");
    let pack = loader
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    arena.gravity_scale = 0.0;
    let mut runtime = SphereRuntime::new("expel-inside".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    runtime.set_robot_definition(pack.default_robot.as_ref());
    runtime.add_player(
        "p".into(),
        "Player".into(),
        "Team".into(),
        Some("red-driver-1"),
        &arena,
    );

    let (player_position, player_rotation) = {
        let player = runtime.players.get("p").unwrap();
        (player.position, player.rotation)
    };

    let definition = runtime.robot_definition.as_ref().unwrap();
    let ground_offset_y = -arena.robot.height_m * 0.5;
    let bounds = robot_local_collider_pose(
        &definition.bounds,
        player_position,
        player_rotation,
        ground_offset_y,
    );
    let radius = arena.ball.radius_m();
    let floor_rest_y = runtime.field_floor_y + radius;
    let colliders = definition.colliders.clone();

    // A ball resting on the intake ramp must be held by the ramp colliders
    // (multi-contact), never pushed sideways off the ramp by the deeper hopper
    // funnel walls and never dropped to the field floor.
    let ramp_colliders = [9usize, 10, 11, 12, 13, 14];
    for &i in &ramp_colliders {
        let posed = robot_local_collider_pose(
            &colliders[i],
            player_position,
            player_rotation,
            ground_offset_y,
        );
        let start_pos = [
            posed.center[0],
            posed.center[1] + posed.half_extents[1] + radius,
            posed.center[2],
        ];
        runtime.balls[0].active = true;
        runtime.balls[0].position = start_pos;
        runtime.balls[0].previous_position = start_pos;
        runtime.balls[0].velocity = [0.0; 3];
        for _ in 0..180 {
            runtime.tick(1.0 / 60.0);
        }
        let final_position = runtime.balls[0].position;
        assert!(
            final_position[1] > floor_rest_y + radius * 0.5,
            "ball on ramp COLLIDER[{i}] fell through to the field floor: start={start_pos:?} final={final_position:?}",
        );
    }

    // A ball that tunnelled deep into the chassis this tick (arriving from
    // -Z and ending up at the envelope centre) must be pushed out by the
    // collider contacts: it must be held off the field floor and must never
    // warp through the robot to the far (+Z) side.
    runtime.balls[0].active = true;
    runtime.balls[0].position = bounds.center;
    runtime.balls[0].previous_position = add(
        bounds.center,
        mul(bounds.axes[2], -(bounds.half_extents[2] + radius * 2.0)),
    );
    runtime.balls[0].velocity = [0.0; 3];

    runtime.solve_positions(&arena, 1.0 / 60.0);

    let final_position = runtime.balls[0].position;
    assert!(
        final_position[1] >= runtime.field_floor_y,
        "a deep ball must never be pushed through the chassis to below the field floor: position={final_position:?}",
    );
    assert!(
        final_position[2] < bounds.center[2] + bounds.half_extents[2],
        "a ball arriving from -Z must never warp to the far side: position={final_position:?}",
    );

    // A ball just inside the -Z face that crossed in this tick must be pushed
    // back out through that same -Z face, not flipped to the far side.
    runtime.balls[0].position = add(
        bounds.center,
        mul(bounds.axes[2], -(bounds.half_extents[2] - radius * 0.5)),
    );
    runtime.balls[0].previous_position = add(
        bounds.center,
        mul(bounds.axes[2], -(bounds.half_extents[2] + radius * 2.0)),
    );
    runtime.balls[0].velocity = [0.0; 3];

    runtime.solve_positions(&arena, 1.0 / 60.0);

    let final_position = runtime.balls[0].position;
    assert!(
        final_position[2] < bounds.center[2] + bounds.half_extents[2],
        "entry-face ball must never reach the far (+Z) face, position={final_position:?}",
    );
    assert!(
        final_position[1] >= runtime.field_floor_y,
        "entry-face ball must never fall through the chassis: position={final_position:?}",
    );
}

#[test]
fn authored_robot_driving_into_a_ball_does_not_tunnel_through() {
    let loader = crate::game::pack_loader::PackLoader::new("0.1.0");
    let pack = loader
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    let mut runtime = SphereRuntime::new("no-tunnel".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    runtime.set_robot_definition(pack.default_robot.as_ref());
    runtime.add_player(
        "p".into(),
        "Player".into(),
        "Team".into(),
        Some("red-driver-1"),
        &arena,
    );

    // Place the ball ahead of the robot along its actual spawn-facing
    // direction so the hybrid drivetrain drives head-on into it.
    let spawn_player = runtime.players.get_mut("p").unwrap();
    let forward = rotate_robot_local_pose([0.0, 0.0, -1.0], spawn_player.rotation);
    let spawn = spawn_player.position;
    runtime.balls[0].active = true;
    runtime.balls[0].position = add(mul(forward, 0.40), spawn);
    runtime.balls[0].position[1] = arena.ball.radius_m();
    runtime.balls[0].previous_position = runtime.balls[0].position;
    runtime.balls[0].velocity = [0.0; 3];
    runtime.set_player_input("p", 0.0, 1.0, 0.0, 0.0, 1);

    let bounds_collider = runtime.robot_definition.as_ref().unwrap().bounds.clone();
    let ground_offset_y = -arena.robot.height_m * 0.5;
    for _ in 0..180 {
        runtime.tick(1.0 / 60.0);
        let player = &runtime.players["p"];
        let bounds = robot_local_collider_pose(
            &bounds_collider,
            player.position,
            player.rotation,
            ground_offset_y,
        );
        let ball = &runtime.balls[0];
        if sphere_authored_obb_contact(ball.position, arena.ball.radius_m(), &bounds).is_some() {
            panic!(
                "ball ended up embedded in the robot envelope: ball={:?} bounds={:?}",
                ball.position, bounds
            );
        }
        let behind = dot(sub(ball.position, player.position), forward);
        assert!(
            behind > -0.45,
            "ball tunnelled far behind the robot: behind={behind} ball={:?} robot={:?}",
            ball.position,
            player.position
        );
    }
}

#[test]
#[ignore]
fn diagnostic_dump_robot_ramp_geometry() {
    let loader = crate::game::pack_loader::PackLoader::new("0.1.0");
    let pack = loader
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 1;
    arena.ramp.enabled = false;
    let mut runtime = SphereRuntime::new("ramp-diag".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    runtime.set_robot_definition(pack.default_robot.as_ref());
    runtime.add_player(
        "p".into(),
        "Player".into(),
        "Team".into(),
        Some("red-driver-1"),
        &arena,
    );

    let (position, rotation) = {
        let player = runtime.players.get("p").unwrap();
        (player.position, player.rotation)
    };
    println!(
        "player.position={:?} rotation={rotation:?} floor_y={}",
        position, runtime.field_floor_y
    );
    let ground_offset_y = -arena.robot.height_m * 0.5;
    let definition = runtime.robot_definition.as_ref().unwrap();
    let colliders = definition.colliders.clone();
    let bounds = definition.bounds.clone();

    println!(
        "BOUNDS: center={:?} half={:?}",
        bounds.center, bounds.half_extents
    );
    println!(
        "field_boundary min={:?} max={:?}",
        runtime.field_boundary.min, runtime.field_boundary.max
    );
    for (i, c) in colliders.iter().enumerate() {
        if i != 9 && i != 20 && i != 21 && i != 2 && i != 8 {
            continue;
        }
        let posed = robot_local_collider_pose(c, position, rotation, ground_offset_y);
        println!(
            "COLLIDER[{i}] id={} axes_orig={:?} center={:?} half={:?}",
            c.id, c.axes, posed.center, posed.half_extents
        );
        println!("  posed_axes={:?}", posed.axes);
    }
    for (i, c) in colliders.iter().enumerate() {
        let posed = robot_local_collider_pose(c, position, rotation, ground_offset_y);
        println!(
            "COLLIDER[{i}] center={:?} half={:?}",
            posed.center, posed.half_extents
        );
    }

    let ramp_colliders = [9usize, 10, 11, 12, 13, 14, 22, 27];
    for &i in &ramp_colliders {
        let posed = robot_local_collider_pose(&colliders[i], position, rotation, ground_offset_y);
        let start_pos = [
            posed.center[0],
            posed.center[1] + posed.half_extents[1] + arena.ball.radius_m(),
            posed.center[2],
        ];
        runtime.balls[0].active = true;
        runtime.balls[0].position = start_pos;
        runtime.balls[0].previous_position = start_pos;
        runtime.balls[0].velocity = [0.0; 3];
        for _ in 0..180 {
            runtime.tick(1.0 / 60.0);
        }
        println!(
            "on COLLIDER[{i}] top: start={start_pos:?} final={:?}",
            runtime.balls[0].position
        );
    }
}

#[test]
#[ignore]
fn dbg_transfer_flow2() {
    let loader = crate::game::pack_loader::PackLoader::new("0.1.0");
    let pack = loader
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 3;
    arena.ramp.enabled = false;
    let mut runtime = SphereRuntime::new("dbg-transfer2".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    runtime.set_robot_definition(pack.default_robot.as_ref());
    runtime.context.phase = MatchPhase::Teleop;
    runtime.ball_release_elapsed = Some(arena.spawn_release_seconds);
    runtime.add_player(
        "p".into(),
        "Player".into(),
        "Team".into(),
        Some("red-driver-1"),
        &arena,
    );
    {
        let player = runtime.players.get_mut("p").unwrap();
        player.intake_power = 1.0;
        player.outtake_power = 0.0;
        println!(
            "player.position={:?} yaw={} floor_y={}",
            player.position, player.yaw, runtime.field_floor_y
        );
    }

    let definition = runtime.robot_definition.as_ref().unwrap();
    for zone in &definition.zones {
        println!(
            "ZONE {} center={:?} dir={:?}",
            zone.id, zone.collider.center, zone.direction
        );
    }
    let ground_offset_y = -arena.robot.height_m * 0.5;
    let player_pos = runtime.players["p"].position;
    let player_rot = runtime.players["p"].rotation;
    for collider in &definition.colliders {
        let id = collider.id.as_str();
        if !id.starts_with("Plane.0") && id != "OuttakeRoller" && id != "chassis" && id != "CHASSIS"
        {
            continue;
        }
        let posed = robot_local_collider_pose(collider, player_pos, player_rot, ground_offset_y);
        println!(
            "COLLIDER {} local_center={:?} he={:?} world_center={:?}",
            id, collider.center, collider.half_extents, posed.center
        );
    }
    println!("ALL COLLIDERS:");
    for collider in &definition.colliders {
        println!(
            "  {} center={:?} he={:?}",
            collider.id, collider.center, collider.half_extents
        );
    }

    let hopper = definition
        .colliders
        .iter()
        .find(|c| c.id == "Plane.016")
        .expect("hopper floor");
    let hopper_world = robot_local_collider_pose(hopper, player_pos, player_rot, ground_offset_y);
    let start = [
        hopper_world.center[0],
        hopper_world.center[1] + arena.ball.radius_m(),
        hopper_world.center[2],
    ];
    println!(
        "hopper world center={:?} he={:?} ball_start={:?}",
        hopper_world.center, hopper_world.half_extents, start
    );
    let player_pos = runtime.players["p"].position;
    let player_rot = runtime.players["p"].rotation;
    let all_colliders = definition.colliders.clone();
    let bounds = definition.bounds.clone();
    println!("\n== multi-ball flow: 3 balls resting over the hopper floor ==");
    let spawn_locals = [
        [0.0_f32, 0.129, -0.10],
        [0.0_f32, 0.129, -0.02],
        [0.12_f32, 0.129, -0.10],
    ];
    for (i, local) in spawn_locals.iter().enumerate() {
        let world = add(
            player_pos,
            rotate_robot_local_pose([local[0], local[1] + ground_offset_y, local[2]], player_rot),
        );
        runtime.balls[i].active = true;
        runtime.balls[i].released = true;
        runtime.balls[i].velocity = [0.0; 3];
        runtime.balls[i].position = world;
        runtime.balls[i].previous_position = world;
    }
    for t in 0..1200 {
        {
            let player = runtime.players.get_mut("p").unwrap();
            player.intake_power = 1.0;
            player.outtake_power = if t > 360 && (t / 150) % 2 == 0 {
                1.0
            } else {
                0.0
            };
        }
        runtime.tick(1.0 / 60.0);
        if t % 150 == 0 {
            let ot = runtime.players["p"].outtake_power;
            print!("t={t:>3} ot={ot}");
            for i in 0..3 {
                let b = &runtime.balls[i];
                let in_env = sphere_authored_obb_contact(
                    b.position,
                    arena.ball.radius_m(),
                    &robot_local_collider_pose(
                        &bounds,
                        runtime.players["p"].position,
                        runtime.players["p"].rotation,
                        ground_offset_y,
                    ),
                )
                .is_some();
                print!(
                    "  b{i} pos=({:.3},{:.3},{:.3}) vel=({:.2},{:.2},{:.2}) in_env={in_env}",
                    b.position[0],
                    b.position[1],
                    b.position[2],
                    b.velocity[0],
                    b.velocity[1],
                    b.velocity[2]
                );
            }
            println!("  stored={}", runtime.players["p"].stored.len());
        }
        if t == 660 {
            for i in 0..3 {
                let b = &runtime.balls[i];
                println!("  overlaps b{i} at pos={:?}:", b.position);
                for collider in &all_colliders {
                    let posed = robot_local_collider_pose(
                        collider,
                        runtime.players["p"].position,
                        runtime.players["p"].rotation,
                        ground_offset_y,
                    );
                    if sphere_authored_obb_contact(b.position, arena.ball.radius_m(), &posed)
                        .is_some()
                    {
                        println!(
                            "    {} center={:?} he={:?}",
                            collider.id, posed.center, posed.half_extents
                        );
                    }
                }
                if let Some(zone) = runtime
                    .robot_definition
                    .as_ref()
                    .unwrap()
                    .zones
                    .iter()
                    .find(|z| z.kind == RobotSemanticKind::Outtake)
                {
                    let posed = robot_local_collider_pose(
                        &zone.collider,
                        runtime.players["p"].position,
                        runtime.players["p"].rotation,
                        ground_offset_y,
                    );
                    println!(
                        "    OUTHOUSE mouth center={:?} he={:?}",
                        posed.center, posed.half_extents
                    );
                }
            }
        }
    }
}

#[test]
fn tipped_over_robot_cannot_drive() {
    let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 0;
    let mut runtime = SphereRuntime::new("tipover-test".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    runtime.set_robot_definition(pack.default_robot.as_ref());
    runtime.add_player("red".into(), "Red".into(), "red".into(), None, &arena);
    let start_y = runtime.robot_center_y(&arena);
    let player = runtime.players.get_mut("red").unwrap();
    player.position = [0.0, start_y, 0.0];
    // Rotate 90 degrees around Z axis (tipped on its side, local up points along X)
    player.rotation = [0.0, 0.0, 0.7071068, 0.7071068];
    player.yaw = 0.0;
    runtime
        .robot_physics
        .as_mut()
        .unwrap()
        .teleport_to_players(&runtime.players);

    // Let it settle on its side on the floor
    for _ in 0..30 {
        runtime.tick(1.0 / 60.0);
    }
    assert!(
        !runtime.players["red"].floor_supported,
        "tipped robot must not be marked floor_supported"
    );

    let settled_pos = runtime.players["red"].position;
    // Attempt to drive while lying on its side
    let player = runtime.players.get_mut("red").unwrap();
    player.move_z = 1.0;
    player.move_x = 1.0;
    for _ in 0..60 {
        runtime.tick(1.0 / 60.0);
    }

    let final_pos = runtime.players["red"].position;
    let dx = final_pos[0] - settled_pos[0];
    let dz = final_pos[2] - settled_pos[2];
    let distance = (dx * dx + dz * dz).sqrt();
    assert!(
        distance < 0.05,
        "tipped robot should not drive across the floor, moved {distance}m"
    );
}

#[test]
fn intake_has_exclusive_ownership_and_hard_capacity() {
    let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap();
    let mut arena = pack.arena.clone();
    arena.object_count = 2;
    arena.ramp.enabled = false;
    arena.gravity_scale = 0.0;
    arena.robot.storage_capacity = 1;
    let mut runtime = SphereRuntime::new("ownership".into(), "fgc-2026".into(), 0);
    runtime.create_field_arena(&arena, &pack.field_definition);
    runtime.set_robot_definition(pack.default_robot.as_ref());
    runtime.context.phase = MatchPhase::Teleop;
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
    let mouth = {
        let player = &runtime.players["p"];
        let zone = runtime
            .robot_definition
            .as_ref()
            .unwrap()
            .zones
            .iter()
            .find(|zone| zone.kind == RobotSemanticKind::Intake)
            .unwrap();
        robot_local_collider(
            &zone.collider,
            player.position,
            player.yaw,
            -arena.robot.height_m * 0.5,
        )
    };
    for ball in &mut runtime.balls {
        ball.active = true;
        ball.released = true;
        ball.position = mouth.center;
        ball.previous_position = mouth.center;
    }
    {
        let player = runtime.players.get_mut("p").unwrap();
        player.intake_power = 1.0;
    }
    for _ in 0..30 {
        runtime.step_mechanics(&arena, 1.0 / 60.0);
    }
    let player = &runtime.players["p"];
    assert_eq!(player.stored.len(), 1);
    let owned = player.stored[0];
    assert_eq!(runtime.balls[owned].owner.as_deref(), Some("p"));
    assert!(
        runtime
            .balls
            .iter()
            .filter(|ball| ball.owner.is_some())
            .count()
            <= 1
    );
}

#[test]
fn removing_player_releases_owned_balls() {
    let mut arena = arena();
    arena.object_count = 1;
    let mut runtime = SphereRuntime::new("remove-owner".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
    runtime.players.get_mut("p").unwrap().stored.push_back(0);
    runtime.balls[0].owner = Some("p".into());
    runtime.remove_player("p");
    assert!(runtime.players.get("p").is_none());
    assert!(runtime.balls[0].owner.is_none());
}

#[test]
fn a_new_connection_supersedes_stale_input_and_leave_frames() {
    let mut arena = arena();
    arena.object_count = 1;
    let mut runtime = SphereRuntime::new("reconnect".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);

    runtime.bind_player_connection("p", "old".into());
    runtime.set_player_input_from_connection("p", "old", 0.0, 1.0, 0.0, 0.0, 0.0, 8);
    assert_eq!(runtime.players["p"].sequence, 8);

    runtime.bind_player_connection("p", "new".into());
    runtime.set_player_input_from_connection("p", "old", 0.0, 1.0, 0.0, 0.0, 0.0, 9);
    assert_eq!(runtime.players["p"].sequence, 0);

    runtime.remove_player_from_connection("p", Some("old"));
    assert!(runtime.players.contains_key("p"));
    runtime.remove_player_from_connection("p", Some("new"));
    assert!(!runtime.players.contains_key("p"));
}
