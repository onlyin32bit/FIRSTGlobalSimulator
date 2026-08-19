use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

use super::match_registry::ObjectPositionsSync;
use super::match_runtime::{MatchContext, MatchPhase, PlayerSnapshot, ScoreState};
use super::pack_loader::{
    ArenaConfig, FieldBoundary, FieldCollider, FieldDefinition, FieldScoringTarget, FieldTrigger,
    RampPhysicsConfig, RestitutionCurveConfig, RobotDefinition, RobotPhysicsConfig,
    RobotSemanticKind,
};

mod brace;
mod collision;
mod hybrid_robot;
mod mechanics;
mod scoring;
use collision::*;
use hybrid_robot::*;

type Vec3 = [f32; 3];

const CONTROL_DEADBAND: f32 = 0.08;
const TURN_BRAKE_MULTIPLIER: f32 = 2.5;
const TURN_STOP_EPSILON_RADPS: f32 = 0.04;

fn apply_control_deadband(value: f32) -> f32 {
    let clamped = value.clamp(-1.0, 1.0);
    let magnitude = clamped.abs();
    if magnitude <= CONTROL_DEADBAND {
        0.0
    } else {
        clamped.signum() * (magnitude - CONTROL_DEADBAND) / (1.0 - CONTROL_DEADBAND)
    }
}


#[derive(Debug, Clone)]
pub struct TransferDebug {
    pub player_name: String,
    pub has_ball: bool,
    pub transfer_power_ok: bool,
    pub inside_robot: bool,
    pub touches_outtake: bool,
    pub has_transfer_zone: bool,
    pub has_robot_definition: bool,
    pub intake_power: f32,
    pub outtake_power: f32,
    pub outtake_force_n: f32,
    pub outtake_target_speed_mps: f32,
    pub outtake_contact_balls: u16,
    pub max_outtake_contact_speed_mps: f32,
}
pub type BallDebugFlag = u8;

#[derive(Debug, Clone, Copy, Default)]
pub struct StepMetrics {
    pub integrate_ms: f64,
    pub broad_phase_ms: f64,
    pub solve_ms: f64,
    pub candidate_pairs: usize,
    pub contacts: usize,
    pub active_balls: usize,
    pub sleeping_balls: usize,
}

/// An authored semantic edge event. Physics stays fully deterministic; these
/// events are emitted only on outside → inside transitions and are consumed by
/// the match rule runtime after the solver step.
#[derive(Debug, Clone)]
pub struct SemanticEvent {
    pub kind: &'static str,
    pub target_id: String,
    pub entity_id: String,
}

#[derive(Clone)]
struct Ball {
    position: Vec3,
    /// Position before this simulation tick. Static collision recovery uses
    /// this to keep a ball on the side of a thin field panel it approached
    /// from, rather than ejecting it through the opposite face.
    previous_position: Vec3,
    velocity: Vec3,
    pre_solve_velocity: Vec3,
    angular_velocity: Vec3,
    quiet_ticks: u16,
    sleeping: bool,
    grounded: bool,
    on_ramp: bool,
    active: bool,
    release_at_seconds: f32,
    /// Whether the EXT dispenser has poured this piece. A released piece that
    /// is later captured or contained must never be poured again.
    released: bool,
    last_outtake_alliance: Option<String>,
}

struct PlayerBody {
    name: String,
    team_name: String,
    position: Vec3,
    velocity: Vec3,
    yaw: f32,
    angular_velocity_y: f32,
    rotation: [f32; 4],
    angular_velocity: Vec3,
    move_x: f32,
    move_z: f32,
    intake_power: f32,
    outtake_power: f32,
    climb_power: f32,
    sequence: u64,
    color: &'static str,
    /// Outward normal of a static surface touched during the previous solver
    /// step. This keeps the drivetrain from turning shallow wall contact into
    /// a complete stop on the following tick.
    wall_contact_normal: Option<Vec3>,
    /// FIFO of ball indices captured into the on-robot hopper. Popping feeds
    /// the flywheel so contained (scored) balls are never recycled.
    stored: VecDeque<usize>,
    /// Fractional outtake accumulator so slow rates don't lose partial balls.
    outtake_accumulator: f32,
    /// Fractional intake accumulator (capture is rate-limited too).
    intake_accumulator: f32,
    /// Per-player adjustable mech spec overrides (capacity, flywheel, rates).
    mech: MechSpec,
    /// Authored brace currently touching the powered groove wheel.
    climbing_brace: Option<String>,
    floor_supported: bool,
    brace_support_impulse: f32,
    climb_wheel_angle: f32,
    climb_wheel_radps: f32,
}

/// Adjustable robot mechanic spec. Unset fields fall back to the arena pack.
#[derive(Debug, Clone, Default)]
pub struct MechSpec {
    pub capacity: Option<usize>,
    pub intake_rate_bps: Option<f32>,
    pub intake_surface_speed_mps: Option<f32>,
    pub intake_normal_force_n: Option<f32>,
    pub transfer_surface_speed_mps: Option<f32>,
    pub transfer_normal_force_n: Option<f32>,
    pub outtake_rate_bps: Option<f32>,
    pub outtake_velocity_mps: Option<f32>,
    pub outtake_normal_force_n: Option<f32>,
    pub outtake_angle_deg: Option<f32>,
    pub flywheel_width_m: Option<f32>,
}

impl MechSpec {
    pub fn capacity_with(&self, base: usize) -> usize {
        self.capacity.unwrap_or(base)
    }
}

/// Effective robot config = arena pack defaults merged with the player's
/// adjustable mechanic overrides.
fn effective_robot<'a>(base: &'a RobotPhysicsConfig, mech: &MechSpec) -> RobotPhysicsConfig {
    let mut robot = base.clone();
    if let Some(capacity) = mech.capacity {
        robot.storage_capacity = capacity;
    }
    if let Some(rate) = mech.intake_rate_bps {
        robot.intake_rate_bps = rate;
    }
    if let Some(speed) = mech.intake_surface_speed_mps {
        robot.intake_surface_speed_mps = speed;
    }
    if let Some(force) = mech.intake_normal_force_n {
        robot.intake_normal_force_n = force;
    }
    if let Some(speed) = mech.transfer_surface_speed_mps {
        robot.transfer_surface_speed_mps = speed;
    }
    if let Some(force) = mech.transfer_normal_force_n {
        robot.transfer_normal_force_n = force;
    }
    if let Some(rate) = mech.outtake_rate_bps {
        robot.outtake_rate_bps = rate;
    }
    if let Some(velocity) = mech.outtake_velocity_mps {
        robot.outtake_velocity_mps = velocity;
    }
    if let Some(force) = mech.outtake_normal_force_n {
        robot.outtake_normal_force_n = force;
    }
    if let Some(angle) = mech.outtake_angle_deg {
        robot.outtake_angle_deg = angle;
    }
    if let Some(width) = mech.flywheel_width_m {
        robot.flywheel_width_m = width;
    }
    robot
}

/// A narrow, deterministic physics backend for the simulator's dominant case:
/// equal-radius spheres, a carpet plane, four walls and planar robot boxes.
/// Keeping these arrays contiguous avoids Rapier's general constraint/island
/// machinery and makes the 1,000-ball workload linear in nearby contacts.
pub struct SphereRuntime {
    pub context: MatchContext,
    pub transfer_debug: Vec<TransferDebug>,
    pub ball_debug: Vec<BallDebugFlag>,
    pub score_state: ScoreState,
    balls: Vec<Ball>,
    players: BTreeMap<String, PlayerBody>,
    arena: Option<ArenaConfig>,
    grid_heads: Vec<i32>,
    grid_next: Vec<i32>,
    grid_cells: Vec<[i32; 3]>,
    pairs: Vec<(usize, usize)>,
    metrics: StepMetrics,
    field_boundary: FieldBoundary,
    field_floor_y: f32,
    ball_spawn: Vec3,
    ball_release_elapsed: Option<f32>,
    field_colliders: Vec<FieldCollider>,
    field_anchors: BTreeMap<String, Vec3>,
    scoring_targets: Vec<FieldScoringTarget>,
    scored_target_by_ball: Vec<Option<usize>>,
    scoring_enabled: bool,
    semantic_events: Vec<SemanticEvent>,
    intake_candidates: Vec<(f32, usize)>,
    robot_definition: Option<RobotDefinition>,
    robot_physics: Option<HybridRobotWorld>,
}

impl SphereRuntime {
    pub fn ball_contact_collider_ids(&self) -> Vec<Vec<String>> {
        self.balls.iter().map(|b| Vec::new()).collect()
    }

    const GRID_BUCKETS: usize = 1 << 14;
    #[cfg(test)]
    const FIELD_HALF_EXTENT: f32 = 8.0;

    pub fn new(match_id: String, game_pack_id: String, match_seed: u64) -> Self {
        Self {
            context: MatchContext {
                match_id,
                game_pack_id,
                game_pack_version: "1.0.0".into(),
                engine_version: "0.1.0".into(),
                match_seed,
                phase: MatchPhase::PreMatch,
                clock: 0.0,
            },
            score_state: ScoreState::default(),
            transfer_debug: Vec::new(),
            ball_debug: Vec::new(),
            balls: Vec::new(),
            players: BTreeMap::new(),
            arena: None,
            grid_heads: vec![-1; Self::GRID_BUCKETS],
            grid_next: Vec::new(),
            grid_cells: Vec::new(),
            pairs: Vec::new(),
            metrics: StepMetrics::default(),
            field_boundary: FieldBoundary::default(),
            field_floor_y: 0.0,
            ball_spawn: [0.0; 3],
            ball_release_elapsed: None,
            field_colliders: Vec::new(),
            field_anchors: BTreeMap::new(),
            scoring_targets: Vec::new(),
            scored_target_by_ball: Vec::new(),
            scoring_enabled: false,
            semantic_events: Vec::new(),
            intake_candidates: Vec::with_capacity(16),
            robot_definition: None,
            robot_physics: None,
        }
    }

