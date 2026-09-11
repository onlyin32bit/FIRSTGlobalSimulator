use std::collections::{BTreeMap, HashMap, HashSet};

use rapier3d::prelude::*;

use super::*;

const ROBOT_SUBSTEPS: usize = 2;
const DRIVETRAIN_CONTACT_FRICTION: f32 = 0.08;
const GROOVE_SEGMENTS: usize = 48;
const FIELD_GROUP: Group = Group::GROUP_1;
const ROBOT_GROUP: Group = Group::GROUP_2;
const BRACE_GROUP: Group = Group::GROUP_3;
const WHEEL_GROUP: Group = Group::GROUP_4;
const GROUND_GROUP: Group = Group::GROUP_5;
const DRIVEBASE_GROUP: Group = Group::GROUP_6;
const BALL_GROUP: Group = Group::GROUP_7;

fn field_groups() -> InteractionGroups {
    InteractionGroups::new(
        FIELD_GROUP,
        ROBOT_GROUP | DRIVEBASE_GROUP | WHEEL_GROUP | BALL_GROUP,
        InteractionTestMode::And,
    )
}

fn ground_groups() -> InteractionGroups {
    InteractionGroups::new(
        GROUND_GROUP,
        DRIVEBASE_GROUP | WHEEL_GROUP | BALL_GROUP,
        InteractionTestMode::And,
    )
}

fn brace_groups() -> InteractionGroups {
    InteractionGroups::new(
        BRACE_GROUP,
        // The brace is a climbing rail, not a chassis obstacle. Letting every
        // robot box touch it over-constrains the wheel joint and makes the bot
        // chatter while it drives underneath. Only the drive wheel loads it.
        WHEEL_GROUP | BALL_GROUP,
        InteractionTestMode::And,
    )
}

fn robot_groups() -> InteractionGroups {
    InteractionGroups::new(
        ROBOT_GROUP,
        FIELD_GROUP | ROBOT_GROUP | DRIVEBASE_GROUP | WHEEL_GROUP | BALL_GROUP,
        InteractionTestMode::And,
    )
}

fn climb_support_groups() -> InteractionGroups {
    InteractionGroups::new(
        ROBOT_GROUP,
        FIELD_GROUP | ROBOT_GROUP | DRIVEBASE_GROUP | WHEEL_GROUP | BALL_GROUP,
        InteractionTestMode::And,
    )
}

fn drivebase_groups() -> InteractionGroups {
    InteractionGroups::new(
        DRIVEBASE_GROUP,
        FIELD_GROUP | GROUND_GROUP | ROBOT_GROUP | DRIVEBASE_GROUP | WHEEL_GROUP | BALL_GROUP,
        InteractionTestMode::And,
    )
}

fn wheel_groups() -> InteractionGroups {
    InteractionGroups::new(
        WHEEL_GROUP,
        FIELD_GROUP
            | GROUND_GROUP
            | BRACE_GROUP
            | ROBOT_GROUP
            | DRIVEBASE_GROUP
            | WHEEL_GROUP
            | BALL_GROUP,
        InteractionTestMode::And,
    )
}

fn ball_groups(ball_to_ball_collisions: bool) -> InteractionGroups {
    let mut filter =
        FIELD_GROUP | GROUND_GROUP | BRACE_GROUP | ROBOT_GROUP | DRIVEBASE_GROUP | WHEEL_GROUP;
    if ball_to_ball_collisions {
        filter |= BALL_GROUP;
    }
    InteractionGroups::new(BALL_GROUP, filter, InteractionTestMode::And)
}

struct RobotHandles {
    chassis: RigidBodyHandle,
    chassis_colliders: Vec<ColliderHandle>,
    wheel: Option<RigidBodyHandle>,
    wheel_colliders: Vec<ColliderHandle>,
    wheel_joint: Option<ImpulseJointHandle>,
    floor_supported: bool,
    wheel_angle: f32,
    brace_contact: Option<String>,
}

struct BraceRail {
    handle: ColliderHandle,
}

pub(super) struct HybridRobotWorld {
    pipeline: PhysicsPipeline,
    gravity: Vector,
    integration: IntegrationParameters,
    islands: IslandManager,
    broad_phase: BroadPhaseBvh,
    narrow_phase: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd: CCDSolver,
    robots: BTreeMap<String, RobotHandles>,
    definition: RobotDefinition,
    support_colliders: HashSet<ColliderHandle>,
    braces: HashMap<String, BraceRail>,
    balls: Vec<RigidBodyHandle>,
    full_rapier: bool,
    floor_y: f32,
    ball_radius: f32,
}

