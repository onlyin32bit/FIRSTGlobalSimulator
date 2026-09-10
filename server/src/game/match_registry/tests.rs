use super::*;

fn bootstrap_with_driver(participant: BootstrapParticipant) -> MatchBootstrap {
    MatchBootstrap {
        match_id: "match".into(),
        host_id: "host".into(),
        assigned_server_id: "server".into(),
        game_pack_id: "fgc-2026".into(),
        game_pack_version: "1.0.0".into(),
        match_seed: 1,
        starts_at: 1,
        duration_seconds: 150,
        max_players: 8,
        scenario_id: "default".into(),
        visibility: "private".into(),
        open_arena: false,
        participants: vec![participant],
    }
}

fn driver(
    robot_id: Option<&str>,
    robot_revision: Option<u64>,
    robot_data: Option<&str>,
) -> BootstrapParticipant {
    BootstrapParticipant {
        user_id: "driver".into(),
        role: "driver".into(),
        slot_id: "red-driver-1".into(),
        alliance: "red".into(),
        robot_id: robot_id.map(str::to_owned),
        robot_revision,
        robot_data: robot_data.map(str::to_owned),
    }
}

#[test]
fn bootstrap_requires_a_frozen_driver_robot_revision() {
    let missing_revision = bootstrap_with_driver(driver(
        Some("saved-robot"),
        None,
        Some(r#"{"driveType":"tank"}"#),
    ));
    assert!(validate_bootstrap_robot_revisions(&missing_revision).is_err());

    let mismatched_pack = bootstrap_with_driver(driver(
        Some("pack:starter-bot"),
        None,
        Some(r#"{"kind":"pack-robot","robotId":"other"}"#),
    ));
    assert!(validate_bootstrap_robot_revisions(&mismatched_pack).is_err());

    let frozen_custom = bootstrap_with_driver(driver(
        Some("saved-robot"),
        Some(42),
        Some(r#"{"driveType":"tank"}"#),
    ));
    assert!(validate_bootstrap_robot_revisions(&frozen_custom).is_ok());
}

#[test]
fn binary_state_has_versioned_header_and_compact_positions() {
    let state = MatchStateSync {
        tick: 7,
        game_pack_id: "pack".into(),
        game_pack_version: "1".into(),
        players: Vec::new(),
        object_id: "ball".into(),
        object_radius: 0.05,
        object_color: "test".to_string(),
        object_positions: ObjectPositionsSync {
            count: 0,
            active_mask: vec![],
            moving_mask: vec![],
            quantized_positions: vec![],
        },
        baseline_object_positions: ObjectPositionsSync {
            count: 0,
            active_mask: vec![],
            moving_mask: vec![],
            quantized_positions: vec![],
        },
        input_acknowledgements: Vec::new(),
        contacts: 0,
        match_clock: 1.0,
        match_duration_seconds: 150.0,
        pre_match_remaining_seconds: 0.0,
        match_running: true,
        simulation_clock: 1.0,
        physics_tick_ms: 2.0,
        physics_load_percent: 12.0,
        ticks_per_second: 60.0,
        target_ticks_per_second: 60.0,
        clock_drift_ms: 0.0,
        step_metrics: StepMetrics::default(),
        physics: PhysicsSync::from(
            &crate::game::pack_loader::PackLoader::new("0.1.0")
                .load_pack("../pkgs/games/fgc-2026/manifest.json")
                .unwrap()
                .arena,
        ),
        drive: DriveSync::default(),
        semantic_events: Vec::new(),
        score: ScoreState::default(),
        practice_running: false,
        transfer_debug: Vec::new(),
        ball_debug: Vec::new(),
        ball_contact_colliders: Vec::new(),
    };
    let encoded = encode_state(&state, ProcessMetrics::default(), false);
    assert_eq!(&encoded[..4], b"FGS1");
    assert_eq!(u16::from_le_bytes(encoded[4..6].try_into().unwrap()), 1);
    assert_eq!(u16::from_le_bytes(encoded[6..8].try_into().unwrap()), 5);
    assert_eq!(u16::from_le_bytes(encoded[8..10].try_into().unwrap()), 1);
    assert_eq!(
        u32::from_le_bytes(encoded[12..16].try_into().unwrap()) as usize,
        encoded.len() - 16
    );
    assert!(
        encoded.len() < 14_000,
        "snapshot was {} bytes",
        encoded.len()
    );
}