    pub fn create_test_arena(&mut self, arena: &ArenaConfig) {
        self.create_field_arena(arena, &FieldDefinition::default());
        self.context.phase = MatchPhase::Teleop;
        self.ball_release_elapsed = Some(arena.spawn_release_seconds.max(0.0));
        self.release_queued_balls(arena);
    }

    /// Enter the live phase. Field packs begin with their game pieces queued
    /// inside the semantic dispenser; no ball is released until this method is
    /// called by the authoritative match clock.
    pub fn begin_match(&mut self) {
        if self.context.phase != MatchPhase::PreMatch {
            return;
        }
        self.context.phase = MatchPhase::Teleop;
        self.ball_release_elapsed = Some(0.0);
    }

    pub fn set_scoring_enabled(&mut self, enabled: bool) {
        self.scoring_enabled = enabled;
    }

    pub fn create_field_arena(&mut self, arena: &ArenaConfig, field: &FieldDefinition) {
        self.arena = Some(arena.clone());
        self.field_boundary = field.boundary.clone();
        self.field_floor_y = field.floor_height_m;
        self.field_colliders = field.colliders.clone();
        self.field_anchors = field.anchors.clone();
        self.scoring_targets = field.scoring_targets.clone();
        self.ball_spawn = self.field_anchors.get("EXTballspawn").copied().unwrap_or([
            0.0,
            self.field_floor_y + arena.spawn_height,
            0.0,
        ]);
        self.ball_spawn[1] = (self.ball_spawn[1] + arena.spawn_offset_y_m)
            .max(self.field_floor_y + arena.ball.radius_m());
        self.ball_release_elapsed = None;
        self.balls.clear();
        self.balls.reserve(arena.object_count);
        self.grid_next.resize(arena.object_count, -1);
        self.grid_cells.resize(arena.object_count, [0; 3]);
        self.pairs.reserve(arena.object_count.saturating_mul(8));

        let count = arena.object_count.max(1) as f32;
        for index in 0..arena.object_count {
            self.balls.push(Ball {
                position: self.ball_spawn,
                previous_position: self.ball_spawn,
                velocity: [0.0; 3],
                pre_solve_velocity: [0.0; 3],
                angular_velocity: [0.0; 3],
                quiet_ticks: 0,
                sleeping: false,
                grounded: false,
                on_ramp: false,
                active: false,
                release_at_seconds: arena.spawn_release_seconds.max(0.0) * index as f32 / count,
                released: false,
                last_outtake_alliance: None,
            });
        }
        self.scored_target_by_ball = vec![None; self.balls.len()];
        self.semantic_events.clear();
    }

    /// Select the pack-authored robot used by this match. Match tickets are
    /// immutable, so all connected players share this validated definition.
    pub fn set_robot_definition(&mut self, definition: Option<&RobotDefinition>) {
        self.robot_definition = definition.cloned();
        self.robot_physics = self.arena.as_ref().and_then(|arena| {
            definition.map(|definition| {
                HybridRobotWorld::new(
                    arena,
                    definition,
                    &self.field_colliders,
                    &self.field_boundary,
                    self.field_floor_y,
                )
            })
        });
        if let (Some(world), Some(arena)) = (self.robot_physics.as_mut(), self.arena.as_ref()) {
            for (id, player) in &self.players {
                world.add_robot(id, player, arena);
            }
        }
    }

    /// Returns the pack-authored field perimeter with the requested clearance.
    /// Keeping this in the runtime avoids treating visual guard-rail geometry
    /// as one giant solid volume.
    fn planar_limits(&self, inset_x: f32, inset_z: f32) -> (f32, f32, f32, f32) {
        (
            self.field_boundary.min[0] + inset_x,
            self.field_boundary.max[0] - inset_x,
            self.field_boundary.min[2] + inset_z,
            self.field_boundary.max[2] - inset_z,
        )
    }

    fn robot_center_y(&self, arena: &ArenaConfig) -> f32 {
        self.field_floor_y + arena.robot.height_m * 0.5
    }

    fn robot_collision_half_extents(&self, arena: &ArenaConfig) -> Vec3 {
        self.robot_definition
            .as_ref()
            .map(|definition| definition.bounds.half_extents)
            .unwrap_or([
                arena.robot.width_m * 0.5,
                arena.robot.height_m * 0.5,
                arena.robot.length_m * 0.5,
            ])
    }

    /// Release queued balls from the semantic EXT dispenser. The trajectory
    /// points into the field centre with a small deterministic fan, so every
    /// client observes the same four-second pour without random state.
    fn release_queued_balls(&mut self, arena: &ArenaConfig) {
        let Some(elapsed) = self.ball_release_elapsed else {
            return;
        };
        let horizontal = [-self.ball_spawn[0], 0.0, -self.ball_spawn[2]];
        let horizontal_length = (horizontal[0] * horizontal[0] + horizontal[2] * horizontal[2])
            .sqrt()
            .max(1.0e-5);
        let forward = [
            horizontal[0] / horizontal_length,
            0.0,
            horizontal[2] / horizontal_length,
        ];
        let lateral = [-forward[2], 0.0, forward[0]];
        for (index, ball) in self.balls.iter_mut().enumerate() {
            if ball.released || ball.release_at_seconds > elapsed {
                continue;
            }
            // Deterministic, hash-like variation prevents the regular
            // phyllotaxis rows from settling into an artificial lattice.
            let lateral_noise = fountain_noise(index as u32, 0.31);
            let forward_noise = fountain_noise(index as u32, 1.73);
            let vertical_noise = fountain_noise(index as u32, 4.19);
            let nozzle_distance = arena.spawn_radius.max(0.0) * forward_noise.abs().sqrt();
            let lateral_speed = lateral_noise * arena.spawn_fountain_spread_mps;
            let forward_speed = arena.spawn_fountain_forward_speed_mps
                + forward_noise * arena.spawn_fountain_spread_mps * 0.30;
            ball.position = add(
                self.ball_spawn,
                add(
                    mul(lateral, lateral_noise * nozzle_distance),
                    mul(forward, forward_noise * nozzle_distance),
                ),
            );
            ball.velocity = add(
                add(mul(forward, forward_speed), mul(lateral, lateral_speed)),
                [
                    0.0,
                    arena.spawn_fountain_vertical_speed_mps + vertical_noise * 0.08,
                    0.0,
                ],
            );
            ball.pre_solve_velocity = ball.velocity;
            ball.angular_velocity = [0.0, lateral_noise * 8.0, 0.0];
            ball.quiet_ticks = 0;
            ball.sleeping = false;
            ball.released = true;
            ball.active = true;
        }
    }

    pub fn add_player(
        &mut self,
        user_id: String,
        name: String,
        team_name: String,
        slot_id: Option<&str>,
        arena: &ArenaConfig,
    ) {
        if self.players.contains_key(&user_id) {
            return;
        }
        let slot = self.players.len();
        let angle = slot as f32 * std::f32::consts::TAU / 8.0;
        let colors = [
            "#f97316", "#2563eb", "#16a34a", "#9333ea", "#dc2626", "#0891b2", "#ca8a04", "#db2777",
        ];
        let color = if team_name == "red" {
            "#ef4444"
        } else if team_name == "blue" {
            "#3b82f6"
        } else {
            colors[slot % colors.len()]
        };
        let anchor_key = slot_id.and_then(|id| {
            let (alliance, role) = id.split_once('-')?;
            let index = role.strip_prefix("driver-")?;
            Some(format!("{alliance}Spawn{index}"))
        });
        let spawn = anchor_key
            .as_deref()
            .and_then(|key| self.field_anchors.get(key))
            .copied();
        let yaw = spawn
            .map(|point| {
                let center_x = (self.field_boundary.min[0] + self.field_boundary.max[0]) * 0.5;
                let center_z = (self.field_boundary.min[2] + self.field_boundary.max[2]) * 0.5;
                let forward_x = center_x - point[0];
                let forward_z = center_z - point[2];
                (-forward_x).atan2(-forward_z)
            })
            .unwrap_or(angle);
        self.players.insert(
            user_id.clone(),
            PlayerBody {
                name,
                team_name,
                position: spawn
                    .map(|point| [point[0], self.robot_center_y(arena), point[2]])
                    .unwrap_or([
                        angle.cos() * 4.0,
                        self.robot_center_y(arena),
                        angle.sin() * 4.0,
                    ]),
                velocity: [0.0; 3],
                yaw,
                angular_velocity_y: 0.0,
                rotation: [0.0, (yaw * 0.5).sin(), 0.0, (yaw * 0.5).cos()],
                angular_velocity: [0.0; 3],
                move_x: 0.0,
                move_z: 0.0,
                intake_power: 0.0,
                outtake_power: 0.0,
                climb_power: 0.0,
                sequence: 0,
                color,
                wall_contact_normal: None,
                stored: VecDeque::new(),
                outtake_accumulator: 0.0,
                intake_accumulator: 0.0,
                mech: MechSpec::default(),
                climbing_brace: None,
                floor_supported: true,
                brace_support_impulse: 0.0,
                climb_wheel_angle: 0.0,
                climb_wheel_radps: 0.0,
            },
        );
        if let (Some(world), Some(player)) =
            (self.robot_physics.as_mut(), self.players.get(&user_id))
        {
            world.add_robot(&user_id, player, arena);
        }
    }