impl HybridRobotWorld {
    pub(super) fn new(
        arena: &ArenaConfig,
        definition: &RobotDefinition,
        field_colliders: &[FieldCollider],
        boundary: &FieldBoundary,
        floor_y: f32,
        full_rapier: bool,
    ) -> Self {
        let mut world = Self {
            pipeline: PhysicsPipeline::new(),
            gravity: vector![0.0, -9.81 * arena.gravity_scale, 0.0].into(),
            integration: IntegrationParameters {
                num_solver_iterations: 8,
                num_internal_pgs_iterations: 2,
                max_ccd_substeps: 2,
                min_island_size: 1,
                ..IntegrationParameters::default()
            },
            islands: IslandManager::new(),
            broad_phase: BroadPhaseBvh::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            robots: BTreeMap::new(),
            definition: definition.clone(),
            support_colliders: HashSet::new(),
            braces: HashMap::new(),
            balls: Vec::new(),
            full_rapier,
            floor_y,
            ball_radius: arena.ball.radius_m(),
        };
        let floor = world.colliders.insert(
            ColliderBuilder::cuboid(
                (boundary.max[0] - boundary.min[0]).abs() * 0.5 + 0.25,
                0.05,
                (boundary.max[2] - boundary.min[2]).abs() * 0.5 + 0.25,
            )
            .translation(
                vector![
                    (boundary.min[0] + boundary.max[0]) * 0.5,
                    floor_y - 0.05,
                    (boundary.min[2] + boundary.max[2]) * 0.5
                ]
                .into(),
            )
            .friction(arena.floor.dynamic_friction.max(0.0))
            .friction_combine_rule(CoefficientCombineRule::Min)
            .restitution(arena.floor.restitution.clamp(0.0, 1.0))
            .collision_groups(ground_groups())
            .build(),
        );
        world.support_colliders.insert(floor);
        world.add_boundary(boundary, floor_y);
        for authored in field_colliders {
            let collider = if is_brace(authored) {
                brace_collider(authored)
            } else {
                obb_collider(authored)
            }
            .friction(arena.metal_wall.dynamic_friction.max(0.0))
            .restitution(0.05)
            .collision_groups(if is_brace(authored) {
                brace_groups()
            } else {
                field_groups()
            })
            .build();
            let handle = world.colliders.insert(collider);
            if is_brace(authored) {
                world
                    .braces
                    .insert(authored.id.clone(), BraceRail { handle });
            } else {
                world.support_colliders.insert(handle);
            }
        }
        world
    }

    fn add_boundary(&mut self, boundary: &FieldBoundary, floor_y: f32) {
        let center_x = (boundary.min[0] + boundary.max[0]) * 0.5;
        let center_z = (boundary.min[2] + boundary.max[2]) * 0.5;
        let half_x = (boundary.max[0] - boundary.min[0]).abs() * 0.5;
        let half_z = (boundary.max[2] - boundary.min[2]).abs() * 0.5;
        for (center, half) in [
            (
                [center_x, floor_y + 0.5, boundary.min[2] - 0.05],
                [half_x + 0.1, 0.55, 0.05],
            ),
            (
                [center_x, floor_y + 0.5, boundary.max[2] + 0.05],
                [half_x + 0.1, 0.55, 0.05],
            ),
            (
                [boundary.min[0] - 0.05, floor_y + 0.5, center_z],
                [0.05, 0.55, half_z + 0.1],
            ),
            (
                [boundary.max[0] + 0.05, floor_y + 0.5, center_z],
                [0.05, 0.55, half_z + 0.1],
            ),
        ] {
            let handle = self.colliders.insert(
                ColliderBuilder::cuboid(half[0], half[1], half[2])
                    .translation(vector![center[0], center[1], center[2]].into())
                    .friction(0.25)
                    .restitution(0.05)
                    .collision_groups(field_groups())
                    .build(),
            );
            self.support_colliders.insert(handle);
        }
    }

