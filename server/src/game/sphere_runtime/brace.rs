use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BraceState {
    pub zone: Option<u8>,
    pub multiplier: f32,
}

impl Default for BraceState {
    fn default() -> Self {
        Self {
            zone: None,
            multiplier: 1.0,
        }
    }
}

pub(super) fn brace_state_for_player(
    player: &PlayerBody,
    definition: Option<&RobotDefinition>,
    ground_offset_y: f32,
    _field_colliders: &[FieldCollider],
    triggers: &[FieldTrigger],
) -> BraceState {
    let Some(definition) = definition.filter(|definition| !definition.climb_colliders.is_empty())
    else {
        return BraceState::default();
    };
    let alliance = player.team_name.to_ascii_lowercase();
    let Some(prefix) = (if alliance.starts_with("blue") {
        Some("blueZone")
    } else if alliance.starts_with("red") {
        Some("redZone")
    } else {
        None
    }) else {
        return BraceState::default();
    };
    let wheels = definition
        .climb_colliders
        .iter()
        .map(|collider| {
            robot_local_collider_pose(collider, player.position, player.rotation, ground_offset_y)
        })
        .collect::<Vec<_>>();
    let brace_contact = player.climbing_brace.is_some();

    for zone in (1_u8..=3).rev() {
        let id = format!("{prefix}{zone}");
        if let Some(trigger) = triggers.iter().find(|trigger| trigger.id == id) {
            if brace_contact
                && wheels
                    .iter()
                    .all(|wheel| aabbs_overlap(wheel.min, wheel.max, trigger.min, trigger.max))
            {
                return BraceState {
                    zone: Some(zone),
                    multiplier: 1.0 + zone as f32 * 0.1,
                };
            }
        }
    }
    if brace_contact {
        BraceState {
            zone: None,
            multiplier: 1.05,
        }
    } else {
        BraceState::default()
    }
}