    pub fn remove_player(&mut self, user_id: &str) {
        self.players.remove(user_id);
        if let Some(world) = &mut self.robot_physics {
            world.remove_robot(user_id);
        }
    }

    pub fn set_player_input(
        &mut self,
        user_id: &str,
        move_x: f32,
        move_z: f32,
        intake_power: f32,
        outtake_power: f32,
        sequence: u64,
    ) {
        self.set_player_input_with_climb(
            user_id,
            move_x,
            move_z,
            intake_power,
            outtake_power,
            0.0,
            sequence,
        );
    }

    pub fn set_player_input_with_climb(
        &mut self,
        user_id: &str,
        move_x: f32,
        move_z: f32,
        intake_power: f32,
        outtake_power: f32,
        climb_power: f32,
        sequence: u64,
    ) {
        if let Some(player) = self.players.get_mut(user_id)
            && sequence >= player.sequence
        {
            player.sequence = sequence;
            player.move_x = apply_control_deadband(move_x);
            player.move_z = apply_control_deadband(move_z);
            player.outtake_power = outtake_power.clamp(0.0, 1.0);
            player.intake_power = intake_power.clamp(0.0, 1.0).max(player.outtake_power);
            player.climb_power = climb_power.clamp(0.0, 1.0);
        }
    }

    pub fn disable_player_controls(&mut self) {
        for player in self.players.values_mut() {
            player.move_x = 0.0;
            player.move_z = 0.0;
            player.intake_power = 0.0;
            player.outtake_power = 0.0;
            player.climb_power = 0.0;
            player.climbing_brace = None;
        }
    }

    pub fn set_player_mech(&mut self, user_id: &str, mech: MechSpec) {
        if let Some(player) = self.players.get_mut(user_id) {
            player.mech = mech;
        }
    }

    pub fn apply_player_drive(&mut self, arena: &ArenaConfig, dt: f32) {
        if self.robot_physics.is_some() {
            return;
        }
        let robot = &arena.robot;
        for player in self.players.values_mut() {
            // Once the wheels are engaged, the brace motor owns travel. Keep
            // the raw input intact so normal driving resumes on release.
            let move_x = if player.climbing_brace.is_some() {
                0.0
            } else {
                player.move_x
            };
            let move_z = if player.climbing_brace.is_some() {
                0.0
            } else {
                player.move_z
            };
            let forward = [-player.yaw.sin(), 0.0, -player.yaw.cos()];
            let right = [-forward[2], 0.0, forward[0]];
            let forward_speed = dot(player.velocity, forward);
            let lateral_speed = dot(player.velocity, right);
            let mut left = move_z + move_x;
            let mut right_power = move_z - move_x;
            let peak = left.abs().max(right_power.abs()).max(1.0);
            left /= peak;
            right_power /= peak;

            let target_speed = (left + right_power) * 0.5 * robot.max_speed_mps;
            let braking = target_speed.abs() < forward_speed.abs()
                || target_speed.signum() != forward_speed.signum()
                || target_speed.abs() < 1.0e-4;
            let mass = robot.mass_kg.max(1.0);
            let traction_limit = robot.traction_friction.max(0.0) * mass * 9.81;
            let force_limit = if braking {
                robot
                    .max_brake_force_n
                    .min(mass * robot.max_deceleration_mps2)
            } else {
                let power_limit = robot.max_drive_power_w
                    / forward_speed.abs().max(robot.max_speed_mps * 0.08).max(0.1);
                robot
                    .max_drive_force_n
                    .min(mass * robot.max_acceleration_mps2)
                    .min(power_limit)
            }
            .min(traction_limit)
            .max(0.0);
            let requested_force = (target_speed - forward_speed) * mass / dt.max(1.0e-5);
            let drive_force = requested_force.clamp(-force_limit, force_limit);
            let forward_delta = drive_force / mass * dt;
            let lateral_delta =
                (-lateral_speed).clamp(-robot.lateral_grip_mps2 * dt, robot.lateral_grip_mps2 * dt);
            let wall_normal = player.wall_contact_normal;
            let mut lateral_change = [right[0] * lateral_delta, 0.0, right[2] * lateral_delta];
            if let Some(normal) = wall_normal {
                // A differential-drive robot rubbing a perimeter panel still
                // has a wall-parallel component of its wheel force. The old
                // virtual lateral-grip model removed all of it, which made a
                // robot stop dead for even a very shallow impact. The carpet
                // rolling resistance below still slows the robot, while the
                // wall itself removes no tangential speed.
                let normal_change = mul(normal, dot(lateral_change, normal));
                lateral_change = normal_change;
            }
            player.velocity[0] += forward[0] * forward_delta + lateral_change[0];
            player.velocity[2] += forward[2] * forward_delta + lateral_change[2];

            if let Some(normal) = wall_normal {
                // A wall is unilateral: remove only motion into it. Applying
                // this before integration avoids a one-frame inward pulse,
                // while keeping the tangent component intact.
                let into_surface = dot(player.velocity, normal);
                if into_surface < 0.0 {
                    player.velocity = sub(player.velocity, mul(normal, into_surface));
                }
            }

            let target_turn = ((right_power - left) * robot.max_speed_mps
                / robot.track_width_m.max(0.1))
            .clamp(-robot.max_turn_rate_radps, robot.max_turn_rate_radps);
            let turn_acceleration = if move_x.abs() <= f32::EPSILON {
                robot.max_angular_acceleration_radps2 * TURN_BRAKE_MULTIPLIER
            } else {
                robot.max_angular_acceleration_radps2
            };
            player.angular_velocity_y += (target_turn - player.angular_velocity_y)
                .clamp(-turn_acceleration * dt, turn_acceleration * dt);
            if move_x.abs() <= f32::EPSILON
                && player.angular_velocity_y.abs() < TURN_STOP_EPSILON_RADPS
            {
                player.angular_velocity_y = 0.0;
            }
        }
    }

    /// Ball hopper mechanics: powered intake captures balls in the roller
    /// mouth into storage, and the wide flywheel launches stored balls at the
    /// adjustable velocity/angle with a deterministic lateral spread. Both
    /// steps are rate-limited so a full robot swallows and spits at a steady
    /// pace instead of vacuuming the field in one tick.
    pub fn tick(&mut self, dt: f64) {
        let dt = dt as f32;
        self.context.clock += dt as f64;
        let Some(arena) = self.arena.clone() else {
            return;
        };
        if let Some(elapsed) = &mut self.ball_release_elapsed {
            *elapsed += dt;
        }
        self.release_queued_balls(&arena);
        for ball in &mut self.balls {
            if ball.active {
                ball.previous_position = ball.position;
            }
        }
        let integrate_started = Instant::now();
        self.integrate(&arena, dt);
        self.metrics.integrate_ms = integrate_started.elapsed().as_secs_f64() * 1_000.0;

        let broad_started = Instant::now();
        self.build_pairs(arena.ball.diameter_m.max(0.001));
        self.metrics.broad_phase_ms = broad_started.elapsed().as_secs_f64() * 1_000.0;
        self.metrics.candidate_pairs = self.pairs.len();

        let solve_started = Instant::now();
        let mut contacts = 0;
        for _ in 0..arena.solver.position_iterations {
            contacts = self.solve_positions(&arena, dt);
        }
        self.reconstruct_velocities(&arena, dt);
        for _ in 0..arena.solver.velocity_iterations {
            self.apply_contact_velocities(&arena, dt);
            self.apply_static_contact_velocities(&arena, dt, false);
            self.apply_robot_wall_velocity_constraints(&arena);
        }
        self.limit_ball_energy(&arena);
        self.step_mechanics(&arena, dt);
        self.settle_retained_balls(dt);
        self.update_sleeping(&arena, dt);
        if let Some(world) = &mut self.robot_physics {
            world.accept_sphere_response(&self.players);
        }
        self.metrics.solve_ms = solve_started.elapsed().as_secs_f64() * 1_000.0;
        self.metrics.contacts = contacts;
        self.metrics.sleeping_balls = self
            .balls
            .iter()
            .filter(|ball| ball.active && ball.sleeping)
            .count();
        self.metrics.active_balls = self
            .balls
            .iter()
            .filter(|ball| ball.active && !ball.sleeping)
            .count();
        if self.scoring_enabled {
            self.reconcile_scoring();
        }
    }

