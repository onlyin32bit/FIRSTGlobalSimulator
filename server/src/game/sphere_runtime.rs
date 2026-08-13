use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::Instant;

use super::match_runtime::{MatchContext, MatchPhase, PlayerSnapshot, ScoreState};
use super::pack_loader::{
    ActuatorConfig, ArenaConfig, FieldBoundary, FieldCollider, FieldDefinition, FieldTrigger,
    RampPhysicsConfig, RestitutionCurveConfig, RobotPhysicsConfig, RobotSemanticZone,
    SemanticZoneKind,
};

type Vec3 = [f32; 3];

/// World-space mechanism mouth of one player, projected each tick from the
/// robot-local zone geometry and the player's pose. The OBB is the authored
/// zone mesh (Intake/Transfer/Outtake), rotated into the world with the same
/// yaw convention as the physics colliders.
#[derive(Clone, Copy)]
struct IntakeZoneWorld {
    zone_center: Vec3,
    zone_axes: [Vec3; 3],
    zone_half_extents: Vec3,
    direction: Vec3,
    player_velocity: Vec3,
}

/// Touch-only reach test for a powered IntakeZone. The authored mouth mesh is
/// a decorative plate that floats at the robot body height, far above a
/// rolling ball, so the test works on the mouth's horizontal footprint:
/// project every OBB corner onto the floor plane (x/z), take the footprint
/// AABB, and inflate it by the ball radius plus a small margin. The margin is
/// not for reaching further into the field — it closes a dead zone where a
/// carried ball presses against the chassis front face (its centre sits one
/// radius out from the face, just past the mouth footprint) and would
/// otherwise lose the pull entirely. The robot must still drive the mouth
/// over the ball to pick it up; the grab distance grows by only a few
/// centimetres.
fn obb_overlap_sphere(ball_position: Vec3, ball_radius: f32, zone: &IntakeZoneWorld) -> bool {
    let h = zone.zone_half_extents;
    let mut x_min = f32::INFINITY;
    let mut x_max = f32::NEG_INFINITY;
    let mut z_min = f32::INFINITY;
    let mut z_max = f32::NEG_INFINITY;
    for sx in [-1.0f32, 1.0] {
        for sy in [-1.0f32, 1.0] {
            for sz in [-1.0f32, 1.0] {
                let corner = add(
                    add(
                        add(zone.zone_center, mul(zone.zone_axes[0], h[0] * sx)),
                        mul(zone.zone_axes[1], h[1] * sy),
                    ),
                    mul(zone.zone_axes[2], h[2] * sz),
                );
                x_min = x_min.min(corner[0]);
                x_max = x_max.max(corner[0]);
                z_min = z_min.min(corner[2]);
                z_max = z_max.max(corner[2]);
            }
        }
    }
    let reach = ball_radius + 0.04;
    ball_position[0] >= x_min - reach
        && ball_position[0] <= x_max + reach
        && ball_position[2] >= z_min - reach
        && ball_position[2] <= z_max + reach
}

/// Directional conveyor a powered semantic zone applies to a ball that is in
/// contact with the zone OBB. The same mechanism backs every zone kind: the
/// ball is pushed along `direction` (the zone's Blender-authored feed
/// direction) and the push fades to zero once the ball reaches a fixed
/// capture depth, so a train of balls queues along the feed path with spacing
/// set by the existing ball-ball contact solver (never teleported, never
/// overlapping) instead of stacking on one point. The ball stays a normal
/// dynamic body: the pull is an ordinary acceleration added during
/// integration.
///
/// The pull is a feed-speed controller, not a constant ram: a resting ball at
/// the mouth gets the full configured `zone_force_mps2`, a ball already
/// feeding at `FEED_SPEED_MPS` relative to the robot gets nothing, and a ball
/// outrunning the feed is braked. A ball that sits in the zone without
/// advancing is jammed against the queue, so it is pressed with at most a
/// fraction of the full force instead of being rammed hard enough that the
/// contact solver converts the squeeze into a vertical launch over the queue.
/// Directional conveyor a powered semantic zone applies to a ball in contact.
/// The zone imparts pure forward acceleration along `zone.direction` (the authored
/// feed direction) while the ball is inside the zone volume (`obb_overlap_sphere`),
/// giving the ball physical momentum so it accelerates through the mechanism and
/// exits naturally under inertia.
fn intake_pull_acceleration(
    ball: &Ball,
    ball_radius: f32,
    zone: IntakeZoneWorld,
    kind: SemanticZoneKind,
    zone_force_mps2: f32,
) -> Vec3 {
    if !obb_overlap_sphere(ball.position, ball_radius, &zone) {
        return [0.0; 3];
    }

    let direction = zone.direction;

    // Target surface feed speed per mechanism (m/s)
    let target_feed_speed_mps = match kind {
        SemanticZoneKind::Intake | SemanticZoneKind::Transfer => 4.5,
        SemanticZoneKind::Outtake => 8.0,
    };

    let rel_vel = sub(ball.velocity, zone.player_velocity);
    let current_feed_speed = dot(rel_vel, direction);

    if current_feed_speed >= target_feed_speed_mps {
        return [0.0; 3];
    }

    let max_push = zone_force_mps2.max(0.0);
    let push = max_push * ((target_feed_speed_mps - current_feed_speed) / target_feed_speed_mps).clamp(0.0, 1.0);
    mul(direction, push)
}

fn required_ball_substeps(max_speed: f32, gravity_scale: f32, dt: f32, radius: f32) -> usize {
    let predicted_speed = max_speed + 9.81 * gravity_scale.abs() * dt;
    let safe_displacement = (radius * 0.5).max(0.001);
    ((predicted_speed * dt / safe_displacement).ceil() as usize).clamp(1, 4)
}

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
    /// Horizontal PD pull applied this substep by a powered IntakeZone. The
    /// value is recomputed every `integrate` call, so it is not a cache that
    /// can go stale between positions.
    pull_acceleration: Vec3,
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
    transfer_power: f32,
    climb_power: f32,
    sequence: u64,
    color: &'static str,
    /// Outward normal of a static surface touched during the previous solver
    /// step. This keeps the drivetrain from turning shallow wall contact into
    /// a complete stop on the following tick.
    wall_contact_normal: Option<Vec3>,
    /// FIFO of ball indices captured into the on-robot hopper. Popping feeds
    /// the flywheel so contained (scored) balls are never recycled.
    #[allow(dead_code)]
    stored: VecDeque<usize>,
    #[allow(dead_code)]
    outtake_accumulator: f32,
    #[allow(dead_code)]
    intake_accumulator: f32,
    /// Per-player adjustable mech spec overrides (capacity, flywheel, rates).
    mech: MechSpec,
    rollers: HashMap<String, RollerState>,
}

#[derive(Clone, Debug)]
pub struct RollerState {
    pub config: ActuatorConfig,
    pub angular_velocity: f32,
    pub angle: f32,
    pub radius: f32,
    pub inertia: f32,
    pub last_friction_impulse: Vec3,
    pub last_reaction_torque_impulse: f32,
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
#[allow(dead_code)]
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
    robot_colliders: Vec<FieldCollider>,
    field_anchors: BTreeMap<String, Vec3>,
    field_triggers: Vec<FieldTrigger>,
    trigger_inside: Vec<bool>,
    semantic_events: Vec<SemanticEvent>,
    /// Robot-local semantic zones (Intake/Transfer/Outtake), if the pack
    /// shipped `bot.semantics.json`. Mirrors the collider yaw convention at
    /// runtime, so the zones stay aligned on the driven robot.
    semantic_zones: Vec<RobotSemanticZone>,
    pub last_contact_telemetry: Option<ContactTelemetryLog>,
}