    /// Add the pack's game pieces to the same Rapier world as the authored
    /// field and robot. SphereRuntime keeps semantic ownership/scoring state;
    /// Rapier owns every physical pose and impulse in this mode.
    pub(super) fn add_balls(&mut self, balls: &mut [super::Ball], arena: &ArenaConfig) {
        if !self.full_rapier || !self.balls.is_empty() {
            return;
        }
        self.balls.reserve(balls.len());
        for ball in balls {
            let body = self.bodies.insert(
                RigidBodyBuilder::dynamic()
                    .translation(
                        vector![ball.position[0], ball.position[1], ball.position[2]].into(),
                    )
                    .linvel(vector![ball.velocity[0], ball.velocity[1], ball.velocity[2]].into())
                    .angvel(
                        vector![
                            ball.angular_velocity[0],
                            ball.angular_velocity[1],
                            ball.angular_velocity[2]
                        ]
                        .into(),
                    )
                    .linear_damping(arena.ball.linear_damping.max(0.0))
                    .angular_damping(arena.ball.angular_damping.max(0.0))
                    .gravity_scale(arena.gravity_scale)
                    .ccd_enabled(true)
                    .soft_ccd_prediction(arena.ball.soft_ccd_prediction_m.max(0.0))
                    .build(),
            );
            self.bodies[body].set_enabled(ball.active);
            self.colliders.insert_with_parent(
                ColliderBuilder::ball(arena.ball.radius_m())
                    .mass(arena.ball.mass_kg.max(0.001))
                    .friction(arena.ball.ball_friction.max(0.0))
                    .restitution(arena.ball.restitution.clamp(0.0, 1.0))
                    .restitution_combine_rule(CoefficientCombineRule::Max)
                    .collision_groups(ball_groups(arena.ball_to_ball_collisions))
                    .build(),
                body,
                &mut self.bodies,
            );
            ball.physics_dirty = false;
            self.balls.push(body);
        }
    }

    pub(super) fn push_dirty_balls(&mut self, balls: &mut [super::Ball]) {
        if !self.full_rapier {
            return;
        }
        for (ball, handle) in balls.iter_mut().zip(&self.balls) {
            let Some(body) = self.bodies.get_mut(*handle) else {
                continue;
            };
            if !ball.active {
                body.set_enabled(false);
                continue;
            }
            if !body.is_enabled() || ball.physics_dirty {
                body.set_enabled(true);
                body.set_translation(
                    Vector::new(ball.position[0], ball.position[1], ball.position[2]),
                    true,
                );
                body.set_linvel(
                    Vector::new(ball.velocity[0], ball.velocity[1], ball.velocity[2]),
                    true,
                );
                body.set_angvel(
                    Vector::new(
                        ball.angular_velocity[0],
                        ball.angular_velocity[1],
                        ball.angular_velocity[2],
                    ),
                    true,
                );
                ball.physics_dirty = false;
            }
        }
    }

    pub(super) fn pull_balls(&self, balls: &mut [super::Ball]) {
        if !self.full_rapier {
            return;
        }
        for (ball, handle) in balls.iter_mut().zip(&self.balls) {
            let Some(body) = self.bodies.get(*handle) else {
                continue;
            };
            if !body.is_enabled() {
                continue;
            }
            let position = body.translation();
            let velocity = body.linvel();
            let angular_velocity = body.angvel();
            ball.position = [position.x, position.y, position.z];
            ball.velocity = [velocity.x, velocity.y, velocity.z];
            ball.pre_solve_velocity = ball.velocity;
            ball.angular_velocity = [angular_velocity.x, angular_velocity.y, angular_velocity.z];
            ball.sleeping = body.is_sleeping();
            ball.grounded = position.y <= self.floor_y + self.ball_radius + 0.003;
            ball.on_ramp = false;
        }
    }

    pub(super) fn contact_count(&self) -> usize {
        self.narrow_phase.contact_pairs().count()
    }