    fn integrate(&mut self, arena: &ArenaConfig, dt: f32) {
        let linear_decay = (-arena.ball.linear_damping * dt).exp();
        let angular_decay = (-arena.ball.angular_damping * dt).exp();
        let cross_section = std::f32::consts::PI * arena.ball.radius_m().powi(2);
        let drag_acceleration_factor =
            0.5 * arena.ball.air_density_kg_m3 * arena.ball.drag_coefficient * cross_section
                / arena.ball.mass_kg.max(0.001);
        for ball in &mut self.balls {
            if !ball.active {
                continue;
            }
            ball.grounded = false;
            ball.on_ramp = false;
            if ball.sleeping {
                continue;
            }
            ball.velocity[1] -= 9.81 * arena.gravity_scale * dt;
            let air_speed = length_sq(ball.velocity).sqrt();
            if air_speed > 1.0e-5 {
                // Quadratic sphere drag: Fd = 1/2 rho Cd A |v|².
                let drag_scale = (1.0 - drag_acceleration_factor * air_speed * dt).max(0.0);
                ball.velocity = mul(ball.velocity, drag_scale);
            }
            ball.velocity = mul(ball.velocity, linear_decay);
            ball.pre_solve_velocity = ball.velocity;
            ball.angular_velocity = mul(ball.angular_velocity, angular_decay);
            ball.position = add(ball.position, mul(ball.velocity, dt));
        }
        if let Some(world) = &mut self.robot_physics {
            world.step(&mut self.players, arena, dt);
            return;
        }
        let robot_center_y = self.robot_center_y(arena);
        let robot_half = self.robot_collision_half_extents(arena);
        let field_boundary = self.field_boundary.clone();
        let ground_offset_y = -arena.robot.height_m * 0.5;
        let robot_definition = self.robot_definition.as_ref();
        for player in self.players.values_mut() {
            // Contacts are refreshed by the position solver below. Keeping a
            // normal for one drive step gives stable wall sliding without
            // constraining a robot that has already driven away.
            player.wall_contact_normal = None;
            player.climbing_brace = None;
            if player.position[1] > robot_center_y {
                player.velocity[1] -= 9.81 * arena.gravity_scale * dt;
                player.position[1] += player.velocity[1] * dt;
            } else {
                player.velocity[1] = 0.0;
            }
            player.position[0] += player.velocity[0] * dt;
            player.position[2] += player.velocity[2] * dt;
            player.position[1] = player.position[1].max(robot_center_y);
            player.yaw = wrap_angle(player.yaw + player.angular_velocity_y * dt);
            player.rotation = [0.0, (player.yaw * 0.5).sin(), 0.0, (player.yaw * 0.5).cos()];
            if let Some(normal) = project_robot_boundary(
                player,
                robot_half,
                robot_definition,
                ground_offset_y,
                &field_boundary,
            ) {
                player.wall_contact_normal = Some(normal);
            }
            let drag = (-arena.robot.rolling_resistance * dt).exp();
            player.velocity[0] *= drag;
            player.velocity[2] *= drag;
        }
    }

    fn build_pairs(&mut self, cell_size: f32) {
        self.grid_heads.fill(-1);
        self.pairs.clear();
        for (index, ball) in self.balls.iter().enumerate() {
            if !ball.active {
                self.grid_next[index] = -1;
                continue;
            }
            let cell = cell_for(ball.position, cell_size);
            let bucket = hash_cell(cell) & (Self::GRID_BUCKETS - 1);
            self.grid_cells[index] = cell;
            self.grid_next[index] = self.grid_heads[bucket];
            self.grid_heads[bucket] = index as i32;
        }
        if !self
            .arena
            .as_ref()
            .is_some_and(|arena| arena.ball_to_ball_collisions)
        {
            return;
        }
        for index in 0..self.balls.len() {
            if !self.balls[index].active {
                continue;
            }
            let cell = self.grid_cells[index];
            for y in -1..=1 {
                for z in -1..=1 {
                    for x in -1..=1 {
                        let neighbor = [cell[0] + x, cell[1] + y, cell[2] + z];
                        let bucket = hash_cell(neighbor) & (Self::GRID_BUCKETS - 1);
                        let mut other = self.grid_heads[bucket];
                        while other >= 0 {
                            let other_index = other as usize;
                            if other_index > index && self.grid_cells[other_index] == neighbor {
                                self.pairs.push((index, other_index));
                            }
                            other = self.grid_next[other_index];
                        }
                    }
                }
            }
        }
    }

    fn solve_positions(&mut self, arena: &ArenaConfig, dt: f32) -> usize {
        let radius = arena.ball.radius_m();
        let diameter_sq = arena.ball.diameter_m * arena.ball.diameter_m;
        let alpha = arena.solver.contact_compliance.max(0.0) / (dt * dt);
        let max_correction = arena.solver.max_depenetration_speed_mps.max(0.0) * dt
            / arena.solver.position_iterations.max(1) as f32;
        let inverse_ball_mass = 1.0 / arena.ball.mass_kg.max(0.001);
        let inverse_robot_mass = 1.0 / arena.robot.mass_kg.max(1.0);
        let robot_fully_dynamic = self.robot_physics.is_some();
        let mut contacts = 0;

        for ball in &mut self.balls {
            if !ball.active {
                continue;
            }
            contacts += project_static_position(
                ball,
                arena,
                radius,
                self.field_floor_y,
                &self.field_boundary,
                &self.field_colliders,
            );
        }
        contacts += self.retain_scored_balls(radius);

        for &(left, right) in &self.pairs {
            let delta = sub(self.balls[right].position, self.balls[left].position);
            let distance_sq = length_sq(delta);
            if distance_sq >= diameter_sq {
                continue;
            }
            contacts += 1;
            let (normal, distance) = if distance_sq > 1.0e-12 {
                let distance = distance_sq.sqrt();
                (mul(delta, 1.0 / distance), distance)
            } else {
                ([1.0, 0.0, 0.0], 0.0)
            };
            let penetration = arena.ball.diameter_m - distance;
            let left_direction = mul(normal, -1.0);
            let right_direction = normal;
            let left_inverse_mass = if boundary_blocks_motion(
                self.balls[left].position,
                left_direction,
                radius,
                &self.field_boundary,
            ) {
                0.0
            } else {
                inverse_ball_mass
            };
            let right_inverse_mass = if boundary_blocks_motion(
                self.balls[right].position,
                right_direction,
                radius,
                &self.field_boundary,
            ) {
                0.0
            } else {
                inverse_ball_mass
            };
            let inverse_mass_sum = left_inverse_mass + right_inverse_mass;
            if inverse_mass_sum > 0.0 {
                let lambda = penetration / (inverse_mass_sum + alpha);
                let left_correction = (left_inverse_mass * lambda).min(max_correction);
                let right_correction = (right_inverse_mass * lambda).min(max_correction);
                self.balls[left].position = add(
                    self.balls[left].position,
                    mul(left_direction, left_correction),
                );
                self.balls[right].position = add(
                    self.balls[right].position,
                    mul(right_direction, right_correction),
                );
            }
            if self.balls[left].sleeping != self.balls[right].sleeping {
                self.balls[left].sleeping = false;
                self.balls[right].sleeping = false;
            }
        }

        let field_colliders = &self.field_colliders;
        let robot_definition = self.robot_definition.clone();
        let robot_center_y = self.robot_center_y(arena);
        let robot_half = self.robot_collision_half_extents(arena);
        let field_boundary = self.field_boundary.clone();
        for player in self.players.values_mut() {
            let intake_mouth = robot_definition.as_ref().and_then(|definition| {
                definition
                    .zones
                    .iter()
                    .find(|zone| zone.kind == RobotSemanticKind::Intake)
                    .map(|zone| {
                        robot_local_collider_pose(
                            &zone.collider,
                            player.position,
                            player.rotation,
                            -arena.robot.height_m * 0.5,
                        )
                    })
            });
            let transfer_mouth = robot_definition.as_ref().and_then(|definition| {
                definition
                    .zones
                    .iter()
                    .find(|zone| zone.kind == RobotSemanticKind::Transfer)
                    .map(|zone| {
                        robot_local_collider_pose(
                            &zone.collider,
                            player.position,
                            player.rotation,
                            -arena.robot.height_m * 0.5,
                        )
                    })
            });
            let outtake_mouth = robot_definition.as_ref().and_then(|definition| {
                definition
                    .zones
                    .iter()
                    .find(|zone| zone.kind == RobotSemanticKind::Outtake)
                    .map(|zone| {
                        robot_local_collider_pose(
                            &zone.collider,
                            player.position,
                            player.rotation,
                            -arena.robot.height_m * 0.5,
                        )
                    })
            });
            if arena.robot.intake_enabled && robot_definition.is_none() {
                for ball in &mut self.balls {
                    if !ball.active {
                        continue;
                    }
                    if let Some((normal, penetration, _, _)) = roller_contact(
                        ball.position,
                        radius,
                        player.position,
                        player.yaw,
                        &arena.robot,
                    ) {
                        contacts += 1;
                        resolve_ball_robot_position(
                            ball,
                            player,
                            normal,
                            penetration,
                            radius,
                            inverse_ball_mass,
                            inverse_robot_mass,
                            alpha,
                            max_correction,
                            &self.field_boundary,
                            robot_fully_dynamic,
                        );
                        ball.sleeping = false;
                        ball.quiet_ticks = 0;
                    }
                }
            }
            for ball in &mut self.balls {
                if !ball.active {
                    continue;
                }
                let touches_intake = intake_mouth.as_ref().is_some_and(|mouth| {
                    sphere_authored_obb_contact(ball.position, radius, mouth).is_some()
                });
                let touches_transfer = transfer_mouth.as_ref().is_some_and(|mouth| {
                    sphere_authored_obb_contact(ball.position, radius, mouth).is_some()
                });
                let touches_outtake = outtake_mouth.as_ref().is_some_and(|mouth| {
                    sphere_authored_obb_contact(ball.position, radius, mouth).is_some()
                });

                if (player.intake_power > 0.0 && touches_intake)
                    || (player.outtake_power > 0.0 && touches_transfer)
                    || (player.outtake_power > 0.0 && touches_outtake)
                {
                    continue;
                }
                if let Some(definition) = &robot_definition {
                    // Resolve every authored physics OBB, not just the deepest
                    // overlap behind a coarse robot envelope. This preserves
                    // ramps and funnel walls as simultaneous contacts and makes
                    // every surface in bot.physics.json a hard obstacle.
                    let ground_offset_y = -arena.robot.height_m * 0.5;
                    for local in &definition.colliders {
                        let collider = robot_local_collider_pose(
                            local,
                            player.position,
                            player.rotation,
                            ground_offset_y,
                        );
                        if let Some((normal, penetration)) = sphere_robot_obb_contact(
                            ball.position,
                            ball.previous_position,
                            radius,
                            &collider,
                        ) {
                            contacts += 1;
                            resolve_ball_robot_position(
                                ball,
                                player,
                                normal,
                                penetration,
                                radius,
                                inverse_ball_mass,
                                inverse_robot_mass,
                                alpha,
                                max_correction,
                                &self.field_boundary,
                                robot_fully_dynamic,
                            );
                            if !robot_fully_dynamic {
                                player.position[1] = player.position[1].max(robot_center_y);
                            }
                            ball.sleeping = false;
                            ball.quiet_ticks = 0;
                        }
                    }
                } else if let Some((normal, penetration)) = sphere_robot_box_contact(
                    ball.position,
                    ball.previous_position,
                    radius,
                    player.position,
                    player.yaw,
                    [
                        arena.robot.width_m * 0.5,
                        arena.robot.height_m * 0.5,
                        arena.robot.length_m * 0.5,
                    ],
                ) {
                    contacts += 1;
                    resolve_ball_robot_position(
                        ball,
                        player,
                        normal,
                        penetration,
                        radius,
                        inverse_ball_mass,
                        inverse_robot_mass,
                        alpha,
                        max_correction,
                        &self.field_boundary,
                        robot_fully_dynamic,
                    );
                    if !robot_fully_dynamic {
                        player.position[1] = player.position[1].max(robot_center_y);
                    }
                    ball.sleeping = false;
                    ball.quiet_ticks = 0;
                }
            }
            if !robot_fully_dynamic {
                if let Some(normal) = project_robot_boundary(
                    player,
                    robot_half,
                    robot_definition.as_ref(),
                    -arena.robot.height_m * 0.5,
                    &field_boundary,
                ) {
                    player.wall_contact_normal = Some(normal);
                }
                let (field_contacts, wall_normal) = project_robot_field_colliders(
                    player,
                    robot_half,
                    robot_definition.as_ref(),
                    -arena.robot.height_m * 0.5,
                    field_colliders,
                    false,
                );
                contacts += field_contacts;
                if wall_normal.is_some() {
                    player.wall_contact_normal = wall_normal;
                }
            }
        }
        // Dynamic contacts can push a ball through a static boundary. End
        // every iteration by projecting onto the field/ramp so the last
        // solver iteration cannot leave an object outside the arena.
        let field_colliders = &self.field_colliders;
        for ball in &mut self.balls {
            if !ball.active {
                continue;
            }
            contacts += project_static_position(
                ball,
                arena,
                radius,
                self.field_floor_y,
                &self.field_boundary,
                field_colliders,
            );
        }
        contacts += self.retain_scored_balls(radius);
        contacts
    }