#[derive(Debug, Clone, Default)]
pub struct ContactTelemetryLog {
    pub ball_velocity_before: Vec3,
    pub wheel_linear_velocity: Vec3,
    pub wheel_angular_velocity: f32,
    pub wheel_local_spin_axis: Vec3,
    pub wheel_world_spin_axis: Vec3,
    pub contact_point: Vec3,
    pub r_arm: Vec3,
    pub wheel_surface_velocity: Vec3,
    pub ball_velocity_at_contact: Vec3,
    pub relative_contact_velocity: Vec3,
    pub normal_relative_velocity: f32,
    pub tangential_relative_velocity: f32,
    pub normal_impulse: f32,
    pub compliant_deformation_m: f32,
    pub compliant_normal_force_n: f32,
    pub friction_impulse: Vec3,
    pub friction_coefficient: f32,
    pub friction_limit: f32,
    pub wheel_reaction_torque: f32,
    pub ball_velocity_after: Vec3,
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
            robot_colliders: Vec::new(),
            field_anchors: BTreeMap::new(),
            field_triggers: Vec::new(),
            trigger_inside: Vec::new(),
            semantic_events: Vec::new(),
            semantic_zones: Vec::new(),
            last_contact_telemetry: None,
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
                pull_acceleration: [0.0; 3],
            });
        }
        self.trigger_inside =
            vec![false; self.balls.len().saturating_mul(self.field_triggers.len())];
        self.semantic_events.clear();
    }

    fn sync_rollers(&mut self, colliders: &[FieldCollider]) {
        for player in self.players.values_mut() {
            for collider in colliders {
                if let Some(actuator) = &collider.actuator {
                    if !player.rollers.contains_key(&collider.id) {
                        player.rollers.insert(
                            collider.id.clone(),
                            RollerState {
                                config: actuator.clone(),
                                radius: roller_radius(collider, actuator.spin_axis),
                                inertia: (0.5
                                    * actuator.mass_kg
                                    * roller_radius(collider, actuator.spin_axis).powi(2))
                                .max(1.0e-6),
                                angular_velocity: 0.0,
                                angle: 0.0,
                                last_friction_impulse: [0.0; 3],
                                last_reaction_torque_impulse: 0.0,
                            },
                        );
                    }
                }
            }
        }
    }

    pub fn set_robot_colliders(&mut self, colliders: &[FieldCollider]) {
        self.robot_colliders = colliders.to_vec();
        self.sync_rollers(colliders);
    }

    /// Configure the powered IntakeZone pull. `None` keeps the intake button
    /// inert (no pull force, balls simply collide with the static volumes).
    pub fn set_semantic_zones(&mut self, zones: Vec<RobotSemanticZone>) {
        self.semantic_zones = zones;
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
                transfer_power: 0.0,
                climb_power: 0.0,
                sequence: 0,
                color,
                wall_contact_normal: None,
                stored: VecDeque::new(),
                outtake_accumulator: 0.0,
                intake_accumulator: 0.0,
                mech: MechSpec::default(),
                rollers: HashMap::new(),
            },
        );
        let colls = self.robot_colliders.clone();
        self.sync_rollers(&colls);
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
        transfer_power: f32,
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
            player.transfer_power = transfer_power.clamp(0.0, 1.0);
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

            if player.climb_power > 0.0 {
                player.velocity[1] = (player.velocity[1] + player.climb_power * 4.0 * dt).min(1.5);
            }
        }
    }
    /// Advance independently rotating roller bodies. The motor is a bounded
    /// torque source driven toward its target surface speed; ball contacts
    /// slow or reverse the roller through their reaction torque.
    fn step_mechanics(&mut self, _arena: &ArenaConfig, dt: f32) {
        for player in self.players.values_mut() {
            for (_id, roller) in &mut player.rollers {
                let channel_power = match roller.config.input_channel.as_str() {
                    "outtake" => player.outtake_power,
                    "climb" => player.climb_power,
                    _ => player.intake_power,
                };
                let speed_scale = roller.config.target_surface_speed_mps;
                let target_v = channel_power * speed_scale;
                let target_w = target_v / roller.radius.max(0.001);
                let speed_err = target_w - roller.angular_velocity;
                let motor_torque = (speed_err * 50.0 * roller.inertia)
                    .clamp(-roller.config.max_torque, roller.config.max_torque);
                roller.angular_velocity += (motor_torque / roller.inertia) * dt;
                roller.angle = wrap_angle(roller.angle + roller.angular_velocity * dt);
                roller.last_friction_impulse = [0.0; 3];
                roller.last_reaction_torque_impulse = 0.0;
            }
        }
    }

    pub fn tick(&mut self, dt: f64) {
        let dt = dt as f32;
        // This is a per-tick representative contact record. The structured
        // trace below still emits every solver contact; retaining the largest
        // tangential impulse here makes the public diagnostic useful after
        // the four velocity iterations have converged to zero slip.
        self.last_contact_telemetry = None;
        self.context.clock += dt as f64;
        let Some(arena) = self.arena.clone() else {
            return;
        };
        if let Some(elapsed) = &mut self.ball_release_elapsed {
            *elapsed += dt;
        }
        self.release_queued_balls(&arena);

        self.metrics = StepMetrics::default();
        let max_speed = self
            .balls
            .iter()
            .filter(|ball| ball.active && !ball.sleeping)
            .map(|ball| length_sq(ball.velocity).sqrt())
            .fold(0.0_f32, f32::max);
        let substeps =
            required_ball_substeps(max_speed, arena.gravity_scale, dt, arena.ball.radius_m());
        let substep_dt = dt / substeps as f32;
        self.step_mechanics(&arena, dt);
        for _ in 0..substeps {
            self.step_substep(&arena, substep_dt);
        }
    }

    fn step_substep(&mut self, arena: &ArenaConfig, dt: f32) {
        let integrate_started = Instant::now();
        self.integrate(&arena, dt);
        self.metrics.integrate_ms += integrate_started.elapsed().as_secs_f64() * 1_000.0;

        let broad_started = Instant::now();
        self.build_pairs(arena.ball.diameter_m.max(0.001));
        self.metrics.broad_phase_ms += broad_started.elapsed().as_secs_f64() * 1_000.0;
        self.metrics.candidate_pairs = self.metrics.candidate_pairs.max(self.pairs.len());

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
        self.update_sleeping(&arena, dt);
        self.metrics.solve_ms += solve_started.elapsed().as_secs_f64() * 1_000.0;
        self.metrics.contacts = self.metrics.contacts.max(contacts);
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

    /// World-space mechanism mouth of every player currently powering one of
    /// its semantic zones. IntakeZone gates on `intake_power`; TransferZone
    /// and OuttakeZone gate on `outtake_power`. Computed once per tick so the
    /// ball loop can borrow `self.balls` mutably without clashing with
    /// `self.players`.
    fn active_semantic_zones(&self) -> Vec<(SemanticZoneKind, IntakeZoneWorld)> {
        if self.semantic_zones.is_empty() {
            return Vec::new();
        }
        let mut zones = Vec::new();
        for (_, player) in self.players.iter() {
            // Mirrors the collider yaw convention used by the contact solver
            // so the pull mouth stays aligned with the robot boxes.
            let (sin, cos) = (player.yaw + std::f32::consts::PI).sin_cos();
            let rotate = |v: Vec3| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]];
            for zone in &self.semantic_zones {
                let powered = match zone.kind {
                    SemanticZoneKind::Intake => player.intake_power > 0.0,
                    SemanticZoneKind::Transfer => player.transfer_power > 0.0,
                    SemanticZoneKind::Outtake => player.outtake_power > 0.0,
                };
                if !powered {
                    continue;
                }
                let geometry = &zone.geometry;
                zones.push((
                    zone.kind,
                    IntakeZoneWorld {
                        zone_center: add(player.position, rotate(geometry.zone_center)),
                        zone_axes: [
                            rotate(geometry.zone_axes[0]),
                            rotate(geometry.zone_axes[1]),
                            rotate(geometry.zone_axes[2]),
                        ],
                        zone_half_extents: geometry.zone_half_extents,
                        direction: rotate(geometry.direction),
                        player_velocity: player.velocity,
                    },
                ));
            }
        }
        zones
    }

    fn integrate(&mut self, arena: &ArenaConfig, dt: f32) {
        let linear_decay = (-arena.ball.linear_damping * dt).exp();
        let angular_decay = (-arena.ball.angular_damping * dt).exp();
        let cross_section = std::f32::consts::PI * arena.ball.radius_m().powi(2);
        let drag_acceleration_factor =
            0.5 * arena.ball.air_density_kg_m3 * arena.ball.drag_coefficient * cross_section
                / arena.ball.mass_kg.max(0.001);
        let intake_zones = self.active_semantic_zones();
        for ball in &mut self.balls {
            if !ball.active {
                continue;
            }
            ball.grounded = false;
            ball.on_ramp = false;
            if ball.sleeping {
                continue;
            }
            ball.pull_acceleration = intake_zones
                .iter()
                .find_map(|(kind, zone)| {
                    let force = match kind {
                        SemanticZoneKind::Intake => arena.robot.intake_force_mps2,
                        SemanticZoneKind::Transfer => arena.robot.transfer_force_mps2,
                        SemanticZoneKind::Outtake => arena.robot.outtake_force_mps2,
                    };
                    let pull = intake_pull_acceleration(ball, arena.ball.radius_m(), *zone, *kind, force);
                    (pull[0] != 0.0 || pull[1] != 0.0 || pull[2] != 0.0).then_some(pull)
                })
                .unwrap_or([0.0; 3]);
            ball.velocity[1] -= 9.81 * arena.gravity_scale * dt;
            if ball.pull_acceleration[0] != 0.0
                || ball.pull_acceleration[1] != 0.0
                || ball.pull_acceleration[2] != 0.0
            {
                ball.velocity = add(ball.velocity, mul(ball.pull_acceleration, dt));
            }
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
        let field_boundary = self.field_boundary.clone();
        let robot_center_y = self.robot_center_y(arena);
        for player in self.players.values_mut() {
            // Contacts are refreshed by the position solver below. Keeping a
            // normal for one drive step gives stable wall sliding without
            // constraining a robot that has already driven away.
            player.wall_contact_normal = None;
            if player.position[1] > robot_center_y {
                if player.climb_power <= 0.0 {
                    player.velocity[1] -= 9.81 * arena.gravity_scale * dt;
                }
                player.position[1] += player.velocity[1] * dt;
            }
            if player.position[1] <= robot_center_y {
                player.position[1] = robot_center_y;
                player.velocity[1] = 0.0;
            }
            player.position[0] += player.velocity[0] * dt;
            player.position[2] += player.velocity[2] * dt;
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
        let robot_colliders = &self.robot_colliders;
        let field_boundary = self.field_boundary.clone();
        for player in self.players.values_mut() {
            for ball in &mut self.balls {
                if !ball.active {
                    continue;
                }
                if robot_colliders.is_empty() {
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
                        ball.sleeping = false;
                        ball.quiet_ticks = 0;
                    }
                } else {
                    for collider in robot_colliders {
                        if let Some((normal, penetration)) =
                            robot_collider_contact(ball.position, radius, player, collider)
                        {
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
            let (field_contacts, wall_normal) = project_robot_field_colliders(
                player,
                &arena.robot,
                robot_colliders,
                field_colliders,
            );
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
                if let Some(normal) =
                    sphere_collider_contact(ball.position, arena.ball.radius_m(), collider)
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
        let diameter_sq = arena.ball.diameter_m * arena.ball.diameter_m;
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

        let robot_colliders = &self.robot_colliders;
        let active_zones = self.active_semantic_zones();
        for player in self.players.values_mut() {
            for ball in &mut self.balls {
                if !ball.active {
                    continue;
                }
                let contacts: Vec<(Vec3, f32, Option<&FieldCollider>)> =
                    if robot_colliders.is_empty() {
                        sphere_obb_contact(
                            ball.position,
                            arena.ball.radius_m(),
                            player.position,
                            player.yaw,
                            [
                                arena.robot.width_m * 0.5,
                                arena.robot.height_m * 0.5,
                                arena.robot.length_m * 0.5,
                            ],
                        )
                        .map(|(n, p)| vec![(n, p, None)])
                        .unwrap_or_default()
                    } else {
                        robot_colliders
                            .iter()
                            .filter_map(|collider| {
                                robot_collider_contact(
                                    ball.position,
                                    arena.ball.radius_m(),
                                    player,
                                    collider,
                                )
                                .map(|(n, p)| (n, p, Some(collider)))
                            })
                            .collect()
                    };
                for (normal, penetration, hit_collider) in contacts {
                    // Keep this snapshot before either the normal or tangential
                    // constraint changes the chassis.  It is the velocity of the
                    // wheel centre, not a synthetic intake velocity.
                    let wheel_linear_velocity_before = player.velocity;
                    let ball_velocity_before_normal = ball.velocity;
                    let incoming_relative =
                        dot(sub(ball.pre_solve_velocity, player.velocity), normal);
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
                    // Detect intake/boot-wheel roller contact to use compliant low-restitution curve.
                    let is_roller_contact = hit_collider
                        .map(|c| {
                            let b = c.id.as_bytes();
                            c.actuator.is_some()
                                || b.starts_with(b"Intake")
                                || b.starts_with(b"Outtake")
                                || b.starts_with(b"Roller")
                                || b.starts_with(b"Wheel")
                        })
                        .unwrap_or(false);
                    let restitution_curve = if is_roller_contact {
                        &arena.robot.intake_restitution_curve
                    } else {
                        &arena.robot.restitution_curve
                    };
                    let target_relative =
                        if incoming_relative < -arena.solver.restitution_velocity_threshold_mps {
                            -restitution_curve.at_speed(-incoming_relative) * incoming_relative
                        } else {
                            0.0
                        };
                    let impulse = ((target_relative - current_relative)
                        / (effective_inv_ball + inv_robot * planar_normal_sq))
                        .max(0.0);
                    if impulse > 1.0e-8 {
                        ball.velocity =
                            add(ball.velocity, mul(normal, impulse * effective_inv_ball));
                        player.velocity[0] -= normal[0] * impulse * inv_robot;
                        player.velocity[2] -= normal[2] * impulse * inv_robot;
                        player.velocity[1] = 0.0;
                    }

                    let yaw = player.yaw + std::f32::consts::PI;
                    let sin = yaw.sin();
                    let cos = yaw.cos();
                    let rotate_yaw =
                        |v: Vec3| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]];

                    let mut roller_spin_vel = [0.0; 3];
                    let mut wheel_angular_velocity_before = 0.0;
                    let mut roller_inv_inertia = 0.0;
                    let mut spin_axis_world = [0.0; 3];
                    let mut sub_arm = [0.0; 3];
                    let mut hit_roller_id_str: Option<&str> = None;
                    let contact_pt = sub(ball.position, mul(normal, arena.ball.radius_m()));

                    if let Some(collider) = hit_collider {
                        if let Some(roller) = player.rollers.get(&collider.id) {
                            hit_roller_id_str = Some(collider.id.as_str());
                            let sub_center = add(player.position, rotate_yaw(collider.center));
                            let spin_axis_robot = [
                                collider.axes[0][0] * roller.config.spin_axis[0]
                                    + collider.axes[1][0] * roller.config.spin_axis[1]
                                    + collider.axes[2][0] * roller.config.spin_axis[2],
                                collider.axes[0][1] * roller.config.spin_axis[0]
                                    + collider.axes[1][1] * roller.config.spin_axis[1]
                                    + collider.axes[2][1] * roller.config.spin_axis[2],
                                collider.axes[0][2] * roller.config.spin_axis[0]
                                    + collider.axes[1][2] * roller.config.spin_axis[1]
                                    + collider.axes[2][2] * roller.config.spin_axis[2],
                            ];
                            spin_axis_world = unit(rotate_yaw(spin_axis_robot));
                            sub_arm = sub(contact_pt, sub_center);
                            let w_vec = mul(spin_axis_world, roller.angular_velocity);
                            roller_spin_vel = cross(w_vec, sub_arm);
                            wheel_angular_velocity_before = roller.angular_velocity;
                            roller_inv_inertia = 1.0 / roller.inertia.max(1.0e-6);
                        }
                    }

                    let ball_arm = mul(normal, -arena.ball.radius_m());
                    let robot_arm = sub(contact_pt, player.position);
                    let wheel_point_velocity_before_normal = add(
                        add(
                            wheel_linear_velocity_before,
                            cross([0.0, player.angular_velocity_y, 0.0], robot_arm),
                        ),
                        roller_spin_vel,
                    );
                    let ball_point_velocity_before_normal = add(
                        ball_velocity_before_normal,
                        cross(ball.angular_velocity, ball_arm),
                    );
                    let normal_relative_velocity_before_normal = dot(
                        sub(
                            ball_point_velocity_before_normal,
                            wheel_point_velocity_before_normal,
                        ),
                        normal,
                    );
                    let (compliant_deformation_m, compliant_normal_force_n) = hit_collider
                        .and_then(|collider| collider.actuator.as_ref())
                        .filter(|_| wheel_angular_velocity_before.abs() > 1.0e-4)
                        .map(|actuator| {
                            let deformation = penetration
                                .max(0.0)
                                .min(actuator.max_compression_m.max(0.0));
                            let compression_speed =
                                (-normal_relative_velocity_before_normal).max(0.0);
                            let force = (actuator.contact_stiffness_n_per_m.max(0.0) * deformation
                                + actuator.contact_damping_n_s_per_m.max(0.0) * compression_speed)
                                .max(0.0);
                            (deformation, force)
                        })
                        .unwrap_or((0.0, 0.0));
                    let compliant_normal_impulse = compliant_normal_force_n * dt
                        / arena.solver.velocity_iterations.max(1) as f32;
                    let additional_compliant_impulse = compliant_normal_impulse;
                    let roller_preload_impulse = if arena.robot.intake_enabled
                        && wheel_angular_velocity_before.abs() > 1.0e-4
                    {
                        arena.robot.intake_normal_force_n * dt
                            / arena.solver.velocity_iterations.max(1) as f32
                    } else {
                        0.0
                    };
                    let normal_impulse =
                        impulse + additional_compliant_impulse + roller_preload_impulse;
                    if additional_compliant_impulse > 1.0e-8 {
                        ball.velocity = add(
                            ball.velocity,
                            mul(normal, additional_compliant_impulse * effective_inv_ball),
                        );
                        player.velocity[0] -= normal[0] * additional_compliant_impulse * inv_robot;
                        player.velocity[2] -= normal[2] * additional_compliant_impulse * inv_robot;
                    }
                    let robot_point_velocity = add(
                        add(
                            player.velocity,
                            cross([0.0, player.angular_velocity_y, 0.0], robot_arm),
                        ),
                        roller_spin_vel,
                    );
                    let ball_point_velocity =
                        add(ball.velocity, cross(ball.angular_velocity, ball_arm));
                    let relative_contact = sub(ball_point_velocity, robot_point_velocity);
                    let tangent_velocity =
                        sub(relative_contact, mul(normal, dot(relative_contact, normal)));
                    let tangent_speed = length_sq(tangent_velocity).sqrt();

                    let mut tangent_impulse = [0.0; 3];
                    let mut reaction_torque = 0.0;
                    let roller_friction = if let Some(c) = hit_collider {
                        if let Some(actuator) = &c.actuator {
                            actuator.friction
                        } else {
                            arena.robot.surface_friction
                        }
                    } else {
                        arena.robot.surface_friction
                    };
                    let friction_limit = roller_friction * normal_impulse;

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
                        let mut roller_torque_arm_proj = 0.0;
                        if hit_roller_id_str.is_some() {
                            let sub_torque_vec = cross(sub_arm, tangent);
                            roller_torque_arm_proj = dot(sub_torque_vec, spin_axis_world);
                        }

                        let tangent_inverse_mass = inv_ball
                            + arena.ball.radius_m().powi(2) / ball_inertia
                            + inv_robot * (tangent[0] * tangent[0] + tangent[2] * tangent[2])
                            + robot_torque_axis * robot_torque_axis / robot_inertia
                            + roller_torque_arm_proj * roller_torque_arm_proj * roller_inv_inertia;

                        let mut tangent_impulse_magnitude = (-tangent_speed / tangent_inverse_mass)
                            .clamp(-friction_limit, friction_limit);

                        // When a mechanism is active and powered, prevent physical contact friction from
                        // acting as a sticky brake on balls moving forward along the feed path.
                        if is_roller_contact && wheel_angular_velocity_before.abs() > 1.0e-4 {
                            let test_impulse = mul(tangent, tangent_impulse_magnitude);
                            let is_opposing_feed = active_zones
                                .first()
                                .map(|(_, z)| dot(test_impulse, z.direction) < 0.0)
                                .unwrap_or(false);
                            if is_opposing_feed {
                                tangent_impulse_magnitude = 0.0;
                            }
                        }

                        tangent_impulse = mul(tangent, tangent_impulse_magnitude);
                        ball.velocity = add(ball.velocity, mul(tangent_impulse, inv_ball));
                        ball.angular_velocity = add(
                            ball.angular_velocity,
                            mul(cross(ball_arm, tangent_impulse), 1.0 / ball_inertia),
                        );
                        player.velocity[0] -= tangent_impulse[0] * inv_robot;
                        player.velocity[2] -= tangent_impulse[2] * inv_robot;
                        player.angular_velocity_y -=
                            cross(robot_arm, tangent_impulse)[1] / robot_inertia;

                        if let Some(id) = hit_roller_id_str {
                            if let Some(roller) = player.rollers.get_mut(id) {
                                reaction_torque = dot(
                                    cross(sub_arm, mul(tangent_impulse, -1.0)),
                                    spin_axis_world,
                                );
                                roller.angular_velocity += reaction_torque / roller.inertia;
                                roller.last_friction_impulse =
                                    add(roller.last_friction_impulse, tangent_impulse);
                                roller.last_reaction_torque_impulse += reaction_torque;
                            }
                        }
                    }

                    if let Some(id) = hit_roller_id_str {
                        if let Some(roller) = player.rollers.get(id) {
                            let telemetry = ContactTelemetryLog {
                                ball_velocity_before: ball.pre_solve_velocity,
                                wheel_linear_velocity: wheel_linear_velocity_before,
                                wheel_angular_velocity: wheel_angular_velocity_before,
                                wheel_local_spin_axis: roller.config.spin_axis,
                                wheel_world_spin_axis: spin_axis_world,
                                contact_point: contact_pt,
                                r_arm: sub_arm,
                                // This is the complete point velocity used by the
                                // contact solver: chassis translation + chassis
                                // yaw + the wheel's omega x r surface motion.
                                wheel_surface_velocity: robot_point_velocity,
                                ball_velocity_at_contact: ball_point_velocity,
                                relative_contact_velocity: relative_contact,
                                normal_relative_velocity: dot(relative_contact, normal),
                                tangential_relative_velocity: tangent_speed,
                                normal_impulse,
                                compliant_deformation_m,
                                compliant_normal_force_n,
                                friction_impulse: tangent_impulse,
                                friction_coefficient: roller_friction,
                                friction_limit,
                                wheel_reaction_torque: reaction_torque,
                                ball_velocity_after: ball.velocity,
                            };
                            let replace_representative =
                                self.last_contact_telemetry.as_ref().is_none_or(|previous| {
                                    length_sq(telemetry.friction_impulse)
                                        > length_sq(previous.friction_impulse)
                                });
                            if replace_representative {
                                self.last_contact_telemetry = Some(telemetry);
                            }
                            tracing::debug!(
                                ball_velocity = ?ball.pre_solve_velocity,
                                wheel_linear_velocity = ?wheel_linear_velocity_before,
                                wheel_angular_velocity = wheel_angular_velocity_before,
                                wheel_local_rotation_axis = ?roller.config.spin_axis,
                                wheel_world_rotation_axis = ?spin_axis_world,
                                contact_point = ?contact_pt,
                                r = ?sub_arm,
                                wheel_surface_velocity = ?robot_point_velocity,
                                ball_velocity_at_contact = ?ball_point_velocity,
                                relative_contact_velocity = ?relative_contact,
                                normal_relative_velocity = dot(relative_contact, normal),
                                tangential_relative_velocity = tangent_speed,
                                normal_impulse,
                                compliant_deformation_m,
                                compliant_normal_force_n,
                                friction_impulse = ?tangent_impulse,
                                friction_coefficient = roller_friction,
                                friction_limit,
                                wheel_reaction_torque = reaction_torque,
                                ball_velocity_after = ?ball.velocity,
                                "ball/boot-wheel contact"
                            );
                        }
                    }
                }
            }
            if player.position[1] <= robot_center_y {
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
                intake_roller_angle: player
                    .rollers
                    .get("IntakeRoller")
                    .map(|roller| roller.angle)
                    .unwrap_or(0.0),
                color: player.color.into(),
                stored_balls: 0,
                capacity: 0,
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
        let Some(index) = entity_id
            .strip_prefix("ball:")
            .and_then(|v| v.parse::<usize>().ok())
        else {
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
    robot_colliders: &[FieldCollider],
    field_colliders: &[FieldCollider],
) -> (usize, Option<Vec3>) {
    let yaw = player.yaw + std::f32::consts::PI;
    let sin = yaw.sin();
    let cos = yaw.cos();
    let rotate = |v: Vec3| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]];

    let fallback_collider;
    let sub_colliders = if robot_colliders.is_empty() {
        fallback_collider = FieldCollider {
            id: "chassis".to_string(),
            min: [
                -robot.width_m * 0.5,
                -robot.height_m * 0.5,
                -robot.length_m * 0.5,
            ],
            max: [
                robot.width_m * 0.5,
                robot.height_m * 0.5,
                robot.length_m * 0.5,
            ],
            center: [0.0, 0.0, 0.0],
            half_extents: [
                robot.width_m * 0.5,
                robot.height_m * 0.5,
                robot.length_m * 0.5,
            ],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: None,
        };
        std::slice::from_ref(&fallback_collider)
    } else {
        robot_colliders
    };

    let mut contacts = 0;
    let mut contact_normal = None;

    for sub_coll in sub_colliders {
        let sub_center = add(player.position, rotate(sub_coll.center));
        let sub_axes = [
            rotate(sub_coll.axes[0]),
            rotate(sub_coll.axes[1]),
            rotate(sub_coll.axes[2]),
        ];
        let sub_half = sub_coll.half_extents;

        let sub_extent_y = (0..3)
            .map(|i| sub_axes[i][1].abs() * sub_half[i])
            .sum::<f32>();
        let sub_min_y = sub_center[1] - sub_extent_y;
        let sub_max_y = sub_center[1] + sub_extent_y;

        for collider in field_colliders {
            if sub_max_y <= collider.min[1] || sub_min_y >= collider.max[1] {
                continue;
            }

            let field_center;
            let field_axes;
            let field_half;

            if collider.half_extents.iter().any(|extent| *extent > 1.0e-6) {
                field_center = collider.center;
                field_axes = collider.axes;
                field_half = collider.half_extents;
            } else {
                field_center = [
                    (collider.min[0] + collider.max[0]) * 0.5,
                    (collider.min[1] + collider.max[1]) * 0.5,
                    (collider.min[2] + collider.max[2]) * 0.5,
                ];
                field_half = [
                    (collider.max[0] - collider.min[0]) * 0.5,
                    (collider.max[1] - collider.min[1]) * 0.5,
                    (collider.max[2] - collider.min[2]) * 0.5,
                ];
                field_axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
            }

            if let Some((normal, penetration)) = obb_obb_contact(
                sub_center,
                sub_axes,
                sub_half,
                field_center,
                field_axes,
                field_half,
            ) {
                player.position = add(player.position, mul(normal, penetration));
                let into_surface = dot(player.velocity, normal);
                if into_surface < 0.0 {
                    player.velocity = sub(player.velocity, mul(normal, into_surface));
                }
                contacts += 1;
                contact_normal = Some(normal);
            }
        }
    }
    (contacts, contact_normal)
}

fn obb_obb_contact(
    center_a: Vec3,
    axes_a: [[f32; 3]; 3],
    half_a: Vec3,
    center_b: Vec3,
    axes_b: [[f32; 3]; 3],
    half_b: Vec3,
) -> Option<(Vec3, f32)> {
    let mut axes = [[0.0; 3]; 15];
    let mut axis_count = 0;
    for axis in axes_a.iter().copied().chain(axes_b.iter().copied()) {
        axes[axis_count] = axis;
        axis_count += 1;
    }
    for axis_a in axes_a.iter().copied() {
        for axis_b in axes_b.iter().copied() {
            let candidate = cross(axis_a, axis_b);
            let length = length_sq(candidate).sqrt();
            if length <= 1.0e-5 {
                continue;
            }
            axes[axis_count] = mul(candidate, 1.0 / length);
            axis_count += 1;
        }
    }

    let center_delta = sub(center_a, center_b);
    let mut minimum_penetration = f32::INFINITY;
    let mut minimum_normal = [0.0, 1.0, 0.0];
    for axis in axes.into_iter().take(axis_count) {
        let radius_a = (0..3)
            .map(|index| half_a[index] * dot(axis, axes_a[index]).abs())
            .sum::<f32>();
        let radius_b = (0..3)
            .map(|index| half_b[index] * dot(axis, axes_b[index]).abs())
            .sum::<f32>();
        let penetration = radius_a + radius_b - dot(center_delta, axis).abs();
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
    _max_correction: f32,
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
    let ball_correction = ball_inverse_mass * lambda;
    let robot_correction = robot_effective_inverse_mass * lambda;
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

fn authored_robot_contact(
    sphere: Vec3,
    radius: f32,
    player: &PlayerBody,
    collider: &FieldCollider,
) -> Option<(Vec3, f32)> {
    let yaw = player.yaw + std::f32::consts::PI;
    let sin = yaw.sin();
    let cos = yaw.cos();
    let rotate = |v: Vec3| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]];
    let center = add(player.position, rotate(collider.center));

    let mut local_axes = collider.axes;
    if let Some(actuator) = &collider.actuator {
        if let Some(roller) = player.rollers.get(&collider.id) {
            let angle = roller.angle;
            if angle.abs() > 1.0e-5 {
                let k = actuator.spin_axis;
                let cos_a = angle.cos();
                let sin_a = angle.sin();
                for i in 0..3 {
                    let v = collider.axes[i];
                    let k_cross_v = cross(k, v);
                    let k_dot_v = dot(k, v);
                    local_axes[i] = [
                        v[0] * cos_a + k_cross_v[0] * sin_a + k[0] * k_dot_v * (1.0 - cos_a),
                        v[1] * cos_a + k_cross_v[1] * sin_a + k[1] * k_dot_v * (1.0 - cos_a),
                        v[2] * cos_a + k_cross_v[2] * sin_a + k[2] * k_dot_v * (1.0 - cos_a),
                    ];
                }
            }
        }
    }

    let axes = [
        rotate(local_axes[0]),
        rotate(local_axes[1]),
        rotate(local_axes[2]),
    ];
    // Intake meshes are authored as cylinders.  Treating their bounds as
    // boxes created collision at the empty box corners, visibly leaving a
    // force-field gap around the roller.  Use the authored cylinder's axial
    // extent and radial extents directly instead.
    if collider.id == "IntakeRoller" {
        return sphere_cylinder_contact(sphere, radius, center, axes, collider.half_extents);
    }
    sphere_obb_contact_axes(sphere, radius, center, axes, collider.half_extents)
}

fn robot_collider_contact(
    sphere: Vec3,
    radius: f32,
    player: &PlayerBody,
    collider: &FieldCollider,
) -> Option<(Vec3, f32)> {
    authored_robot_contact(sphere, radius, player, collider)
}

#[allow(dead_code)]
fn mechanism_is_spinning(player: &PlayerBody, collider: &FieldCollider) -> bool {
    collider.actuator.is_some()
        && player
            .rollers
            .get(&collider.id)
            .is_some_and(|roller| roller.angular_velocity.abs() > 0.01)
}

fn roller_radius(collider: &FieldCollider, local_axis: Vec3) -> f32 {
    let axis = dominant_axis(local_axis);
    collider
        .half_extents
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != axis)
        .map(|(_, extent)| *extent)
        .fold(1.0e-4_f32, f32::max)
}

fn dominant_axis(axis: Vec3) -> usize {
    if axis[0].abs() >= axis[1].abs() && axis[0].abs() >= axis[2].abs() {
        0
    } else if axis[1].abs() >= axis[2].abs() {
        1
    } else {
        2
    }
}

#[allow(dead_code)]
fn unit(vector: Vec3) -> Vec3 {
    let length = length_sq(vector).sqrt();
    if length > 1.0e-8 {
        mul(vector, 1.0 / length)
    } else {
        [1.0, 0.0, 0.0]
    }
}

#[allow(dead_code)]
fn rotate_about_axis(vector: Vec3, axis: Vec3, angle: f32) -> Vec3 {
    let (sin, cos) = angle.sin_cos();
    add(
        add(mul(vector, cos), mul(cross(axis, vector), sin)),
        mul(axis, dot(axis, vector) * (1.0 - cos)),
    )
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
    let axes = [[cos, 0.0, -sin], [0.0, 1.0, 0.0], [sin, 0.0, cos]];
    sphere_obb_contact_axes(sphere, radius, center, axes, half)
}

fn sphere_obb_contact_axes(
    sphere: Vec3,
    radius: f32,
    center: Vec3,
    axes: [[f32; 3]; 3],
    half: Vec3,
) -> Option<(Vec3, f32)> {
    let relative = sub(sphere, center);
    let local = [
        dot(relative, axes[0]),
        dot(relative, axes[1]),
        dot(relative, axes[2]),
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
        axes[0][0] * local_normal[0] + axes[1][0] * local_normal[1] + axes[2][0] * local_normal[2],
        axes[0][1] * local_normal[0] + axes[1][1] * local_normal[1] + axes[2][1] * local_normal[2],
        axes[0][2] * local_normal[0] + axes[1][2] * local_normal[1] + axes[2][2] * local_normal[2],
    ];
    Some((world_normal, penetration))
}

/// Exact sphere contact against a finite cylinder.  Intake meshes are
/// cylinders in the authored physics asset; their longest local extent is
/// the axial half-length and the remaining two extents define the radius.
fn sphere_cylinder_contact(
    sphere: Vec3,
    sphere_radius: f32,
    center: Vec3,
    axes: [[f32; 3]; 3],
    half_extents: Vec3,
) -> Option<(Vec3, f32)> {
    let axial_axis = (0..3)
        .max_by(|&left, &right| half_extents[left].total_cmp(&half_extents[right]))
        .unwrap_or(0);
    let axial_half = half_extents[axial_axis].max(0.0);
    let radial_radius = (0..3)
        .filter(|axis| *axis != axial_axis)
        .map(|axis| half_extents[axis])
        .sum::<f32>()
        * 0.5;
    let axis = axes[axial_axis];
    let relative = sub(sphere, center);
    let axial = dot(relative, axis);
    let radial = sub(relative, mul(axis, axial));
    let radial_length = length_sq(radial).sqrt();
    let closest_axial = axial.clamp(-axial_half, axial_half);
    let closest_radial = if radial_length > radial_radius && radial_length > 1.0e-8 {
        mul(radial, radial_radius / radial_length)
    } else {
        radial
    };
    let closest = add(center, add(mul(axis, closest_axial), closest_radial));
    let delta = sub(sphere, closest);
    let distance_sq = length_sq(delta);
    if distance_sq >= sphere_radius * sphere_radius {
        return None;
    }
    if distance_sq > 1.0e-12 {
        let distance = distance_sq.sqrt();
        return Some((mul(delta, 1.0 / distance), sphere_radius - distance));
    }

    let side_distance = radial_radius - radial_length;
    let cap_distance = axial_half - axial.abs();
    if side_distance <= cap_distance {
        let normal = if radial_length > 1.0e-8 {
            mul(radial, 1.0 / radial_length)
        } else {
            axes[(axial_axis + 1) % 3]
        };
        Some((normal, sphere_radius + side_distance.max(0.0)))
    } else {
        let normal = mul(axis, if axial < 0.0 { -1.0 } else { 1.0 });
        Some((normal, sphere_radius + cap_distance.max(0.0)))
    }
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

#[allow(dead_code)]
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
        use crate::game::pack_loader::RobotIntakeGeometry;
    use std::time::Instant;

    fn arena() -> ArenaConfig {
        crate::game::pack_loader::PackLoader::new("0.1.0")
            .load_pack("../pkgs/games/fgc-2026/manifest.json")
            .unwrap()
            .arena
    }

    #[test]
    fn slow_ball_keeps_the_single_step_path() {
        assert_eq!(required_ball_substeps(1.0, 1.0, 1.0 / 60.0, 0.05), 1);
    }

    #[test]
    fn fast_ball_uses_substeps_below_half_radius() {
        assert_eq!(required_ball_substeps(12.0, 0.0, 1.0 / 60.0, 0.05), 9);
    }

    #[test]
    fn fast_ball_cannot_tunnel_through_a_thin_field_wall() {
        let mut arena = arena();
        arena.object_count = 1;
        arena.gravity_scale = 0.0;
        arena.ramp.enabled = false;
        let wall = FieldCollider {
            id: "thin-wall".into(),
            min: [-0.025, 0.0, -1.0],
            max: [0.025, 0.5, 1.0],
            center: [0.0, 0.25, 0.0],
            half_extents: [0.025, 0.25, 1.0],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: None,
        };
        let field = FieldDefinition {
            colliders: vec![wall],
            anchors: BTreeMap::new(),
            triggers: Vec::new(),
            floor_height_m: 0.0,
            boundary: FieldBoundary {
                min: [-8.0, 0.0, -8.0],
                max: [8.0, 1.0, 8.0],
            },
        };
        let mut runtime = SphereRuntime::new("thin-wall".into(), "fgc-2026".into(), 0);
        runtime.create_field_arena(&arena, &field);
        runtime.balls[0].position = [-0.20, 0.05, 0.0];
        runtime.balls[0].velocity = [12.0, 0.0, 0.0];
        runtime.balls[0].pre_solve_velocity = runtime.balls[0].velocity;
        runtime.balls[0].active = true;
        runtime.balls[0].released = true;

        runtime.tick(1.0 / 60.0);

        assert!(
            runtime.balls[0].position[0] <= -0.075 + 1.0e-4,
            "ball crossed thin wall: {:?}",
            runtime.balls[0].position
        );
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
        runtime.set_player_input("p", 0.25, 1.0, 0.0, 0.0, 0.0, 1);
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

    fn robot_field_obb_contact(
        robot_center: Vec3,
        robot_yaw: f32,
        robot_half: Vec3,
        collider: &FieldCollider,
    ) -> Option<(Vec3, f32)> {
        let sin = robot_yaw.sin();
        let cos = robot_yaw.cos();
        let robot_axes = [[cos, 0.0, -sin], [0.0, 1.0, 0.0], [sin, 0.0, cos]];
        obb_obb_contact(
            robot_center,
            robot_axes,
            robot_half,
            collider.center,
            collider.axes,
            collider.half_extents,
        )
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
            actuator: None,
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
        runtime.set_player_input("p", 0.0, 1.0, 0.0, 0.0, 0.0, 1);

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
        runtime.set_player_input("p", 0.0, 1.0, 0.0, 0.0, 0.0, 1);

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
        runtime.set_player_input("p", 0.0, 1.0, 0.0, 0.0, 0.0, 1);

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
            pull_acceleration: [0.0; 3],
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
    #[ignore = "legacy intake behavior test; mechanisms are intentionally disabled"]
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
        assert!(runtime.balls[0].active, "intake must not remove balls");
        assert!(runtime.players["p"].stored.is_empty());
        assert_eq!(runtime.players["p"].stored[0], 0);
        assert!(
            !runtime
                .drain_semantic_events()
                .iter()
                .any(|event| event.kind == "never")
        );
    }

    #[test]
    fn intake_zone_pulls_a_ball_from_the_mouth_into_the_robot_without_teleporting() {
        let mut arena = arena();
        arena.object_count = 1;
        arena.ramp.enabled = false;
        let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
            .load_pack("../pkgs/games/fgc-2026/manifest.json")
            .unwrap();
        let geometry = pack
            .semantic_zones
            .iter()
            .find(|zone| zone.kind == SemanticZoneKind::Intake)
            .expect("bot.semantics.json must ship an IntakeZone")
            .geometry
            .clone();
        let mut runtime = SphereRuntime::new("intake-zone".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        runtime.set_semantic_zones(pack.semantic_zones.clone());
        runtime.set_robot_colliders(&pack.robot_colliders);
        runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
        let player = runtime.players.get_mut("p").unwrap();
        player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        player.yaw = 0.0;
        player.intake_power = 1.0;

        // Rotate exactly like the runtime does.
        let (sin, cos) = (player.yaw + std::f32::consts::PI).sin_cos();
        let rotate =
            |v: [f32; 3]| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]];
        let world_zone = add(player.position, rotate(geometry.zone_center));
        let world_direction = rotate(geometry.direction);
        // The mouth must sit at the robot's front (world -z at yaw 0) and the
        // conveyor must point INTO the robot (world +z).
        assert!(
            world_zone[2] < player.position[2],
            "zone mouth must face the robot front, got {world_zone:?}"
        );
        assert!(
            world_direction[2] > 0.0,
            "conveyor must point into the robot, got direction {world_direction:?}"
        );
        assert!(
            dot(world_direction, world_direction) > 0.999,
            "feed direction must be unit length, got {world_direction:?}"
        );

        // A ball sitting in the mouth is carried in by the conveyor, as a
        // normal dynamic body (bounded speed, no teleporting).
        runtime.balls[0].position = [world_zone[0], arena.ball.radius_m(), world_zone[2]];
        runtime.balls[0].velocity = [0.0; 3];
        runtime.balls[0].pre_solve_velocity = [0.0; 3];
        let start_z = runtime.balls[0].position[2];
        let mut max_speed = 0.0f32;
        for _ in 0..300 {
            runtime.tick(1.0 / 60.0);
            max_speed = max_speed.max(length_sq(runtime.balls[0].velocity).sqrt());
        }
        assert!(
            runtime.balls[0].position[2] > start_z + 0.02,
            "ball z moved from {start_z} to {} — the conveyor must carry it in",
            runtime.balls[0].position[2]
        );
        assert!(max_speed < 30.0, "pull must stay a physical acceleration");
        assert!(runtime.balls[0].active, "the ball must not be removed");
    }

    #[test]
    fn intake_zone_ignores_balls_not_touching_the_mouth() {
        let mut arena = arena();
        arena.object_count = 1;
        arena.ramp.enabled = false;
        let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
            .load_pack("../pkgs/games/fgc-2026/manifest.json")
            .unwrap();
        let mut runtime = SphereRuntime::new("intake-reach".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        runtime.set_semantic_zones(pack.semantic_zones.clone());
        runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
        let player = runtime.players.get_mut("p").unwrap();
        player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        player.yaw = 0.0;
        player.intake_power = 1.0;

        // Ball well ahead of the mouth (outside the OBB): the intake must not
        // magnetise it from a distance.
        runtime.balls[0].position = [0.0, arena.ball.radius_m(), -0.6];
        runtime.balls[0].velocity = [0.0; 3];
        runtime.balls[0].pre_solve_velocity = [0.0; 3];
        let start_z = runtime.balls[0].position[2];
        for _ in 0..60 {
            runtime.tick(1.0 / 60.0);
        }
        assert!(
            (runtime.balls[0].position[2] - start_z).abs() < 0.03,
            "ball moved to {:?} although it never touched the mouth",
            runtime.balls[0].position
        );
    }

    #[test]
    fn intake_zone_queues_multiple_balls_without_stacking() {
        let mut arena = arena();
        arena.object_count = 3;
        arena.ramp.enabled = false;
        let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
            .load_pack("../pkgs/games/fgc-2026/manifest.json")
            .unwrap();
        let geometry = pack
            .semantic_zones
            .iter()
            .find(|zone| zone.kind == SemanticZoneKind::Intake)
            .expect("bot.semantics.json must ship an IntakeZone")
            .geometry
            .clone();
        let mut runtime = SphereRuntime::new("intake-queue".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        runtime.set_semantic_zones(pack.semantic_zones.clone());
        runtime.set_robot_colliders(&pack.robot_colliders);
        runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
        let player = runtime.players.get_mut("p").unwrap();
        player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        player.yaw = 0.0;
        player.intake_power = 1.0;

        let (sin, cos) = (player.yaw + std::f32::consts::PI).sin_cos();
        let rotate =
            |v: [f32; 3]| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]];
        let mouth_z = add(player.position, rotate(geometry.zone_center))[2];
        let radius = arena.ball.radius_m();
        // Forward drive (world -z at yaw 0) with the intake powered: the
        // robot must scoop the row of balls since the mouth only grabs balls
        // that touch its footprint.
        runtime.set_player_input("p", 0.0, -1.0, 1.0, 0.0, 0.0, 0);
        // Three balls spaced a diameter apart, starting at the mouth and
        // extending forward along the intake axis.
        for (i, ball) in runtime.balls.iter_mut().enumerate() {
            ball.active = true;
            ball.position = [0.0, radius, mouth_z - i as f32 * (2.0 * radius)];
            ball.velocity = [0.0; 3];
            ball.pre_solve_velocity = [0.0; 3];
        }

        for tick in 0..600 {
            runtime.apply_player_drive(&arena, 1.0 / 60.0);
            runtime.tick(1.0 / 60.0);
        }

        let player = &runtime.players["p"];
        let (sin, cos) = (player.yaw + std::f32::consts::PI).sin_cos();
        let rotate =
            |v: [f32; 3]| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]];
        let mouth_now = add(player.position, rotate(geometry.zone_center));
        eprintln!(
            "[QUEUE TEST] robot_z={:.3} mouth_z_now={:.3} final positions={:?}",
            player.position[2],
            mouth_now[2],
            runtime
                .balls
                .iter()
                .map(|b| b.position)
                .collect::<Vec<_>>()
        );
        for ball in &runtime.balls {
            assert!(ball.active, "intake must not remove balls");
        }
        // Balls must never overlap; the contact solver owns the spacing.
        for i in 0..runtime.balls.len() {
            for j in (i + 1)..runtime.balls.len() {
                let separation =
                    length_sq(sub(runtime.balls[i].position, runtime.balls[j].position)).sqrt();
                assert!(
                    separation > 2.0 * radius - 0.02,
                    "ball pair ({i},{j}) overlaps at {separation:.3}m (2r={:.3})",
                    2.0 * radius
                );
            }
        }
        // And at least one ball must have been carried into the robot body
        // (deeper than the mouth line, i.e. world +z at yaw 0).
        let carried_in = runtime
            .balls
            .iter()
            .any(|ball| ball.position[2] > mouth_now[2] + radius * 0.5);
        assert!(
            carried_in,
            "no ball was carried into the robot (mouth now z={:.3})",
            mouth_now[2]
        );
    }

    #[test]
    fn blocked_rear_ball_is_not_launched_up_over_the_front_ball() {
        let mut arena = arena();
        arena.object_count = 3;
        arena.ramp.enabled = false;
        let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
            .load_pack("../pkgs/games/fgc-2026/manifest.json")
            .unwrap();
        let geometry = pack
            .semantic_zones
            .iter()
            .find(|zone| zone.kind == SemanticZoneKind::Intake)
            .expect("bot.semantics.json must ship an IntakeZone")
            .geometry
            .clone();
        let mut runtime = SphereRuntime::new("intake-climb".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        runtime.set_semantic_zones(pack.semantic_zones.clone());
        runtime.set_robot_colliders(&pack.robot_colliders);
        runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
        let player = runtime.players.get_mut("p").unwrap();
        player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        player.yaw = 0.0;
        player.intake_power = 1.0;

        let (sin, cos) = (player.yaw + std::f32::consts::PI).sin_cos();
        let rotate =
            |v: [f32; 3]| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]];
        let mouth_z = add(player.position, rotate(geometry.zone_center))[2];
        let radius = arena.ball.radius_m();

        // The real jam: three balls scooped into the intake while the robot
        // drives forward. The bay only holds about two, so the rearmost ball
        // keeps shoving the jammed pair, which is what turns an unbounded
        // intake ram into a vertical launch.
        runtime.set_player_input("p", 0.0, -1.0, 1.0, 0.0, 0.0, 0);
        for (i, ball) in runtime.balls.iter_mut().enumerate() {
            ball.active = true;
            ball.position = [0.0, radius, mouth_z - i as f32 * (2.0 * radius)];
            ball.velocity = [0.0; 3];
            ball.pre_solve_velocity = [0.0; 3];
        }

        let mut max_y = 0.0f32;
        for _ in 0..600 {
            runtime.apply_player_drive(&arena, 1.0 / 60.0);
            runtime.tick(1.0 / 60.0);
            for ball in &runtime.balls {
                max_y = max_y.max(ball.position[1]);
            }
        }
        eprintln!(
            "[CLIMB TEST] radius={radius:.3} max_y={max_y:.4} max_lift={:.4} final={:?}",
            max_y - radius,
            runtime
                .balls
                .iter()
                .map(|b| b.position)
                .collect::<Vec<_>>()
        );
        assert!(
            max_y < radius + 0.10,
            "rear ball climbed over the queue: max_y={max_y:.4} (radius={radius:.3})"
        );
    }

    #[test]
    fn intake_zone_without_power_leaves_the_ball_at_rest() {
        let mut arena = arena();
        arena.object_count = 1;
        arena.ramp.enabled = false;
        let mut runtime = SphereRuntime::new("intake-off".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
        let player = runtime.players.get_mut("p").unwrap();
        player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        player.yaw = 0.0;
        runtime.balls[0].position = [0.0, arena.ball.radius_m(), 0.2];
        runtime.balls[0].velocity = [0.0; 3];
        runtime.balls[0].pre_solve_velocity = [0.0; 3];
        let start_z = runtime.balls[0].position[2];
        for _ in 0..30 {
            runtime.tick(1.0 / 60.0);
        }
        assert!(
            (runtime.balls[0].position[2] - start_z).abs() < 0.02,
            "ball moved to {:?} without intake power",
            runtime.balls[0].position
        );
    }

    #[test]
    #[ignore = "legacy intake behavior test; mechanisms are intentionally disabled"]
    fn driving_with_intake_captures_a_floor_ball() {
        let mut arena = arena();
        arena.object_count = 1;
        arena.ramp.enabled = false;
        let mut runtime = SphereRuntime::new("drive-intake".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
        runtime.set_player_input("p", 0.0, 1.0, 1.0, 0.0, 0.0, 1);
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
        assert!(runtime.players["p"].stored.is_empty());
        assert_eq!(runtime.players["p"].stored[0], 0);
        assert!(
            !runtime
                .drain_semantic_events()
                .iter()
                .any(|event| event.kind == "never")
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
    #[ignore = "legacy intake behavior test; mechanisms are intentionally disabled"]
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
        assert_eq!(
            runtime.players["p"].stored.len(),
            1,
            "hopper must not drain"
        );
        assert!(
            ball.velocity[1] < 0.5,
            "mechanism must not launch the ball: {}",
            ball.velocity[1]
        );
        assert!(
            ball.velocity[2].abs() < 0.001,
            "mechanism must not launch the ball: {}",
            ball.velocity[2]
        );
        assert!(
            ball.position[1] < arena.robot.height_m + arena.ball.radius_m(),
            "mechanism must not move the ball to a launch height: {}",
            ball.position[1]
        );
        assert!(
            !runtime
                .drain_semantic_events()
                .iter()
                .any(|event| event.kind == "never")
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
    #[ignore = "manual release-mode 1,000-ball performance benchmark"]
    fn benchmark_1000_ball_robot_interaction() {
        let mut arena = arena();
        arena.object_count = 1000;
        let mut runtime = SphereRuntime::new("benchmark".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
        runtime.set_player_input("p", 0.28, 1.0, 1.0, 0.0, 0.0, 1);
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

    #[test]
    fn test_pure_roller_contact_physics() {
        let mut arena = arena();
        arena.gravity_scale = 1.0;
        let roller_collider = FieldCollider {
            id: "Roller1".into(),
            min: [-0.5, -0.1, -0.1],
            max: [0.5, 0.1, 0.1],
            center: [0.0, 0.0, 0.0],
            half_extents: [0.5, 0.1, 0.1],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: Some(ActuatorConfig {
                input_channel: "intake".into(),
                spin_axis: [1.0, 0.0, 0.0],
                max_torque: 15.0,
                mass_kg: 0.8,
                target_surface_speed_mps: 5.0,
                friction: 0.90,
                contact_stiffness_n_per_m: 500.0,
                contact_damping_n_s_per_m: 5.0,
                max_compression_m: 0.010,
            }),
        };
        let field = FieldDefinition {
            colliders: vec![],
            anchors: std::collections::BTreeMap::new(),
            triggers: Vec::new(),
            floor_height_m: 0.0,
            boundary: FieldBoundary {
                min: [-8.0, 0.0, -8.0],
                max: [8.0, 1.0, 8.0],
            },
        };
        arena.object_count = 1;
        let mut runtime = SphereRuntime::new("test".into(), "test".into(), 0);
        runtime.create_field_arena(&arena, &field);
        runtime.set_robot_colliders(&[roller_collider]);
        runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
        if let Some(p) = runtime.players.get_mut("p1") {
            p.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        }
        // Turn ON intake motor
        runtime.set_player_input("p1", 0.0, 0.0, 1.0, 0.0, 0.0, 0);

        // Place ball touching top of roller
        let initial_pos = [
            0.0,
            arena.robot.height_m * 0.5 + 0.1 + arena.ball.radius_m() - 0.001,
            0.0,
        ];
        {
            let ball = &mut runtime.balls[0];
            ball.active = true;
            ball.position = initial_pos;
            ball.velocity = [0.0, 0.0, 0.0];
            ball.sleeping = false;
        }

        let mut friction_seen = false;
        let mut prev_pos = initial_pos;

        for tick in 0..10 {
            runtime.tick(1.0 / 60.0);
            let b = &runtime.balls[0];
            let player = &runtime.players["p1"];
            let roller = &player.rollers["Roller1"];

            let contact_pt = [
                b.position[0],
                b.position[1] - arena.ball.radius_m(),
                b.position[2],
            ];
            let roller_center = [player.position[0], player.position[1], player.position[2]];
            let r_arm = sub(contact_pt, roller_center);
            let world_w = mul([1.0, 0.0, 0.0], roller.angular_velocity);
            let v_surface = cross(world_w, r_arm);

            let disp = (length_sq(sub(b.position, prev_pos))).sqrt();
            prev_pos = b.position;

            eprintln!(
                "[PHYSICS VALIDATION] tick={tick} pos={:?} vel={:?} roller_w={:.2} v_surf={:?} friction_imp={:?} reaction_torque={:.4} disp={:.4}",
                b.position,
                b.velocity,
                roller.angular_velocity,
                v_surface,
                roller.last_friction_impulse,
                roller.last_reaction_torque_impulse,
                disp
            );

            // Verify continuous motion: displacement per tick is small (no teleport jumps)
            assert!(
                disp < 0.2,
                "Ball must not teleport or jump discontinuously: disp={disp}"
            );
            // Ball stays active at all times
            assert!(b.active, "Ball must stay active in the 3D physics world");

            friction_seen |=
                roller.last_friction_impulse[2].abs() > 0.0 || b.velocity[2].abs() > 0.01;
        }

        let final_ball = &runtime.balls[0];
        assert!(
            final_ball.velocity[2] < 0.0,
            "Ball must accelerate purely via contact friction surface velocity"
        );
        assert!(
            friction_seen,
            "Contact friction impulse must transfer momentum to the ball"
        );
    }

    #[test]
    fn test_idle_ball_gripped_only_by_intake_preload() {
        // P1 regression: a ball that simply rests against a powered roller
        // (no closing velocity, no tread compression) has no restitution or
        // compliant normal impulse, so without the authored preload the
        // Coulomb friction has a zero limit and the roller can never grip it.
        // Zero gravity and a rigid tread (no stiffness/damping) force every
        // other grip source off; only the intake preload may move the ball.
        let mut arena = arena();
        arena.gravity_scale = 0.0;
        let roller_collider = FieldCollider {
            id: "Roller1".into(),
            min: [-0.5, -0.1, -0.1],
            max: [0.5, 0.1, 0.1],
            center: [0.0, 0.0, 0.0],
            half_extents: [0.5, 0.1, 0.1],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: Some(ActuatorConfig {
                input_channel: "intake".into(),
                spin_axis: [1.0, 0.0, 0.0],
                max_torque: 15.0,
                mass_kg: 0.8,
                target_surface_speed_mps: 5.0,
                friction: 1.00,
                contact_stiffness_n_per_m: 0.0,
                contact_damping_n_s_per_m: 0.0,
                max_compression_m: 0.010,
            }),
        };
        let field = FieldDefinition {
            colliders: vec![],
            anchors: std::collections::BTreeMap::new(),
            triggers: Vec::new(),
            floor_height_m: 0.0,
            boundary: FieldBoundary {
                min: [-8.0, 0.0, -8.0],
                max: [8.0, 1.0, 8.0],
            },
        };
        arena.object_count = 1;
        let mut runtime = SphereRuntime::new("test".into(), "test".into(), 0);
        runtime.create_field_arena(&arena, &field);
        runtime.set_robot_colliders(&[roller_collider]);
        runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
        if let Some(p) = runtime.players.get_mut("p1") {
            p.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        }
        runtime.set_player_input("p1", 0.0, 0.0, 1.0, 0.0, 0.0, 0);

        let touching_top = [
            0.0,
            arena.robot.height_m * 0.5 + 0.1 + arena.ball.radius_m() - 0.001,
            0.0,
        ];
        {
            let ball = &mut runtime.balls[0];
            ball.active = true;
            ball.position = touching_top;
            ball.velocity = [0.0, 0.0, 0.0];
            ball.sleeping = false;
        }

        for _ in 0..30 {
            runtime.tick(1.0 / 60.0);
        }

        let ball = &runtime.balls[0];
        assert!(
            ball.velocity[2] < -0.01,
            "idle ball must be dragged by the spinning roller via the intake preload; velocity={:?}",
            ball.velocity
        );
    }

    #[test]
    fn test_diagnostic_contact_telemetry() {
        let mut arena = arena();
        arena.gravity_scale = 0.0;
        let roller_collider = FieldCollider {
            id: "IntakeRoller".into(),
            min: [-0.25, -0.045, -0.045],
            max: [0.25, 0.045, 0.045],
            center: [0.0, 0.075, 0.38],
            half_extents: [0.25, 0.045, 0.045],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: Some(ActuatorConfig {
                input_channel: "intake".into(),
                spin_axis: [1.0, 0.0, 0.0],
                max_torque: 100.0,
                mass_kg: 0.8,
                target_surface_speed_mps: 5.0,
                friction: 1.60,
                contact_stiffness_n_per_m: 500.0,
                contact_damping_n_s_per_m: 5.0,
                max_compression_m: 0.010,
            }),
        };
        let field = FieldDefinition {
            colliders: vec![],
            anchors: std::collections::BTreeMap::new(),
            triggers: Vec::new(),
            floor_height_m: 0.0,
            boundary: FieldBoundary {
                min: [-8.0, 0.0, -8.0],
                max: [8.0, 1.0, 8.0],
            },
        };
        arena.object_count = 1;
        // The wheel is an isolated body attached to a stationary chassis. A
        // ball starts just inside the top contact plane and travels into the
        // wheel; X is the wheel's local spin axis, so its surface motion at
        // this point is along Z. This fixture has no intake/storage logic.
        let chassis_center_y = arena.robot.height_m * 0.5;
        let top_impact_position = [
            0.0,
            chassis_center_y
                + roller_collider.center[1]
                + roller_collider.half_extents[1]
                + arena.ball.radius_m()
                - 0.001,
            // `authored_robot_contact` applies the robot model's PI yaw
            // basis conversion, so this authored +Z centre is world -Z at
            // yaw zero.
            -roller_collider.center[2],
        ];

        let log_telemetry = |label: &str, t: &ContactTelemetryLog| {
            println!("=======================================================");
            println!("=== DIAGNOSTIC CONTACT LOG ({label}) ===");
            println!("ball_velocity_before: {:?}", t.ball_velocity_before);
            println!("wheel_linear_velocity: {:?}", t.wheel_linear_velocity);
            println!(
                "wheel_angular_velocity: {:.2} rad/s",
                t.wheel_angular_velocity
            );
            println!("wheel_local_spin_axis: {:?}", t.wheel_local_spin_axis);
            println!("wheel_world_spin_axis: {:?}", t.wheel_world_spin_axis);
            println!("contact_point: {:?}", t.contact_point);
            println!("r = contact_point - wheel_center: {:?}", t.r_arm);
            println!("wheel_surface_velocity: {:?}", t.wheel_surface_velocity);
            println!("ball_velocity_at_contact: {:?}", t.ball_velocity_at_contact);
            println!(
                "relative_contact_velocity: {:?}",
                t.relative_contact_velocity
            );
            println!(
                "normal_relative_velocity: {:.4} m/s",
                t.normal_relative_velocity
            );
            println!(
                "tangential_relative_velocity: {:.4} m/s",
                t.tangential_relative_velocity
            );
            println!("normal_impulse: {:.4} Ns", t.normal_impulse);
            println!("compliant_deformation: {:.4} m", t.compliant_deformation_m);
            println!(
                "compliant_normal_force: {:.4} N",
                t.compliant_normal_force_n
            );
            println!("friction_impulse: {:?}", t.friction_impulse);
            println!("friction_coefficient: {:.2}", t.friction_coefficient);
            println!("friction_limit: {:.4} Ns", t.friction_limit);
            println!("wheel_reaction_torque: {:.4} Nm", t.wheel_reaction_torque);
            println!("ball_velocity_after: {:?}", t.ball_velocity_after);
            println!("=======================================================");
        };

        // --- TEST A: Wheel stationary ---
        {
            let mut runtime = SphereRuntime::new("testA".into(), "test".into(), 0);
            runtime.create_field_arena(&arena, &field);
            runtime.set_robot_colliders(&[roller_collider.clone()]);
            runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
            if let Some(p) = runtime.players.get_mut("p1") {
                p.position = [0.0, chassis_center_y, 0.0];
                p.yaw = 0.0;
            }
            runtime.set_player_input("p1", 0.0, 0.0, 0.0, 0.0, 0.0, 0); // motor OFF
            let ball = &mut runtime.balls[0];
            ball.active = true;
            ball.released = true;
            ball.position = top_impact_position;
            ball.velocity = [0.0, -1.0, 0.0];

            runtime.tick(1.0 / 60.0);
            let t = runtime
                .last_contact_telemetry
                .as_ref()
                .expect("Telemetry recorded for Test A");
            log_telemetry("TEST A: STATIONARY WHEEL", t);
            assert_eq!(
                t.wheel_angular_velocity, 0.0,
                "Test A wheel must be stationary"
            );
            assert_eq!(
                t.wheel_surface_velocity,
                [0.0, 0.0, 0.0],
                "Stationary wheel surface velocity must be zero"
            );
        }

        // --- TEST B: Wheel spinning forward ---
        let test_b_ball_vel_z;
        {
            let mut runtime = SphereRuntime::new("testB".into(), "test".into(), 0);
            runtime.create_field_arena(&arena, &field);
            runtime.set_robot_colliders(&[roller_collider.clone()]);
            runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
            if let Some(p) = runtime.players.get_mut("p1") {
                p.position = [0.0, chassis_center_y, 0.0];
                p.yaw = 0.0;
            }
            runtime.set_player_input("p1", 0.0, 0.0, 1.0, 0.0, 0.0, 0); // motor ON (+1.0)
            let ball = &mut runtime.balls[0];
            ball.active = true;
            ball.released = true;
            ball.position = top_impact_position;
            ball.velocity = [0.0, -1.0, 0.0];

            runtime.tick(1.0 / 60.0);
            let t = runtime
                .last_contact_telemetry
                .as_ref()
                .expect("Telemetry recorded for Test B");
            log_telemetry("TEST B: SPINNING FORWARD", t);
            assert!(
                t.wheel_angular_velocity.abs() > 0.0,
                "Test B wheel must spin"
            );
            assert!(
                t.wheel_surface_velocity[2].abs() > 0.0,
                "Test B wheel surface velocity must be non-zero"
            );
            assert!(
                t.compliant_deformation_m > 0.0,
                "Powered wheel contact must compress its compliant tread"
            );
            assert!(
                t.compliant_deformation_m <= 0.010,
                "Tread compression must respect its configured maximum"
            );
            assert!(
                t.compliant_normal_force_n > 0.0,
                "Compressed tread must produce a normal force"
            );
            test_b_ball_vel_z = t.ball_velocity_after[2];
        }

        // --- TEST C: Wheel spinning opposite ---
        {
            let mut reverse_roller = roller_collider.clone();
            reverse_roller
                .actuator
                .as_mut()
                .expect("test wheel has an actuator")
                .target_surface_speed_mps = -5.0;
            let mut runtime = SphereRuntime::new("testC".into(), "test".into(), 0);
            runtime.create_field_arena(&arena, &field);
            runtime.set_robot_colliders(&[reverse_roller]);
            runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
            if let Some(p) = runtime.players.get_mut("p1") {
                p.position = [0.0, chassis_center_y, 0.0];
                p.yaw = 0.0;
            }
            runtime.set_player_input("p1", 0.0, 0.0, 1.0, 0.0, 0.0, 0);
            let ball = &mut runtime.balls[0];
            ball.active = true;
            ball.released = true;
            ball.position = top_impact_position;
            ball.velocity = [0.0, -1.0, 0.0];

            runtime.tick(1.0 / 60.0);
            let t = runtime
                .last_contact_telemetry
                .as_ref()
                .expect("Telemetry recorded for Test C");
            log_telemetry("TEST C: SPINNING REVERSE", t);
            let test_c_ball_vel_z = t.ball_velocity_after[2];

            assert!(
                (test_b_ball_vel_z - test_c_ball_vel_z).abs() > 1.0e-4,
                "Reversing wheel direction MUST reverse/change frictional acceleration on the ball"
            );
            assert!(
                test_b_ball_vel_z.signum() != test_c_ball_vel_z.signum(),
                "Tangential ball acceleration direction MUST flip when wheel rotation direction is reversed"
            );
        }

        // --- TEST D: High friction, low restitution ---
        {
            let mut runtime = SphereRuntime::new("testD".into(), "test".into(), 0);
            runtime.create_field_arena(&arena, &field);
            runtime.set_robot_colliders(&[roller_collider.clone()]);
            runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
            if let Some(p) = runtime.players.get_mut("p1") {
                p.position = [0.0, chassis_center_y, 0.0];
                p.yaw = 0.0;
            }
            runtime.set_player_input("p1", 0.0, 0.0, 1.0, 0.0, 0.0, 0);
            let ball = &mut runtime.balls[0];
            ball.active = true;
            ball.released = true;
            ball.position = top_impact_position;
            ball.velocity = [0.0, -0.5, 0.0];

            runtime.tick(1.0 / 60.0);
            let t = runtime
                .last_contact_telemetry
                .as_ref()
                .expect("Telemetry recorded for Test D");
            log_telemetry("TEST D: HIGH FRICTION, LOW RESTITUTION", t);
            assert!(
                t.friction_coefficient >= 1.0,
                "High friction coefficient should be applied"
            );
            assert!(
                t.normal_impulse >= 0.0,
                "Normal impulse must be non-negative"
            );
            assert!(t.friction_limit > 0.0, "Friction limit must be non-zero");
        }
    }

    #[test]
    fn test_negative_roller_off_no_movement() {
        let mut arena = arena();
        arena.gravity_scale = 0.0; // no gravity, no input
        let roller_collider = FieldCollider {
            id: "Roller1".into(),
            min: [-0.5, -0.1, -0.1],
            max: [0.5, 0.1, 0.1],
            center: [0.0, 0.0, 0.0],
            half_extents: [0.5, 0.1, 0.1],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: Some(ActuatorConfig {
                input_channel: "intake".into(),
                spin_axis: [1.0, 0.0, 0.0],
                max_torque: 15.0,
                mass_kg: 0.8,
                target_surface_speed_mps: 5.0,
                friction: 0.90,
                contact_stiffness_n_per_m: 500.0,
                contact_damping_n_s_per_m: 5.0,
                max_compression_m: 0.010,
            }),
        };
        let field = FieldDefinition {
            colliders: vec![],
            anchors: std::collections::BTreeMap::new(),
            triggers: Vec::new(),
            floor_height_m: 0.0,
            boundary: FieldBoundary {
                min: [-8.0, 0.0, -8.0],
                max: [8.0, 1.0, 8.0],
            },
        };
        arena.object_count = 1;
        let mut runtime = SphereRuntime::new("test".into(), "test".into(), 0);
        runtime.create_field_arena(&arena, &field);
        runtime.set_robot_colliders(&[roller_collider]);
        runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
        if let Some(p) = runtime.players.get_mut("p1") {
            p.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        }
        // Motor power = 0
        runtime.set_player_input("p1", 0.0, 0.0, 0.0, 0.0, 0.0, 0);

        let initial_pos = [
            0.0,
            arena.robot.height_m * 0.5 + 0.1 + arena.ball.radius_m(),
            0.0,
        ];
        {
            let ball = &mut runtime.balls[0];
            ball.active = true;
            ball.position = initial_pos;
            ball.velocity = [0.0, 0.0, 0.0];
            ball.sleeping = false;
        }

        for _ in 0..10 {
            runtime.tick(1.0 / 60.0);
        }

        let ball = &runtime.balls[0];
        assert_eq!(
            ball.velocity,
            [0.0, 0.0, 0.0],
            "Roller OFF must NOT move the ball"
        );
        assert_eq!(
            ball.position, initial_pos,
            "Roller OFF must NOT change ball position"
        );
    }

    #[test]
    fn test_negative_roller_on_too_far() {
        let mut arena = arena();
        arena.gravity_scale = 0.0;
        let roller_collider = FieldCollider {
            id: "Roller1".into(),
            min: [-0.5, -0.1, -0.1],
            max: [0.5, 0.1, 0.1],
            center: [0.0, 0.0, 0.0],
            half_extents: [0.5, 0.1, 0.1],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: Some(ActuatorConfig {
                input_channel: "intake".into(),
                spin_axis: [1.0, 0.0, 0.0],
                max_torque: 15.0,
                mass_kg: 0.8,
                target_surface_speed_mps: 5.0,
                friction: 0.90,
                contact_stiffness_n_per_m: 500.0,
                contact_damping_n_s_per_m: 5.0,
                max_compression_m: 0.010,
            }),
        };
        let field = FieldDefinition {
            colliders: vec![],
            anchors: std::collections::BTreeMap::new(),
            triggers: Vec::new(),
            floor_height_m: 0.0,
            boundary: FieldBoundary {
                min: [-8.0, 0.0, -8.0],
                max: [8.0, 1.0, 8.0],
            },
        };
        arena.object_count = 1;
        let mut runtime = SphereRuntime::new("test".into(), "test".into(), 0);
        runtime.create_field_arena(&arena, &field);
        runtime.set_robot_colliders(&[roller_collider]);
        runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
        if let Some(p) = runtime.players.get_mut("p1") {
            p.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        }
        // Motor ON
        runtime.set_player_input("p1", 0.0, 0.0, 1.0, 0.0, 0.0, 0);

        // Place ball 1 meter away from roller (no physical contact)
        let far_pos = [0.0, arena.robot.height_m * 0.5 + 1.0, 0.0];
        {
            let ball = &mut runtime.balls[0];
            ball.active = true;
            ball.position = far_pos;
            ball.velocity = [0.0, 0.0, 0.0];
            ball.sleeping = false;
        }

        for _ in 0..10 {
            runtime.tick(1.0 / 60.0);
        }

        let ball = &runtime.balls[0];
        assert_eq!(
            ball.velocity,
            [0.0, 0.0, 0.0],
            "Non-contact ball must NOT be attracted or moved by spinning roller"
        );
        assert_eq!(
            ball.position, far_pos,
            "Non-contact ball position must remain unchanged"
        );
    }

    #[test]
    fn test_negative_insufficient_friction() {
        let mut arena = arena();
        arena.gravity_scale = 0.0;
        let roller_collider = FieldCollider {
            id: "ZeroFrictionRoller".into(),
            min: [-0.5, -0.1, -0.1],
            max: [0.5, 0.1, 0.1],
            center: [0.0, 0.0, 0.0],
            half_extents: [0.5, 0.1, 0.1],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: Some(ActuatorConfig {
                input_channel: "intake".into(),
                spin_axis: [1.0, 0.0, 0.0],
                max_torque: 15.0,
                mass_kg: 0.8,
                target_surface_speed_mps: 5.0,
                friction: 0.00, // ZERO FRICTION
                contact_stiffness_n_per_m: 500.0,
                contact_damping_n_s_per_m: 5.0,
                max_compression_m: 0.010,
            }),
        };
        let field = FieldDefinition {
            colliders: vec![],
            anchors: std::collections::BTreeMap::new(),
            triggers: Vec::new(),
            floor_height_m: 0.0,
            boundary: FieldBoundary {
                min: [-8.0, 0.0, -8.0],
                max: [8.0, 1.0, 8.0],
            },
        };
        arena.object_count = 1;
        let mut runtime = SphereRuntime::new("test".into(), "test".into(), 0);
        runtime.create_field_arena(&arena, &field);
        runtime.set_robot_colliders(&[roller_collider]);
        runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
        if let Some(p) = runtime.players.get_mut("p1") {
            p.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        }
        runtime.set_player_input("p1", 0.0, 0.0, 1.0, 0.0, 0.0, 0);

        let initial_pos = [
            0.0,
            arena.robot.height_m * 0.5 + 0.1 + arena.ball.radius_m() - 0.001,
            0.0,
        ];
        {
            let ball = &mut runtime.balls[0];
            ball.active = true;
            ball.position = initial_pos;
            ball.velocity = [0.0, 0.0, 0.0];
            ball.sleeping = false;
        }

        for _ in 0..10 {
            runtime.tick(1.0 / 60.0);
        }

        let ball = &runtime.balls[0];
        assert_eq!(
            ball.velocity[2], 0.0,
            "Zero friction roller must NOT transfer tangential momentum"
        );
    }

    #[test]
    fn test_negative_physical_obstruction() {
        let mut arena = arena();
        arena.gravity_scale = 0.0;
        let roller_collider = FieldCollider {
            id: "Roller1".into(),
            min: [-0.5, -0.1, -0.1],
            max: [0.5, 0.1, 0.1],
            center: [0.0, 0.0, 0.0],
            half_extents: [0.5, 0.1, 0.1],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: Some(ActuatorConfig {
                input_channel: "intake".into(),
                spin_axis: [1.0, 0.0, 0.0],
                max_torque: 15.0,
                mass_kg: 0.8,
                target_surface_speed_mps: 5.0,
                friction: 0.90,
                contact_stiffness_n_per_m: 500.0,
                contact_damping_n_s_per_m: 5.0,
                max_compression_m: 0.010,
            }),
        };
        let barrier = FieldCollider {
            id: "Barrier".into(),
            min: [-0.5, -0.5, -0.3],
            max: [0.5, 0.5, -0.1],
            center: [0.0, 0.0, -0.2],
            half_extents: [0.5, 0.5, 0.1],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: None,
        };
        let field = FieldDefinition {
            colliders: vec![barrier],
            anchors: std::collections::BTreeMap::new(),
            triggers: Vec::new(),
            floor_height_m: 0.0,
            boundary: FieldBoundary {
                min: [-8.0, 0.0, -8.0],
                max: [8.0, 1.0, 8.0],
            },
        };
        arena.object_count = 1;
        let mut runtime = SphereRuntime::new("test".into(), "test".into(), 0);
        runtime.create_field_arena(&arena, &field);
        runtime.set_robot_colliders(&[roller_collider]);
        runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
        if let Some(p) = runtime.players.get_mut("p1") {
            p.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        }
        runtime.set_player_input("p1", 0.0, 0.0, 1.0, 0.0, 0.0, 0);

        let initial_pos = [0.0, arena.robot.height_m * 0.5, -0.05];
        {
            let ball = &mut runtime.balls[0];
            ball.active = true;
            ball.position = initial_pos;
            ball.velocity = [0.0, 0.0, 0.0];
            ball.sleeping = false;
        }

        for _ in 0..10 {
            runtime.tick(1.0 / 60.0);
        }

        let ball = &runtime.balls[0];
        assert!(
            ball.position[2] >= -0.25,
            "Ball must remain physically obstructed by barrier"
        );
    }

    #[test]
    fn test_negative_ball_contacts_frame_instead_of_roller() {
        let mut arena = arena();
        arena.gravity_scale = 0.0;
        let frame_collider = FieldCollider {
            id: "RobotFrame".into(),
            min: [-0.5, -0.1, -0.1],
            max: [0.5, 0.1, 0.1],
            center: [0.0, 0.0, 0.0],
            half_extents: [0.5, 0.1, 0.1],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: None, // NO ACTUATOR (static frame part)
        };
        let field = FieldDefinition {
            colliders: vec![],
            anchors: std::collections::BTreeMap::new(),
            triggers: Vec::new(),
            floor_height_m: 0.0,
            boundary: FieldBoundary {
                min: [-8.0, 0.0, -8.0],
                max: [8.0, 1.0, 8.0],
            },
        };
        arena.object_count = 1;
        let mut runtime = SphereRuntime::new("test".into(), "test".into(), 0);
        runtime.create_field_arena(&arena, &field);
        runtime.set_robot_colliders(&[frame_collider]);
        runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
        if let Some(p) = runtime.players.get_mut("p1") {
            p.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        }

        let initial_pos = [
            0.0,
            arena.robot.height_m * 0.5 + 0.1 + arena.ball.radius_m() - 0.001,
            0.0,
        ];
        {
            let ball = &mut runtime.balls[0];
            ball.active = true;
            ball.position = initial_pos;
            ball.velocity = [0.0, 0.0, 0.0];
            ball.sleeping = false;
        }

        for _ in 0..10 {
            runtime.tick(1.0 / 60.0);
        }

        let ball = &runtime.balls[0];
        assert_eq!(
            ball.velocity,
            [0.0, 0.0, 0.0],
            "Static frame contact must produce normal collision response without roller drive"
        );
    }

    #[test]
    fn test_positive_spinning_roller_friction_momentum_transfer() {
        let mut arena = arena();
        arena.gravity_scale = 1.0;
        let roller_collider = FieldCollider {
            id: "Roller1".into(),
            min: [-0.5, -0.1, -0.1],
            max: [0.5, 0.1, 0.1],
            center: [0.0, 0.0, 0.0],
            half_extents: [0.5, 0.1, 0.1],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: Some(ActuatorConfig {
                input_channel: "intake".into(),
                spin_axis: [1.0, 0.0, 0.0],
                max_torque: 15.0,
                mass_kg: 0.8,
                target_surface_speed_mps: 5.0,
                friction: 0.90,
                contact_stiffness_n_per_m: 500.0,
                contact_damping_n_s_per_m: 5.0,
                max_compression_m: 0.010,
            }),
        };
        let field = FieldDefinition {
            colliders: vec![],
            anchors: std::collections::BTreeMap::new(),
            triggers: Vec::new(),
            floor_height_m: 0.0,
            boundary: FieldBoundary {
                min: [-8.0, 0.0, -8.0],
                max: [8.0, 1.0, 8.0],
            },
        };
        arena.object_count = 1;
        let mut runtime = SphereRuntime::new("test".into(), "test".into(), 0);
        runtime.create_field_arena(&arena, &field);
        runtime.set_robot_colliders(&[roller_collider]);
        runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
        if let Some(p) = runtime.players.get_mut("p1") {
            p.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        }
        runtime.set_player_input("p1", 0.0, 0.0, 1.0, 0.0, 0.0, 0);

        let initial_pos = [
            0.0,
            arena.robot.height_m * 0.5 + 0.1 + arena.ball.radius_m() - 0.001,
            0.0,
        ];
        {
            let ball = &mut runtime.balls[0];
            ball.active = true;
            ball.position = initial_pos;
            ball.velocity = [0.0, 0.0, 0.0];
            ball.sleeping = false;
        }

        for _ in 0..10 {
            runtime.tick(1.0 / 60.0);
        }

        let ball = &runtime.balls[0];
        assert!(
            ball.velocity[2] < 0.0,
            "Spinning roller contact must transfer momentum to the ball"
        );
    }

    #[test]
    fn test_negative_disabled_roller() {
        let mut arena = arena();
        arena.gravity_scale = 0.0;
        let roller_collider = FieldCollider {
            id: "Roller1".into(),
            min: [-0.5, -0.1, -0.1],
            max: [0.5, 0.1, 0.1],
            center: [0.0, 0.0, 0.0],
            half_extents: [0.5, 0.1, 0.1],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: None, // Disabled actuator
        };
        let field = FieldDefinition {
            colliders: vec![],
            anchors: std::collections::BTreeMap::new(),
            triggers: Vec::new(),
            floor_height_m: 0.0,
            boundary: FieldBoundary {
                min: [-8.0, 0.0, -8.0],
                max: [8.0, 1.0, 8.0],
            },
        };
        arena.object_count = 1;
        let mut runtime = SphereRuntime::new("test".into(), "test".into(), 0);
        runtime.create_field_arena(&arena, &field);
        runtime.set_robot_colliders(&[roller_collider]);
        runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
        if let Some(p) = runtime.players.get_mut("p1") {
            p.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        }
        runtime.set_player_input("p1", 0.0, 0.0, 1.0, 0.0, 0.0, 0);

        let initial_pos = [
            0.0,
            arena.robot.height_m * 0.5 + 0.1 + arena.ball.radius_m() - 0.001,
            0.0,
        ];
        {
            let ball = &mut runtime.balls[0];
            ball.active = true;
            ball.position = initial_pos;
            ball.velocity = [0.0, 0.0, 0.0];
            ball.sleeping = false;
        }

        for _ in 0..10 {
            runtime.tick(1.0 / 60.0);
        }

        let ball = &runtime.balls[0];
        assert_eq!(
            ball.velocity,
            [0.0, 0.0, 0.0],
            "Disabled roller must not exert any active roller force"
        );
    }

    #[test]
    fn test_negative_no_proximity_threshold() {
        let mut arena = arena();
        arena.gravity_scale = 0.0;
        let roller_collider = FieldCollider {
            id: "Roller1".into(),
            min: [-0.5, -0.1, -0.1],
            max: [0.5, 0.1, 0.1],
            center: [0.0, 0.0, 0.0],
            half_extents: [0.5, 0.1, 0.1],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: Some(ActuatorConfig {
                input_channel: "intake".into(),
                spin_axis: [1.0, 0.0, 0.0],
                max_torque: 15.0,
                mass_kg: 0.8,
                target_surface_speed_mps: 5.0,
                friction: 0.90,
                contact_stiffness_n_per_m: 500.0,
                contact_damping_n_s_per_m: 5.0,
                max_compression_m: 0.010,
            }),
        };
        let field = FieldDefinition {
            colliders: vec![],
            anchors: std::collections::BTreeMap::new(),
            triggers: Vec::new(),
            floor_height_m: 0.0,
            boundary: FieldBoundary {
                min: [-8.0, 0.0, -8.0],
                max: [8.0, 1.0, 8.0],
            },
        };
        arena.object_count = 1;
        let mut runtime = SphereRuntime::new("test".into(), "test".into(), 0);
        runtime.create_field_arena(&arena, &field);
        runtime.set_robot_colliders(&[roller_collider]);
        runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
        if let Some(p) = runtime.players.get_mut("p1") {
            p.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        }
        runtime.set_player_input("p1", 0.0, 0.0, 1.0, 0.0, 0.0, 0);

        // Test multiple proximity distances outside physical contact
        for dist in [0.01, 0.05, 0.10, 0.50] {
            let pos = [
                0.0,
                arena.robot.height_m * 0.5 + 0.1 + arena.ball.radius_m() + dist,
                0.0,
            ];
            {
                let ball = &mut runtime.balls[0];
                ball.active = true;
                ball.position = pos;
                ball.velocity = [0.0, 0.0, 0.0];
                ball.sleeping = false;
            }
            runtime.tick(1.0 / 60.0);
            let ball = &runtime.balls[0];
            assert_eq!(
                ball.velocity,
                [0.0, 0.0, 0.0],
                "Proximity dist={dist} must NOT cause ball transfer or movement"
            );
            assert_eq!(
                ball.position, pos,
                "Proximity dist={dist} must NOT change ball position"
            );
        }
    }

    #[test]
    fn test_negative_roller_reverse_direction() {
        let mut arena = arena();
        arena.gravity_scale = 1.0;
        let roller_collider = FieldCollider {
            id: "Roller1".into(),
            min: [-0.5, -0.1, -0.1],
            max: [0.5, 0.1, 0.1],
            center: [0.0, 0.0, 0.0],
            half_extents: [0.5, 0.1, 0.1],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: Some(ActuatorConfig {
                input_channel: "outtake".into(),
                spin_axis: [1.0, 0.0, 0.0],
                max_torque: 15.0,
                mass_kg: 0.8,
                target_surface_speed_mps: -5.0,
                friction: 0.90,
                contact_stiffness_n_per_m: 500.0,
                contact_damping_n_s_per_m: 5.0,
                max_compression_m: 0.010,
            }),
        };
        let field = FieldDefinition {
            colliders: vec![],
            anchors: std::collections::BTreeMap::new(),
            triggers: Vec::new(),
            floor_height_m: 0.0,
            boundary: FieldBoundary {
                min: [-8.0, 0.0, -8.0],
                max: [8.0, 1.0, 8.0],
            },
        };
        arena.object_count = 1;
        let mut runtime = SphereRuntime::new("test".into(), "test".into(), 0);
        runtime.create_field_arena(&arena, &field);
        runtime.set_robot_colliders(&[roller_collider]);
        runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
        if let Some(p) = runtime.players.get_mut("p1") {
            p.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        }
        // Outtake power (reverse spin)
        runtime.set_player_input("p1", 0.0, 0.0, 0.0, 1.0, 0.0, 0);

        let initial_pos = [
            0.0,
            arena.robot.height_m * 0.5 + 0.1 + arena.ball.radius_m() - 0.001,
            0.0,
        ];
        {
            let ball = &mut runtime.balls[0];
            ball.active = true;
            ball.position = initial_pos;
            ball.velocity = [0.0, 0.0, 0.0];
            ball.sleeping = false;
        }

        for tick in 0..10 {
            runtime.tick(1.0 / 60.0);
            let b = &runtime.balls[0];
            let player = &runtime.players["p1"];
            let roller = &player.rollers["Roller1"];
            eprintln!(
                "[REVERSE TEST] tick={tick} pos={:?} vel={:?} roller_w={:.2} outtake_power={:.2}",
                b.position, b.velocity, roller.angular_velocity, player.outtake_power,
            );
        }

        let ball = &runtime.balls[0];
        assert!(
            ball.velocity[2] > 0.0,
            "Reversing roller spin direction must produce reversed tangential ball acceleration: velocity={:?}",
            ball.velocity
        );
    }

    #[test]
    fn test_full_robot_physical_intake() {
        let mut arena = arena();
        arena.gravity_scale = 1.0;
        let roller_collider = FieldCollider {
            id: "IntakeRoller".into(),
            min: [-0.3, 0.05, -0.3],
            max: [0.3, 0.15, -0.2],
            center: [0.0, 0.1, -0.25],
            half_extents: [0.3, 0.05, 0.05],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: Some(ActuatorConfig {
                input_channel: "intake".into(),
                spin_axis: [1.0, 0.0, 0.0],
                max_torque: 20.0,
                mass_kg: 1.0,
                target_surface_speed_mps: 5.0,
                friction: 0.95,
                contact_stiffness_n_per_m: 500.0,
                contact_damping_n_s_per_m: 5.0,
                max_compression_m: 0.010,
            }),
        };
        let field = FieldDefinition {
            colliders: vec![],
            anchors: std::collections::BTreeMap::new(),
            triggers: Vec::new(),
            floor_height_m: 0.0,
            boundary: FieldBoundary {
                min: [-8.0, 0.0, -8.0],
                max: [8.0, 1.0, 8.0],
            },
        };
        arena.object_count = 1;
        let mut runtime = SphereRuntime::new("test".into(), "test".into(), 0);
        runtime.create_field_arena(&arena, &field);
        runtime.set_robot_colliders(&[roller_collider]);
        runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
        if let Some(p) = runtime.players.get_mut("p1") {
            p.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        }
        runtime.set_player_input("p1", 0.0, 0.0, 1.0, 0.0, 0.0, 0);

        let initial_pos = [
            0.0,
            arena.robot.height_m * 0.5 + 0.10 + arena.ball.radius_m() - 0.001,
            0.25,
        ];
        {
            let ball = &mut runtime.balls[0];
            ball.active = true;
            ball.position = initial_pos;
            ball.velocity = [0.0, 0.0, 0.0];
            ball.sleeping = false;
        }

        for tick in 0..15 {
            runtime.tick(1.0 / 60.0);
            let b = &runtime.balls[0];
            eprintln!(
                "[FULL INTAKE TEST] tick={tick} pos={:?} vel={:?}",
                b.position, b.velocity
            );
        }

        let ball = &runtime.balls[0];
        assert!(ball.active, "Ball stays active during physical intake");
        assert!(
            ball.velocity[2] < 0.0,
            "Ball must be physically driven into the robot geometry: vel={:?}",
            ball.velocity
        );
    }

    struct IntakeHarness {
        runtime: SphereRuntime,
        arena: ArenaConfig,
        geometry: RobotIntakeGeometry,
    }

    impl IntakeHarness {
        fn new(match_name: &str, object_count: usize, force_mps2: f32) -> Self {
            let mut arena = arena();
            arena.object_count = object_count;
            arena.ramp.enabled = false;
            arena.robot.intake_force_mps2 = force_mps2;
            let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
                .load_pack("../pkgs/games/fgc-2026/manifest.json")
                .unwrap();
            let geometry = pack
                .semantic_zones
                .iter()
                .find(|zone| zone.kind == SemanticZoneKind::Intake)
                .expect("bot.semantics.json must ship an IntakeZone")
                .geometry
                .clone();
            let mut runtime = SphereRuntime::new(match_name.into(), "fgc-2026".into(), 0);
            runtime.create_test_arena(&arena);
            runtime.set_semantic_zones(pack.semantic_zones.clone());
            runtime.set_robot_colliders(&pack.robot_colliders);
            runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
            {
                let player = runtime.players.get_mut("p").unwrap();
                player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
                player.yaw = 0.0;
                player.intake_power = 1.0;
            }
            Self { runtime, arena, geometry }
        }

        /// World-space mouth centre for the player at its current pose.
        fn mouth(&self) -> Vec3 {
            let player = &self.runtime.players["p"];
            // yaw=0; mirrors active_intake_zones() exactly.
            let (sin, cos) = (player.yaw + std::f32::consts::PI).sin_cos();
            let rotate = |v: Vec3| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]];
            add(player.position, rotate(self.geometry.zone_center))
        }

        fn feed_direction(&self) -> Vec3 {
            let player = &self.runtime.players["p"];
            let (sin, cos) = (player.yaw + std::f32::consts::PI).sin_cos();
            let rotate = |v: Vec3| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]];
            rotate(self.geometry.direction)
        }

        /// Place `count` balls in a line along the feed axis, the first
        /// touching the mouth centre, each a full diameter behind the last.
        fn place_balls(&mut self, count: usize) {
            let mouth = self.mouth();
            let direction = self.feed_direction();
            let radius = self.arena.ball.radius_m();
            for (i, ball) in self.runtime.balls.iter_mut().enumerate() {
                ball.active = true;
                ball.position = [
                    mouth[0] + direction[0] * (i as f32 * 2.0 * radius),
                    radius,
                    mouth[2] + direction[2] * (i as f32 * 2.0 * radius),
                ];
                ball.velocity = [0.0; 3];
                ball.pre_solve_velocity = [0.0; 3];
                ball.sleeping = false;
            }
        }

        fn run_ticks(&mut self, ticks: usize) {
            for _ in 0..ticks {
                self.runtime.tick(1.0 / 60.0);
            }
        }
    }

    #[test]
    #[ignore = "manual tuning sweep over intake_force_mps2 for the intake feed force"]
    fn tune_intake_force_sweep() {
        for force in [40.0, 80.0, 120.0, 160.0, 200.0, 300.0] {
            let mut single = IntakeHarness::new("tune-single", 1, force);
            single.place_balls(1);
            let mut max_speed_single = 0.0f32;
            for _ in 0..180 {
                single.runtime.tick(1.0 / 60.0);
                max_speed_single =
                    max_speed_single.max(length_sq(single.runtime.balls[0].velocity).sqrt());
            }
            let single_z = single.runtime.balls[0].position[2];
            let single_y = single.runtime.balls[0].position[1];

            let mut pair = IntakeHarness::new("tune-pair", 2, force);
            pair.place_balls(2);
            pair.run_ticks(180);
            let front_z = pair.runtime.balls[0].position[2];

            let mut blocked = IntakeHarness::new("tune-blocked", 1, force);
            let blocked_wall = FieldCollider {
                id: "intake-block-wall".into(),
                min: [-0.30, 0.0, -0.05],
                max: [0.30, 0.4, 0.05],
                center: [0.0, 0.2, 0.0],
                half_extents: [0.30, 0.2, 0.05],
                axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                actuator: None,
            };
            blocked.runtime.robot_colliders.push(blocked_wall);
            let colliders = blocked.runtime.robot_colliders.clone();
            blocked.runtime.sync_rollers(&colliders);
            blocked.place_balls(1);
            blocked.run_ticks(180);
            let blocked_z = blocked.runtime.balls[0].position[2];
            let blocked_max_y = blocked
                .runtime
                .balls
                .iter()
                .map(|b| b.position[1])
                .fold(blocked.arena.ball.radius_m(), f32::max);

            let mut queue = IntakeHarness::new("tune-queue", 4, force);
            queue.place_balls(4);
            let mut queue_max_y = 0.0f32;
            for _ in 0..300 {
                queue.runtime.tick(1.0 / 60.0);
                for ball in &queue.runtime.balls {
                    queue_max_y = queue_max_y.max(ball.position[1]);
                }
            }
            let queue_front_z = queue.runtime.balls[0].position[2];
            let queue_min_sep = {
                let mut min_sep = f32::INFINITY;
                for i in 0..queue.runtime.balls.len() {
                    for j in (i + 1)..queue.runtime.balls.len() {
                        min_sep = min_sep.min(length_sq(
                            sub(
                                queue.runtime.balls[i].position,
                                queue.runtime.balls[j].position,
                            ),
                        ).sqrt());
                    }
                }
                min_sep
            };

            eprintln!(
                "[TUNE] force={force:>4} single_z={single_z:+.3} single_y={single_y:+.3} \
                 single_max_v={max_speed_single:4.2} pair_front_z={front_z:+.3} \
                 blocked_z={blocked_z:+.3} blocked_max_y={blocked_max_y:.3} \
                 queue_front_z={queue_front_z:+.3} queue_max_y={queue_max_y:.3} \
                 queue_min_sep={queue_min_sep:.3}"
            );
        }
    }

    #[test]
    fn single_ball_reliably_feeds_into_the_intake() {
        let mut harness = IntakeHarness::new("single-feed", 1, 120.0);
        harness.place_balls(1);
        let start_z = harness.runtime.balls[0].position[2];
        let mut max_speed = 0.0f32;
        let mut max_y = 0.0f32;
        for _ in 0..180 {
            harness.runtime.tick(1.0 / 60.0);
            max_speed = max_speed.max(length_sq(harness.runtime.balls[0].velocity).sqrt());
            max_y = max_y.max(harness.runtime.balls[0].position[1]);
        }
        let ball = &harness.runtime.balls[0];
        let radius = harness.arena.ball.radius_m();
        eprintln!(
            "[SINGLE FEED] start_z={start_z:.3} final={:?} max_speed={max_speed:.3} \
             max_y={max_y:.3}",
            ball.position
        );
        // A single ball must be carried a full diameter into the robot body,
        // not merely nudged a centimetre and left wedged against the mouth.
        assert!(
            ball.position[2] > start_z + harness.arena.ball.diameter_m,
            "single ball only advanced {} m: {:?}",
            ball.position[2] - start_z,
            ball.position
        );
        assert!(ball.active, "the ball must not be removed");
        // A captured ball may ride the physical intake roller as it enters the
        // chute, but it must settle low inside the robot and never be launched
        // up over the mechanism.
        assert!(
            ball.position[1] < radius + 0.15,
            "ball did not settle in the chute: {:?}",
            ball.position
        );
        assert!(
            max_y < radius + 0.30,
            "single ball was launched up to y={max_y:.3} (radius={radius:.3})"
        );
    }

    #[test]
    fn rear_ball_pushes_the_front_ball_inward() {
        let mut harness = IntakeHarness::new("pair-push", 2, 120.0);
        harness.place_balls(2);
        let front_start = harness.runtime.balls[0].position[2];
        let rear_start = harness.runtime.balls[1].position[2];
        harness.run_ticks(180);
        let front = &harness.runtime.balls[0];
        let rear = &harness.runtime.balls[1];
        let radius = harness.arena.ball.radius_m();
        eprintln!(
            "[PAIR PUSH] front {:?} -> {:?}, rear {:?} -> {:?}",
            front_start, front.position, rear_start, rear.position
        );
        // The rear ball must shove the front ball deeper into the robot.
        assert!(
            front.position[2] > front_start + 0.4 * harness.arena.ball.diameter_m,
            "front ball was not pushed inward: {:?}",
            front.position
        );
        assert!(
            front.position[2] - rear.position[2] > radius - 0.01,
            "balls pushed through each other: front={:?} rear={:?}",
            front.position,
            rear.position
        );
        assert!(front.active && rear.active, "intake must not remove balls");
    }

    #[test]
    fn blocked_ball_keeps_pushing_but_remains_physically_blocked() {
        let mut arena = arena();
        arena.object_count = 1;
        arena.ramp.enabled = false;
        arena.robot.intake_force_mps2 = 120.0;
        let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
            .load_pack("../pkgs/games/fgc-2026/manifest.json")
            .unwrap();
        let geometry = pack
            .semantic_zones
            .iter()
            .find(|zone| zone.kind == SemanticZoneKind::Intake)
            .expect("bot.semantics.json must ship an IntakeZone")
            .geometry
            .clone();
        let mut runtime = SphereRuntime::new("blocked-geometry".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        runtime.set_semantic_zones(pack.semantic_zones.clone());
        runtime.set_robot_colliders(&pack.robot_colliders);
        // A solid panel across the intake path, a quarter of a diameter inside
        // the mouth, so the conveyor presses the ball against real geometry.
        let block_wall = FieldCollider {
            id: "intake-block-wall".into(),
            min: [-0.30, 0.0, -0.02],
            max: [0.30, 0.4, 0.02],
            center: [0.0, 0.2, 0.0],
            half_extents: [0.30, 0.2, 0.02],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: None,
        };
        runtime.robot_colliders.push(block_wall);
        let colliders = runtime.robot_colliders.clone();
        runtime.sync_rollers(&colliders);
        runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
        {
            let player = runtime.players.get_mut("p").unwrap();
            player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
            player.yaw = 0.0;
            player.intake_power = 1.0;
        }
        let (sin, cos) = (std::f32::consts::PI).sin_cos();
        let rotate =
            |v: Vec3| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]];
        let mouth = add([0.0, arena.robot.height_m * 0.5, 0.0], rotate(geometry.zone_center));
        let radius = arena.ball.radius_m();
        runtime.balls[0].active = true;
        runtime.balls[0].position = [mouth[0], radius, mouth[2] + radius * 0.25];
        runtime.balls[0].velocity = [0.0; 3];
        runtime.balls[0].pre_solve_velocity = [0.0; 3];

        // The wall normal faces -z (blocks +z travel), so the ball may not
        // advance past the wall face (z=+0.02) minus its radius.
        let wall_z_limit = 0.02 - radius;
        let mut saw_pull = false;
        let mut max_y = 0.0f32;
        for _ in 0..240 {
            runtime.tick(1.0 / 60.0);
            saw_pull |= runtime.balls[0].pull_acceleration[0] != 0.0
                || runtime.balls[0].pull_acceleration[1] != 0.0
                || runtime.balls[0].pull_acceleration[2] != 0.0;
            max_y = max_y.max(runtime.balls[0].position[1]);
        }
        let ball = &runtime.balls[0];
        eprintln!(
            "[BLOCKED GEOMETRY] mouth={mouth:?} final={:?} wall_z_limit={wall_z_limit:.3} \
             max_y={max_y:.3} saw_pull={saw_pull}",
            ball.position
        );
        assert!(saw_pull, "the conveyor force must keep pushing the ball");
        assert!(
            ball.position[2] <= wall_z_limit + radius * 0.5,
            "ball tunnelled through the block wall: {:?}",
            ball.position
        );
        assert!(
            max_y < radius + 0.15,
            "blocked ball was launched upward to y={max_y:.3}",
        );
        assert!(ball.active, "the blocked ball must not be removed");
    }

    #[test]
    fn several_balls_form_a_plausible_queue() {
        let mut harness = IntakeHarness::new("queue-plausible", 4, 120.0);
        harness.place_balls(4);
        let radius = harness.arena.ball.radius_m();
        let mut max_y = 0.0f32;
        for _ in 0..300 {
            harness.runtime.tick(1.0 / 60.0);
            for ball in &harness.runtime.balls {
                max_y = max_y.max(ball.position[1]);
            }
        }
        let positions: Vec<_> = harness
            .runtime
            .balls
            .iter()
            .map(|b| b.position)
            .collect();
        eprintln!("[QUEUE] radius={radius:.3} max_lift={:.3} positions={positions:?}", max_y - radius);
        for ball in &harness.runtime.balls {
            assert!(ball.active, "intake must not remove balls");
        }
        // No overlaps: the contact solver owns spacing, never the force.
        for i in 0..harness.runtime.balls.len() {
            for j in (i + 1)..harness.runtime.balls.len() {
                let separation = length_sq(
                    sub(harness.runtime.balls[i].position, harness.runtime.balls[j].position),
                )
                .sqrt();
                assert!(
                    separation > 2.0 * radius - 0.02,
                    "ball pair ({i},{j}) overlaps at {separation:.3}m (2r={:.3})",
                    2.0 * radius
                );
            }
        }
        // No ball thrown upward like a jam ramp. A queue may climb at most a
        // couple of centimetres while rear balls ride over the train.
        assert!(
            max_y < radius + 0.08,
            "a ball was launched upward to y={max_y:.3} (radius={radius:.3})"
        );
        // The queue must actually make progress into the robot: the leading
        // ball gets pushed deeper than a full diameter past its start.
        let lead = &harness.runtime.balls[0];
        let lead_start = harness.mouth();
        let lead_start_dot = dot(sub(lead_start, lead_start), harness.feed_direction());
        let lead_now = dot(sub(lead.position, lead_start), harness.feed_direction());
        eprintln!("[QUEUE] lead_start_dot={lead_start_dot:.3} lead_now={lead_now:.3}");
        assert!(
            lead_now > harness.arena.ball.diameter_m,
            "queue lead ball did not feed into the robot (offset {lead_now:.3}m)"
        );
    }

    #[test]
    fn test_instrumented_ball_momentum_transfer() {
        let mut arena = arena();
        arena.object_count = 1;
        arena.ramp.enabled = false;
        let pack = crate::game::pack_loader::PackLoader::new("0.1.0")
            .load_pack("../pkgs/games/fgc-2026/manifest.json")
            .unwrap();
        let mut runtime = SphereRuntime::new("instrumented-test".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        runtime.set_semantic_zones(pack.semantic_zones.clone());
        runtime.set_robot_colliders(&pack.robot_colliders);
        runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);

        let player = runtime.players.get_mut("p").unwrap();
        player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
        player.yaw = 0.0;
        player.intake_power = 1.0;

        let zone = runtime.semantic_zones.iter().find(|z| z.kind == SemanticZoneKind::Intake).unwrap();
        let (sin, cos) = (player.yaw + std::f32::consts::PI).sin_cos();
        let rotate = |v: [f32; 3]| [cos * v[0] + sin * v[2], v[1], -sin * v[0] + cos * v[2]];
        let world_mouth = add(player.position, rotate(zone.geometry.zone_center));

        // Position ball at mouth of IntakeZone
        runtime.balls[0].position = [world_mouth[0], arena.ball.radius_m(), world_mouth[2]];
        runtime.balls[0].velocity = [0.0; 3];

        let dt = 1.0_f32 / 60.0_f32;
        let mut entered = false;
        let mut entry_vel = [0.0; 3];
        let mut exit_vel = [0.0; 3];
        let mut inside_ticks = 0;
        let mut inside_velocities = Vec::new();
        let mut force_after_exit = [0.0; 3];

        for _tick in 0..120 {
            let was_inside = obb_overlap_sphere(runtime.balls[0].position, arena.ball.radius_m(), &runtime.active_semantic_zones()[0].1);
            if was_inside && !entered {
                entered = true;
                entry_vel = runtime.balls[0].velocity;
            }

            runtime.tick(dt as f64);

            let ball = &runtime.balls[0];
            let active_zones = runtime.active_semantic_zones();
            let is_inside = !active_zones.is_empty() && obb_overlap_sphere(ball.position, arena.ball.radius_m(), &active_zones[0].1);

            if is_inside {
                inside_ticks += 1;
                inside_velocities.push(length_sq(ball.velocity).sqrt());
            } else if entered && exit_vel == [0.0; 3] {
                exit_vel = ball.velocity;
                force_after_exit = ball.pull_acceleration;
            }
        }

        let entry_speed = length_sq(entry_vel).sqrt();
        let exit_speed = length_sq(exit_vel).sqrt();
        let duration_secs = inside_ticks as f32 * dt;

        eprintln!("=== DIAGNOSTIC INSTRUMENTATION REPORT ===");
        eprintln!("* Force/acceleration applied: 120.0 m/s^2 along intake feed axis");
        eprintln!("* Ball velocity when entering zone: {entry_speed:.3} m/s ({entry_vel:?})");
        eprintln!("* Ball velocity while inside: {:?}", inside_velocities);
        eprintln!("* Ball velocity when leaving: {exit_speed:.3} m/s ({exit_vel:?})");
        eprintln!("* Time inside zone: {duration_secs:.3} s ({inside_ticks} ticks)");
        eprintln!("* Force active after reaching feed path / exiting zone: {force_after_exit:?}");
        eprintln!("=========================================");

        assert!(entered, "Ball must enter IntakeZone");
        assert!(exit_speed > 0.5, "Ball must exit with strong forward momentum");
        assert!(inside_ticks > 0, "Ball must spend ticks accelerating inside zone");
    }
}
