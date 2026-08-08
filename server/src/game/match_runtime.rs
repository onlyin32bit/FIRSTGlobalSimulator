use rapier3d::prelude::*;
use serde::Serialize;
use std::collections::HashMap;

use super::pack_loader::ArenaConfig;
use super::match_registry::ObjectPositionsSync;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPhase {
    PreMatch,
    Autonomous,
    Teleop,
    Endgame,
    PostMatch,
}

#[derive(Debug, Clone)]
pub struct MatchContext {
    pub match_id: String,
    pub game_pack_id: String,
    pub game_pack_version: String,
    pub engine_version: String,
    pub match_seed: u64,
    pub phase: MatchPhase,
    pub clock: f64,
}

#[derive(Debug, Clone, Default)]
pub struct ScoreState {
    pub blue_score: i32,
    pub red_score: i32,
    pub global_score: i32,
    pub breakdown: HashMap<String, i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerSnapshot {
    pub id: String,
    pub name: String,
    #[serde(rename = "teamName")]
    pub team_name: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    #[serde(rename = "headingDeg")]
    pub heading_deg: f32,
    #[serde(rename = "velocityX")]
    pub velocity_x: f32,
    #[serde(rename = "velocityY")]
    pub velocity_y: f32,
    #[serde(rename = "velocityZ")]
    pub velocity_z: f32,
    #[serde(rename = "angularVelocityY")]
    pub angular_velocity_y: f32,
    pub color: String,
    #[serde(rename = "storedBalls")]
    pub stored_balls: usize,
    pub capacity: usize,
}

struct PlayerBody {
    name: String,
    team_name: String,
    body: RigidBodyHandle,
    collider: ColliderHandle,
    move_x: f32,
    move_z: f32,
    sequence: u64,
    color: &'static str,
}

struct FieldObject {
    body: RigidBodyHandle,
}

pub struct MatchRuntime {
    pub context: MatchContext,
    pub score_state: ScoreState,
    pub rigid_body_set: RigidBodySet,
    pub collider_set: ColliderSet,
    pub gravity: Vector,
    pub integration_parameters: IntegrationParameters,
    pub physics_pipeline: PhysicsPipeline,
    pub island_manager: IslandManager,
    pub broad_phase: BroadPhaseBvh,
    pub narrow_phase: NarrowPhase,
    pub impulse_joint_set: ImpulseJointSet,
    pub multibody_joint_set: MultibodyJointSet,
    pub ccd_solver: CCDSolver,
    players: HashMap<String, PlayerBody>,
    objects: Vec<FieldObject>,
    ball_radius: f32,
    ball_rolling_resistance_mps2: f32,
    storage_capacity: usize,
}

impl MatchRuntime {
    pub fn new(match_id: String, game_pack_id: String, match_seed: u64) -> Self {
        // Four solver passes are Rapier's accuracy-oriented default. The
        // optimized broad phase and instanced client keep 500 balls affordable
        // without sacrificing dense-contact stability here.
        let integration_parameters = IntegrationParameters {
            num_solver_iterations: 4,
            min_island_size: 64,
            ..IntegrationParameters::default()
        };

        Self {
            context: MatchContext {
                match_id,
                game_pack_id,
                game_pack_version: "1.0.0".to_string(),
                engine_version: "0.1.0".to_string(),
                match_seed,
                phase: MatchPhase::PreMatch,
                clock: 0.0,
            },
            score_state: ScoreState::default(),
            rigid_body_set: RigidBodySet::new(),
            collider_set: ColliderSet::new(),
            gravity: vector![0.0, -9.81, 0.0].into(),
            integration_parameters,
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: BroadPhaseBvh::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joint_set: ImpulseJointSet::new(),
            multibody_joint_set: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            players: HashMap::new(),
            objects: Vec::new(),
            ball_radius: 0.05,
            ball_rolling_resistance_mps2: 0.0,
            storage_capacity: 0,
        }
    }

