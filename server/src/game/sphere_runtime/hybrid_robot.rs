use std::collections::{BTreeMap, HashMap, HashSet};

use rapier3d::prelude::*;

use super::*;

const ROBOT_SUBSTEPS: usize = 2;
const MOTOR_DAMPING: f32 = 1.0;
const DRIVETRAIN_CONTACT_FRICTION: f32 = 0.08;
const FIELD_GROUP: Group = Group::GROUP_1;
const ROBOT_GROUP: Group = Group::GROUP_2;
const BRACE_GROUP: Group = Group::GROUP_3;
const WHEEL_GROUP: Group = Group::GROUP_4;
const GROUND_GROUP: Group = Group::GROUP_5;
const DRIVEBASE_GROUP: Group = Group::GROUP_6;

fn field_groups() -> InteractionGroups {
    InteractionGroups::new(
        FIELD_GROUP,
        ROBOT_GROUP | DRIVEBASE_GROUP | WHEEL_GROUP,
        InteractionTestMode::And,
    )
}

fn ground_groups() -> InteractionGroups {
    InteractionGroups::new(
        GROUND_GROUP,
        DRIVEBASE_GROUP | WHEEL_GROUP,
        InteractionTestMode::And,
    )
}

fn brace_groups() -> InteractionGroups {
    InteractionGroups::new(
        BRACE_GROUP,
        WHEEL_GROUP | ROBOT_GROUP,
        InteractionTestMode::And,
    )
}

fn robot_groups() -> InteractionGroups {
    InteractionGroups::new(
        ROBOT_GROUP,
        FIELD_GROUP | ROBOT_GROUP | DRIVEBASE_GROUP | WHEEL_GROUP,
        InteractionTestMode::And,
    )
}

fn climb_support_groups() -> InteractionGroups {
    InteractionGroups::new(
        ROBOT_GROUP,
        FIELD_GROUP | BRACE_GROUP | ROBOT_GROUP | DRIVEBASE_GROUP | WHEEL_GROUP,
        InteractionTestMode::And,
    )
}

fn drivebase_groups() -> InteractionGroups {
    InteractionGroups::new(
        DRIVEBASE_GROUP,
        FIELD_GROUP | GROUND_GROUP | ROBOT_GROUP | DRIVEBASE_GROUP | WHEEL_GROUP,
        InteractionTestMode::And,
    )
}

fn wheel_groups() -> InteractionGroups {
    InteractionGroups::new(
        WHEEL_GROUP,
        FIELD_GROUP | GROUND_GROUP | BRACE_GROUP | ROBOT_GROUP | DRIVEBASE_GROUP | WHEEL_GROUP,
        InteractionTestMode::And,
    )
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
    brace_capture: Option<BraceCapture>,
}

struct BraceCapture {
    id: String,
}

struct BraceRail {
    handle: ColliderHandle,
    center: Vector,
    ascent: Vector,
    half_length: f32,
    radius: f32,
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
}

