use super::*;

pub(super) fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(super) fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(super) fn mul(value: Vec3, scalar: f32) -> Vec3 {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}

pub(super) fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(super) fn point_inside_aabb(point: Vec3, min: Vec3, max: Vec3) -> bool {
    point[0] >= min[0]
        && point[0] <= max[0]
        && point[1] >= min[1]
        && point[1] <= max[1]
        && point[2] >= min[2]
        && point[2] <= max[2]
}

pub(super) fn length_sq(value: Vec3) -> f32 {
    dot(value, value)
}

#[cfg(test)]
pub(super) fn robot_planar_extents(robot: &RobotPhysicsConfig, yaw: f32) -> (f32, f32) {
    robot_planar_extents_from_half(
        [
            robot.width_m * 0.5,
            robot.height_m * 0.5,
            robot.length_m * 0.5,
        ],
        yaw,
    )
}

pub(super) fn robot_planar_extents_from_half(robot_half: Vec3, yaw: f32) -> (f32, f32) {
    let half_x = robot_half[0];
    let half_z = robot_half[2];
    let cos = yaw.cos().abs();
    let sin = yaw.sin().abs();
    (half_x * cos + half_z * sin, half_x * sin + half_z * cos)
}

pub(super) fn project_robot_boundary(
    player: &mut PlayerBody,
    robot_half: Vec3,
    robot_definition: Option<&RobotDefinition>,
    ground_offset_y: f32,
    boundary: &FieldBoundary,
) -> Option<Vec3> {
    let (min_x, max_x, min_z, max_z) = if let Some(definition) = robot_definition {
        let bounds = robot_local_collider(
            &definition.bounds,
            player.position,
            player.yaw,
            ground_offset_y,
        );
        (bounds.min[0], bounds.max[0], bounds.min[2], bounds.max[2])
    } else {
        let (extent_x, extent_z) = robot_planar_extents_from_half(robot_half, player.yaw);
        (
            player.position[0] - extent_x,
            player.position[0] + extent_x,
            player.position[2] - extent_z,
            player.position[2] + extent_z,
        )
    };

    let mut corrected_min_x = min_x;
    let mut corrected_max_x = max_x;
    let mut corrected_min_z = min_z;
    let mut corrected_max_z = max_z;
    if min_x < boundary.min[0] {
        let correction = boundary.min[0] - min_x;
        player.position[0] += correction;
        corrected_min_x += correction;
        corrected_max_x += correction;
    } else if max_x > boundary.max[0] {
        let correction = boundary.max[0] - max_x;
        player.position[0] += correction;
        corrected_min_x += correction;
        corrected_max_x += correction;
    }
    if min_z < boundary.min[2] {
        let correction = boundary.min[2] - min_z;
        player.position[2] += correction;
        corrected_min_z += correction;
        corrected_max_z += correction;
    } else if max_z > boundary.max[2] {
        let correction = boundary.max[2] - max_z;
        player.position[2] += correction;
        corrected_min_z += correction;
        corrected_max_z += correction;
    }

    const CONTACT_SLOP_M: f32 = 1.0e-5;
    let normal = if corrected_min_x <= boundary.min[0] + CONTACT_SLOP_M {
        Some([1.0, 0.0, 0.0])
    } else if corrected_max_x >= boundary.max[0] - CONTACT_SLOP_M {
        Some([-1.0, 0.0, 0.0])
    } else if corrected_min_z <= boundary.min[2] + CONTACT_SLOP_M {
        Some([0.0, 0.0, 1.0])
    } else if corrected_max_z >= boundary.max[2] - CONTACT_SLOP_M {
        Some([0.0, 0.0, -1.0])
    } else {
        None
    };
    if let Some(normal) = normal {
        let into_surface = dot(player.velocity, normal);
        if into_surface < 0.0 {
            player.velocity = sub(player.velocity, mul(normal, into_surface));
        }
    }
    normal
}