    pub(super) fn add_robot(&mut self, id: &str, player: &PlayerBody, arena: &ArenaConfig) {
        if self.robots.contains_key(id) {
            return;
        }
        let rotation = Rotation::from_xyzw(
            player.rotation[0],
            player.rotation[1],
            player.rotation[2],
            player.rotation[3],
        )
        .normalize();
        let chassis_mass = (arena.robot.mass_kg
            - self
                .definition
                .climber
                .as_ref()
                .map_or(0.0, |climber| climber.wheel_mass_kg))
        .max(1.0);
        let width = arena.robot.width_m.max(0.05);
        let height = arena.robot.height_m.max(0.05);
        let length = arena.robot.length_m.max(0.05);
        let mass_properties = MassProperties::new(
            Vector::new(0.0, -height * 0.12, 0.0),
            chassis_mass,
            vector![
                chassis_mass * (height * height + length * length) / 12.0,
                chassis_mass * (width * width + length * length) / 12.0,
                chassis_mass * (width * width + height * height) / 12.0
            ]
            .into(),
        );
        let chassis = self.bodies.insert(
            RigidBodyBuilder::dynamic()
                .pose(Pose::from_parts(
                    vector![player.position[0], player.position[1], player.position[2]].into(),
                    rotation,
                ))
                .linvel(vector![player.velocity[0], player.velocity[1], player.velocity[2]].into())
                .angvel(
                    vector![
                        player.angular_velocity[0],
                        player.angular_velocity[1],
                        player.angular_velocity[2]
                    ]
                    .into(),
                )
                .additional_mass_properties(mass_properties)
                .linear_damping(arena.robot.rolling_resistance.max(0.0) * 0.12)
                .angular_damping(0.18)
                .ccd_enabled(true)
                .soft_ccd_prediction(0.02)
                .build(),
        );
        self.bodies[chassis].set_additional_solver_iterations(8);

        let ground_offset = -arena.robot.height_m * 0.5;
        let mut chassis_colliders = Vec::new();
        for local in self.definition.colliders.iter().filter(|local| {
            !self
                .definition
                .climb_colliders
                .iter()
                .any(|wheel| wheel.id == local.id)
        }) {
            let corrected = robot_local_collider(local, [0.0; 3], 0.0, ground_offset);
            let is_climb_support = self
                .definition
                .climber
                .as_ref()
                .is_some_and(|climber| climber.support_parts.contains(&local.id));
            let collision_groups = if is_climb_support {
                climb_support_groups()
            } else {
                robot_groups()
            };
            let handle = self.colliders.insert_with_parent(
                ColliderBuilder::cuboid(
                    corrected.half_extents[0],
                    corrected.half_extents[1],
                    corrected.half_extents[2],
                )
                .position(collider_pose(&corrected))
                .density(0.0)
                .friction(if is_climb_support {
                    0.0
                } else {
                    DRIVETRAIN_CONTACT_FRICTION
                })
                .friction_combine_rule(CoefficientCombineRule::Min)
                .restitution(arena.robot.restitution.clamp(0.0, 1.0))
                .contact_skin(0.001)
                .collision_groups(collision_groups)
                .build(),
                chassis,
                &mut self.bodies,
            );
            chassis_colliders.push(handle);
        }
        let wheelbase = self.colliders.insert_with_parent(
            ColliderBuilder::cuboid(width * 0.42, 0.025, length * 0.42)
                .translation(Vector::new(0.0, -height * 0.5 + 0.026, 0.0))
                .density(0.0)
                .friction(DRIVETRAIN_CONTACT_FRICTION)
                .friction_combine_rule(CoefficientCombineRule::Min)
                .restitution(0.0)
                .contact_skin(0.001)
                .collision_groups(drivebase_groups())
                .build(),
            chassis,
            &mut self.bodies,
        );
        chassis_colliders.push(wheelbase);
        let mut handles = RobotHandles {
            chassis,
            chassis_colliders,
            wheel: None,
            wheel_colliders: Vec::new(),
            wheel_joint: None,
            floor_supported: true,
            wheel_angle: 0.0,
            brace_contact: None,
        };
        if let Some(climber) = self.definition.climber.as_ref()
            && !self.definition.climb_colliders.is_empty()
        {
            let corrected = self
                .definition
                .climb_colliders
                .iter()
                .map(|wheel| robot_local_collider(wheel, [0.0; 3], 0.0, ground_offset))
                .collect::<Vec<_>>();
            let wheel_center = corrected.iter().fold(Vector::ZERO, |center, wheel| {
                center + Vector::new(wheel.center[0], wheel.center[1], wheel.center[2])
            }) / corrected.len() as f32;
            let chassis_pose = *self.bodies[chassis].position();
            let wheel_pose = chassis_pose * Pose::from_parts(wheel_center, Rotation::IDENTITY);
            let wheel_radius = corrected
                .iter()
                .flat_map(|wheel| wheel.half_extents)
                .fold(0.0_f32, f32::max);
            // Include the geared motor's inertia reflected at the axle.
            let wheel_inertia = 2.0 * 0.4 * climber.wheel_mass_kg * wheel_radius * wheel_radius;
            let wheel = self.bodies.insert(
                RigidBodyBuilder::dynamic()
                    .pose(wheel_pose)
                    .additional_mass_properties(MassProperties::new(
                        Vector::ZERO,
                        climber.wheel_mass_kg.max(0.01),
                        Vector::splat(wheel_inertia.max(0.0001)),
                    ))
                    .angular_damping(0.02)
                    .ccd_enabled(true)
                    .soft_ccd_prediction(0.01)
                    .build(),
            );
            self.bodies[wheel].set_additional_solver_iterations(10);
            for cone in &corrected {
                let axle_index = cone
                    .half_extents
                    .iter()
                    .enumerate()
                    .min_by(|(_, left), (_, right)| left.total_cmp(right))
                    .map(|(index, _)| index)
                    .unwrap_or(0);
                let authored_axis = Vector::new(
                    cone.axes[axle_index][0],
                    cone.axes[axle_index][1],
                    cone.axes[axle_index][2],
                )
                .normalize_or_zero();
                let cone_center = Vector::new(cone.center[0], cone.center[1], cone.center[2]);
                let toward_groove = (wheel_center - cone_center).normalize_or_zero();
                let axis = if toward_groove.length_squared() > 0.0 {
                    toward_groove
                } else {
                    authored_axis
                };
                let cone_rotation = Rotation::from_rotation_arc(Vector::Y, axis);
                let center = cone_center - wheel_center;
                let half_width = cone.half_extents[axle_index].max(0.002);
                let mut points = Vec::with_capacity(GROOVE_SEGMENTS * 2);
                for segment in 0..GROOVE_SEGMENTS {
                    let angle = segment as f32 * std::f32::consts::TAU / GROOVE_SEGMENTS as f32;
                    let (sin, cos) = angle.sin_cos();
                    points.push(Vector::new(
                        cos * climber.groove_outer_radius_m,
                        -half_width,
                        sin * climber.groove_outer_radius_m,
                    ));
                    points.push(Vector::new(
                        cos * climber.groove_root_radius_m,
                        half_width,
                        sin * climber.groove_root_radius_m,
                    ));
                }
                let handle = self.colliders.insert_with_parent(
                    ColliderBuilder::round_convex_hull(&points, climber.contact_skin_m * 0.35)
                        .expect("a sampled groove frustum must form a convex hull")
                        .position(Pose::from_parts(center, cone_rotation))
                        .density(0.0)
                        // Each half is a convex frustum. Together they form
                        // the non-convex V groove around the brace.
                        .friction(climber.static_friction.max(climber.dynamic_friction))
                        .friction_combine_rule(CoefficientCombineRule::Max)
                        .restitution(0.0)
                        .contact_skin(climber.contact_skin_m.max(0.0))
                        .collision_groups(wheel_groups())
                        .build(),
                    wheel,
                    &mut self.bodies,
                );
                handles.wheel_colliders.push(handle);
            }
            let authored_axle = rotate_robot_local(climber.axle, 0.0);
            let axle = Vector::new(authored_axle[0], authored_axle[1], authored_axle[2])
                .normalize_or_zero();
            let joint = RevoluteJointBuilder::new(axle)
                .local_anchor1(wheel_center)
                .local_anchor2(Vector::ZERO)
                .contacts_enabled(false);
            handles.wheel_joint = Some(self.joints.insert(chassis, wheel, joint, true));
            handles.wheel = Some(wheel);
        }
        self.robots.insert(id.to_owned(), handles);
    }

