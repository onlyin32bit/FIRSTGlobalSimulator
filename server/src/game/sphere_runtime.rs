use std::collections::BTreeMap;
use std::time::Instant;

use super::match_runtime::{MatchContext, MatchPhase, PlayerSnapshot, ScoreState};
use super::pack_loader::{
    ArenaConfig, RampPhysicsConfig, RestitutionCurveConfig, RobotPhysicsConfig,
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
    sequence: u64,
    color: &'static str,
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
                phase: MatchPhase::Teleop,
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
        }
    }

    pub fn create_test_arena(&mut self, arena: &ArenaConfig) {
        self.arena = Some(arena.clone());
        self.balls.clear();
        self.balls.reserve(arena.object_count);
        self.grid_next.resize(arena.object_count, -1);
        self.grid_cells.resize(arena.object_count, [0; 3]);
        self.pairs.reserve(arena.object_count.saturating_mul(8));

        let radius = arena.ball.radius_m();
        let count = arena.object_count.max(1) as f32;
        let golden_angle = std::f32::consts::PI * (3.0 - 5.0_f32.sqrt());
        for index in 0..arena.object_count {
            let angle = index as f32 * golden_angle;
            let distance = arena.spawn_radius * ((index as f32 + 0.5) / count).sqrt();
            let position = [
                angle.cos() * distance,
                arena.spawn_height + (index % 3) as f32 * radius * 2.25,
                angle.sin() * distance,
            ];
            self.balls.push(Ball {
                position,
                velocity: [0.0; 3],
                pre_solve_velocity: [0.0; 3],
                angular_velocity: [0.0; 3],
                quiet_ticks: 0,
                sleeping: false,
                grounded: false,
                on_ramp: false,
            });
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
        let colors = [
            "#f97316", "#2563eb", "#16a34a", "#9333ea", "#dc2626", "#0891b2", "#ca8a04", "#db2777",
        ];
        self.players.insert(
            user_id,
            PlayerBody {
                name,
                team_name,
                position: [
                    angle.cos() * 4.0,
                    arena.robot.height_m * 0.5,
                    angle.sin() * 4.0,
                ],
                velocity: [0.0; 3],
                yaw: angle,
                angular_velocity_y: 0.0,
                move_x: 0.0,
                move_z: 0.0,
                intake_power: 0.0,
                sequence: 0,
                color: colors[slot % colors.len()],
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
        sequence: u64,
    ) {
        if let Some(player) = self.players.get_mut(user_id)
            && sequence >= player.sequence
        {
            player.sequence = sequence;
            player.move_x = move_x.clamp(-1.0, 1.0);
            player.move_z = move_z.clamp(-1.0, 1.0);
            player.intake_power = intake_power.clamp(0.0, 1.0);
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
            player.velocity[0] += forward[0] * forward_delta + right[0] * lateral_delta;
            player.velocity[2] += forward[2] * forward_delta + right[2] * lateral_delta;

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

    pub fn tick(&mut self, dt: f64) {
        let dt = dt as f32;
        self.context.clock += dt as f64;
        let Some(arena) = self.arena.clone() else {
            return;
        };
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
        self.update_sleeping(&arena, dt);
        self.metrics.solve_ms = solve_started.elapsed().as_secs_f64() * 1_000.0;
        self.metrics.contacts = contacts;
        self.metrics.sleeping_balls = self.balls.iter().filter(|ball| ball.sleeping).count();
        self.metrics.active_balls = self.balls.len() - self.metrics.sleeping_balls;
    }

    fn integrate(&mut self, arena: &ArenaConfig, dt: f32) {
        let linear_decay = (-arena.ball.linear_damping * dt).exp();
        let angular_decay = (-arena.ball.angular_damping * dt).exp();
        let cross_section = std::f32::consts::PI * arena.ball.radius_m().powi(2);
        let drag_acceleration_factor =
            0.5 * arena.ball.air_density_kg_m3 * arena.ball.drag_coefficient * cross_section
                / arena.ball.mass_kg.max(0.001);
        for ball in &mut self.balls {
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
        for player in self.players.values_mut() {
            // The robot is a carpet-supported planar body. Ball contacts may
            // transfer X/Z momentum and yaw, but must never integrate lift.
            player.velocity[1] = 0.0;
            player.position[0] += player.velocity[0] * dt;
            player.position[2] += player.velocity[2] * dt;
            player.position[1] = arena.robot.height_m * 0.5;
            player.yaw = wrap_angle(player.yaw + player.angular_velocity_y * dt);
            let hx = arena.robot.width_m * 0.5;
            let hz = arena.robot.length_m * 0.5;
            player.position[0] = player.position[0]
                .clamp(-Self::FIELD_HALF_EXTENT + hx, Self::FIELD_HALF_EXTENT - hx);
            player.position[2] = player.position[2]
                .clamp(-Self::FIELD_HALF_EXTENT + hz, Self::FIELD_HALF_EXTENT - hz);
            if (player.position[0] <= -Self::FIELD_HALF_EXTENT + hx + 1.0e-6
                && player.velocity[0] < 0.0)
                || (player.position[0] >= Self::FIELD_HALF_EXTENT - hx - 1.0e-6
                    && player.velocity[0] > 0.0)
            {
                player.velocity[0] = 0.0;
            }
            if (player.position[2] <= -Self::FIELD_HALF_EXTENT + hz + 1.0e-6
                && player.velocity[2] < 0.0)
                || (player.position[2] >= Self::FIELD_HALF_EXTENT - hz - 1.0e-6
                    && player.velocity[2] > 0.0)
            {
                player.velocity[2] = 0.0;
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
            contacts += project_static_position(ball, arena, radius);
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
                Self::FIELD_HALF_EXTENT,
            ) {
                0.0
            } else {
                inverse_ball_mass
            };
            let right_inverse_mass = if boundary_blocks_motion(
                self.balls[right].position,
                right_direction,
                radius,
                Self::FIELD_HALF_EXTENT,
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

        for player in self.players.values_mut() {
            if arena.robot.intake_enabled {
                for ball in &mut self.balls {
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
                        );
                        ball.sleeping = false;
                        ball.quiet_ticks = 0;
                    }
                }
            }
            for ball in &mut self.balls {
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
                    );
                    player.position[1] = arena.robot.height_m * 0.5;
                    ball.sleeping = false;
                    ball.quiet_ticks = 0;
                }
            }
            let hx = arena.robot.width_m * 0.5;
            let hz = arena.robot.length_m * 0.5;
            player.position[0] = player.position[0]
                .clamp(-Self::FIELD_HALF_EXTENT + hx, Self::FIELD_HALF_EXTENT - hx);
            player.position[2] = player.position[2]
                .clamp(-Self::FIELD_HALF_EXTENT + hz, Self::FIELD_HALF_EXTENT - hz);
        }
        // Dynamic contacts can push a ball through a static boundary. End
        // every iteration by projecting onto the field/ramp so the last
        // solver iteration cannot leave an object outside the arena.
        for ball in &mut self.balls {
            contacts += project_static_position(ball, arena, radius);
        }
        contacts
    }

    fn reconstruct_velocities(&mut self, arena: &ArenaConfig, dt: f32) {
        for ball in &mut self.balls {
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
        for ball in &mut self.balls {
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
            let limit = Self::FIELD_HALF_EXTENT - arena.ball.radius_m();
            for (axis, negative_normal, positive_normal) in [
                (0, [1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]),
                (2, [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]),
            ] {
                let normal = if ball.position[axis] <= -limit + 1.0e-5 {
                    Some(negative_normal)
                } else if ball.position[axis] >= limit - 1.0e-5 {
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
        }
    }

    fn limit_ball_energy(&mut self, arena: &ArenaConfig) {
        let max_speed = arena.solver.max_ball_speed_mps.max(0.1);
        let max_angular_speed = arena.solver.max_ball_angular_speed_radps.max(1.0);
        for ball in &mut self.balls {
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
        let x_limit = Self::FIELD_HALF_EXTENT - arena.robot.width_m * 0.5;
        let z_limit = Self::FIELD_HALF_EXTENT - arena.robot.length_m * 0.5;
        for player in self.players.values_mut() {
            if (player.position[0] <= -x_limit + 1.0e-5 && player.velocity[0] < 0.0)
                || (player.position[0] >= x_limit - 1.0e-5 && player.velocity[0] > 0.0)
            {
                player.velocity[0] = 0.0;
            }
            if (player.position[2] <= -z_limit + 1.0e-5 && player.velocity[2] < 0.0)
                || (player.position[2] >= z_limit - 1.0e-5 && player.velocity[2] > 0.0)
            {
                player.velocity[2] = 0.0;
            }
        }
    }

    fn apply_contact_velocities(&mut self, arena: &ArenaConfig, dt: f32) {
        let diameter_sq = arena.ball.diameter_m * arena.ball.diameter_m * 1.002;
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
                Self::FIELD_HALF_EXTENT,
            ) {
                0.0
            } else {
                inverse_mass
            };
            let right_inverse_mass = if boundary_blocks_motion(
                self.balls[right].position,
                normal,
                radius,
                Self::FIELD_HALF_EXTENT,
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
                        arena.robot.intake_surface_speed_mps * player.intake_power
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
            for ball in &mut self.balls {
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
                    Self::FIELD_HALF_EXTENT,
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
            player.position[1] = arena.robot.height_m * 0.5;
            player.velocity[1] = 0.0;
        }
    }

    fn update_sleeping(&mut self, arena: &ArenaConfig, dt: f32) {
        let sleep_ticks = (arena.solver.sleep_after_seconds / dt).max(1.0) as u16;
        let linear_sq = arena.solver.sleep_linear_threshold_mps.powi(2);
        let angular_sq = arena.solver.sleep_angular_threshold_radps.powi(2);
        for ball in &mut self.balls {
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
                color: player.color.into(),
            })
            .collect()
    }

    pub fn field_object_positions(&self) -> Vec<[f32; 3]> {
        self.balls.iter().map(|ball| ball.position).collect()
    }

    pub fn contact_count(&self) -> usize {
        self.metrics.contacts
    }

    pub fn step_metrics(&self) -> StepMetrics {
        self.metrics
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

fn project_static_position(ball: &mut Ball, arena: &ArenaConfig, radius: f32) -> usize {
    let mut contacts = 0;
    if ball.position[1] < radius {
        ball.position[1] = radius;
    }
    if ball.position[1] <= radius + 1.0e-5 {
        ball.grounded = true;
        contacts += 1;
    }
    for axis in [0, 2] {
        let min = -SphereRuntime::FIELD_HALF_EXTENT + radius;
        let max = SphereRuntime::FIELD_HALF_EXTENT - radius;
        let clamped = ball.position[axis].clamp(min, max);
        if clamped != ball.position[axis] {
            ball.position[axis] = clamped;
            contacts += 1;
        }
    }
    if let Some((normal, penetration)) = ramp_contact(ball.position, radius, &arena.ramp) {
        ball.position = add(ball.position, mul(normal, penetration));
        ball.grounded = false;
        ball.on_ramp = true;
        contacts += 1;
    }
    contacts
}

fn boundary_blocks_motion(
    position: Vec3,
    direction: Vec3,
    radius: f32,
    field_half_extent: f32,
) -> bool {
    let limit = field_half_extent - radius;
    (position[0] <= -limit + 1.0e-5 && direction[0] < 0.0)
        || (position[0] >= limit - 1.0e-5 && direction[0] > 0.0)
        || (position[2] <= -limit + 1.0e-5 && direction[2] < 0.0)
        || (position[2] >= limit - 1.0e-5 && direction[2] > 0.0)
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
) {
    let ball_inverse_mass = if boundary_blocks_motion(
        ball.position,
        normal,
        radius,
        SphereRuntime::FIELD_HALF_EXTENT,
    ) {
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
    let center = [
        robot_position[0] + forward[0] * robot.intake_forward_offset_m,
        robot.intake_center_height_m,
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
    fn robot_drive_is_bounded_and_wakes_contacts() {
        let mut arena = arena();
        arena.object_count = 32;
        let mut runtime = SphereRuntime::new("test".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        runtime.add_player("p".into(), "Player".into(), "Team".into(), &arena);
        runtime.set_player_input("p", 0.25, 1.0, 0.0, 1);
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
    fn balls_cannot_lift_the_carpet_supported_robot() {
        let mut arena = arena();
        arena.object_count = 8;
        let mut runtime = SphereRuntime::new("grounded-robot".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        runtime.add_player("p".into(), "Player".into(), "Team".into(), &arena);
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
        runtime.add_player("p".into(), "Player".into(), "Team".into(), &arena);
        let wall_limit = -SphereRuntime::FIELD_HALF_EXTENT + arena.robot.length_m * 0.5;
        let player = runtime.players.get_mut("p").unwrap();
        player.position = [0.0, arena.robot.height_m * 0.5, wall_limit];
        player.yaw = 0.0;
        runtime.set_player_input("p", 0.0, 1.0, 0.0, 1);

        for _ in 0..180 {
            runtime.apply_player_drive(&arena, 1.0 / 60.0);
            runtime.tick(1.0 / 60.0);
        }

        let player = runtime.players.get("p").unwrap();
        assert!((player.position[2] - wall_limit).abs() < 1.0e-5);
        assert!(player.velocity[2].abs() < 1.0e-5);
    }

    #[test]
    fn trapped_ball_stays_bounded_and_pushes_back_on_robot() {
        let mut arena = arena();
        arena.object_count = 1;
        arena.ramp.enabled = false;
        arena.robot.intake_enabled = false;
        let mut runtime = SphereRuntime::new("trapped-ball".into(), "fgc-2026".into(), 0);
        runtime.create_test_arena(&arena);
        runtime.add_player("p".into(), "Player".into(), "Team".into(), &arena);
        let ball_limit = -SphereRuntime::FIELD_HALF_EXTENT + arena.ball.radius_m();
        let robot_start = ball_limit + arena.ball.radius_m() + arena.robot.length_m * 0.5;
        let player = runtime.players.get_mut("p").unwrap();
        player.position = [0.0, arena.robot.height_m * 0.5, robot_start];
        player.yaw = 0.0;
        runtime.balls[0].position = [0.0, arena.ball.radius_m(), ball_limit];
        runtime.balls[0].velocity = [0.0; 3];
        runtime.set_player_input("p", 0.0, 1.0, 0.0, 1);

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
            runtime.add_player("p".into(), "Player".into(), "Team".into(), &arena);
            let player = runtime.players.get_mut("p").unwrap();
            player.position = [0.0, arena.robot.height_m * 0.5, 0.0];
            player.yaw = 0.0;
            player.intake_power = power;
            let combined_radius = arena.ball.radius_m() + arena.robot.intake_radius_m;
            runtime.balls[0].position = [
                0.0,
                arena.robot.intake_center_height_m + combined_radius,
                -arena.robot.intake_forward_offset_m,
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
        assert!(powered > 0.1, "powered intake velocity={powered:.3}");
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
        runtime.add_player("p".into(), "Player".into(), "Team".into(), &arena);
        runtime.set_player_input("p", 0.28, 1.0, 1.0, 1);
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