pub(super) fn project_static_position(
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
    let min_x = ball.position[0].min(ball.previous_position[0]) - radius;
    let max_x = ball.position[0].max(ball.previous_position[0]) + radius;
    let min_y = ball.position[1].min(ball.previous_position[1]) - radius;
    let max_y = ball.position[1].max(ball.previous_position[1]) + radius;
    let min_z = ball.position[2].min(ball.previous_position[2]) - radius;
    let max_z = ball.position[2].max(ball.previous_position[2]) + radius;
    for collider in field_colliders {
        if max_x < collider.min[0] || min_x > collider.max[0] ||
           max_y < collider.min[1] || min_y > collider.max[1] ||
           max_z < collider.min[2] || min_z > collider.max[2] {
            continue;
        }
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
pub(super) fn project_sphere_aabb(ball: &mut Ball, collider: &FieldCollider, radius: f32) -> usize {
    let (center, half_extents, axes) =
        if collider.half_extents.iter().any(|extent| *extent > 1.0e-6) {
            (collider.center, collider.half_extents, collider.axes)
        } else {
            (
                [
                    (collider.min[0] + collider.max[0]) * 0.5,
                    (collider.min[1] + collider.max[1]) * 0.5,
                    (collider.min[2] + collider.max[2]) * 0.5,
                ],
                [
                    (collider.max[0] - collider.min[0]).abs() * 0.5,
                    (collider.max[1] - collider.min[1]).abs() * 0.5,
                    (collider.max[2] - collider.min[2]).abs() * 0.5,
                ],
                [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            )
        };

    // A final-position-only projection lets a fast ball cross a thin field
    // panel completely between ticks. Use the same swept, entry-face-aware
    // contact logic as robot colliders so an outtake cannot void through a
    // wall simply because it moved more than the wall thickness this frame.
    if let Some((normal, correction)) = sphere_box_contact_with_sweep(
        ball.position,
        ball.previous_position,
        radius,
        center,
        axes,
        half_extents,
    ) {
        ball.position = add(ball.position, mul(normal, correction));
        return 1;
    }
    0
}

/// Return a sphere/box contact while retaining continuous collision detection
/// for a sphere that traverses the entire box in one simulation tick.
///
/// The returned correction is normally positive, but can be signed when a
/// ball was already inside the swept volume at the beginning of the tick. In
/// that case the correction puts it back on the nearest face instead of
/// allowing it to exit through the opposite side.
fn sphere_box_contact_with_sweep(
    position: Vec3,
    previous_position: Vec3,
    radius: f32,
    center: Vec3,
    axes: [Vec3; 3],
    half_extents: Vec3,
) -> Option<(Vec3, f32)> {
    let relative = sub(position, center);
    let local = [
        dot(relative, axes[0]),
        dot(relative, axes[1]),
        dot(relative, axes[2]),
    ];
    let previous_relative = sub(previous_position, center);
    let previous_local = [
        dot(previous_relative, axes[0]),
        dot(previous_relative, axes[1]),
        dot(previous_relative, axes[2]),
    ];

    // A centre inside the box is a discrete contact. Preserve the face the
    // ball entered from; choosing the nearest face from the current position
    // flips to the far side after a fast crossing.
    if (0..3).all(|axis| local[axis].abs() <= half_extents[axis] + 1.0e-6) {
        let (axis, sign, face_distance) = obb_entry_face(previous_local, local, half_extents);
        return Some((mul(axes[axis], sign), radius + face_distance));
    }

    let movement = sub(local, previous_local);
    let mut entry_time = f32::NEG_INFINITY;
    let mut exit_time = f32::INFINITY;
    let mut entry_axis = 0;
    let mut entry_sign = 1.0;
    let mut previous_inside_expanded = true;

    for axis in 0..3 {
        let limit = half_extents[axis] + radius;
        previous_inside_expanded &= previous_local[axis].abs() <= limit;
        let speed = movement[axis];
        if speed.abs() <= 1.0e-8 {
            if previous_local[axis] < -limit || previous_local[axis] > limit {
                return sphere_box_discrete_contact(position, previous_position, radius, center, axes, half_extents);
            }
            continue;
        }
        let near = (-limit - previous_local[axis]) / speed;
        let far = (limit - previous_local[axis]) / speed;
        let (axis_entry, axis_exit, sign) = if near <= far {
            (near, far, -1.0)
        } else {
            (far, near, 1.0)
        };
        if axis_entry > entry_time {
            entry_time = axis_entry;
            entry_axis = axis;
            entry_sign = sign;
        }
        exit_time = exit_time.min(axis_exit);
    }

    // If the previous point was already inside the expanded box and the
    // current point is outside, keep the ball on the side it approached
    // from. This handles a transfer impulse that starts while a ball is
    // already slightly embedded in a wall: the previous side is the one the
    // ball entered from, so a robot pushing it past the far face must not
    // strand it on the far side of a thin panel.
    let current_inside_expanded =
        (0..3).all(|axis| local[axis].abs() <= half_extents[axis] + radius);
    if previous_inside_expanded && !current_inside_expanded && entry_time < 0.0 {
        let axis = (0..3)
            .find(|axis| local[*axis].abs() > half_extents[*axis] + radius)
            .unwrap_or(0);
        let sign = if previous_local[axis] < 0.0 { -1.0 } else { 1.0 };
        let target = sign * (half_extents[axis] + radius);
        let correction = sign * (target - local[axis]);
        return Some((mul(axes[axis], sign), correction));
    }

    if entry_time <= exit_time && (0.0..=1.0).contains(&entry_time) {
        let limit = half_extents[entry_axis] + radius;
        let correction = (local[entry_axis] - entry_sign * limit) * -entry_sign;
        if correction > 0.0 {
            return Some((mul(axes[entry_axis], entry_sign), correction));
        }
    }

    sphere_box_discrete_contact(position, previous_position, radius, center, axes, half_extents)
}

fn sphere_box_discrete_contact(
    position: Vec3,
    previous_position: Vec3,
    radius: f32,
    center: Vec3,
    axes: [Vec3; 3],
    half_extents: Vec3,
) -> Option<(Vec3, f32)> {
    let relative = sub(position, center);
    let local = [
        dot(relative, axes[0]),
        dot(relative, axes[1]),
        dot(relative, axes[2]),
    ];
    let previous_relative = sub(previous_position, center);
    let previous_local = [
        dot(previous_relative, axes[0]),
        dot(previous_relative, axes[1]),
        dot(previous_relative, axes[2]),
    ];
    let closest = [
        local[0].clamp(-half_extents[0], half_extents[0]),
        local[1].clamp(-half_extents[1], half_extents[1]),
        local[2].clamp(-half_extents[2], half_extents[2]),
    ];
    let delta = sub(local, closest);
    let distance_sq = length_sq(delta);
    if distance_sq >= radius * radius {
        return None;
    }
    let (local_normal, penetration) = if distance_sq > 1.0e-12 {
        let (axis, sign, face_distance) = obb_entry_face(previous_local, local, half_extents);
        if previous_local[axis] * local[axis] < 0.0 && previous_local[axis].abs() > half_extents[axis] {
            let mut normal = [0.0; 3];
            normal[axis] = sign;
            (normal, radius + face_distance)
        } else {
            let distance = distance_sq.sqrt();
            (mul(delta, 1.0 / distance), radius - distance)
        }
    } else {
        let (axis, sign, face_distance) = obb_entry_face(previous_local, local, half_extents);
        let mut normal = [0.0; 3];
        normal[axis] = sign;
        (normal, radius + face_distance)
    };
    let normal = add(
        add(mul(axes[0], local_normal[0]), mul(axes[1], local_normal[1])),
        mul(axes[2], local_normal[2]),
    );
    Some((normal, penetration))
}

pub(super) fn project_sphere_obb(ball: &mut Ball, collider: &FieldCollider, radius: f32) -> usize {
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
    let (nearest_axis, sign, nearest_distance) =
        inside_obb_exit_face(ball.previous_position, local, collider);
    let normal = mul(collider.axes[nearest_axis], sign);
    ball.position = add(
        ball.position,
        mul(normal, radius + nearest_distance.max(0.0)),
    );
    1
}

/// Pick the exit face for a sphere whose centre is already inside an OBB.
///
/// Ball-to-ball projection can move a ball into a thin polycarbonate panel
/// after that panel has been resolved for the current solver pass. Choosing
/// the nearest face then lets a dense pile "tunnel" through the panel once it
/// crosses the midpoint. If the ball began this tick outside the OBB, retain
/// that approached-from face; otherwise use the normal nearest-face rule.
pub(super) fn inside_obb_exit_face(
    previous_position: Vec3,
    local_position: Vec3,
    collider: &FieldCollider,
) -> (usize, f32, f32) {
    let previous_delta = sub(previous_position, collider.center);
    let previous_local = [
        dot(previous_delta, collider.axes[0]),
        dot(previous_delta, collider.axes[1]),
        dot(previous_delta, collider.axes[2]),
    ];
    obb_entry_face(previous_local, local_position, collider.half_extents)
}

/// Choose the OBB face a sphere whose centre is inside the box should be
/// expelled through. Returns `(axis, sign, face_distance)`. If the sphere
/// crossed a face this tick, that entry face is retained so a fast ball cannot
/// pass the midpoint and tunnel out the far side; otherwise the nearest face
/// is used.
fn obb_entry_face(previous_local: Vec3, local: Vec3, half_extents: Vec3) -> (usize, f32, f32) {
    let mut crossed_axis = None;
    let mut entry_time = f32::NEG_INFINITY;
    for axis in 0..3 {
        let previous_distance = previous_local[axis].abs() - half_extents[axis];
        let movement_toward_face = previous_local[axis].abs() - local[axis].abs();
        if previous_distance > 0.0 && movement_toward_face > 1.0e-6 {
            // For a diagonal path, the last slab boundary crossed is the
            // actual entry face of the OBB.
            let axis_entry_time = previous_distance / movement_toward_face;
            if axis_entry_time > entry_time {
                crossed_axis = Some(axis);
                entry_time = axis_entry_time;
            }
        }
    }
    if let Some(axis) = crossed_axis {
        let sign = if previous_local[axis] < 0.0 {
            -1.0
        } else {
            1.0
        };
        return (axis, sign, half_extents[axis] - local[axis].abs());
    }

    let mut nearest_axis = 0;
    let mut nearest_distance = f32::INFINITY;
    for axis in 0..3 {
        let distance = half_extents[axis] - local[axis].abs();
        if distance < nearest_distance {
            nearest_distance = distance;
            nearest_axis = axis;
        }
    }
    let sign = if local[nearest_axis] < 0.0 {
        -1.0
    } else {
        1.0
    };
    (nearest_axis, sign, nearest_distance)
}

/// Compute contact normal between a ball and a field collider (AABB or OBB)
pub(super) fn sphere_collider_contact(
    position: Vec3,
    radius: f32,
    collider: &FieldCollider,
) -> Option<Vec3> {
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
pub(super) fn project_robot_field_colliders(
    player: &mut PlayerBody,
    robot_half: Vec3,
    robot_definition: Option<&RobotDefinition>,
    ground_offset_y: f32,
    field_colliders: &[FieldCollider],
    climbing: bool,
) -> (usize, Option<Vec3>) {
    if let Some(definition) = robot_definition {
        return project_authored_robot_field_colliders(
            player,
            definition,
            ground_offset_y,
            field_colliders,
            climbing,
        );
    }

    let half_x = robot_half[0];
    let half_z = robot_half[2];
    let robot_min_y = player.position[1] - robot_half[1];
    let robot_max_y = player.position[1] + robot_half[1];
    let mut contacts = 0;
    let mut contact_normal = None;

    for collider in field_colliders {
        // The climb motor constrains both wheels to its authored rail. Letting
        // the generic chassis solver fight any of the brace subparts causes a
        // one-tick nudge that drops the attachment.
        if climbing && collider.id.starts_with("Cylinder") {
            continue;
        }
        if robot_max_y <= collider.min[1] || robot_min_y >= collider.max[1] {
            continue;
        }
        if collider.half_extents.iter().any(|extent| *extent > 1.0e-6) {
            if let Some((normal, penetration)) =
                robot_field_obb_contact(player.position, player.yaw, robot_half, collider)
            {
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

fn project_authored_robot_field_colliders(
    player: &mut PlayerBody,
    definition: &RobotDefinition,
    ground_offset_y: f32,
    field_colliders: &[FieldCollider],
    climbing: bool,
) -> (usize, Option<Vec3>) {
    let mut contacts = 0;
    let mut contact_normal = None;
    let mut envelope = robot_local_collider(
        &definition.bounds,
        player.position,
        player.yaw,
        ground_offset_y,
    );

    for field in field_colliders {
        if climbing && field.id.starts_with("Cylinder") {
            continue;
        }
        if !collider_bounds_overlap(&envelope, field) {
            continue;
        }

        let contact = definition
            .colliders
            .iter()
            .filter_map(|local| {
                let robot =
                    robot_local_collider(local, player.position, player.yaw, ground_offset_y);
                collider_bounds_overlap(&robot, field)
                    .then(|| obb_obb_contact(&robot, field))
                    .flatten()
            })
            .max_by(|left, right| left.1.total_cmp(&right.1));

        let Some((normal, penetration)) = contact else {
            continue;
        };
        player.position = add(player.position, mul(normal, penetration));
        let into_surface = dot(player.velocity, normal);
        if into_surface < 0.0 {
            player.velocity = sub(player.velocity, mul(normal, into_surface));
        }
        contacts += 1;
        contact_normal = Some(normal);
        envelope = robot_local_collider(
            &definition.bounds,
            player.position,
            player.yaw,
            ground_offset_y,
        );
    }
    (contacts, contact_normal)
}

fn collider_bounds_overlap(left: &FieldCollider, right: &FieldCollider) -> bool {
    (0..3).all(|axis| left.min[axis] <= right.max[axis] && left.max[axis] >= right.min[axis])
}

pub(super) fn obb_obb_contact(left: &FieldCollider, right: &FieldCollider) -> Option<(Vec3, f32)> {
    let mut axes = [[0.0; 3]; 15];
    let mut axis_count = 0;
    for axis in left.axes.iter().copied().chain(right.axes.iter().copied()) {
        axes[axis_count] = axis;
        axis_count += 1;
    }
    for left_axis in left.axes.iter().copied() {
        for right_axis in right.axes.iter().copied() {
            let candidate = cross(left_axis, right_axis);
            let length = length_sq(candidate).sqrt();
            if length <= 1.0e-5 {
                continue;
            }
            axes[axis_count] = mul(candidate, 1.0 / length);
            axis_count += 1;
        }
    }

    let center_delta = sub(left.center, right.center);
    let mut minimum_penetration = f32::INFINITY;
    let mut minimum_normal = [0.0, 1.0, 0.0];
    for axis in axes.into_iter().take(axis_count) {
        let left_radius = (0..3)
            .map(|index| left.half_extents[index] * dot(axis, left.axes[index]).abs())
            .sum::<f32>();
        let right_radius = (0..3)
            .map(|index| right.half_extents[index] * dot(axis, right.axes[index]).abs())
            .sum::<f32>();
        let penetration = left_radius + right_radius - dot(center_delta, axis).abs();
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

/// Return the minimum-translation contact for the rotated robot box against
/// one authored field OBB. The offline scene gets this rotation from Rapier's
/// rigid body; using SAT here keeps the server's planar solver in the same
/// coordinate space instead of testing a permanently axis-aligned chassis.
pub(super) fn robot_field_obb_contact(
    robot_center: Vec3,
    robot_yaw: f32,
    robot_half: Vec3,
    collider: &FieldCollider,
) -> Option<(Vec3, f32)> {
    let sin = robot_yaw.sin();
    let cos = robot_yaw.cos();
    let robot_axes = [[cos, 0.0, -sin], [0.0, 1.0, 0.0], [sin, 0.0, cos]];
    let robot = FieldCollider {
        id: "robot-envelope".into(),
        min: [0.0; 3],
        max: [0.0; 3],
        center: robot_center,
        half_extents: robot_half,
        axes: robot_axes,
    };
    obb_obb_contact(&robot, collider)
}

pub(super) fn boundary_blocks_motion(
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
pub(super) fn resolve_ball_robot_position(
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
    robot_fully_dynamic: bool,
) {
    let ball_inverse_mass = if boundary_blocks_motion(ball.position, normal, radius, field_boundary)
    {
        // A field wall supports the ball, so the chassis must take the
        // positional correction instead of repeatedly pushing through it.
        0.0
    } else {
        inverse_ball_mass
    };
    let planar_normal_sq = normal[0] * normal[0] + normal[2] * normal[2];
    let robot_effective_inverse_mass = inverse_robot_mass
        * if robot_fully_dynamic {
            1.0
        } else {
            planar_normal_sq
        };
    let inverse_mass_sum = ball_inverse_mass + robot_effective_inverse_mass;
    if inverse_mass_sum <= 0.0 {
        return;
    }
    let lambda = penetration / (inverse_mass_sum + alpha);
    // The depenetration cap is tuned for dense 500-ball stacks, where one full
    // correction per frame would explode a wedged pile across the field. A ball
    // contacting a robot is a rare, low-contact case: push it fully out of the
    // chassis in a single iteration so it never lags half-embedded behind a
    // moving robot. The heavy robot still takes its capped share.
    let ball_correction = ball_inverse_mass * lambda;
    let robot_correction = (inverse_robot_mass * lambda).min(max_correction);
    ball.position = add(ball.position, mul(normal, ball_correction));
    player.position[0] -= normal[0] * robot_correction;
    player.position[2] -= normal[2] * robot_correction;
    if robot_fully_dynamic {
        player.position[1] -= normal[1] * robot_correction;
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_sphere_surface_velocity(
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

pub(super) fn approach_zero(value: f32, amount: f32) -> f32 {
    if value > 0.0 {
        (value - amount).max(0.0)
    } else {
        (value + amount).min(0.0)
    }
}

pub(super) fn cell_for(position: Vec3, cell_size: f32) -> [i32; 3] {
    [
        (position[0] / cell_size).floor() as i32,
        (position[1] / cell_size).floor() as i32,
        (position[2] / cell_size).floor() as i32,
    ]
}

pub(super) fn hash_cell(cell: [i32; 3]) -> usize {
    let x = (cell[0] as u32).wrapping_mul(73_856_093);
    let y = (cell[1] as u32).wrapping_mul(19_349_663);
    let z = (cell[2] as u32).wrapping_mul(83_492_791);
    (x ^ y ^ z) as usize
}

pub(super) fn wrap_angle(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

/// Stable pseudo-random number in [-1, 1] without storing per-ball RNG
/// state. Its inputs are only the ball index and a fixed channel salt.
pub(super) fn fountain_noise(index: u32, salt: f32) -> f32 {
    ((index as f32 * 12.9898 + salt).sin() * 43_758.547).rem_euclid(1.0) * 2.0 - 1.0
}

pub(super) fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(super) fn sphere_obb_contact(
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

/// Sphere contact against an arbitrary authored OBB. Robot pack colliders are
/// expressed this way so the server never needs to approximate a whole bot as
/// a single chassis cube.
pub(crate) fn sphere_authored_obb_contact(
    sphere: Vec3,
    radius: f32,
    collider: &FieldCollider,
) -> Option<(Vec3, f32)> {
    let relative = sub(sphere, collider.center);
    let local = [
        dot(relative, collider.axes[0]),
        dot(relative, collider.axes[1]),
        dot(relative, collider.axes[2]),
    ];
    let closest = [
        local[0].clamp(-collider.half_extents[0], collider.half_extents[0]),
        local[1].clamp(-collider.half_extents[1], collider.half_extents[1]),
        local[2].clamp(-collider.half_extents[2], collider.half_extents[2]),
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
            collider.half_extents[0] - local[0].abs(),
            collider.half_extents[1] - local[1].abs(),
            collider.half_extents[2] - local[2].abs(),
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
    let normal = add(
        add(
            mul(collider.axes[0], local_normal[0]),
            mul(collider.axes[1], local_normal[1]),
        ),
        mul(collider.axes[2], local_normal[2]),
    );
    Some((normal, penetration))
}

/// Sphere contact against a robot OBB that preserves the face the ball
/// approached from. `sphere_authored_obb_contact` selects the nearest face when
/// the centre is inside, which flips once a fast ball crosses the OBB midpoint
/// and makes it warp through the chassis. Only a handful of balls are ever
/// inside a robot, so the extra swept test is cheap.
pub(super) fn sphere_robot_obb_contact(
    position: Vec3,
    previous_position: Vec3,
    radius: f32,
    collider: &FieldCollider,
) -> Option<(Vec3, f32)> {
    let relative = sub(position, collider.center);
    let local = [
        dot(relative, collider.axes[0]),
        dot(relative, collider.axes[1]),
        dot(relative, collider.axes[2]),
    ];
    if (0..3).all(|axis| local[axis].abs() <= collider.half_extents[axis] + 1.0e-6) {
        let previous_relative = sub(previous_position, collider.center);
        let previous_local = [
            dot(previous_relative, collider.axes[0]),
            dot(previous_relative, collider.axes[1]),
            dot(previous_relative, collider.axes[2]),
        ];
        let (axis, sign, face_distance) =
            obb_entry_face(previous_local, local, collider.half_extents);
        let normal = mul(collider.axes[axis], sign);
        return Some((normal, radius + face_distance));
    }

    // A discrete overlap check misses a ball which traverses an entire thin
    // panel in one tick: both its previous and current centres can be outside
    // the OBB. Sweep the centre through the OBB expanded by the ball radius
    // (Minkowski sum) and return the entry face in that case. The positional
    // solver then puts the ball back on that face instead of letting it emerge
    // from the opposite side.
    let previous_relative = sub(previous_position, collider.center);
    let previous_local = [
        dot(previous_relative, collider.axes[0]),
        dot(previous_relative, collider.axes[1]),
        dot(previous_relative, collider.axes[2]),
    ];
    let movement = sub(local, previous_local);
    let mut entry_time = f32::NEG_INFINITY;
    let mut exit_time = f32::INFINITY;
    let mut entry_axis = 0;
    let mut entry_sign = 1.0;
    for axis in 0..3 {
        let limit = collider.half_extents[axis] + radius;
        let speed = movement[axis];
        if speed.abs() <= 1.0e-8 {
            if previous_local[axis] < -limit || previous_local[axis] > limit {
                return sphere_authored_obb_contact(position, radius, collider);
            }
            continue;
        }
        let near = (-limit - previous_local[axis]) / speed;
        let far = (limit - previous_local[axis]) / speed;
        let (axis_entry, axis_exit, sign) = if near <= far {
            (near, far, -1.0)
        } else {
            (far, near, 1.0)
        };
        if axis_entry > entry_time {
            entry_time = axis_entry;
            entry_axis = axis;
            entry_sign = sign;
        }
        exit_time = exit_time.min(axis_exit);
    }
    if entry_time <= exit_time && (0.0..=1.0).contains(&entry_time) {
        let limit = collider.half_extents[entry_axis] + radius;
        let penetration = (local[entry_axis] - entry_sign * limit) * -entry_sign;
        if penetration > 0.0 {
            return Some((
                mul(collider.axes[entry_axis], entry_sign),
                penetration,
            ));
        }
    }
    sphere_authored_obb_contact(position, radius, collider)
}

/// Entry-face-aware sphere/box contact for the planar-chassis fallback path
/// used when no authored robot definition is loaded.
pub(super) fn sphere_robot_box_contact(
    sphere: Vec3,
    previous_sphere: Vec3,
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
    if (0..3).all(|axis| local[axis].abs() <= half[axis] + 1.0e-6) {
        let previous_relative = sub(previous_sphere, center);
        let previous_local = [
            cos * previous_relative[0] - sin * previous_relative[2],
            previous_relative[1],
            sin * previous_relative[0] + cos * previous_relative[2],
        ];
        let (axis, sign, face_distance) = obb_entry_face(previous_local, local, half);
        let mut normal = [0.0; 3];
        normal[axis] = sign;
        let world_normal = [
            cos * normal[0] + sin * normal[2],
            normal[1],
            -sin * normal[0] + cos * normal[2],
        ];
        return Some((world_normal, radius + face_distance));
    }
    sphere_obb_contact(sphere, radius, center, yaw, half)
}

pub(super) fn robot_local_collider(
    local: &FieldCollider,
    player_position: Vec3,
    yaw: f32,
    ground_offset_y: f32,
) -> FieldCollider {
    robot_local_collider_pose(
        local,
        player_position,
        [0.0, (yaw * 0.5).sin(), 0.0, (yaw * 0.5).cos()],
        ground_offset_y,
    )
}

pub(super) fn robot_local_collider_pose(
    local: &FieldCollider,
    player_position: Vec3,
    rotation: [f32; 4],
    ground_offset_y: f32,
) -> FieldCollider {
    let local_center = [
        local.center[0],
        local.center[1] + ground_offset_y,
        local.center[2],
    ];
    let offset = rotate_robot_local_pose(local_center, rotation);
    let center = add(player_position, offset);
    let axes = [
        rotate_robot_local_pose(local.axes[0], rotation),
        rotate_robot_local_pose(local.axes[1], rotation),
        rotate_robot_local_pose(local.axes[2], rotation),
    ];
    let mut min = center;
    let mut max = center;
    for world_axis in 0..3 {
        let radius = (0..3)
            .map(|axis| axes[axis][world_axis].abs() * local.half_extents[axis])
            .sum::<f32>();
        min[world_axis] -= radius;
        max[world_axis] += radius;
    }
    FieldCollider {
        id: local.id.clone(),
        min,
        max,
        center,
        half_extents: local.half_extents,
        axes,
    }
}

pub(super) fn rotate_robot_local(vector: Vec3, yaw: f32) -> Vec3 {
    rotate_robot_local_pose(vector, [0.0, (yaw * 0.5).sin(), 0.0, (yaw * 0.5).cos()])
}

pub(super) fn rotate_robot_local_pose(vector: Vec3, rotation: [f32; 4]) -> Vec3 {
    // Blender's robot forward is opposite the runtime frame.
    let vector = [-vector[0], vector[1], -vector[2]];
    rotate_quaternion(vector, rotation)
}

fn rotate_quaternion(vector: Vec3, rotation: [f32; 4]) -> Vec3 {
    let q = [rotation[0], rotation[1], rotation[2]];
    let t = mul(cross(q, vector), 2.0);
    add(vector, add(mul(t, rotation[3]), cross(q, t)))
}

pub(super) fn ramp_contact(
    position: Vec3,
    radius: f32,
    ramp: &RampPhysicsConfig,
) -> Option<(Vec3, f32)> {
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

pub(super) fn roller_contact(
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