    pub(super) fn remove_robot(&mut self, id: &str) {
        let Some(handles) = self.robots.remove(id) else {
            return;
        };
        if let Some(joint) = handles.wheel_joint {
            self.joints.remove(joint, true);
        }
        if let Some(wheel) = handles.wheel {
            self.bodies.remove(
                wheel,
                &mut self.islands,
                &mut self.colliders,
                &mut self.joints,
                &mut self.multibody_joints,
                true,
            );
        }
        self.bodies.remove(
            handles.chassis,
            &mut self.islands,
            &mut self.colliders,
            &mut self.joints,
            &mut self.multibody_joints,
            true,
        );
    }

    pub(super) fn step(
        &mut self,
        players: &mut BTreeMap<String, PlayerBody>,
        arena: &ArenaConfig,
        dt: f32,
    ) {
        self.apply_controls(players, arena, dt);
        self.integration.dt = dt / ROBOT_SUBSTEPS as f32;
        for _ in 0..ROBOT_SUBSTEPS {
            self.apply_ball_rolling_resistance(arena, self.integration.dt);
            self.pipeline.step(
                self.gravity,
                &self.integration,
                &mut self.islands,
                &mut self.broad_phase,
                &mut self.narrow_phase,
                &mut self.bodies,
                &mut self.colliders,
                &mut self.joints,
                &mut self.multibody_joints,
                &mut self.ccd,
                &(),
                &(),
            );
        }
        self.sync_players(players, dt);
    }