fn aabbs_overlap(left_min: Vec3, left_max: Vec3, right_min: Vec3, right_max: Vec3) -> bool {
    (0..3).all(|axis| left_min[axis] <= right_max[axis] && left_max[axis] >= right_min[axis])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn collider(id: &str, min: Vec3, max: Vec3) -> FieldCollider {
        FieldCollider {
            id: id.into(),
            min,
            max,
            center: [
                (min[0] + max[0]) * 0.5,
                (min[1] + max[1]) * 0.5,
                (min[2] + max[2]) * 0.5,
            ],
            half_extents: [
                (max[0] - min[0]) * 0.5,
                (max[1] - min[1]) * 0.5,
                (max[2] - min[2]) * 0.5,
            ],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    fn player() -> PlayerBody {
        PlayerBody {
            name: "player".into(),
            team_name: "red".into(),
            position: [0.0; 3],
            velocity: [0.0; 3],
            yaw: 0.0,
            angular_velocity_y: 0.0,
            rotation: [0.0, 0.0, 0.0, 1.0],
            angular_velocity: [0.0; 3],
            move_x: 0.0,
            move_z: 0.0,
            intake_power: 0.0,
            outtake_power: 0.0,
            climb_power: 0.0,
            sequence: 0,
            color: "#ef4444",
            wall_contact_normal: None,
            stored: VecDeque::new(),
            outtake_accumulator: 0.0,
            intake_accumulator: 0.0,
            mech: MechSpec::default(),
            climbing_brace: Some("Cylinder.003".into()),
            floor_supported: true,
            brace_support_impulse: 0.0,
            climb_wheel_angle: 0.0,
            climb_wheel_radps: 0.0,
        }
    }

    fn definition() -> RobotDefinition {
        let wheels = vec![
            collider("ClimbWheel1", [-0.12, 0.35, -0.05], [-0.04, 0.45, 0.05]),
            collider("ClimbWheel2", [0.04, 0.35, -0.05], [0.12, 0.45, 0.05]),
        ];
        RobotDefinition {
            id: "test-bot".into(),
            colliders: wheels.clone(),
            climb_colliders: wheels,
            bounds: collider("envelope", [-0.2, 0.0, -0.1], [0.2, 0.5, 0.1]),
            zones: Vec::new(),
            climber: None,
        }
    }

    #[test]
    fn brace_zone_requires_both_climb_wheels_in_the_matching_zone() {
        let trigger = FieldTrigger {
            id: "redZone3".into(),
            min: [-0.2, 0.3, -0.1],
            max: [0.2, 0.5, 0.1],
        };
        let brace = collider("Cylinder.003", [-0.2, 0.3, -0.1], [0.2, 0.5, 0.1]);
        let state =
            brace_state_for_player(&player(), Some(&definition()), 0.0, &[brace], &[trigger]);
        assert_eq!(state.zone, Some(3));
        assert!((state.multiplier - 1.3).abs() < f32::EPSILON);
    }

    #[test]
    fn brace_contact_without_a_zone_is_not_a_full_climb() {
        let state = brace_state_for_player(
            &player(),
            Some(&definition()),
            0.0,
            &[collider("Cylinder.003", [-0.2, 0.3, -0.1], [0.2, 0.5, 0.1])],
            &[],
        );
        assert_eq!(state.zone, None);
        assert!((state.multiplier - 1.05).abs() < f32::EPSILON);
    }

    #[test]
    fn starter_bot_cannot_climb_without_physical_brace_contact() {
        let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
            .load_pack("../pkgs/games/fgc-2026/manifest.json")
            .unwrap();
        let mut arena = pack.arena.clone();
        arena.object_count = 0;
        let mut runtime = SphereRuntime::new("brace-test".into(), "fgc-2026".into(), 0);
        runtime.create_field_arena(&arena, &pack.field_definition);
        runtime.set_robot_definition(pack.default_robot.as_ref());
        runtime.add_player("red".into(), "Red".into(), "red".into(), None, &arena);
        let center_y = runtime.robot_center_y(&arena);
        let player = runtime.players.get_mut("red").unwrap();
        player.yaw = 0.0;
        player.climb_power = 1.0;
        for _ in 0..90 {
            runtime.tick(1.0 / 60.0);
        }
        let player = runtime.players.get("red").unwrap();
        assert_eq!(player.climbing_brace, None);
        assert!(
            player.position[1] < center_y + 0.05,
            "a spinning wheel without brace contact must not lift the robot, got {}",
            player.position[1]
        );
    }

    #[test]
    fn starter_bot_climbs_from_wheel_brace_friction() {
        let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
            .load_pack("../pkgs/games/fgc-2026/manifest.json")
            .unwrap();
        let mut arena = pack.arena.clone();
        arena.object_count = 0;
        let mut runtime = SphereRuntime::new("brace-friction-test".into(), "fgc-2026".into(), 0);
        runtime.create_field_arena(&arena, &pack.field_definition);
        runtime.set_robot_definition(pack.default_robot.as_ref());
        runtime.add_player("red".into(), "Red".into(), "red".into(), None, &arena);
        let start_y = runtime.robot_center_y(&arena);
        let player = runtime.players.get_mut("red").unwrap();
        player.position = [-2.126, start_y, 2.646];
        player.yaw = 0.0;
        player.rotation = [0.0, 0.0, 0.0, 1.0];
        runtime
            .robot_physics
            .as_mut()
            .unwrap()
            .teleport_to_players(&runtime.players);

        let settled_y = runtime.players["red"].position[1];
        let player = runtime.players.get_mut("red").unwrap();
        player.climb_power = 1.0;

        let mut contact_ticks = 0;
        let mut highest = settled_y;
        let mut strongest_support = 0.0_f32;
        for _ in 0..360 {
            runtime.tick(1.0 / 60.0);
            let player = &runtime.players["red"];
            contact_ticks += usize::from(player.climbing_brace.is_some());
            highest = highest.max(player.position[1]);
            strongest_support = strongest_support.max(player.brace_support_impulse);
        }
        assert!(
            contact_ticks > 10,
            "climb wheel never established brace contact: contacts={contact_ticks} final={:?}",
            runtime.players["red"].position
        );
        assert!(
            highest > settled_y + 0.25,
            "brace friction produced no lift: settled={settled_y} highest={highest} contacts={contact_ticks} support={strongest_support} wheel_radps={} final={:?}",
            runtime.players["red"].climb_wheel_radps,
            runtime.players["red"].position
        );
    }
}
