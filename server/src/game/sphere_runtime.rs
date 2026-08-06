use std::collections::{BTreeMap, VecDeque};
use std::time::Instant;

use super::match_runtime::{MatchContext, MatchPhase, PlayerSnapshot, ScoreState};
use super::pack_loader::{
    ArenaConfig, FieldBoundary, FieldCollider, FieldDefinition, FieldTrigger, RampPhysicsConfig,
    RestitutionCurveConfig, RobotPhysicsConfig,
};

type Vec3 = [f32; 3];

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

#[derive(Clone, Copy)]
struct Ball {
    position: Vec3,
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
}

struct PlayerBody {
    name: String,
    team_name: String,
    position: Vec3,
    velocity: Vec3,
    yaw: f32,
    angular_velocity_y: f32,
    move_x: f32,
    move_z: f32,
    intake_power: f32,
    outtake_power: f32,
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
}

/// Adjustable robot mechanic spec. Unset fields fall back to the arena pack.
#[derive(Debug, Clone, Default)]
pub struct MechSpec {
    pub capacity: Option<usize>,
    pub intake_rate_bps: Option<f32>,
    pub intake_surface_speed_mps: Option<f32>,
    pub outtake_rate_bps: Option<f32>,
    pub outtake_velocity_mps: Option<f32>,
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
    if let Some(rate) = mech.outtake_rate_bps {
        robot.outtake_rate_bps = rate;
    }
    if let Some(velocity) = mech.outtake_velocity_mps {
        robot.outtake_velocity_mps = velocity;
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
    field_triggers: Vec<FieldTrigger>,
    trigger_inside: Vec<bool>,
    semantic_events: Vec<SemanticEvent>,
}

impl SphereRuntime {
    const GRID_BUCKETS: usize = 1 << 14;
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
            field_triggers: Vec::new(),
            trigger_inside: Vec::new(),
            semantic_events: Vec::new(),
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