    fn apply_ball_rolling_resistance(&mut self, arena: &ArenaConfig, dt: f32) {
        if !self.full_rapier {
            return;
        }
        let ground_height = self.floor_y + self.ball_radius + 0.004;
        let deceleration = arena.floor.rolling_resistance_mps2.max(0.0) * dt;
        for handle in &self.balls {
            let Some(body) = self.bodies.get_mut(*handle) else {
                continue;
            };
            if !body.is_enabled() || body.is_sleeping() || body.translation().y > ground_height {
                continue;
            }
            let velocity = body.linvel();
            let horizontal = Vector::new(velocity.x, 0.0, velocity.z);
            let speed = horizontal.length();
            if speed <= f32::EPSILON {
                continue;
            }
            let delta = deceleration.min(speed);
            body.apply_impulse(-horizontal / speed * (body.mass() * delta), true);
        }
    }

    fn apply_controls(
        &mut self,
        players: &BTreeMap<String, PlayerBody>,
        arena: &ArenaConfig,
        dt: f32,
    ) {
        for (id, handles) in &mut self.robots {
            let Some(player) = players.get(id) else {
                continue;
            };
            let powered = player.climb_power > CONTROL_DEADBAND;
            if let Some(climber) = self.definition.climber.as_ref()
                && let Some(wheel_handle) = handles.wheel
                && let (Some(chassis), Some(wheel)) = (
                    self.bodies.get(handles.chassis),
                    self.bodies.get(wheel_handle),
                )
            {
                let authored_axle = rotate_robot_local(climber.axle, 0.0);
                let axle = *chassis.rotation()
                    * Vector::new(authored_axle[0], authored_axle[1], authored_axle[2])
                        .normalize_or_zero();
                let relative_radps = (wheel.angvel() - chassis.angvel()).dot(axle);
                let torque = if powered {
                    let free_speed = climber
                        .free_speed_radps
                        .min(climber.max_climb_speed_mps / climber.groove_outer_radius_m);
                    let target = free_speed * player.climb_power;
                    climber.stall_torque_nm
                        * ((target - relative_radps) / free_speed).clamp(-1.0, 1.0)
                        * player.climb_power
                } else {
                    -relative_radps.signum() * climber.brake_torque_nm
                };
                let impulse = axle * (torque * dt);
                if let Some(wheel) = self.bodies.get_mut(wheel_handle) {
                    wheel.apply_torque_impulse(impulse, true);
                }
                if let Some(chassis) = self.bodies.get_mut(handles.chassis) {
                    // The motor reacts against its mount, as a real gearbox does.
                    chassis.apply_torque_impulse(-impulse, true);
                }
            }
            let Some(body) = self.bodies.get_mut(handles.chassis) else {
                continue;
            };
            let rotation = body.rotation();
            let local_up = *rotation * Vector::Y;
            if !handles.floor_supported || local_up.y < 0.70 {
                continue;
            }
            let raw_forward = *rotation * Vector::NEG_Z;
            let forward = Vector::new(raw_forward.x, 0.0, raw_forward.z).normalize_or_zero();
            let right = Vector::new(-forward.z, 0.0, forward.x);
            let velocity = body.linvel();
            let forward_speed = velocity.dot(forward);
            let lateral_speed = velocity.dot(right);
            let mut left = player.move_z + player.move_x;
            let mut right_power = player.move_z - player.move_x;
            let peak = left.abs().max(right_power.abs()).max(1.0);
            left /= peak;
            right_power /= peak;
            let target_speed = (left + right_power) * 0.5 * arena.robot.max_speed_mps;
            let acceleration = if target_speed.abs() < forward_speed.abs()
                || target_speed.signum() != forward_speed.signum()
            {
                arena.robot.max_deceleration_mps2
            } else {
                arena.robot.max_acceleration_mps2
            }
            .min(arena.robot.traction_friction * 9.81);
            let forward_delta =
                (target_speed - forward_speed).clamp(-acceleration * dt, acceleration * dt);
            let lateral_delta = (-lateral_speed).clamp(
                -arena.robot.lateral_grip_mps2 * dt,
                arena.robot.lateral_grip_mps2 * dt,
            );
            let mass = body.mass();
            body.apply_impulse(
                (forward * forward_delta + right * lateral_delta) * mass,
                true,
            );
            let target_turn = ((right_power - left) * arena.robot.max_speed_mps
                / arena.robot.track_width_m.max(0.1))
            .clamp(
                -arena.robot.max_turn_rate_radps,
                arena.robot.max_turn_rate_radps,
            );
            let current = body.angvel().y;
            let max_delta = arena.robot.max_angular_acceleration_radps2 * dt;
            let next = current + (target_turn - current).clamp(-max_delta, max_delta);
            let inertia_y = body
                .mass_properties()
                .effective_angular_inertia()
                .m22
                .max(0.01);
            body.apply_torque_impulse(Vector::new(0.0, (next - current) * inertia_y, 0.0), true);
        }
    }