    pub fn begin_match(&mut self) {
        self.context.phase = MatchPhase::Teleop;
    }

    pub fn create_test_arena(&mut self, arena: &ArenaConfig) {
        let floor_groups = InteractionGroups::new(
            Group::GROUP_1,
            Group::GROUP_2 | Group::GROUP_3,
            InteractionTestMode::And,
        );
        let wall_groups = InteractionGroups::new(
            Group::GROUP_4,
            Group::GROUP_2 | Group::GROUP_3,
            InteractionTestMode::And,
        );
        let ball_filter = if arena.ball_to_ball_collisions {
            Group::GROUP_1 | Group::GROUP_2 | Group::GROUP_3 | Group::GROUP_4
        } else {
            Group::GROUP_1 | Group::GROUP_3 | Group::GROUP_4
        };
        let ball_groups =
            InteractionGroups::new(Group::GROUP_2, ball_filter, InteractionTestMode::And);
        let floor = RigidBodyBuilder::fixed()
            .translation(vector![0.0, -0.25, 0.0].into())
            .build();
        let floor_handle = self.rigid_body_set.insert(floor);
        self.collider_set.insert_with_parent(
            ColliderBuilder::cuboid(8.0, 0.25, 8.0)
                .friction(arena.floor.friction.max(0.0))
                .restitution(arena.floor.restitution.clamp(0.0, 1.0))
                .collision_groups(floor_groups)
                .build(),
            floor_handle,
            &mut self.rigid_body_set,
        );
        for (x, z, hx, hz) in [
            (0.0, -8.0, 8.25, 0.25),
            (0.0, 8.0, 8.25, 0.25),
            (-8.0, 0.0, 0.25, 8.25),
            (8.0, 0.0, 0.25, 8.25),
        ] {
            let body = self.rigid_body_set.insert(
                RigidBodyBuilder::fixed()
                    .translation(vector![x, 0.5, z].into())
                    .build(),
            );
            self.collider_set.insert_with_parent(
                ColliderBuilder::cuboid(hx, 0.75, hz)
                    .restitution(0.2)
                    .collision_groups(wall_groups)
                    .build(),
                body,
                &mut self.rigid_body_set,
            );
        }

        let object_radius = arena.ball.radius_m();
        self.ball_radius = object_radius;
        self.ball_rolling_resistance_mps2 = arena.ball.rolling_resistance_mps2.max(0.0);
        self.storage_capacity = arena.robot.storage_capacity;
        let object_count = arena.object_count.max(1) as f32;
        let golden_angle = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
        for index in 0..arena.object_count {
            // A Vogel disk gives every ball useful separation while preserving
            // a deterministic, pack-sized spawn area. The old five-ring layout
            // concentrated the robot and hundreds of contacts in the same band.
            let angle = index as f32 * golden_angle;
            let distance = arena.spawn_radius * ((index as f32 + 0.5) / object_count).sqrt();
            let body = self.rigid_body_set.insert(
                RigidBodyBuilder::dynamic()
                    .translation(
                        vector![
                            angle.cos() * distance,
                            arena.spawn_height + (index % 3) as f32 * object_radius * 2.25,
                            angle.sin() * distance
                        ]
                        .into(),
                    )
                    .gravity_scale(arena.gravity_scale)
                    .linear_damping(arena.ball.linear_damping.max(0.0))
                    .angular_damping(arena.ball.angular_damping.max(0.0))
                    // Soft CCD predicts contacts without expensive shape casts
                    // and prevents 100 mm balls crossing each other at speed.
                    .soft_ccd_prediction(arena.ball.soft_ccd_prediction_m.max(0.0))
                    .ccd_enabled(false)
                    .build(),
            );
            self.collider_set.insert_with_parent(
                ColliderBuilder::ball(object_radius)
                    .mass(arena.ball.mass_kg.max(0.001))
                    .restitution(arena.ball.restitution.clamp(0.0, 1.0))
                    .restitution_combine_rule(CoefficientCombineRule::Max)
                    .friction(arena.ball.friction.max(0.0))
                    .collision_groups(ball_groups)
                    .build(),
                body,
                &mut self.rigid_body_set,
            );
            self.objects.push(FieldObject { body });
        }
    }