impl HybridRobotWorld {
    pub(super) fn new(
        arena: &ArenaConfig,
        definition: &RobotDefinition,
        field_colliders: &[FieldCollider],
        boundary: &FieldBoundary,
        floor_y: f32,
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
                world.braces.insert(
                    authored.id.clone(),
                    BraceRail {
                        handle,
                        center: Vector::new(
                            authored.center[0],
                            authored.center[1],
                            authored.center[2],
                        ),
                        ascent: collider_ascent_axis(authored),
                        half_length: authored
                            .half_extents
                            .iter()
                            .copied()
                            .fold(0.0_f32, f32::max),
                        radius: authored
                            .half_extents
                            .iter()
                            .enumerate()
                            .filter(|(index, _)| {
                                *index
                                    != authored
                                        .half_extents
                                        .iter()
                                        .enumerate()
                                        .max_by(|(_, left), (_, right)| left.total_cmp(right))
                                        .map(|(index, _)| index)
                                        .unwrap_or(1)
                            })
                            .map(|(_, radius)| *radius)
                            .fold(0.0_f32, f32::max),
                    },
                );
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
        self.bodies[chassis].set_additional_solver_iterations(4);

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
            brace_capture: None,
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
            let wheel_inertia = 0.4 * climber.wheel_mass_kg * wheel_radius * wheel_radius;
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
            self.bodies[wheel].set_additional_solver_iterations(6);
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
                let mut points = Vec::with_capacity(32);
                for segment in 0..16 {
                    let angle = segment as f32 * std::f32::consts::TAU / 16.0;
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
                    ColliderBuilder::convex_hull(&points)
                        .expect("a sampled groove frustum must form a convex hull")
                        .position(Pose::from_parts(center, cone_rotation))
                        .density(0.0)
                        // Rapier resolves the groove's normal contacts. The
                        // reduced wheel constraint below owns longitudinal
                        // traction so it is applied once with the right sign.
                        .friction(0.0)
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
                .contacts_enabled(false)
                .motor_velocity(0.0, MOTOR_DAMPING)
                .motor_max_force(climber.brake_torque_nm.max(0.0));
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
            let mut wheel_constraint = None;
            if let Some(climber) = self.definition.climber.as_ref()
                && let Some(joint) = handles
                    .wheel_joint
                    .and_then(|joint| self.joints.get_mut(joint, true))
                && let Some(revolute) = joint.data.as_revolute_mut()
            {
                let powered = player.climb_power > CONTROL_DEADBAND;
                let target = if powered {
                    climber.free_speed_radps * player.climb_power
                } else {
                    0.0
                };
                let torque = if powered {
                    climber.stall_torque_nm
                } else {
                    climber.brake_torque_nm
                };
                revolute.set_motor_velocity(target, MOTOR_DAMPING);
                revolute.set_motor_max_force(torque.max(0.0));

                if let Some(brace_id) = alliance_brace(&player.team_name)
                    && let Some(brace) = self.braces.get(brace_id)
                    && let Some(wheel_handle) = handles.wheel
                    && let Some(wheel) = self.bodies.get(wheel_handle)
                {
                    let mut point_sum = Vector::ZERO;
                    let mut point_count = 0_u32;
                    let mut support_impulse = 0.0;
                    for collider in &handles.wheel_colliders {
                        let Some(pair) = self.narrow_phase.contact_pair(*collider, brace.handle)
                        else {
                            continue;
                        };
                        support_impulse += pair.total_impulse_magnitude();
                        for contact in pair
                            .manifolds
                            .iter()
                            .flat_map(|manifold| &manifold.data.solver_contacts)
                        {
                            point_sum += contact.point;
                            point_count += 1;
                        }
                    }
                    let relative = wheel.translation() - brace.center;
                    let along = relative.dot(brace.ascent);
                    let closest = brace.center
                        + brace.ascent * along.clamp(-brace.half_length, brace.half_length);
                    let groove_radial = wheel.translation() - closest;
                    let seated_radius = brace.radius + climber.groove_root_radius_m
                        - (climber.contact_skin_m + 0.001).min(climber.groove_root_radius_m * 0.5);
                    let capture_radius = brace.radius + climber.groove_outer_radius_m + 0.02;
                    if powered
                        && point_count > 0
                        && groove_radial.length() <= capture_radius
                        && handles.brace_capture.is_none()
                    {
                        handles.brace_capture = Some(BraceCapture {
                            id: brace_id.to_owned(),
                        });
                    }
                    let capture_valid = handles.brace_capture.as_ref().is_some_and(|capture| {
                        capture.id == brace_id
                            && along.abs() <= brace.half_length + 0.06
                            && groove_radial.length() <= capture_radius + 0.04
                    });
                    if !capture_valid {
                        handles.brace_capture = None;
                    }
                    if handles.brace_capture.is_some() {
                        let radial_direction = groove_radial.normalize_or_zero();
                        let point = if point_count > 0 {
                            point_sum / point_count as f32
                        } else {
                            closest + radial_direction * brace.radius
                        };
                        let Some(chassis) = self.bodies.get(handles.chassis) else {
                            continue;
                        };
                        let arm = point - chassis.translation();
                        let point_velocity = chassis.linvel() + chassis.angvel().cross(arm);
                        let radial_velocity =
                            point_velocity - brace.ascent * point_velocity.dot(brace.ascent);
                        let mass = chassis.mass();
                        let seated = radial_direction * seated_radius;
                        let guide_force =
                            (seated - groove_radial) * 20_000.0 - radial_velocity * 120.0;
                        let guide = (guide_force * dt).clamp_length_max(4.5);

                        let traction = if powered {
                            let contact_radius = climber.groove_outer_radius_m.max(0.005);
                            let along_speed = point_velocity.dot(brace.ascent);
                            let desired_speed = climber.max_climb_speed_mps * player.climb_power;
                            let free_surface_speed = climber.free_speed_radps * contact_radius;
                            let motor_force = climber.stall_torque_nm / contact_radius
                                * (1.0 - along_speed.abs() / free_surface_speed.max(0.1))
                                    .clamp(0.0, 1.0)
                                * player.climb_power;
                            let speed_impulse = mass * (desired_speed - along_speed).max(0.0);
                            let motor_impulse = motor_force * dt;
                            let normal_impulse = support_impulse + guide.length();
                            let friction_impulse = climber.dynamic_friction * normal_impulse;
                            brace.ascent * speed_impulse.min(motor_impulse).min(friction_impulse)
                        } else {
                            Vector::ZERO
                        };
                        wheel_constraint = Some((guide + traction, point));
                    }
                }
            }
            let Some(body) = self.bodies.get_mut(handles.chassis) else {
                continue;
            };
            if let Some((impulse, point)) = wheel_constraint {
                // The wheel is a reduced articulated body. Apply its resolved
                // external traction to the chassis at the same world point so
                // the net force and moment match the wheel/joint assembly.
                body.apply_impulse_at_point(impulse, point, true);
            }
            if !handles.floor_supported || handles.brace_capture.is_some() {
                continue;
            }
            let rotation = body.rotation();
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
            handles.floor_supported = handles
                .chassis_colliders
                .iter()
                .chain(handles.wheel_colliders.iter())
                .any(|collider| {
                    has_upward_support(&self.narrow_phase, *collider, &self.support_colliders)
                });
            let brace_id = alliance_brace(&player.team_name);
            let brace_handle = brace_id.and_then(|id| self.braces.get(id).map(|rail| rail.handle));
            handles.brace_contact = handles
                .brace_capture
                .as_ref()
                .map(|capture| capture.id.clone());
            let mut support_impulse = 0.0;
            if let (Some(brace_id), Some(brace)) = (brace_id, brace_handle) {
                for wheel in &handles.wheel_colliders {
                    if let Some(pair) = self.narrow_phase.contact_pair(*wheel, brace)
                        && pair.has_any_active_contact()
                    {
                        handles
                            .brace_contact
                            .get_or_insert_with(|| brace_id.to_owned());
                        support_impulse += pair.total_impulse_magnitude();
                    }
                }
            }
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
            body.set_position(pose, true);
            if let Some(wheel) = handles.wheel.and_then(|wheel| self.bodies.get_mut(wheel)) {
                let mut pose = *wheel.position();
                pose.translation += delta;
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

fn collider_ascent_axis(collider: &FieldCollider) -> Vector {
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
    if axis.y >= 0.0 { axis } else { -axis }
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