    fn sync_players(&mut self, players: &mut BTreeMap<String, PlayerBody>, dt: f32) {
        for (id, handles) in &mut self.robots {
            let Some(player) = players.get_mut(id) else {
                continue;
            };
            let is_upright = self
                .bodies
                .get(handles.chassis)
                .map(|body| (*body.rotation() * Vector::Y).y >= 0.70)
                .unwrap_or(true);
            handles.floor_supported = is_upright
                && handles
                    .chassis_colliders
                    .iter()
                    .chain(handles.wheel_colliders.iter())
                    .any(|collider| {
                        has_upward_support(&self.narrow_phase, *collider, &self.support_colliders)
                    });
            let brace_id = alliance_brace(&player.team_name);
            let brace_handle = brace_id.and_then(|id| self.braces.get(id).map(|rail| rail.handle));
            let mut support_impulse = 0.0;
            let mut wheel_contacts = 0;
            if let Some(brace) = brace_handle {
                for wheel in &handles.wheel_colliders {
                    if let Some(pair) = self.narrow_phase.contact_pair(*wheel, brace)
                        && pair.has_any_active_contact()
                    {
                        support_impulse += pair.total_impulse_magnitude();
                        wheel_contacts += 1;
                    }
                }
            }
            // A seated groove touches both authored frustums. This is only
            // state reporting; it never creates a constraint or a force.
            handles.brace_contact = (wheel_contacts >= 2)
                .then(|| brace_id.expect("brace handle always has an id").to_owned());
            let Some(body) = self.bodies.get(handles.chassis) else {
                continue;
            };
            let pose = body.position();
            let rotation = pose.rotation;
            let velocity = body.linvel();
            let angular = body.angvel();
            let yaw = (2.0 * (rotation.w * rotation.y + rotation.x * rotation.z))
                .atan2(1.0 - 2.0 * (rotation.y * rotation.y + rotation.z * rotation.z));
            let wheel_radps = handles
                .wheel
                .and_then(|wheel| self.bodies.get(wheel))
                .map(|wheel| {
                    let axle = rotation * Vector::NEG_X;
                    wheel.angvel().dot(axle)
                })
                .unwrap_or(0.0);
            handles.wheel_angle += wheel_radps * dt;
            player.position = [pose.translation.x, pose.translation.y, pose.translation.z];
            player.velocity = [velocity.x, velocity.y, velocity.z];
            player.rotation = [rotation.x, rotation.y, rotation.z, rotation.w];
            player.angular_velocity = [angular.x, angular.y, angular.z];
            player.yaw = yaw;
            player.angular_velocity_y = angular.y;
            player.floor_supported = handles.floor_supported;
            player.climbing_brace = handles.brace_contact.clone();
            player.brace_support_impulse = support_impulse;
            player.climb_wheel_angle = handles.wheel_angle;
            player.climb_wheel_radps = wheel_radps;
            player.wall_contact_normal = None;
        }
    }

    pub(super) fn accept_sphere_response(&mut self, players: &BTreeMap<String, PlayerBody>) {
        for (id, player) in players {
            let Some(handles) = self.robots.get(id) else {
                continue;
            };
            let Some(body) = self.bodies.get_mut(handles.chassis) else {
                continue;
            };
            // Sphere contacts are solved by the custom ball engine. Import
            // their momentum response, but never teleport the Rapier body to
            // a positional correction accumulated across hundreds of balls.
            body.set_linvel(
                Vector::new(player.velocity[0], player.velocity[1], player.velocity[2]),
                true,
            );
            body.set_angvel(
                Vector::new(
                    player.angular_velocity[0],
                    player.angular_velocity[1],
                    player.angular_velocity[2],
                ),
                true,
            );
        }
    }