    pub fn add_player(
        &mut self,
        user_id: String,
        name: String,
        team_name: String,
        arena: &ArenaConfig,
    ) {
        if self.players.contains_key(&user_id) {
            return;
        }
        let slot = self.players.len();
        let angle = slot as f32 * std::f32::consts::TAU / 8.0;
        let body = RigidBodyBuilder::dynamic()
            .translation(
                vector![
                    angle.cos() * 4.0,
                    arena.robot.height_m * 0.5,
                    angle.sin() * 4.0
                ]
                .into(),
            )
            .linear_damping(arena.robot.rolling_resistance.max(0.0))
            .angular_damping(0.15)
            .enabled_rotations(false, true, false)
            // At 60 Hz the robot moves less than its own half-extent per tick,
            // so discrete contacts are sufficient and avoid hundreds of swept
            // collision tests while pushing through balls.
            .ccd_enabled(false)
            .build();
        let body_handle = self.rigid_body_set.insert(body);
        let collider_handle = self.collider_set.insert_with_parent(
            ColliderBuilder::cuboid(
                arena.robot.width_m * 0.5,
                arena.robot.height_m * 0.5,
                arena.robot.length_m * 0.5,
            )
            .mass(arena.robot.mass_kg.max(1.0))
            // The chassis contacts balls and walls, but not the floor. This
            // restores tangential foam/chassis interaction without carpet
            // friction cancelling the explicit drivetrain forces.
            .friction(arena.robot.surface_friction.max(0.0))
            .restitution(arena.robot.restitution.clamp(0.0, 1.0))
            .collision_groups(InteractionGroups::new(
                Group::GROUP_3,
                Group::GROUP_2 | Group::GROUP_4,
                InteractionTestMode::And,
            ))
            .build(),
            body_handle,
            &mut self.rigid_body_set,
        );
        // A massless support shape handles only floor contact. Its zero
        // friction leaves forward traction and lateral wheel scrub to the
        // drivetrain model instead of an isotropic box contact.
        self.collider_set.insert_with_parent(
            ColliderBuilder::cuboid(
                arena.robot.width_m * 0.5,
                arena.robot.height_m * 0.5,
                arena.robot.length_m * 0.5,
            )
            .mass(0.0)
            .friction(0.0)
            .friction_combine_rule(CoefficientCombineRule::Min)
            .collision_groups(InteractionGroups::new(
                Group::GROUP_3,
                Group::GROUP_1,
                InteractionTestMode::And,
            ))
            .build(),
            body_handle,
            &mut self.rigid_body_set,
        );
        let colors = [
            "#f97316", "#2563eb", "#16a34a", "#9333ea", "#dc2626", "#0891b2", "#ca8a04", "#db2777",
        ];
        self.players.insert(
            user_id,
            PlayerBody {
                name,
                team_name,
                body: body_handle,
                collider: collider_handle,
                move_x: 0.0,
                move_z: 0.0,
                sequence: 0,
                color: colors[slot % colors.len()],
            },
        );
    }

    pub fn remove_player(&mut self, user_id: &str) {
        if let Some(player) = self.players.remove(user_id) {
            self.collider_set.remove(
                player.collider,
                &mut self.island_manager,
                &mut self.rigid_body_set,
                true,
            );
            self.rigid_body_set.remove(
                player.body,
                &mut self.island_manager,
                &mut self.collider_set,
                &mut self.impulse_joint_set,
                &mut self.multibody_joint_set,
                true,
            );
        }
    }

    pub fn set_player_input(&mut self, user_id: &str, move_x: f32, move_z: f32, sequence: u64) {
        if let Some(player) = self.players.get_mut(user_id)
            && sequence >= player.sequence
        {
            player.sequence = sequence;
            player.move_x = move_x.clamp(-1.0, 1.0);
            player.move_z = move_z.clamp(-1.0, 1.0);
        }
    }