    fn reconstruct_velocities(&mut self, arena: &ArenaConfig, dt: f32) {
        for ball in &mut self.balls {
            if !ball.active {
                continue;
            }
            if ball.sleeping {
                ball.velocity = [0.0; 3];
                ball.angular_velocity = [0.0; 3];
                continue;
            }
            // Split impulse: penetration correction changes geometry only.
            // Turning that correction into velocity is what previously made
            // deeply trapped balls explode out of the chassis.
            ball.velocity = ball.pre_solve_velocity;
        }
        self.apply_static_contact_velocities(arena, dt, true);
    }

    fn apply_static_contact_velocities(
        &mut self,
        arena: &ArenaConfig,
        dt: f32,
        apply_rolling_resistance: bool,
    ) {
        let (min_x, max_x, min_z, max_z) =
            self.planar_limits(arena.ball.radius_m(), arena.ball.radius_m());
        for ball in &mut self.balls {
            if !ball.active {
                continue;
            }
            if ball.sleeping {
                continue;
            }
            if ball.grounded {
                resolve_sphere_surface_velocity(
                    ball,
                    [0.0, 1.0, 0.0],
                    [0.0; 3],
                    &arena.floor.restitution_curve,
                    arena.floor.static_friction,
                    arena.floor.dynamic_friction,
                    arena.ball.radius_m(),
                    arena.ball.mass_kg,
                    arena.ball.inertia_factor,
                    arena.ball.mass_kg * 9.81 * arena.gravity_scale * dt,
                    arena.solver.restitution_velocity_threshold_mps,
                );
                let speed = (ball.velocity[0] * ball.velocity[0]
                    + ball.velocity[2] * ball.velocity[2])
                    .sqrt();
                if apply_rolling_resistance && speed > 0.0 {
                    // Carpet hysteresis is modeled independently from Coulomb
                    // slip friction as a rolling force/torque pair.
                    let rolling_step = arena.floor.rolling_resistance_mps2.max(0.0) * dt;
                    let next = (speed - rolling_step).max(0.0) / speed;
                    ball.velocity[0] *= next;
                    ball.velocity[2] *= next;
                    let angular_step = rolling_step / arena.ball.radius_m().max(0.001);
                    ball.angular_velocity[0] =
                        approach_zero(ball.angular_velocity[0], angular_step);
                    ball.angular_velocity[2] =
                        approach_zero(ball.angular_velocity[2], angular_step);
                }
            }
            if ball.on_ramp {
                let angle = arena.ramp.angle_deg.to_radians();
                let normal = [0.0, angle.cos(), -angle.sin()];
                resolve_sphere_surface_velocity(
                    ball,
                    normal,
                    [0.0; 3],
                    &arena.ramp.surface.restitution_curve,
                    arena.ramp.surface.static_friction,
                    arena.ramp.surface.dynamic_friction,
                    arena.ball.radius_m(),
                    arena.ball.mass_kg,
                    arena.ball.inertia_factor,
                    arena.ball.mass_kg * 9.81 * angle.cos() * dt,
                    arena.solver.restitution_velocity_threshold_mps,
                );
            }
            for (axis, min, max, negative_normal, positive_normal) in [
                (0, min_x, max_x, [1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]),
                (2, min_z, max_z, [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]),
            ] {
                let normal = if ball.position[axis] <= min + 1.0e-5 {
                    Some(negative_normal)
                } else if ball.position[axis] >= max - 1.0e-5 {
                    Some(positive_normal)
                } else {
                    None
                };
                if let Some(normal) = normal {
                    // The test fixture uses polycarbonate at the goal ends
                    // (Z) and metal perimeter structure at the sidelines (X).
                    let surface = if axis == 2 {
                        &arena.goal_wall
                    } else {
                        &arena.metal_wall
                    };
                    resolve_sphere_surface_velocity(
                        ball,
                        normal,
                        [0.0; 3],
                        &surface.restitution_curve,
                        surface.static_friction,
                        surface.dynamic_friction,
                        arena.ball.radius_m(),
                        arena.ball.mass_kg,
                        arena.ball.inertia_factor,
                        0.0,
                        arena.solver.restitution_velocity_threshold_mps,
                    );
                }
            }

            // Resolve velocity, restitution (bouncing), and friction for all 3D field colliders (including SU goal walls)
            for collider in &self.field_colliders {
                if let Some(normal) =
                    sphere_collider_contact(ball.position, arena.ball.radius_m() * 1.01, collider)
                {
                    let id_lower = collider.id.to_lowercase();
                    let surface = if id_lower.contains("su")
                        || id_lower.contains("goal")
                        || id_lower.contains("polycarbonate")
                    {
                        &arena.goal_wall
                    } else {
                        &arena.metal_wall
                    };
                    resolve_sphere_surface_velocity(
                        ball,
                        normal,
                        [0.0; 3],
                        &surface.restitution_curve,
                        surface.static_friction,
                        surface.dynamic_friction,
                        arena.ball.radius_m(),
                        arena.ball.mass_kg,
                        arena.ball.inertia_factor,
                        0.0,
                        arena.solver.restitution_velocity_threshold_mps,
                    );
                }
            }
        }
    }

    fn limit_ball_energy(&mut self, arena: &ArenaConfig) {
        let max_speed = arena.solver.max_ball_speed_mps.max(0.1);
        let max_angular_speed = arena.solver.max_ball_angular_speed_radps.max(1.0);
        for ball in &mut self.balls {
            if !ball.active {
                continue;
            }
            let speed = length_sq(ball.velocity).sqrt();
            if speed > max_speed {
                ball.velocity = mul(ball.velocity, max_speed / speed);
            }
            let angular_speed = length_sq(ball.angular_velocity).sqrt();
            if angular_speed > max_angular_speed {
                ball.angular_velocity =
                    mul(ball.angular_velocity, max_angular_speed / angular_speed);
            }
        }
    }

    fn apply_robot_wall_velocity_constraints(&mut self, arena: &ArenaConfig) {
        if self.robot_physics.is_some() {
            return;
        }
        let field_boundary = self.field_boundary.clone();
        let robot_half = self.robot_collision_half_extents(arena);
        let robot_definition = self.robot_definition.as_ref();
        for player in self.players.values_mut() {
            project_robot_boundary(
                player,
                robot_half,
                robot_definition,
                -arena.robot.height_m * 0.5,
                &field_boundary,
            );
        }
    }