    #[cfg(test)]
    pub(super) fn teleport_to_players(&mut self, players: &BTreeMap<String, PlayerBody>) {
        for (id, player) in players {
            let Some(handles) = self.robots.get(id) else {
                continue;
            };
            let target = Vector::new(player.position[0], player.position[1], player.position[2]);
            let Some(body) = self.bodies.get_mut(handles.chassis) else {
                continue;
            };
            let delta = target - body.translation();
            let mut pose = *body.position();
            pose.translation = target;
            pose.rotation = Rotation::from_xyzw(
                player.rotation[0],
                player.rotation[1],
                player.rotation[2],
                player.rotation[3],
            );
            body.set_position(pose, true);
            if let Some(wheel) = handles.wheel.and_then(|wheel| self.bodies.get_mut(wheel)) {
                let mut pose = *wheel.position();
                pose.translation += delta;
                pose.rotation = Rotation::from_xyzw(
                    player.rotation[0],
                    player.rotation[1],
                    player.rotation[2],
                    player.rotation[3],
                );
                wheel.set_position(pose, true);
            }
        }
    }
}

fn alliance_brace(team_name: &str) -> Option<&'static str> {
    let team = team_name.to_ascii_lowercase();
    if team.starts_with("blue") {
        Some("Cylinder.002")
    } else if team.starts_with("red") {
        Some("Cylinder.003")
    } else {
        None
    }
}

fn is_brace(collider: &FieldCollider) -> bool {
    matches!(collider.id.as_str(), "Cylinder.002" | "Cylinder.003")
}

fn has_upward_support(
    narrow_phase: &NarrowPhase,
    robot: ColliderHandle,
    support_colliders: &HashSet<ColliderHandle>,
) -> bool {
    narrow_phase.contact_pairs_with(robot).any(|pair| {
        let other = if pair.collider1 == robot {
            pair.collider2
        } else {
            pair.collider1
        };
        if !support_colliders.contains(&other) {
            return false;
        }
        pair.manifolds.iter().any(|manifold| {
            if manifold.data.solver_contacts.is_empty() {
                return false;
            }
            let normal_on_robot = if pair.collider1 == robot {
                -manifold.data.normal
            } else {
                manifold.data.normal
            };
            normal_on_robot.y > 0.35
        })
    })
}

fn brace_collider(collider: &FieldCollider) -> ColliderBuilder {
    let axis_index = collider
        .half_extents
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(index, _)| index)
        .unwrap_or(1);
    let axis = Vector::new(
        collider.axes[axis_index][0],
        collider.axes[axis_index][1],
        collider.axes[axis_index][2],
    )
    .normalize_or_zero();
    let radius = collider
        .half_extents
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != axis_index)
        .map(|(_, radius)| *radius)
        .fold(0.0_f32, f32::max);
    ColliderBuilder::cylinder(collider.half_extents[axis_index], radius)
        .position(Pose::from_parts(
            Vector::new(collider.center[0], collider.center[1], collider.center[2]),
            Rotation::from_rotation_arc(Vector::Y, axis),
        ))
        .contact_skin(0.001)
}

fn obb_collider(collider: &FieldCollider) -> ColliderBuilder {
    ColliderBuilder::cuboid(
        collider.half_extents[0],
        collider.half_extents[1],
        collider.half_extents[2],
    )
    .position(collider_pose(collider))
}

fn collider_pose(collider: &FieldCollider) -> Pose {
    let rotation = Rotation::from_mat3(&Matrix::from_cols(
        Vector::new(
            collider.axes[0][0],
            collider.axes[0][1],
            collider.axes[0][2],
        ),
        Vector::new(
            collider.axes[1][0],
            collider.axes[1][1],
            collider.axes[1][2],
        ),
        Vector::new(
            collider.axes[2][0],
            collider.axes[2][1],
            collider.axes[2][2],
        ),
    ));
    Pose::from_parts(
        Vector::new(collider.center[0], collider.center[1], collider.center[2]),
        rotation.normalize(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brace_only_contacts_the_drive_wheel() {
        let brace = brace_groups();
        assert!(!brace.test(robot_groups()));
        assert!(!brace.test(drivebase_groups()));
        assert!(!brace.test(climb_support_groups()));
        assert!(brace.test(wheel_groups()));
    }
}