    pub fn apply_player_drive(&mut self, arena: &ArenaConfig) {
        let dt = self.integration_parameters.dt.max(1.0 / 240.0);
        let robot = &arena.robot;
        for player in self.players.values() {
            if let Some(body) = self.rigid_body_set.get_mut(player.body) {
                let rotation = body.rotation();
                let forward_x = -2.0 * (rotation.x * rotation.z + rotation.w * rotation.y);
                let forward_z = -1.0 + 2.0 * (rotation.x * rotation.x + rotation.y * rotation.y);
                let right_x = -forward_z;
                let right_z = forward_x;
                let velocity = body.linvel();
                let forward_speed = velocity.x * forward_x + velocity.z * forward_z;
                let lateral_speed = velocity.x * right_x + velocity.z * right_z;

                // Convert arcade input to normalized left/right wheel power.
                // Combining full throttle and steering therefore reduces the
                // forward component just as it does on a differential drive.
                let mut left_power = player.move_z + player.move_x;
                let mut right_power = player.move_z - player.move_x;
                let peak_power = left_power.abs().max(right_power.abs()).max(1.0);
                left_power /= peak_power;
                right_power /= peak_power;

                let target_speed = (left_power + right_power) * 0.5 * robot.max_speed_mps;
                let acceleration_limit = if target_speed.abs() < forward_speed.abs()
                    || target_speed.signum() != forward_speed.signum()
                {
                    robot.max_deceleration_mps2
                } else {
                    robot.max_acceleration_mps2
                }
                .min(robot.traction_friction * 9.81);
                let forward_delta = (target_speed - forward_speed)
                    .clamp(-acceleration_limit * dt, acceleration_limit * dt);
                let lateral_acceleration =
                    robot.lateral_grip_mps2.min(robot.traction_friction * 9.81);
                let lateral_delta =
                    (-lateral_speed).clamp(-lateral_acceleration * dt, lateral_acceleration * dt);
                let mass = body.mass();
                body.apply_impulse(
                    vector![
                        (forward_x * forward_delta + right_x * lateral_delta) * mass,
                        0.0,
                        (forward_z * forward_delta + right_z * lateral_delta) * mass
                    ]
                    .into(),
                    true,
                );

                let wheel_delta = right_power - left_power;
                let target_turn_rate = (wheel_delta * robot.max_speed_mps
                    / robot.track_width_m.max(0.1))
                .clamp(-robot.max_turn_rate_radps, robot.max_turn_rate_radps);
                let current_turn_rate = body.angvel().y;
                let turn_delta = (target_turn_rate - current_turn_rate).clamp(
                    -robot.max_angular_acceleration_radps2 * dt,
                    robot.max_angular_acceleration_radps2 * dt,
                );
                body.set_angvel(
                    vector![0.0, current_turn_rate + turn_delta, 0.0].into(),
                    true,
                );
            }
        }
    }

    pub fn player_snapshots(&self) -> Vec<PlayerSnapshot> {
        self.players
            .iter()
            .filter_map(|(id, player)| {
                self.rigid_body_set.get(player.body).map(|body| {
                    let p = body.translation();
                    let r = body.rotation();
                    // Rapier uses a right-handed Y-up quaternion. The previous
                    // implementation multiplied atan2's result by two, which
                    // made the visual turn diverge from the physics heading.
                    let yaw =
                        (2.0 * (r.w * r.y + r.x * r.z)).atan2(1.0 - 2.0 * (r.y * r.y + r.z * r.z));
                    let velocity = body.linvel();
                    let angular_velocity = body.angvel();
                    PlayerSnapshot {
                        id: id.clone(),
                        name: player.name.clone(),
                        team_name: player.team_name.clone(),
                        x: p.x,
                        y: p.y,
                        z: p.z,
                        yaw,
                        heading_deg: yaw.to_degrees(),
                        velocity_x: velocity.x,
                        velocity_y: velocity.y,
                        velocity_z: velocity.z,
                        angular_velocity_y: angular_velocity.y,
                        color: player.color.to_string(),
                        stored_balls: 0,
                        capacity: self.storage_capacity,
                    }
                })
            })
            .collect()
    }