    fn apply_contact_velocities(&mut self, arena: &ArenaConfig, dt: f32) {
        let diameter_sq = arena.ball.diameter_m * arena.ball.diameter_m * 1.002;
        let robot_center_y = self.robot_center_y(arena);
        let robot_fully_dynamic = self.robot_physics.is_some();
        let robot_definition = self.robot_definition.clone();
        self.transfer_debug.clear();
        for &(left, right) in &self.pairs {
            let delta = sub(self.balls[right].position, self.balls[left].position);
            let distance_sq = length_sq(delta);
            if distance_sq <= 1.0e-12 || distance_sq > diameter_sq {
                continue;
            }
            let normal = mul(delta, 1.0 / distance_sq.sqrt());
            let radius = arena.ball.radius_m();
            let mass = arena.ball.mass_kg.max(0.001);
            let inertia =
                (arena.ball.inertia_factor.max(0.05) * mass * radius * radius).max(1.0e-8);
            let left_arm = mul(normal, radius);
            let right_arm = mul(normal, -radius);
            let incoming_left = add(
                self.balls[left].pre_solve_velocity,
                cross(self.balls[left].angular_velocity, left_arm),
            );
            let incoming_right = add(
                self.balls[right].pre_solve_velocity,
                cross(self.balls[right].angular_velocity, right_arm),
            );
            let incoming = sub(incoming_right, incoming_left);
            let relative = dot(incoming, normal);
            let current_left = add(
                self.balls[left].velocity,
                cross(self.balls[left].angular_velocity, left_arm),
            );
            let current_right = add(
                self.balls[right].velocity,
                cross(self.balls[right].angular_velocity, right_arm),
            );
            let current_contact_relative = sub(current_right, current_left);
            let current_relative = dot(current_contact_relative, normal);
            let same_retention = self.scored_target_by_ball[left]
                .zip(self.scored_target_by_ball[right])
                .is_some_and(|(left_target, right_target)| {
                    left_target == right_target
                        && self.scoring_targets[left_target].retention.is_some()
                });
            let target_relative =
                if !same_retention && relative < -arena.solver.restitution_velocity_threshold_mps {
                    -arena.ball.restitution_curve.at_speed(-relative) * relative
                } else {
                    0.0
                };
            let inverse_mass = 1.0 / mass;
            let left_inverse_mass = if boundary_blocks_motion(
                self.balls[left].position,
                mul(normal, -1.0),
                radius,
                &self.field_boundary,
            ) {
                0.0
            } else {
                inverse_mass
            };
            let right_inverse_mass = if boundary_blocks_motion(
                self.balls[right].position,
                normal,
                radius,
                &self.field_boundary,
            ) {
                0.0
            } else {
                inverse_mass
            };
            let normal_inverse_mass = left_inverse_mass + right_inverse_mass;
            if normal_inverse_mass <= 0.0 {
                continue;
            }
            let normal_impulse_magnitude =
                ((target_relative - current_relative) / normal_inverse_mass).max(0.0);
            if normal_impulse_magnitude <= 1.0e-8 {
                continue;
            }
            let normal_impulse = mul(normal, normal_impulse_magnitude);
            self.balls[left].velocity = sub(
                self.balls[left].velocity,
                mul(normal_impulse, left_inverse_mass),
            );
            self.balls[right].velocity = add(
                self.balls[right].velocity,
                mul(normal_impulse, right_inverse_mass),
            );

            let tangent_velocity = sub(
                current_contact_relative,
                mul(normal, dot(current_contact_relative, normal)),
            );
            let tangent_speed_sq = length_sq(tangent_velocity);
            if tangent_speed_sq > 1.0e-10 {
                let tangent = mul(tangent_velocity, 1.0 / tangent_speed_sq.sqrt());
                let tangent_relative = dot(current_contact_relative, tangent);
                let tangent_inverse_mass = 2.0 / mass + 2.0 * radius * radius / inertia;
                let friction = if same_retention {
                    // A scored hopper is a deadened pocket, not a pinball
                    // launcher. High tangential loss lets its pile settle.
                    0.72
                } else {
                    arena.ball.ball_friction
                };
                let friction_limit = friction * normal_impulse_magnitude.abs();
                let friction_impulse_magnitude = (-tangent_relative / tangent_inverse_mass)
                    .clamp(-friction_limit, friction_limit);
                let friction_impulse = mul(tangent, friction_impulse_magnitude);
                self.balls[left].velocity =
                    sub(self.balls[left].velocity, mul(friction_impulse, 1.0 / mass));
                self.balls[right].velocity = add(
                    self.balls[right].velocity,
                    mul(friction_impulse, 1.0 / mass),
                );
                let angular_delta = mul(cross(left_arm, friction_impulse), 1.0 / inertia);
                self.balls[left].angular_velocity =
                    sub(self.balls[left].angular_velocity, angular_delta);
                self.balls[right].angular_velocity =
                    sub(self.balls[right].angular_velocity, angular_delta);
            }
        }

        for player in self.players.values_mut() {
            let ground_offset_y = -arena.robot.height_m * 0.5;
            let intake_zone_info = robot_definition.as_ref().and_then(|definition| {
                let zone = definition
                    .zones
                    .iter()
                    .find(|zone| zone.kind == RobotSemanticKind::Intake)?;
                let mouth = robot_local_collider_pose(
                    &zone.collider,
                    player.position,
                    player.rotation,
                    ground_offset_y,
                );
                let dir_world = rotate_robot_local_pose(zone.direction, player.rotation);
                let envelope = robot_local_collider_pose(
                    &definition.bounds,
                    player.position,
                    player.rotation,
                    ground_offset_y,
                );
                Some((mouth, dir_world, envelope))
            });

            let transfer_zone_info = robot_definition.as_ref().and_then(|definition| {
                let zone = definition
                    .zones
                    .iter()
                    .find(|zone| zone.kind == RobotSemanticKind::Transfer)?;
                let mouth = robot_local_collider_pose(
                    &zone.collider,
                    player.position,
                    player.rotation,
                    ground_offset_y,
                );
                let dir_world = rotate_robot_local_pose(zone.direction, player.rotation);
                Some((mouth, dir_world))
            });

            let outtake_zone_info = robot_definition.as_ref().and_then(|definition| {
                let zone = definition
                    .zones
                    .iter()
                    .find(|zone| zone.kind == RobotSemanticKind::Outtake)?;
                let mouth = robot_local_collider_pose(
                    &zone.collider,
                    player.position,
                    player.rotation,
                    ground_offset_y,
                );
                let dir_world = rotate_robot_local_pose(zone.direction, player.rotation);
                Some((mouth, dir_world))
            });

            if arena.robot.intake_enabled && robot_definition.is_none() {
                for ball in &mut self.balls {
                    if !ball.active {
                        continue;
                    }
                    let Some((normal, _, roller_point, roller_axis)) = roller_contact(
                        ball.position,
                        arena.ball.radius_m() * 1.01,
                        player.position,
                        player.yaw,
                        &arena.robot,
                    ) else {
                        continue;
                    };
                    let robot_arm = sub(roller_point, player.position);
                    let robot_point_velocity =
                        add(player.velocity, cross(player.angular_velocity, robot_arm));
                    let roller_angular_velocity = mul(
                        roller_axis,
                        -arena.robot.intake_surface_speed_mps * player.intake_power
                            / arena.robot.intake_radius_m.max(0.001),
                    );
                    let roller_surface_velocity = add(
                        robot_point_velocity,
                        cross(
                            roller_angular_velocity,
                            mul(normal, arena.robot.intake_radius_m),
                        ),
                    );
                    resolve_sphere_surface_velocity(
                        ball,
                        normal,
                        roller_surface_velocity,
                        &arena.robot.intake_restitution_curve,
                        arena.robot.intake_friction,
                        arena.robot.intake_friction,
                        arena.ball.radius_m(),
                        arena.ball.mass_kg,
                        arena.ball.inertia_factor,
                        arena.robot.intake_normal_force_n * player.intake_power * dt,
                        arena.solver.restitution_velocity_threshold_mps,
                    );
                }
            }
            let robot = effective_robot(&arena.robot, &player.mech);
            for ball in &mut self.balls {
                if !ball.active {
                    continue;
                }
                let (touches_intake_mouth, inside_robot) = if let Some((mouth, _, envelope)) = &intake_zone_info {
                    let in_mouth = sphere_authored_obb_contact(ball.position, arena.ball.radius_m(), mouth).is_some();
                    let in_env = sphere_authored_obb_contact(ball.position, arena.ball.radius_m(), envelope).is_some();
                    (in_mouth, in_env)
                } else {
                    (false, false)
                };

                let touches_transfer = if let Some((mouth, _)) = &transfer_zone_info {
                    sphere_authored_obb_contact(ball.position, arena.ball.radius_m(), mouth).is_some()
                } else {
                    false
                };

                let touches_outtake = if let Some((mouth, _)) = &outtake_zone_info {
                    sphere_authored_obb_contact(ball.position, arena.ball.radius_m(), mouth).is_some()
                } else {
                    false
                };

                // Sync balls inside the robot's mechanical zones to prevent them from falling back during fast movement
                if inside_robot || touches_transfer || touches_outtake {
                    let sync_factor = (15.0 * dt).min(1.0);
                    ball.velocity[0] += (player.velocity[0] - ball.velocity[0]) * sync_factor;
                    ball.velocity[2] += (player.velocity[2] - ball.velocity[2]) * sync_factor;
                }

                // Intake physical force
                if player.intake_power > 0.0 {
                    let intake_dir = if let Some((_, dir_world, _)) = &intake_zone_info {
                        Some(*dir_world)
                    } else if arena.robot.intake_enabled {
                        Some(rotate_robot_local_pose([0.0, 0.0, 1.0], player.rotation))
                    } else {
                        None
                    };

                    if let Some(dir_world) = intake_dir {
                        // Intake power drives only the intake mouth. Applying
                        // this force to the whole robot envelope made Space
                        // behave like it also powered the transfer conveyor.
                        if touches_intake_mouth {
                            let target_speed = robot.intake_surface_speed_mps * player.intake_power;
                            let force_n = (robot.intake_normal_force_n * player.intake_power * 4.0).max(10.0 * player.intake_power);
                            let mass = arena.ball.mass_kg.max(0.001);
                            let rel_vel = sub(ball.velocity, player.velocity);
                            let current_speed = dot(rel_vel, dir_world);

                            if current_speed < target_speed {
                                let speed_needed = target_speed - current_speed;
                                let dv = (force_n / mass * dt).min(speed_needed);
                                ball.velocity = add(ball.velocity, mul(dir_world, dv));
                            }

                            let perp_vel = sub(rel_vel, mul(dir_world, dot(rel_vel, dir_world)));
                            let damp_factor = (1.0 - 6.0 * dt).max(0.0);
                            ball.velocity = sub(ball.velocity, mul(perp_vel, 1.0 - damp_factor));

                            ball.sleeping = false;
                            ball.quiet_ticks = 0;
                        }
                    }
                }

                // The transfer only feeds balls toward the shooter while the
                // outtake is commanded. Intake runs its own roller path and
                // must not also energize the internal transfer.
                if player.outtake_power > 0.0 && (touches_transfer || inside_robot) {
                    if let Some((_, dir_world)) = &transfer_zone_info {
                        let power = player.outtake_power;
                        let target_speed = robot.transfer_surface_speed_mps * power;
                        let force_n = (robot.transfer_normal_force_n * power * 4.0).max(10.0 * power);
                        let mass = arena.ball.mass_kg.max(0.001);
                        let rel_vel = sub(ball.velocity, player.velocity);
                        let current_speed = dot(rel_vel, *dir_world);

                        if current_speed < target_speed {
                            let speed_needed = target_speed - current_speed;
                            let dv = (force_n / mass * dt).min(speed_needed);
                            ball.velocity = add(ball.velocity, mul(*dir_world, dv));
                        }

                        let perp_vel = sub(rel_vel, mul(*dir_world, dot(rel_vel, *dir_world)));
                        let damp_factor = (1.0 - 6.0 * dt).max(0.0);
                        ball.velocity = sub(ball.velocity, mul(perp_vel, 1.0 - damp_factor));

                        ball.sleeping = false;
                        ball.quiet_ticks = 0;
                    }
                }

                // Outtake physical force: affects balls touching OuttakeZone
                if player.outtake_power > 0.0 && touches_outtake {
                    let outtake_dir = if let Some((_, dir_world)) = &outtake_zone_info {
                        *dir_world
                    } else {
                        rotate_robot_local_pose([0.0, 0.0, 1.0], player.rotation)
                    };
                    let target_speed = robot.outtake_velocity_mps * player.outtake_power;
                    let force_n = (robot.outtake_normal_force_n * player.outtake_power * 4.0).max(10.0 * player.outtake_power);
                    let mass = arena.ball.mass_kg.max(0.001);
                    let rel_vel = sub(ball.velocity, player.velocity);
                    let current_speed = dot(rel_vel, outtake_dir);

                    if current_speed < target_speed {
                        let speed_needed = target_speed - current_speed;
                        let dv = (force_n / mass * dt).min(speed_needed);
                        ball.velocity = add(ball.velocity, mul(outtake_dir, dv));
                    }

                    let perp_vel = sub(rel_vel, mul(outtake_dir, dot(rel_vel, outtake_dir)));
                    let damp_factor = (1.0 - 6.0 * dt).max(0.0);
                    ball.velocity = sub(ball.velocity, mul(perp_vel, 1.0 - damp_factor));

                    ball.last_outtake_alliance = Some(player.team_name.clone());
                    ball.sleeping = false;
                    ball.quiet_ticks = 0;
                }

                if (player.intake_power > 0.0 && touches_intake_mouth)
                    || (player.outtake_power > 0.0 && touches_transfer)
                    || (player.outtake_power > 0.0 && touches_outtake)
                {
                    continue;
                }
                let authored_contact = robot_definition.as_ref().and_then(|definition| {
                    let ground_offset_y = -arena.robot.height_m * 0.5;
                    let envelope = robot_local_collider_pose(
                        &definition.bounds,
                        player.position,
                        player.rotation,
                        ground_offset_y,
                    );
                    sphere_authored_obb_contact(
                        ball.position,
                        arena.ball.radius_m() * 1.01,
                        &envelope,
                    )?;
                    definition
                        .colliders
                        .iter()
                        .filter_map(|local| {
                            let collider = robot_local_collider_pose(
                                local,
                                player.position,
                                player.rotation,
                                ground_offset_y,
                            );
                            sphere_authored_obb_contact(
                                ball.position,
                                arena.ball.radius_m() * 1.01,
                                &collider,
                            )
                        })
                        .max_by(|left, right| left.1.total_cmp(&right.1))
                });
                if robot_definition.is_some() && authored_contact.is_none() {
                    continue;
                }
                // Allow balls entering the intake opening when intake is enabled to pass into the hopper
                if robot_definition.is_none()
                    && arena.robot.intake_enabled
                    && player.intake_power > 0.0
                {
                    let forward = [-player.yaw.sin(), 0.0, -player.yaw.cos()];
                    let right = [-forward[2], 0.0, forward[0]];
                    let delta = sub(ball.position, player.position);
                    let forward_dist = dot(delta, forward);
                    let lateral_dist = dot(delta, right).abs();
                    let intake_world_y =
                        (player.position[1] - robot.height_m * 0.5) + robot.intake_center_height_m;
                    let vertical_dist = (ball.position[1] - intake_world_y).abs();

                    if forward_dist >= 0.0
                        && forward_dist
                            <= robot.intake_forward_offset_m + arena.ball.radius_m() * 1.5
                        && lateral_dist <= robot.intake_width_m * 0.5 + arena.ball.radius_m() * 0.5
                        && vertical_dist <= arena.ball.radius_m() + robot.intake_radius_m + 0.10
                    {
                        continue; // skip rigid front chassis bounce for intaking balls
                    }
                }

                let Some((normal, _)) = authored_contact.or_else(|| {
                    sphere_obb_contact(
                        ball.position,
                        arena.ball.radius_m() * 1.01,
                        player.position,
                        player.yaw,
                        [
                            arena.robot.width_m * 0.5,
                            arena.robot.height_m * 0.5,
                            arena.robot.length_m * 0.5,
                        ],
                    )
                }) else {
                    continue;
                };
                let incoming_relative = dot(sub(ball.pre_solve_velocity, player.velocity), normal);
                let inv_ball = 1.0 / arena.ball.mass_kg.max(0.001);
                let inv_robot = 1.0 / arena.robot.mass_kg.max(1.0);
                let planar_normal_sq = normal[0] * normal[0] + normal[2] * normal[2];
                let effective_inv_ball = if boundary_blocks_motion(
                    ball.position,
                    normal,
                    arena.ball.radius_m(),
                    &self.field_boundary,
                ) {
                    0.0
                } else {
                    inv_ball
                };
                let current_relative = dot(sub(ball.velocity, player.velocity), normal);
                let target_relative = if incoming_relative
                    < -arena.solver.restitution_velocity_threshold_mps
                {
                    -arena.robot.restitution_curve.at_speed(-incoming_relative) * incoming_relative
                } else {
                    0.0
                };
                let impulse = ((target_relative - current_relative)
                    / (effective_inv_ball
                        + inv_robot
                            * if robot_fully_dynamic {
                                1.0
                            } else {
                                planar_normal_sq
                            }))
                .max(0.0);
                if impulse <= 1.0e-8 {
                    continue;
                }
                ball.velocity = add(ball.velocity, mul(normal, impulse * effective_inv_ball));
                player.velocity[0] -= normal[0] * impulse * inv_robot;
                player.velocity[2] -= normal[2] * impulse * inv_robot;
                if robot_fully_dynamic {
                    player.velocity[1] -= normal[1] * impulse * inv_robot;
                } else {
                    player.velocity[1] = 0.0;
                }

                let ball_arm = mul(normal, -arena.ball.radius_m());
                let robot_arm = sub(ball.position, player.position);
                let robot_point_velocity =
                    add(player.velocity, cross(player.angular_velocity, robot_arm));
                let ball_point_velocity =
                    add(ball.velocity, cross(ball.angular_velocity, ball_arm));
                let relative_contact = sub(ball_point_velocity, robot_point_velocity);
                let tangent_velocity =
                    sub(relative_contact, mul(normal, dot(relative_contact, normal)));
                let tangent_speed = length_sq(tangent_velocity).sqrt();
                if tangent_speed > 1.0e-6 {
                    let tangent = mul(tangent_velocity, 1.0 / tangent_speed);
                    let ball_inertia = (arena.ball.inertia_factor.max(0.05)
                        * arena.ball.mass_kg.max(0.001)
                        * arena.ball.radius_m().powi(2))
                    .max(1.0e-8);
                    let robot_inertia = (arena.robot.mass_kg
                        * (arena.robot.width_m.powi(2) + arena.robot.length_m.powi(2))
                        / 12.0)
                        .max(0.01);
                    let robot_torque_axis = cross(robot_arm, tangent)[1];
                    let tangent_inverse_mass = inv_ball
                        + arena.ball.radius_m().powi(2) / ball_inertia
                        + inv_robot * (tangent[0] * tangent[0] + tangent[2] * tangent[2])
                        + robot_torque_axis * robot_torque_axis / robot_inertia;
                    let friction_limit = arena.robot.surface_friction * impulse.abs();
                    let tangent_impulse_magnitude = (-tangent_speed / tangent_inverse_mass)
                        .clamp(-friction_limit, friction_limit);
                    let tangent_impulse = mul(tangent, tangent_impulse_magnitude);
                    ball.velocity = add(ball.velocity, mul(tangent_impulse, inv_ball));
                    ball.angular_velocity = add(
                        ball.angular_velocity,
                        mul(cross(ball_arm, tangent_impulse), 1.0 / ball_inertia),
                    );
                    player.velocity[0] -= tangent_impulse[0] * inv_robot;
                    player.velocity[2] -= tangent_impulse[2] * inv_robot;
                    if robot_fully_dynamic {
                        player.velocity[1] -= tangent_impulse[1] * inv_robot;
                        let torque = cross(robot_arm, tangent_impulse);
                        let inertias = [
                            (arena.robot.mass_kg
                                * (arena.robot.height_m.powi(2) + arena.robot.length_m.powi(2))
                                / 12.0)
                                .max(0.01),
                            robot_inertia,
                            (arena.robot.mass_kg
                                * (arena.robot.width_m.powi(2) + arena.robot.height_m.powi(2))
                                / 12.0)
                                .max(0.01),
                        ];
                        for axis in 0..3 {
                            player.angular_velocity[axis] -= torque[axis] / inertias[axis];
                        }
                        player.angular_velocity_y = player.angular_velocity[1];
                    } else {
                        player.angular_velocity_y -=
                            cross(robot_arm, tangent_impulse)[1] / robot_inertia;
                        player.angular_velocity[1] = player.angular_velocity_y;
                    }
                }
            }
            let bounds = robot_definition.as_ref().map(|definition| {
                robot_local_collider_pose(
                    &definition.bounds,
                    player.position,
                    player.rotation,
                    ground_offset_y,
                )
            });
            let has_ball = self.balls.iter().any(|ball| ball.active);
            let inside_robot = bounds.as_ref().is_some_and(|bounds| {
                self.balls.iter().any(|ball| {
                    ball.active
                        && sphere_authored_obb_contact(ball.position, arena.ball.radius_m(), bounds)
                            .is_some()
                })
            });
            let mut outtake_contact_balls = 0u16;
            let mut max_outtake_contact_speed_mps = 0.0f32;
            if let Some((outtake, direction)) = &outtake_zone_info {
                for ball in &self.balls {
                    if ball.active
                        && sphere_authored_obb_contact(
                            ball.position,
                            arena.ball.radius_m(),
                            outtake,
                        )
                        .is_some()
                    {
                        outtake_contact_balls = outtake_contact_balls.saturating_add(1);
                        max_outtake_contact_speed_mps = max_outtake_contact_speed_mps.max(dot(
                            sub(ball.velocity, player.velocity),
                            *direction,
                        ));
                    }
                }
            }
            let touches_outtake = outtake_contact_balls > 0;
            self.transfer_debug.push(TransferDebug {
                player_name: player.name.clone(),
                has_ball,
                // Transfer force is deliberately powered by outtake only.
                transfer_power_ok: player.outtake_power > 0.0,
                inside_robot,
                touches_outtake,
                has_transfer_zone: transfer_zone_info.is_some(),
                has_robot_definition: robot_definition.is_some(),
                intake_power: player.intake_power,
                outtake_power: player.outtake_power,
                outtake_force_n: robot.outtake_normal_force_n,
                outtake_target_speed_mps: robot.outtake_velocity_mps * player.outtake_power,
                outtake_contact_balls,
                max_outtake_contact_speed_mps,
            });

            if !robot_fully_dynamic && player.climbing_brace.is_none() {
                player.position[1] = robot_center_y;
                player.velocity[1] = 0.0;
            }
        }
    }