    pub fn create_field_arena(&mut self, arena: &ArenaConfig, field: &FieldDefinition) {
        self.arena = Some(arena.clone());
        self.field_boundary = field.boundary.clone();
        self.field_floor_y = field.floor_height_m;
        self.field_colliders = field.colliders.clone();
        self.field_anchors = field.anchors.clone();
        self.field_triggers = field.triggers.clone();
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
            });
        }
        self.trigger_inside =
            vec![false; self.balls.len().saturating_mul(self.field_triggers.len())];
        self.semantic_events.clear();
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
        self.players.insert(
            user_id,
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
                yaw: angle,
                angular_velocity_y: 0.0,
                move_x: 0.0,
                move_z: 0.0,
                intake_power: 0.0,
                outtake_power: 0.0,
                sequence: 0,
                color,
                wall_contact_normal: None,
                stored: VecDeque::new(),
                outtake_accumulator: 0.0,
                intake_accumulator: 0.0,
                mech: MechSpec::default(),
            },
        );
    }

    pub fn remove_player(&mut self, user_id: &str) {
        self.players.remove(user_id);
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
        if let Some(player) = self.players.get_mut(user_id)
            && sequence >= player.sequence
        {
            player.sequence = sequence;
            player.move_x = move_x.clamp(-1.0, 1.0);
            player.move_z = move_z.clamp(-1.0, 1.0);
            player.intake_power = intake_power.clamp(0.0, 1.0);
            player.outtake_power = outtake_power.clamp(0.0, 1.0);
        }
    }

    pub fn set_player_mech(&mut self, user_id: &str, mech: MechSpec) {
        if let Some(player) = self.players.get_mut(user_id) {
            player.mech = mech;
        }
    }

    pub fn apply_player_drive(&mut self, arena: &ArenaConfig, dt: f32) {
        let robot = &arena.robot;
        for player in self.players.values_mut() {
            let forward = [-player.yaw.sin(), 0.0, -player.yaw.cos()];
            let right = [-forward[2], 0.0, forward[0]];
            let forward_speed = dot(player.velocity, forward);
            let lateral_speed = dot(player.velocity, right);
            let mut left = player.move_z + player.move_x;
            let mut right_power = player.move_z - player.move_x;
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
            let turn_delta = (target_turn - player.angular_velocity_y).clamp(
                -robot.max_angular_acceleration_radps2 * dt,
                robot.max_angular_acceleration_radps2 * dt,
            );
            player.angular_velocity_y += turn_delta;
        }
    }

    /// Ball hopper mechanics: powered intake captures balls in the roller
    /// mouth into storage, and the wide flywheel launches stored balls at the
    /// adjustable velocity/angle with a deterministic lateral spread. Both
    /// steps are rate-limited so a full robot swallows and spits at a steady
    /// pace instead of vacuuming the field in one tick.
    fn step_mechanics(&mut self, arena: &ArenaConfig, dt: f32) {
        let radius = arena.ball.radius_m();
        for (player_id, player) in self.players.iter_mut() {
            let robot = effective_robot(&arena.robot, &player.mech);

            // Intake capture.
            if player.intake_power > 0.0
                && robot.storage_capacity > 0
                && robot.intake_rate_bps > 0.0
            {
                let forward = [-player.yaw.sin(), 0.0, -player.yaw.cos()];
                let right = [-forward[2], 0.0, forward[0]];
                player.intake_accumulator = (player.intake_accumulator
                    + robot.intake_rate_bps * player.intake_power * dt)
                    .min(120.0);
                let mut candidates: Vec<(f32, usize)> = Vec::with_capacity(16);
                let intake_world_y = (player.position[1] - robot.height_m * 0.5) + robot.intake_center_height_m;
                for (index, ball) in self.balls.iter().enumerate() {
                    if !ball.active {
                        continue;
                    }
                    let delta = sub(ball.position, player.position);
                    let forward_dist = dot(delta, forward);
                    if forward_dist < -0.10
                        || forward_dist > robot.intake_forward_offset_m + radius + 0.10
                    {
                        continue;
                    }
                    let lateral_dist = dot(delta, right);
                    if lateral_dist.abs() > robot.intake_width_m * 0.5 + 0.08 {
                        continue;
                    }
                    let vertical_dist = (ball.position[1] - intake_world_y).abs();
                    if vertical_dist > radius + robot.intake_radius_m + 0.10 {
                        continue;
                    }
                    candidates.push((forward_dist.abs(), index));
                }
                candidates.sort_by(|left, right| left.0.partial_cmp(&right.0).unwrap_or(std::cmp::Ordering::Equal));
                for (_, index) in candidates {
                    if player.intake_accumulator < 1.0 {
                        break;
                    }
                    if player.stored.len() >= robot.storage_capacity {
                        break;
                    }
                    if !self.balls[index].active {
                        continue;
                    }
                    self.balls[index].active = false;
                    player.stored.push_back(index);
                    player.intake_accumulator -= 1.0;
                    self.semantic_events.push(SemanticEvent {
                        kind: "intake",
                        target_id: player_id.clone(),
                        entity_id: format!("ball:{index}"),
                    });
                }
            }

            // Wide flywheel outtake.
            if player.outtake_power > 0.0
                && !player.stored.is_empty()
                && robot.outtake_rate_bps > 0.0
                && robot.outtake_velocity_mps > 0.0
            {
                let forward = [-player.yaw.sin(), 0.0, -player.yaw.cos()];
                let right = [-forward[2], 0.0, forward[0]];
                player.outtake_accumulator +=
                    robot.outtake_rate_bps * player.outtake_power * dt;
                let pitch = robot.outtake_angle_deg.to_radians();
                let horizontal = robot.outtake_velocity_mps * pitch.cos();
                let vertical = robot.outtake_velocity_mps * pitch.sin();
                let outtake_world_y = (player.position[1] - robot.height_m * 0.5) + robot.outtake_height_m;
                while player.outtake_accumulator >= 1.0 && !player.stored.is_empty() {
                    player.outtake_accumulator -= 1.0;
                    let index = player.stored.pop_front().unwrap();
                    // Deterministic hash spread so the wide mouth actually
                    // spits across its width without breaking replayability.
                    let jitter = (((index as u32).wrapping_mul(2654435761u32)) as f32
                        / 4294967296.0)
                        - 0.5;
                    let exit = add(
                        add(player.position, mul(forward, robot.outtake_forward_offset_m)),
                        mul(right, jitter * robot.flywheel_width_m),
                    );
                    let ball = &mut self.balls[index];
                    ball.position = [exit[0], outtake_world_y, exit[2]];
                    ball.velocity = [
                        forward[0] * horizontal + player.velocity[0],
                        vertical + player.velocity[1],
                        forward[2] * horizontal + player.velocity[2],
                    ];
                    ball.pre_solve_velocity = ball.velocity;
                    // Realistic flywheel backspin (spin axis along right vector)
                    let spin_rate = horizontal / arena.ball.radius_m().max(0.01);
                    ball.angular_velocity = mul(right, -spin_rate);
                    ball.quiet_ticks = 0;
                    ball.sleeping = false;
                    ball.grounded = false;
                    ball.active = true;
                    self.semantic_events.push(SemanticEvent {
                        kind: "outtake",
                        target_id: player_id.clone(),
                        entity_id: format!("ball:{index}"),
                    });
                }
            }
        }
    }

    pub fn tick(&mut self, dt: f64) {
        let dt = dt as f32;
        self.context.clock += dt as f64;        let Some(arena) = self.arena.clone() else {
            return;
        };
        if let Some(elapsed) = &mut self.ball_release_elapsed {
            *elapsed += dt;
        }
        self.release_queued_balls(&arena);
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
        self.update_sleeping(&arena, dt);
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
        self.detect_trigger_entries();
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
        let robot_center_y = self.robot_center_y(arena);
        let field_boundary = self.field_boundary.clone();
        for player in self.players.values_mut() {
            // Contacts are refreshed by the position solver below. Keeping a
            // normal for one drive step gives stable wall sliding without
            // constraining a robot that has already driven away.
            player.wall_contact_normal = None;
            // The robot is a carpet-supported planar body. Ball contacts may
            // transfer X/Z momentum and yaw, but must never integrate lift.
            player.velocity[1] = 0.0;
            player.position[0] += player.velocity[0] * dt;
            player.position[2] += player.velocity[2] * dt;
            player.position[1] = robot_center_y;
            player.yaw = wrap_angle(player.yaw + player.angular_velocity_y * dt);
            let (robot_x_extent, robot_z_extent) = robot_planar_extents(&arena.robot, player.yaw);
            let min_x = field_boundary.min[0] + robot_x_extent;
            let max_x = field_boundary.max[0] - robot_x_extent;
            let min_z = field_boundary.min[2] + robot_z_extent;
            let max_z = field_boundary.max[2] - robot_z_extent;
            player.position[0] = player.position[0].clamp(min_x, max_x);
            player.position[2] = player.position[2].clamp(min_z, max_z);
            if player.position[0] <= min_x + 1.0e-6 || player.position[0] >= max_x - 1.0e-6 {
                let normal = if player.position[0] <= min_x + 1.0e-6 {
                    [1.0, 0.0, 0.0]
                } else {
                    [-1.0, 0.0, 0.0]
                };
                player.wall_contact_normal = Some(normal);
                let into_surface = dot(player.velocity, normal);
                if into_surface < 0.0 {
                    player.velocity = sub(player.velocity, mul(normal, into_surface));
                }
            }
            if player.position[2] <= min_z + 1.0e-6 || player.position[2] >= max_z - 1.0e-6 {
                let normal = if player.position[2] <= min_z + 1.0e-6 {
                    [0.0, 0.0, 1.0]
                } else {
                    [0.0, 0.0, -1.0]
                };
                player.wall_contact_normal = Some(normal);
                let into_surface = dot(player.velocity, normal);
                if into_surface < 0.0 {
                    player.velocity = sub(player.velocity, mul(normal, into_surface));
                }
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
        let robot_center_y = self.robot_center_y(arena);
        let field_boundary = self.field_boundary.clone();
        for player in self.players.values_mut() {
            if arena.robot.intake_enabled {
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
                if let Some((normal, penetration)) = sphere_obb_contact(
                    ball.position,
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
                    );
                    player.position[1] = robot_center_y;
                    ball.sleeping = false;
                    ball.quiet_ticks = 0;
                }
            }
            // The chassis is rotated, so its projected footprint—not the
            // unrotated 50 cm box—sets the perimeter clearance.
            let (robot_x_extent, robot_z_extent) = robot_planar_extents(&arena.robot, player.yaw);
            player.position[0] = player.position[0].clamp(
                field_boundary.min[0] + robot_x_extent,
                field_boundary.max[0] - robot_x_extent,
            );
            player.position[2] = player.position[2].clamp(
                field_boundary.min[2] + robot_z_extent,
                field_boundary.max[2] - robot_z_extent,
            );
            let (field_contacts, wall_normal) =
                project_robot_field_colliders(player, &arena.robot, field_colliders);
            contacts += field_contacts;
            if wall_normal.is_some() {
                player.wall_contact_normal = wall_normal;
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
                if let Some(normal) = sphere_collider_contact(ball.position, arena.ball.radius_m() * 1.01, collider) {
                    let id_lower = collider.id.to_lowercase();
                    let surface = if id_lower.contains("su") || id_lower.contains("goal") || id_lower.contains("polycarbonate") {
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
        let field_boundary = self.field_boundary.clone();
        for player in self.players.values_mut() {
            let (robot_x_extent, robot_z_extent) = robot_planar_extents(&arena.robot, player.yaw);
            let min_x = field_boundary.min[0] + robot_x_extent;
            let max_x = field_boundary.max[0] - robot_x_extent;
            let min_z = field_boundary.min[2] + robot_z_extent;
            let max_z = field_boundary.max[2] - robot_z_extent;
            if (player.position[0] <= min_x + 1.0e-5 && player.velocity[0] < 0.0)
                || (player.position[0] >= max_x - 1.0e-5 && player.velocity[0] > 0.0)
            {
                player.velocity[0] = 0.0;
            }
            if (player.position[2] <= min_z + 1.0e-5 && player.velocity[2] < 0.0)
                || (player.position[2] >= max_z - 1.0e-5 && player.velocity[2] > 0.0)
            {
                player.velocity[2] = 0.0;
            }
        }
    }

    fn apply_contact_velocities(&mut self, arena: &ArenaConfig, dt: f32) {
        let diameter_sq = arena.ball.diameter_m * arena.ball.diameter_m * 1.002;
        let robot_center_y = self.robot_center_y(arena);
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
            let target_relative = if relative < -arena.solver.restitution_velocity_threshold_mps {
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
                let friction_limit = arena.ball.ball_friction * normal_impulse_magnitude.abs();
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
            if arena.robot.intake_enabled {
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
                    let robot_point_velocity = add(
                        player.velocity,
                        cross([0.0, player.angular_velocity_y, 0.0], robot_arm),
                    );
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
                // Allow balls entering the intake opening when intake is enabled to pass into the hopper
                if arena.robot.intake_enabled && (player.intake_power > 0.0 || player.stored.len() < robot.storage_capacity) {
                    let forward = [-player.yaw.sin(), 0.0, -player.yaw.cos()];
                    let right = [-forward[2], 0.0, forward[0]];
                    let delta = sub(ball.position, player.position);
                    let forward_dist = dot(delta, forward);
                    let lateral_dist = dot(delta, right).abs();
                    let intake_world_y = (player.position[1] - robot.height_m * 0.5) + robot.intake_center_height_m;
                    let vertical_dist = (ball.position[1] - intake_world_y).abs();

                    if forward_dist >= 0.0
                        && forward_dist <= robot.intake_forward_offset_m + arena.ball.radius_m() * 1.5
                        && lateral_dist <= robot.intake_width_m * 0.5 + arena.ball.radius_m() * 0.5
                        && vertical_dist <= arena.ball.radius_m() + robot.intake_radius_m + 0.10
                    {
                        continue; // skip rigid front chassis bounce for intaking balls
                    }
                }

                let Some((normal, _)) = sphere_obb_contact(
                    ball.position,
                    arena.ball.radius_m() * 1.01,
                    player.position,
                    player.yaw,
                    [
                        arena.robot.width_m * 0.5,
                        arena.robot.height_m * 0.5,
                        arena.robot.length_m * 0.5,
                    ],
                ) else {
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
                    / (effective_inv_ball + inv_robot * planar_normal_sq))
                    .max(0.0);
                if impulse <= 1.0e-8 {
                    continue;
                }
                ball.velocity = add(ball.velocity, mul(normal, impulse * effective_inv_ball));
                player.velocity[0] -= normal[0] * impulse * inv_robot;
                player.velocity[2] -= normal[2] * impulse * inv_robot;
                player.velocity[1] = 0.0;

                let ball_arm = mul(normal, -arena.ball.radius_m());
                let robot_arm = sub(ball.position, player.position);
                let robot_point_velocity = add(
                    player.velocity,
                    cross([0.0, player.angular_velocity_y, 0.0], robot_arm),
                );
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
                    player.angular_velocity_y -=
                        cross(robot_arm, tangent_impulse)[1] / robot_inertia;
                }
            }
            player.position[1] = robot_center_y;
            player.velocity[1] = 0.0;
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
        let base_capacity = self
            .arena
            .as_ref()
            .map(|arena| arena.robot.storage_capacity)
            .unwrap_or(0);
        self.players
            .iter()
            .map(|(id, player)| PlayerSnapshot {
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
                color: player.color.into(),
                stored_balls: player.stored.len(),
                capacity: player.mech.capacity_with(base_capacity),
            })
            .collect()
    }

    pub fn field_object_positions(&self) -> Vec<[f32; 3]> {
        // Unreleased balls remain inside the dispenser and must not be drawn
        // as a visible stack before the match-start signal.
        self.balls
            .iter()
            .filter(|ball| ball.active)
            .map(|ball| ball.position)
            .collect()
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

    /// Contain a game piece identified by a `ball:{index}` entity string —
    /// deactivating it so it is removed from play and can never be re-scored.
    /// Used when WILDFIRE enters a SUPPRESSION UNIT or the EXTINGUISHER.
    pub fn contain_ball(&mut self, entity_id: &str) -> bool {
        let Some(index) = entity_id.strip_prefix("ball:").and_then(|v| v.parse::<usize>().ok()) else {
            return false;
        };
        if let Some(ball) = self.balls.get_mut(index)
            && ball.active
        {
            ball.active = false;
            return true;
        }
        false
    }

    fn detect_trigger_entries(&mut self) {
        if self.field_triggers.is_empty() || self.balls.is_empty() {
            return;
        }
        for (ball_index, ball) in self.balls.iter().enumerate() {
            if !ball.active {
                continue;
            }
            for (trigger_index, trigger) in self.field_triggers.iter().enumerate() {
                let inside = point_inside_aabb(ball.position, trigger.min, trigger.max);
                let state_index = ball_index * self.field_triggers.len() + trigger_index;
                let was_inside = self.trigger_inside[state_index];
                self.trigger_inside[state_index] = inside;
                if inside && !was_inside {
                    self.semantic_events.push(SemanticEvent {
                        kind: "trigger_enter",
                        target_id: trigger.id.clone(),
                        entity_id: format!("ball:{ball_index}"),
                    });
                }
            }
        }
    }
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn mul(value: Vec3, scalar: f32) -> Vec3 {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length_sq(value: Vec3) -> f32 {
    dot(value, value)
}

fn robot_planar_extents(robot: &RobotPhysicsConfig, yaw: f32) -> (f32, f32) {
    let half_x = robot.width_m * 0.5;
    let half_z = robot.length_m * 0.5;
    let cos = yaw.cos().abs();
    let sin = yaw.sin().abs();
    (half_x * cos + half_z * sin, half_x * sin + half_z * cos)
}

fn point_inside_aabb(point: Vec3, min: Vec3, max: Vec3) -> bool {
    point[0] >= min[0]
        && point[0] <= max[0]
        && point[1] >= min[1]
        && point[1] <= max[1]
        && point[2] >= min[2]
        && point[2] <= max[2]
}

fn project_static_position(
    ball: &mut Ball,
    arena: &ArenaConfig,
    radius: f32,
    floor_y: f32,
    field_boundary: &FieldBoundary,
    field_colliders: &[FieldCollider],
) -> usize {
    let mut contacts = 0;
    let floor_contact_y = floor_y + radius;
    if ball.position[1] < floor_contact_y {
        ball.position[1] = floor_contact_y;
    }
    if ball.position[1] <= floor_contact_y + 1.0e-5 {
        ball.grounded = true;
        contacts += 1;
    }
    for (axis, min, max) in [
        (
            0,
            field_boundary.min[0] + radius,
            field_boundary.max[0] - radius,
        ),
        (
            2,
            field_boundary.min[2] + radius,
            field_boundary.max[2] - radius,
        ),
    ] {
        let clamped = ball.position[axis].clamp(min, max);
        if clamped != ball.position[axis] {
            ball.position[axis] = clamped;
            contacts += 1;
        }
    }
    for collider in field_colliders {
        contacts += project_sphere_aabb(ball, collider, radius);
    }
    if let Some((normal, penetration)) = ramp_contact(ball.position, radius, &arena.ramp) {
        ball.position = add(ball.position, mul(normal, penetration));
        ball.grounded = false;
        ball.on_ramp = true;
        contacts += 1;
    }
    contacts
}

/// Resolve a ball against an authored field collision volume. The pack loader
/// converts every Assimp mesh into a tight oriented box once at startup, so the
/// 60 Hz solver does not parse JSON or traverse CAD triangles.
fn project_sphere_aabb(ball: &mut Ball, collider: &FieldCollider, radius: f32) -> usize {
    if collider.half_extents.iter().any(|extent| *extent > 1.0e-6) {
        return project_sphere_obb(ball, collider, radius);
    }
    let closest = [
        ball.position[0].clamp(collider.min[0], collider.max[0]),
        ball.position[1].clamp(collider.min[1], collider.max[1]),
        ball.position[2].clamp(collider.min[2], collider.max[2]),
    ];
    let delta = sub(ball.position, closest);
    let distance_sq = length_sq(delta);
    if distance_sq >= radius * radius {
        return 0;
    }
    if distance_sq > 1.0e-10 {
        let distance = distance_sq.sqrt();
        ball.position = add(ball.position, mul(delta, (radius - distance) / distance));
        return 1;
    }
    // Center is inside a volume: select the nearest face deterministically.
    let candidates = [
        (ball.position[0] - collider.min[0], [-1.0, 0.0, 0.0]),
        (collider.max[0] - ball.position[0], [1.0, 0.0, 0.0]),
        (ball.position[1] - collider.min[1], [0.0, -1.0, 0.0]),
        (collider.max[1] - ball.position[1], [0.0, 1.0, 0.0]),
        (ball.position[2] - collider.min[2], [0.0, 0.0, -1.0]),
        (collider.max[2] - ball.position[2], [0.0, 0.0, 1.0]),
    ];
    if let Some((distance, normal)) = candidates
        .into_iter()
        .min_by(|left, right| left.0.total_cmp(&right.0))
    {
        ball.position = add(ball.position, mul(normal, radius + distance.max(0.0)));
        return 1;
    }
    0
}

fn project_sphere_obb(ball: &mut Ball, collider: &FieldCollider, radius: f32) -> usize {
    let delta = sub(ball.position, collider.center);
    let local = [
        dot(delta, collider.axes[0]),
        dot(delta, collider.axes[1]),
        dot(delta, collider.axes[2]),
    ];
    let closest = [
        local[0].clamp(-collider.half_extents[0], collider.half_extents[0]),
        local[1].clamp(-collider.half_extents[1], collider.half_extents[1]),
        local[2].clamp(-collider.half_extents[2], collider.half_extents[2]),
    ];
    let local_delta = [
        local[0] - closest[0],
        local[1] - closest[1],
        local[2] - closest[2],
    ];
    let distance_sq = dot(local_delta, local_delta);
    if distance_sq >= radius * radius {
        return 0;
    }
    if distance_sq > 1.0e-10 {
        let distance = distance_sq.sqrt();
        let normal = [
            collider.axes[0][0] * local_delta[0] / distance
                + collider.axes[1][0] * local_delta[1] / distance
                + collider.axes[2][0] * local_delta[2] / distance,
            collider.axes[0][1] * local_delta[0] / distance
                + collider.axes[1][1] * local_delta[1] / distance
                + collider.axes[2][1] * local_delta[2] / distance,
            collider.axes[0][2] * local_delta[0] / distance
                + collider.axes[1][2] * local_delta[1] / distance
                + collider.axes[2][2] * local_delta[2] / distance,
        ];
        ball.position = add(ball.position, mul(normal, (radius - distance).max(0.0)));
        return 1;
    }
    let mut nearest_axis = 0;
    let mut nearest_distance = f32::INFINITY;
    for axis in 0..3 {
        let distance = collider.half_extents[axis] - local[axis].abs();
        if distance < nearest_distance {
            nearest_distance = distance;
            nearest_axis = axis;
        }
    }
    let sign = if local[nearest_axis] < 0.0 { -1.0 } else { 1.0 };
    let normal = mul(collider.axes[nearest_axis], sign);
    ball.position = add(
        ball.position,
        mul(normal, radius + nearest_distance.max(0.0)),
    );
    1
}

/// Compute contact normal between a ball and a field collider (AABB or OBB)
fn sphere_collider_contact(position: Vec3, radius: f32, collider: &FieldCollider) -> Option<Vec3> {
    if collider.half_extents.iter().any(|extent| *extent > 1.0e-6) {
        let delta = sub(position, collider.center);
        let local = [
            dot(delta, collider.axes[0]),
            dot(delta, collider.axes[1]),
            dot(delta, collider.axes[2]),
        ];
        let closest = [
            local[0].clamp(-collider.half_extents[0], collider.half_extents[0]),
            local[1].clamp(-collider.half_extents[1], collider.half_extents[1]),
            local[2].clamp(-collider.half_extents[2], collider.half_extents[2]),
        ];
        let local_delta = [
            local[0] - closest[0],
            local[1] - closest[1],
            local[2] - closest[2],
        ];
        let distance_sq = dot(local_delta, local_delta);
        if distance_sq >= radius * radius {
            return None;
        }
        if distance_sq > 1.0e-10 {
            let distance = distance_sq.sqrt();
            let normal = [
                collider.axes[0][0] * local_delta[0] / distance
                    + collider.axes[1][0] * local_delta[1] / distance
                    + collider.axes[2][0] * local_delta[2] / distance,
                collider.axes[0][1] * local_delta[0] / distance
                    + collider.axes[1][1] * local_delta[1] / distance
                    + collider.axes[2][1] * local_delta[2] / distance,
                collider.axes[0][2] * local_delta[0] / distance
                    + collider.axes[1][2] * local_delta[1] / distance
                    + collider.axes[2][2] * local_delta[2] / distance,
            ];
            return Some(normal);
        }
        let mut nearest_axis = 0;
        let mut nearest_distance = f32::INFINITY;
        for axis in 0..3 {
            let distance = collider.half_extents[axis] - local[axis].abs();
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest_axis = axis;
            }
        }
        let sign = if local[nearest_axis] < 0.0 { -1.0 } else { 1.0 };
        return Some(mul(collider.axes[nearest_axis], sign));
    }

    let closest = [
        position[0].clamp(collider.min[0], collider.max[0]),
        position[1].clamp(collider.min[1], collider.max[1]),
        position[2].clamp(collider.min[2], collider.max[2]),
    ];
    let delta = sub(position, closest);
    let distance_sq = length_sq(delta);
    if distance_sq >= radius * radius {
        return None;
    }
    if distance_sq > 1.0e-10 {
        let distance = distance_sq.sqrt();
        return Some(mul(delta, 1.0 / distance));
    }
    let candidates = [
        (position[0] - collider.min[0], [-1.0, 0.0, 0.0]),
        (collider.max[0] - position[0], [1.0, 0.0, 0.0]),
        (position[1] - collider.min[1], [0.0, -1.0, 0.0]),
        (collider.max[1] - position[1], [0.0, 1.0, 0.0]),
        (position[2] - collider.min[2], [0.0, 0.0, -1.0]),
        (collider.max[2] - position[2], [0.0, 0.0, 1.0]),
    ];
    candidates
        .into_iter()
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, normal)| normal)
}

/// The high-throughput backend represents the robot chassis as a planar box.
/// Projecting that box against the authored static bounds prevents drive input
/// from passing through field structures without adding an expensive general
/// rigid-body solver to every 60 Hz tick.
fn project_robot_field_colliders(
    player: &mut PlayerBody,
    robot: &RobotPhysicsConfig,
    field_colliders: &[FieldCollider],
) -> (usize, Option<Vec3>) {
    let half_x = robot.width_m * 0.5;
    let half_z = robot.length_m * 0.5;
    let robot_min_y = player.position[1] - robot.height_m * 0.5;
    let robot_max_y = player.position[1] + robot.height_m * 0.5;
    let mut contacts = 0;
    let mut contact_normal = None;

    for collider in field_colliders {
        if robot_max_y <= collider.min[1] || robot_min_y >= collider.max[1] {
            continue;
        }
        if collider.half_extents.iter().any(|extent| *extent > 1.0e-6) {
            if let Some((normal, penetration)) = robot_field_obb_contact(
                player.position,
                player.yaw,
                [half_x, robot.height_m * 0.5, half_z],
                collider,
            ) {
                player.position = add(player.position, mul(normal, penetration));
                let into_surface = dot(player.velocity, normal);
                if into_surface < 0.0 {
                    player.velocity = sub(player.velocity, mul(normal, into_surface));
                }
                contacts += 1;
                contact_normal = Some(normal);
            }
            continue;
        }
        let robot_min_x = player.position[0] - half_x;
        let robot_max_x = player.position[0] + half_x;
        let robot_min_z = player.position[2] - half_z;
        let robot_max_z = player.position[2] + half_z;
        if robot_max_x <= collider.min[0]
            || robot_min_x >= collider.max[0]
            || robot_max_z <= collider.min[2]
            || robot_min_z >= collider.max[2]
        {
            continue;
        }

        let push_left = robot_max_x - collider.min[0];
        let push_right = collider.max[0] - robot_min_x;
        let push_back = robot_max_z - collider.min[2];
        let push_front = collider.max[2] - robot_min_z;
        let candidates = [
            (push_left, [-1.0, 0.0, 0.0]),
            (push_right, [1.0, 0.0, 0.0]),
            (push_back, [0.0, 0.0, -1.0]),
            (push_front, [0.0, 0.0, 1.0]),
        ];
        if let Some((distance, normal)) = candidates
            .into_iter()
            .min_by(|left, right| left.0.total_cmp(&right.0))
        {
            player.position = add(player.position, mul(normal, distance.max(0.0)));
            let into_surface = dot(player.velocity, normal);
            if into_surface < 0.0 {
                player.velocity = sub(player.velocity, mul(normal, into_surface));
            }
            contacts += 1;
            contact_normal = Some(normal);
        }
    }
    (contacts, contact_normal)
}

/// Return the minimum-translation contact for the rotated robot box against
/// one authored field OBB. The offline scene gets this rotation from Rapier's
/// rigid body; using SAT here keeps the server's planar solver in the same
/// coordinate space instead of testing a permanently axis-aligned chassis.
fn robot_field_obb_contact(
    robot_center: Vec3,
    robot_yaw: f32,
    robot_half: Vec3,
    collider: &FieldCollider,
) -> Option<(Vec3, f32)> {
    let sin = robot_yaw.sin();
    let cos = robot_yaw.cos();
    let robot_axes = [[cos, 0.0, -sin], [0.0, 1.0, 0.0], [sin, 0.0, cos]];
    let collider_axes = collider.axes;
    let mut axes = [[0.0; 3]; 15];
    let mut axis_count = 0;
    for axis in robot_axes
        .iter()
        .copied()
        .chain(collider_axes.iter().copied())
    {
        axes[axis_count] = axis;
        axis_count += 1;
    }
    for robot_axis in robot_axes.iter().copied() {
        for collider_axis in collider_axes.iter().copied() {
            let candidate = cross(robot_axis, collider_axis);
            let length = length_sq(candidate).sqrt();
            if length <= 1.0e-5 {
                continue;
            }
            axes[axis_count] = mul(candidate, 1.0 / length);
            axis_count += 1;
        }
    }

    let center_delta = sub(robot_center, collider.center);
    let mut minimum_penetration = f32::INFINITY;
    let mut minimum_normal = [0.0, 1.0, 0.0];
    for axis in axes.into_iter().take(axis_count) {
        let robot_radius = (0..3)
            .map(|index| robot_half[index] * dot(axis, robot_axes[index]).abs())
            .sum::<f32>();
        let collider_radius = (0..3)
            .map(|index| collider.half_extents[index] * dot(axis, collider_axes[index]).abs())
            .sum::<f32>();
        let penetration = robot_radius + collider_radius - dot(center_delta, axis).abs();
        if penetration <= 0.0 {
            return None;
        }
        if penetration < minimum_penetration {
            minimum_penetration = penetration;
            minimum_normal = if dot(center_delta, axis) < 0.0 {
                mul(axis, -1.0)
            } else {
                axis
            };
        }
    }
    Some((minimum_normal, minimum_penetration))
}

fn boundary_blocks_motion(
    position: Vec3,
    direction: Vec3,
    radius: f32,
    field_boundary: &FieldBoundary,
) -> bool {
    (position[0] <= field_boundary.min[0] + radius + 1.0e-5 && direction[0] < 0.0)
        || (position[0] >= field_boundary.max[0] - radius - 1.0e-5 && direction[0] > 0.0)
        || (position[2] <= field_boundary.min[2] + radius + 1.0e-5 && direction[2] < 0.0)
        || (position[2] >= field_boundary.max[2] - radius - 1.0e-5 && direction[2] > 0.0)
}

#[allow(clippy::too_many_arguments)]
fn resolve_ball_robot_position(
    ball: &mut Ball,
    player: &mut PlayerBody,
    normal: Vec3,
    penetration: f32,
    radius: f32,
    inverse_ball_mass: f32,
    inverse_robot_mass: f32,
    alpha: f32,
    max_correction: f32,
    field_boundary: &FieldBoundary,
) {
    let ball_inverse_mass = if boundary_blocks_motion(ball.position, normal, radius, field_boundary)
    {
        // A field wall supports the ball, so the chassis must take the
        // positional correction instead of repeatedly pushing through it.
        0.0
    } else {
        inverse_ball_mass
    };
    // Carpet supports the robot vertically; it only responds in X/Z and yaw.
    let planar_normal_sq = normal[0] * normal[0] + normal[2] * normal[2];
    let robot_effective_inverse_mass = inverse_robot_mass * planar_normal_sq;
    let inverse_mass_sum = ball_inverse_mass + robot_effective_inverse_mass;
    if inverse_mass_sum <= 0.0 {
        return;
    }
    let lambda = penetration / (inverse_mass_sum + alpha);
    let ball_correction = (ball_inverse_mass * lambda).min(max_correction);
    let robot_correction = (inverse_robot_mass * lambda).min(max_correction);
    ball.position = add(ball.position, mul(normal, ball_correction));
    player.position[0] -= normal[0] * robot_correction;
    player.position[2] -= normal[2] * robot_correction;
}

#[allow(clippy::too_many_arguments)]
fn resolve_sphere_surface_velocity(
    ball: &mut Ball,
    normal: Vec3,
    surface_velocity: Vec3,
    restitution: &RestitutionCurveConfig,
    static_friction: f32,
    dynamic_friction: f32,
    radius: f32,
    mass: f32,
    inertia_factor: f32,
    support_impulse: f32,
    restitution_threshold: f32,
) {
    let mass = mass.max(0.001);
    let inertia = (inertia_factor.max(0.05) * mass * radius * radius).max(1.0e-8);
    let incoming_normal = dot(sub(ball.pre_solve_velocity, surface_velocity), normal);
    let current_normal = dot(sub(ball.velocity, surface_velocity), normal);
    let target_normal = if incoming_normal < -restitution_threshold {
        -restitution.at_speed(-incoming_normal) * incoming_normal
    } else {
        0.0
    };
    let normal_velocity_change = (target_normal - current_normal).max(0.0);
    if normal_velocity_change > 0.0 {
        ball.velocity = add(ball.velocity, mul(normal, normal_velocity_change));
    }
    // Contact impulses may push but never pull. The explicit preload is a
    // lower bound for powered rollers and resting support friction.
    let normal_impulse = (mass * normal_velocity_change).max(support_impulse);
    if normal_impulse <= 0.0 {
        return;
    }

    let contact_arm = mul(normal, -radius);
    let contact_velocity = sub(
        add(ball.velocity, cross(ball.angular_velocity, contact_arm)),
        surface_velocity,
    );
    let tangent_velocity = sub(contact_velocity, mul(normal, dot(contact_velocity, normal)));
    let tangent_speed = length_sq(tangent_velocity).sqrt();
    if tangent_speed <= 1.0e-6 {
        return;
    }
    let inverse_effective_mass = 1.0 / mass + radius * radius / inertia;
    let required_impulse = tangent_speed / inverse_effective_mass;
    let impulse_magnitude = if required_impulse <= static_friction.max(0.0) * normal_impulse {
        required_impulse
    } else {
        dynamic_friction.max(0.0) * normal_impulse
    };
    let tangent_impulse = mul(tangent_velocity, -impulse_magnitude / tangent_speed);
    ball.velocity = add(ball.velocity, mul(tangent_impulse, 1.0 / mass));
    ball.angular_velocity = add(
        ball.angular_velocity,
        mul(cross(contact_arm, tangent_impulse), 1.0 / inertia),
    );
}

fn approach_zero(value: f32, amount: f32) -> f32 {
    if value > 0.0 {
        (value - amount).max(0.0)
    } else {
        (value + amount).min(0.0)
    }
}

fn cell_for(position: Vec3, cell_size: f32) -> [i32; 3] {
    [
        (position[0] / cell_size).floor() as i32,
        (position[1] / cell_size).floor() as i32,
        (position[2] / cell_size).floor() as i32,
    ]
}

fn hash_cell(cell: [i32; 3]) -> usize {
    let x = (cell[0] as u32).wrapping_mul(73_856_093);
    let y = (cell[1] as u32).wrapping_mul(19_349_663);
    let z = (cell[2] as u32).wrapping_mul(83_492_791);
    (x ^ y ^ z) as usize
}

fn wrap_angle(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Stable pseudo-random number in [-1, 1] without storing per-ball RNG
/// state. Its inputs are only the ball index and a fixed channel salt.
fn fountain_noise(index: u32, salt: f32) -> f32 {
    ((index as f32 * 12.9898 + salt).sin() * 43_758.547).rem_euclid(1.0) * 2.0 - 1.0
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn sphere_obb_contact(
    sphere: Vec3,
    radius: f32,
    center: Vec3,
    yaw: f32,
    half: Vec3,
) -> Option<(Vec3, f32)> {
    let sin = yaw.sin();
    let cos = yaw.cos();
    let relative = sub(sphere, center);
    let local = [
        cos * relative[0] - sin * relative[2],
        relative[1],
        sin * relative[0] + cos * relative[2],
    ];
    let closest = [
        local[0].clamp(-half[0], half[0]),
        local[1].clamp(-half[1], half[1]),
        local[2].clamp(-half[2], half[2]),
    ];
    let delta = sub(local, closest);
    let distance_sq = length_sq(delta);
    if distance_sq >= radius * radius {
        return None;
    }
    let (local_normal, penetration) = if distance_sq > 1.0e-12 {
        let distance = distance_sq.sqrt();
        (mul(delta, 1.0 / distance), radius - distance)
    } else {
        let gaps = [
            half[0] - local[0].abs(),
            half[1] - local[1].abs(),
            half[2] - local[2].abs(),
        ];
        let axis = if gaps[0] <= gaps[1] && gaps[0] <= gaps[2] {
            0
        } else if gaps[1] <= gaps[2] {
            1
        } else {
            2
        };
        let mut normal = [0.0; 3];
        normal[axis] = if local[axis] >= 0.0 { 1.0 } else { -1.0 };
        (normal, radius + gaps[axis])
    };
    let world_normal = [
        cos * local_normal[0] + sin * local_normal[2],
        local_normal[1],
        -sin * local_normal[0] + cos * local_normal[2],
    ];
    Some((world_normal, penetration))
}

fn ramp_contact(position: Vec3, radius: f32, ramp: &RampPhysicsConfig) -> Option<(Vec3, f32)> {
    if !ramp.enabled
        || position[0] < ramp.center_x - ramp.width_m * 0.5 - radius
        || position[0] > ramp.center_x + ramp.width_m * 0.5 + radius
        || position[2] < ramp.start_z - radius
        || position[2] > ramp.start_z + ramp.length_m + radius
    {
        return None;
    }
    let angle = ramp.angle_deg.to_radians();
    let normal = [0.0, angle.cos(), -angle.sin()];
    let signed_distance = dot(sub(position, [ramp.center_x, 0.0, ramp.start_z]), normal);
    if signed_distance >= radius {
        None
    } else {
        Some((normal, radius - signed_distance))
    }
}

fn roller_contact(
    sphere: Vec3,
    sphere_radius: f32,
    robot_position: Vec3,
    yaw: f32,
    robot: &RobotPhysicsConfig,
) -> Option<(Vec3, f32, Vec3, Vec3)> {
    let forward = [-yaw.sin(), 0.0, -yaw.cos()];
    let right = [-forward[2], 0.0, forward[0]];
    let intake_world_y = (robot_position[1] - robot.height_m * 0.5) + robot.intake_center_height_m;
    let center = [
        robot_position[0] + forward[0] * robot.intake_forward_offset_m,
        intake_world_y,
        robot_position[2] + forward[2] * robot.intake_forward_offset_m,
    ];
    let along_axis = dot(sub(sphere, center), right)
        .clamp(-robot.intake_width_m * 0.5, robot.intake_width_m * 0.5);
    let closest = add(center, mul(right, along_axis));
    let delta = sub(sphere, closest);
    let distance_sq = length_sq(delta);
    let combined_radius = sphere_radius + robot.intake_radius_m;
    if distance_sq > combined_radius * combined_radius {
        return None;
    }
    let (normal, distance) = if distance_sq > 1.0e-12 {
        let distance = distance_sq.sqrt();
        (mul(delta, 1.0 / distance), distance)
    } else {
        (forward, 0.0)
    };
    Some((normal, combined_radius - distance, closest, right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

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
        assert_eq!(positions.len(), arena.object_count);
        assert!(
            positions
                .iter()
                .all(|p| p[1] >= arena.ball.radius_m() - 0.001)
        );
        assert!(
            positions
                .iter()
                .all(|p| p[0].abs() <= 8.0 && p[2].abs() <= 8.0)
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
    fn emits_one_semantic_event_when_a_ball_enters_a_trigger() {
        let mut arena = arena();
        arena.object_count = 1;
        arena.gravity_scale = 0.0;
        arena.ramp.enabled = false;
        let field = FieldDefinition {
            colliders: Vec::new(),
            anchors: BTreeMap::new(),
            triggers: vec![FieldTrigger {
                id: "blueSUscore".into(),
                min: [-1.0, 0.0, -1.0],
                max: [1.0, 1.0, 1.0],
            }],
            floor_height_m: 0.0,
            boundary: FieldBoundary::default(),
        };
        let mut runtime = SphereRuntime::new("semantic".into(), "fgc-2026".into(), 0);
        runtime.create_field_arena(&arena, &field);
        runtime.balls[0].position = [0.0, arena.ball.radius_m(), 0.0];
        runtime.balls[0].active = true;
        runtime.tick(1.0 / 60.0);
        let events = runtime.drain_semantic_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, "trigger_enter");
        assert_eq!(events[0].target_id, "blueSUscore");
        runtime.tick(1.0 / 60.0);
        assert!(runtime.drain_semantic_events().is_empty());
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
            maximum_ball_speed =
                maximum_ball_speed.max(length_sq(runtime.balls[0].velocity).sqrt());
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
        assert!(!runtime.balls[0].active, "ball should be captured into the hopper");
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
        assert_eq!(runtime.players["p"].stored.len(), 1, "driving into a floor ball with intake should capture it");
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
        assert!(runtime.balls[0].active, "ball stays free when hopper capacity is zero");
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
        assert!(runtime.players["p"].stored.is_empty(), "hopper should drain");
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
    fn contain_ball_deactivates_a_scored_piece() {
        let mut arena = arena();
        arena.object_count = 1;
        arena.ramp.enabled = false;
        let mut runtime = SphereRuntime::new("contain".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        assert!(runtime.contain_ball("ball:0"));
        assert!(!runtime.balls[0].active);
        assert!(!runtime.contain_ball("ball:0"), "already-contained piece");
        assert!(!runtime.contain_ball("ball:999"));
        assert!(!runtime.contain_ball("object:3"));
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
    #[ignore = "manual release-mode 1,000-ball performance benchmark"]
    fn benchmark_1000_ball_robot_interaction() {
        let mut arena = arena();
        arena.object_count = 1000;
        let mut runtime = SphereRuntime::new("benchmark".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
        runtime.set_player_input("p", 0.28, 1.0, 1.0, 0.0, 1);
        for _ in 0..120 {
            runtime.apply_player_drive(&arena, 1.0 / 60.0);
            runtime.tick(1.0 / 60.0);
        }
        let started = Instant::now();
        let mut samples = Vec::with_capacity(600);
        let mut maximum_candidates = 0;
        let mut maximum_contacts = 0;
        let mut minimum_active = arena.object_count;
        for tick in 0..600 {
            // Re-energize the field once per second. This prevents sleeping
            // from turning a sustained-contact benchmark into an idle test.
            if tick % 60 == 0 {
                for (index, ball) in runtime.balls.iter_mut().enumerate() {
                    let angle = index as f32 * 0.618_034;
                    ball.velocity[0] += angle.cos() * 1.5;
                    ball.velocity[2] += angle.sin() * 1.5;
                    ball.sleeping = false;
                    ball.quiet_ticks = 0;
                }
            }
            let tick_started = Instant::now();
            runtime.apply_player_drive(&arena, 1.0 / 60.0);
            runtime.tick(1.0 / 60.0);
            samples.push(tick_started.elapsed().as_secs_f64() * 1_000.0);
            let tick_metrics = runtime.step_metrics();
            maximum_candidates = maximum_candidates.max(tick_metrics.candidate_pairs);
            maximum_contacts = maximum_contacts.max(tick_metrics.contacts);
            minimum_active = minimum_active.min(tick_metrics.active_balls);
        }
        samples.sort_by(f64::total_cmp);
        let p95 = samples[(samples.len() as f32 * 0.95) as usize];
        let p99 = samples[(samples.len() as f32 * 0.99) as usize];
        let average = started.elapsed().as_secs_f64() * 1_000.0 / samples.len() as f64;
        let metrics = runtime.step_metrics();
        eprintln!(
            "sphere_xpbd balls={} avg={average:.3}ms p95={p95:.3}ms p99={p99:.3}ms candidates(max)={} contacts(max)={} active(min)={} sleeping(final)={}",
            arena.object_count,
            maximum_candidates,
            maximum_contacts,
            minimum_active,
            metrics.sleeping_balls,
        );
        assert_eq!(runtime.field_object_positions().len(), 1000);
        assert!(p95 <= 12.0, "p95 tick time was {p95:.3}ms");
        assert!(p99 <= 16.67, "p99 tick time was {p99:.3}ms");
    }
}
