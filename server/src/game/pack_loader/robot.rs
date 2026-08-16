use super::*;

/// Collision and semantic data authored in robot-local coordinates. The
/// visual GLB is deliberately not part of the simulation contract.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RobotDefinition {
    pub id: String,
    pub colliders: Vec<FieldCollider>,
    pub climb_colliders: Vec<FieldCollider>,
    pub climber: Option<RobotClimberConfig>,
    pub bounds: FieldCollider,
    pub zones: Vec<RobotSemanticZone>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RobotSemanticZone {
    pub id: String,
    pub kind: RobotSemanticKind,
    pub collider: FieldCollider,
    /// The semantic's local -Z axis. This is the authored outward direction
    /// for an intake/outtake mouth.
    pub direction: [f32; 3],
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RobotSemanticKind {
    Intake,
    Transfer,
    Outtake,
}

pub(super) fn load_robot_definition(
    id: &str,
    physics: &serde_json::Value,
    semantics: &serde_json::Value,
    climber: Option<RobotClimberConfig>,
) -> Result<RobotDefinition, GameError> {
    if let Some(config) = &climber {
        let axle_length_sq = config.axle.iter().map(|axis| axis * axis).sum::<f32>();
        let finite = config
            .axle
            .iter()
            .chain([
                &config.wheel_mass_kg,
                &config.groove_root_radius_m,
                &config.groove_outer_radius_m,
                &config.max_climb_speed_mps,
                &config.free_speed_radps,
                &config.stall_torque_nm,
                &config.brake_torque_nm,
                &config.static_friction,
                &config.dynamic_friction,
                &config.contact_skin_m,
            ])
            .all(|value| value.is_finite());
        if config.wheel_parts.is_empty()
            || !finite
            || axle_length_sq <= 1.0e-8
            || config.wheel_mass_kg <= 0.0
            || config.groove_root_radius_m <= 0.0
            || config.groove_outer_radius_m <= config.groove_root_radius_m
            || config.max_climb_speed_mps <= 0.0
            || config.free_speed_radps <= 0.0
            || config.stall_torque_nm <= 0.0
            || config.brake_torque_nm < 0.0
            || config.static_friction < 0.0
            || config.dynamic_friction < 0.0
            || config.contact_skin_m < 0.0
        {
            return Err(GameError::ManifestParseError(format!(
                "Robot {id} has an invalid climber configuration"
            )));
        }
    }
    let colliders = assimp_obb_nodes(physics)
        .into_iter()
        .filter(|collider| {
            collider
                .half_extents
                .iter()
                .all(|extent| extent.is_finite())
        })
        .map(extrude_robot_surface)
        .collect::<Vec<_>>();
    if colliders.is_empty() {
        return Err(GameError::ManifestParseError(format!(
            "Robot {id} has no usable authored collision nodes"
        )));
    }
    let bounds = combined_bounds(&colliders).ok_or_else(|| {
        GameError::ManifestParseError(format!("Robot {id} has invalid collision bounds"))
    })?;
    let wheel_parts = climber
        .as_ref()
        .map(|climber| climber.wheel_parts.as_slice())
        .unwrap_or(&[]);
    let climb_colliders = colliders
        .iter()
        .filter(|collider| {
            wheel_parts.iter().any(|part| part == &collider.id)
                || (wheel_parts.is_empty() && collider.id.starts_with("ClimbWheel"))
        })
        .cloned()
        .collect::<Vec<_>>();
    if climber.is_some() && climb_colliders.len() != wheel_parts.len() {
        return Err(GameError::ManifestParseError(format!(
            "Robot {id} climber references missing wheel collision parts"
        )));
    }
    if let Some(climber) = &climber {
        for part in &climber.support_parts {
            if !colliders.iter().any(|collider| &collider.id == part) {
                return Err(GameError::ManifestParseError(format!(
                    "Robot {id} climber references missing support part {part}"
                )));
            }
        }
    }
    let node_positions = assimp_node_positions(semantics);
    let zones = assimp_obb_nodes(semantics)
        .into_iter()
        .filter_map(|collider| {
            let (kind, target_name) = match collider.id.as_str() {
                "IntakeZone" => (RobotSemanticKind::Intake, Some("IntakeTarget")),
                "TransferZone" => (RobotSemanticKind::Transfer, Some("TransferTarget")),
                "OuttakeZone" => (RobotSemanticKind::Outtake, Some("OuttakeTarget")),
                _ => return None,
            };
            let direction = if let Some(target_name) = target_name {
                if let Some(&target_pos) = node_positions.get(target_name) {
                    let delta = [
                        target_pos[0] - collider.center[0],
                        target_pos[1] - collider.center[1],
                        target_pos[2] - collider.center[2],
                    ];
                    let len = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
                    if len > 1.0e-6 {
                        [delta[0] / len, delta[1] / len, delta[2] / len]
                    } else {
                        [
                            -collider.axes[2][0],
                            -collider.axes[2][1],
                            -collider.axes[2][2],
                        ]
                    }
                } else {
                    [
                        -collider.axes[2][0],
                        -collider.axes[2][1],
                        -collider.axes[2][2],
                    ]
                }
            } else {
                [
                    -collider.axes[2][0],
                    -collider.axes[2][1],
                    -collider.axes[2][2],
                ]
            };
            Some(RobotSemanticZone {
                id: collider.id.clone(),
                kind,
                collider: extrude_robot_surface(collider),
                direction,
            })
        })
        .collect::<Vec<_>>();
    for kind in [
        RobotSemanticKind::Intake,
        RobotSemanticKind::Transfer,
        RobotSemanticKind::Outtake,
    ] {
        if zones.iter().filter(|zone| zone.kind == kind).count() != 1 {
            return Err(GameError::ManifestParseError(format!(
                "Robot {id} must define exactly one {kind:?}Zone"
            )));
        }
    }
    info!(
        robot = id,
        colliders = colliders.len(),
        climb_colliders = climb_colliders.len(),
        zones = zones.len(),
        "Loaded robot physics and semantics"
    );
    Ok(RobotDefinition {
        id: id.into(),
        colliders,
        climb_colliders,
        climber,
        bounds,
        zones,
    })
}

fn assimp_node_positions(scene: &serde_json::Value) -> std::collections::BTreeMap<String, [f32; 3]> {
    scene
        .get("rootnode")
        .and_then(|node| node.get("children"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|node| {
            let id = node.get("name")?.as_str()?.to_string();
            let matrix = assimp_matrix(node)?;
            let position = transform_point(&matrix, [0.0, 0.0, 0.0]);
            Some((id, position))
        })
        .collect()
}

fn assimp_obb_nodes(scene: &serde_json::Value) -> Vec<FieldCollider> {
    scene
        .get("rootnode")
        .and_then(|node| node.get("children"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|node| assimp_obb_node(node, scene))
        .collect()
}

fn assimp_obb_node(node: &serde_json::Value, scene: &serde_json::Value) -> Option<FieldCollider> {
    let id = node.get("name")?.as_str()?.to_string();
    let mesh_index = node.get("meshes")?.as_array()?.first()?.as_u64()? as usize;
    let vertices = scene
        .get("meshes")?
        .as_array()?
        .get(mesh_index)?
        .get("vertices")?
        .as_array()?;
    let matrix = assimp_matrix(node)?;
    let mut local_min = [f32::INFINITY; 3];
    let mut local_max = [f32::NEG_INFINITY; 3];
    for xyz in vertices.chunks_exact(3) {
        for axis in 0..3 {
            let value = xyz[axis].as_f64()? as f32;
            local_min[axis] = local_min[axis].min(value);
            local_max[axis] = local_max[axis].max(value);
        }
    }
    if !local_min.iter().all(|value| value.is_finite()) {
        return None;
    }
    let local_center = [
        (local_min[0] + local_max[0]) * 0.5,
        (local_min[1] + local_max[1]) * 0.5,
        (local_min[2] + local_max[2]) * 0.5,
    ];
    let mut axes = [[0.0; 3]; 3];
    let mut half_extents = [0.0; 3];
    for axis in 0..3 {
        let raw = [matrix[axis], matrix[axis + 4], matrix[axis + 8]];
        let length = (raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2])
            .sqrt()
            .max(1.0e-6);
        axes[axis] = [raw[0] / length, raw[1] / length, raw[2] / length];
        half_extents[axis] = (local_max[axis] - local_min[axis]) * 0.5 * length;
    }
    let center = transform_point(&matrix, local_center);
    Some(make_bounds(id, center, half_extents, axes))
}

fn assimp_matrix(node: &serde_json::Value) -> Option<[f32; 16]> {
    let values = node.get("transformation")?.as_array()?;
    let mut matrix = [0.0; 16];
    for (index, value) in values.iter().take(16).enumerate() {
        matrix[index] = value.as_f64()? as f32;
    }
    Some(matrix)
}

fn transform_point(matrix: &[f32; 16], point: [f32; 3]) -> [f32; 3] {
    [
        matrix[0] * point[0] + matrix[1] * point[1] + matrix[2] * point[2] + matrix[3],
        matrix[4] * point[0] + matrix[5] * point[1] + matrix[6] * point[2] + matrix[7],
        matrix[8] * point[0] + matrix[9] * point[1] + matrix[10] * point[2] + matrix[11],
    ]
}

fn extrude_robot_surface(mut collider: FieldCollider) -> FieldCollider {
    for extent in &mut collider.half_extents {
        *extent = extent.max(0.01);
    }
    make_bounds(
        collider.id,
        collider.center,
        collider.half_extents,
        collider.axes,
    )
}

fn combined_bounds(colliders: &[FieldCollider]) -> Option<FieldCollider> {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for collider in colliders {
        for axis in 0..3 {
            min[axis] = min[axis].min(collider.min[axis]);
            max[axis] = max[axis].max(collider.max[axis]);
        }
    }
    min[0].is_finite().then(|| {
        let center = [
            (min[0] + max[0]) * 0.5,
            (min[1] + max[1]) * 0.5,
            (min[2] + max[2]) * 0.5,
        ];
        make_bounds(
            "robot-envelope".into(),
            center,
            [
                (max[0] - min[0]) * 0.5,
                (max[1] - min[1]) * 0.5,
                (max[2] - min[2]) * 0.5,
            ],
            default_axes(),
        )
    })
}

fn make_bounds(
    id: String,
    center: [f32; 3],
    half_extents: [f32; 3],
    axes: [[f32; 3]; 3],
) -> FieldCollider {
    let mut min = center;
    let mut max = center;
    for world_axis in 0..3 {
        let radius = (0..3)
            .map(|local_axis| axes[local_axis][world_axis].abs() * half_extents[local_axis])
            .sum::<f32>();
        min[world_axis] -= radius;
        max[world_axis] += radius;
    }
    FieldCollider {
        id,
        min,
        max,
        center,
        half_extents,
        axes,
    }
}