    fn update_sleeping(&mut self, arena: &ArenaConfig, dt: f32) {
        let sleep_ticks = (arena.solver.sleep_after_seconds / dt).max(1.0) as u16;
        let linear_sq = arena.solver.sleep_linear_threshold_mps.powi(2);
        let angular_sq = arena.solver.sleep_angular_threshold_radps.powi(2);
        for ball in &mut self.balls {
            if !ball.active {
                continue;
            }
            if ball.grounded
                && length_sq(ball.velocity) <= linear_sq
                && length_sq(ball.angular_velocity) <= angular_sq
            {
                ball.quiet_ticks = ball.quiet_ticks.saturating_add(1);
                if ball.quiet_ticks >= sleep_ticks {
                    ball.sleeping = true;
                    ball.velocity = [0.0; 3];
                    ball.pre_solve_velocity = [0.0; 3];
                    ball.angular_velocity = [0.0; 3];
                }
            } else {
                ball.quiet_ticks = 0;
            }
        }
    }

    pub fn player_snapshots(&self) -> Vec<PlayerSnapshot> {
        self.player_snapshots_with_brace_states(&[])
    }

    pub fn player_snapshots_with_brace_states(
        &self,
        triggers: &[FieldTrigger],
    ) -> Vec<PlayerSnapshot> {
        let base_capacity = self
            .arena
            .as_ref()
            .map(|arena| arena.robot.storage_capacity)
            .unwrap_or(0);
        let ground_offset_y = self
            .arena
            .as_ref()
            .map(|arena| -arena.robot.height_m * 0.5)
            .unwrap_or_default();
        self.players
            .iter()
            .map(|(id, player)| {
                let brace = brace::brace_state_for_player(
                    player,
                    self.robot_definition.as_ref(),
                    ground_offset_y,
                    &self.field_colliders,
                    triggers,
                );
                PlayerSnapshot {
                    id: id.clone(),
                    name: player.name.clone(),
                    team_name: player.team_name.clone(),
                    x: player.position[0],
                    y: player.position[1],
                    z: player.position[2],
                    yaw: player.yaw,
                    heading_deg: player.yaw.to_degrees(),
                    velocity_x: player.velocity[0],
                    velocity_y: player.velocity[1],
                    velocity_z: player.velocity[2],
                    angular_velocity_y: player.angular_velocity_y,
                    rotation_x: player.rotation[0],
                    rotation_y: player.rotation[1],
                    rotation_z: player.rotation[2],
                    rotation_w: player.rotation[3],
                    angular_velocity_x: player.angular_velocity[0],
                    angular_velocity_z: player.angular_velocity[2],
                    color: player.color.into(),
                    stored_balls: player.stored.len(),
                    capacity: player.mech.capacity_with(base_capacity),
                    brace_zone: brace.zone,
                    brace_multiplier: brace.multiplier,
                    floor_supported: player.floor_supported,
                    brace_contact: player.climbing_brace.is_some(),
                    brace_support_impulse: player.brace_support_impulse,
                    climb_wheel_angle: player.climb_wheel_angle,
                    climb_wheel_radps: player.climb_wheel_radps,
                }
            })
            .collect()
    }

