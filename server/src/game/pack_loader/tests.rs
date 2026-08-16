use super::{GamePackRuntimeSnapshot, PackLoader};
use std::collections::BTreeMap;

#[test]
fn loads_manifest_and_all_rhai_rules() {
    let loader = PackLoader::new("0.1.0");
    let root = std::path::Path::new("../pkgs/games/fgc-2026");
    let manifest =
        serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).unwrap())
            .unwrap();
    let field_physics =
        serde_json::from_str(&std::fs::read_to_string(root.join("field.physics.json")).unwrap())
            .unwrap();
    let field_semantics =
        serde_json::from_str(&std::fs::read_to_string(root.join("field.semantics.json")).unwrap())
            .unwrap();
    let scripts = [
        "rules/arena.rhai",
        "rules/penalties.rhai",
        "rules/robot.rhai",
        "rules/scoring.rhai",
    ]
    .into_iter()
    .map(|path| {
        (
            path.to_string(),
            std::fs::read_to_string(root.join(path)).unwrap(),
        )
    })
    .collect::<BTreeMap<_, _>>();
    let robots = ["starter-bot"]
        .into_iter()
        .map(|id| {
            let physics = serde_json::from_str(
                &std::fs::read_to_string(root.join(format!("robots/{id}/bot.physics.json")))
                    .unwrap(),
            )
            .unwrap();
            let semantics = serde_json::from_str(
                &std::fs::read_to_string(root.join(format!("robots/{id}/bot.semantics.json")))
                    .unwrap(),
            )
            .unwrap();
            (
                id.to_string(),
                super::RobotRuntimeAssets { physics, semantics },
            )
        })
        .collect();
    let metadata = loader
        .load_runtime_snapshot(GamePackRuntimeSnapshot {
            manifest,
            field_physics,
            field_semantics,
            scripts,
            robots,
        })
        .unwrap();
    assert_eq!(metadata.manifest.id, "fgc-2026");
    assert_eq!(metadata.scripts.len(), 4);
    assert_eq!(metadata.arena.object_count, 500);
    assert_eq!(metadata.arena.ball.diameter_m, 0.100);
    assert_eq!(metadata.arena.ball.mass_kg, 0.062);
    assert_eq!(metadata.arena.ball.inertia_factor, 0.4);
    assert_eq!(metadata.arena.ball.drag_coefficient, 0.47);
    assert_eq!(metadata.arena.floor.material, "low-pile carpet");
    assert!(metadata.arena.floor.rolling_resistance_mps2 > 0.0);
    assert!(metadata.arena.robot.intake_enabled);
    assert_eq!(metadata.arena.robot.mass_kg, 18.0);
    assert_eq!(metadata.arena.robot.width_m, 0.50);
    assert_eq!(metadata.arena.robot.height_m, 0.50);
    assert_eq!(metadata.arena.robot.length_m, 0.50);
    let starter_bot = metadata
        .default_robot
        .as_ref()
        .expect("default robot must load");
    assert!(starter_bot.colliders.len() >= 30);
    assert_eq!(starter_bot.zones.len(), 3);
    assert_eq!(starter_bot.climb_colliders.len(), 2);
    let climber = starter_bot
        .climber
        .as_ref()
        .expect("starter bot climber must load");
    assert_eq!(climber.wheel_parts, ["ClimbWheel1", "ClimbWheel2"]);
    assert!(climber.stall_torque_nm > 0.0);
    assert!(metadata.arena.ramp.enabled);
    assert!(metadata.field_definition.colliders.len() >= 70);
    let front_wall = metadata
        .field_definition
        .colliders
        .iter()
        .find(|collider| collider.id == "blueSUfront")
        .expect("authored planar wall must be loaded");
    assert!(front_wall.max[2] - front_wall.min[2] >= 0.05 - f32::EPSILON);
    assert!(
        front_wall
            .half_extents
            .iter()
            .any(|extent| *extent >= 0.025)
    );
    assert!(metadata.field_definition.anchors.contains_key("redSpawn1"));
    assert!(metadata.field_definition.anchors.contains_key("blueSpawn3"));
    assert!(
        metadata
            .field_definition
            .anchors
            .contains_key("EXTballspawn")
    );
    assert!(
        metadata
            .field_definition
            .triggers
            .iter()
            .any(|trigger| trigger.id == "EXTscore")
    );
    let red_su = metadata
        .field_definition
        .scoring_targets
        .iter()
        .find(|target| target.id == "red-suppression-unit")
        .expect("red SU scoring area must be loaded");
    assert!(
        red_su.min[1] < 1.0,
        "hopper area extends below the sensor plane"
    );
    assert!(red_su.max[1] > 1.9);
    assert!((metadata.field_definition.boundary.min[0] + 3.5).abs() < 0.01);
    assert!((metadata.field_definition.boundary.max[0] - 3.5).abs() < 0.01);
    assert!(metadata.field_definition.floor_height_m > 0.65);
    assert!(
        metadata
            .scripts
            .iter()
            .any(|script| script.path.ends_with("scoring.rhai"))
    );
    assert!(
        metadata
            .scripts
            .iter()
            .flat_map(|script| script.functions.iter())
            .any(|function| function.name == "validate_build")
    );
}