    pub fn field_object_positions(&self) -> ObjectPositionsSync {
        let count = self.objects.len() as u32;
        let mask_bytes = (count as usize + 7) / 8;
        let mut active_mask = vec![0u8; mask_bytes];
        let mut moving_mask = vec![0u8; mask_bytes];
        let mut quantized_positions = Vec::with_capacity(count as usize * 3);

        for (i, object) in self.objects.iter().enumerate() {
            if let Some(body) = self.rigid_body_set.get(object.body) {
                active_mask[i / 8] |= 1 << (i % 8);
                if !body.is_sleeping() {
                    moving_mask[i / 8] |= 1 << (i % 8);
                    let position = body.translation();
                    let quantize = |v: f32| -> u16 {
                        ((v + 8.0) * 4095.9375).clamp(0.0, 65535.0) as u16
                    };
                    quantized_positions.push(quantize(position.x));
                    quantized_positions.push(quantize(position.y));
                    quantized_positions.push(quantize(position.z));
                }
            }
        }

        ObjectPositionsSync {
            count,
            active_mask,
            moving_mask,
            quantized_positions,
        }
    }

    pub fn contact_count(&self) -> usize {
        self.narrow_phase.contact_pairs().count()
    }

    pub fn tick(&mut self, dt: f64) {
        self.context.clock += dt;
        self.integration_parameters.dt = dt as f32;
        self.physics_pipeline.step(
            self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.ccd_solver,
            &(),
            &(),
        );
        self.apply_ball_rolling_resistance(dt as f32);
    }

    fn apply_ball_rolling_resistance(&mut self, dt: f32) {
        let deceleration = self.ball_rolling_resistance_mps2 * dt.max(0.0);
        if deceleration <= 0.0 {
            return;
        }

        for object in &self.objects {
            let Some(body) = self.rigid_body_set.get_mut(object.body) else {
                continue;
            };
            let velocity = body.linvel();
            // The floor top is y=0. Restrict this force to settled floor
            // contacts so airborne balls retain realistic ballistic motion.
            if body.translation().y > self.ball_radius + 0.003 || velocity.y.abs() > 0.35 {
                continue;
            }
            let horizontal_speed = (velocity.x * velocity.x + velocity.z * velocity.z).sqrt();
            if horizontal_speed <= f32::EPSILON {
                continue;
            }
            let scale = (horizontal_speed - deceleration).max(0.0) / horizontal_speed;
            body.set_linvel(
                vector![velocity.x * scale, velocity.y, velocity.z * scale].into(),
                true,
            );
        }
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

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

    #[test]
    #[ignore = "manual Rapier performance comparison"]
    fn benchmark_rapier_ball_interaction() {
        let arena = arena();
        let mut runtime = MatchRuntime::new("perf".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        runtime.add_player("player".into(), "Player".into(), "Team".into(), &arena);
        runtime.set_player_input("player", 0.35, 1.0, 1);

        let started = Instant::now();
        for _ in 0..300 {
            runtime.apply_player_drive(&arena);
            runtime.tick(1.0 / 60.0);
        }
        let elapsed = started.elapsed();
        let milliseconds_per_tick = elapsed.as_secs_f64() * 1_000.0 / 300.0;

        assert_eq!(runtime.field_object_positions().len(), arena.object_count);
        eprintln!(
            "Rapier {}-ball interaction: {:.2} ms/tick ({:.1} simulated FPS)",
            arena.object_count,
            milliseconds_per_tick,
            1_000.0 / milliseconds_per_tick
        );
    }
}