    pub fn brace_multipliers(&self, triggers: &[FieldTrigger]) -> (f32, f32) {
        let ground_offset_y = self
            .arena
            .as_ref()
            .map(|arena| -arena.robot.height_m * 0.5)
            .unwrap_or_default();
        let mut blue_multiplier = 1.0;
        let mut red_multiplier = 1.0;
        for player in self.players.values() {
            let brace = brace::brace_state_for_player(
                player,
                self.robot_definition.as_ref(),
                ground_offset_y,
                &self.field_colliders,
                triggers,
            );
            if player.team_name.eq_ignore_ascii_case("blue") {
                blue_multiplier += brace.multiplier - 1.0;
            } else if player.team_name.eq_ignore_ascii_case("red") {
                red_multiplier += brace.multiplier - 1.0;
            }
        }
        (blue_multiplier, red_multiplier)
    }

    pub fn apply_endgame_brace_multipliers(&mut self, blue_multiplier: f32, red_multiplier: f32) {
        self.apply_su_multiplier("blue", blue_multiplier);
        self.apply_su_multiplier("red", red_multiplier);
    }

    fn apply_su_multiplier(&mut self, alliance: &str, multiplier: f32) {
        let su_score = match alliance {
            "blue" => self.score_state.blue_su_score,
            "red" => self.score_state.red_su_score,
            _ => return,
        };
        let adjusted = (su_score as f32 * multiplier).round() as i32;
        let delta = adjusted - su_score;
        match alliance {
            "blue" => {
                self.score_state.blue_su_score = adjusted;
                self.score_state.blue_score += delta;
            }
            "red" => {
                self.score_state.red_su_score = adjusted;
                self.score_state.red_score += delta;
            }
            _ => unreachable!(),
        }
        if delta != 0 {
            *self
                .score_state
                .breakdown
                .entry(format!("{alliance}_brace_multiplier"))
                .or_insert(0) += delta;
        }
    }

    pub fn field_object_positions(&self) -> ObjectPositionsSync {
        let count = self.balls.len() as u32;
        let mask_bytes = (count as usize + 7) / 8;
        let mut active_mask = vec![0u8; mask_bytes];
        let mut moving_mask = vec![0u8; mask_bytes];
        let mut quantized_positions = Vec::with_capacity(count as usize * 3);

        for (i, ball) in self.balls.iter().enumerate() {
            if ball.active {
                active_mask[i / 8] |= 1 << (i % 8);
                if !ball.sleeping {
                    moving_mask[i / 8] |= 1 << (i % 8);
                    let quantize =
                        |v: f32| -> u16 { ((v + 8.0) * 4095.9375).clamp(0.0, 65535.0) as u16 };
                    quantized_positions.push(quantize(ball.position[0]));
                    quantized_positions.push(quantize(ball.position[1]));
                    quantized_positions.push(quantize(ball.position[2]));
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
        self.metrics.contacts
    }

    pub fn step_metrics(&self) -> StepMetrics {
        self.metrics
    }

    pub fn drain_semantic_events(&mut self) -> Vec<SemanticEvent> {
        std::mem::take(&mut self.semantic_events)
    }
}

#[cfg(test)]
#[path = "sphere_runtime/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "sphere_runtime/performance_tests.rs"]
mod performance_tests;
