use super::*;

/// Collision and semantic data authored in robot-local coordinates. The
/// visual GLB is deliberately not part of the simulation contract.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RobotDefinition {
    pub id: String,
    pub colliders: Vec<FieldCollider>,
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
) -> Result<RobotDefinition, GameError> {
    let colliders = assimp_obb_nodes(physics)
        .into_iter()
        .filter(|collider| collider.half_extents.iter().all(|extent| extent.is_finite()))
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
    let zones = assimp_obb_nodes(semantics)
        .into_iter()
        .filter_map(|collider| {
            let kind = match collider.id.as_str() {
                "IntakeZone" => RobotSemanticKind::Intake,
                "TransferZone" => RobotSemanticKind::Transfer,
                "OuttakeZone" => RobotSemanticKind::Outtake,
                _ => return None,
            };
            let direction = [-collider.axes[2][0], -collider.axes[2][1], -collider.axes[2][2]];
            Some(RobotSemanticZone {
                id: collider.id.clone(),
                kind,
                collider: extrude_robot_surface(collider),
                direction,
            })
        })
        .collect::<Vec<_>>();
    for kind in [RobotSemanticKind::Intake, RobotSemanticKind::Transfer, RobotSemanticKind::Outtake] {
        if zones.iter().filter(|zone| zone.kind == kind).count() != 1 {
            return Err(GameError::ManifestParseError(format!(
                "Robot {id} must define exactly one {kind:?}Zone"
            )));
        }
    }
    info!(robot = id, colliders = colliders.len(), zones = zones.len(), "Loaded robot physics and semantics");
    Ok(RobotDefinition { id: id.into(), colliders, bounds, zones })
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
    let vertices = scene.get("meshes")?.as_array()?.get(mesh_index)?.get("vertices")?.as_array()?;
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
    if !local_min.iter().all(|value| value.is_finite()) { return None; }
    let local_center = [
        (local_min[0] + local_max[0]) * 0.5,
        (local_min[1] + local_max[1]) * 0.5,
        (local_min[2] + local_max[2]) * 0.5,
    ];
    let mut axes = [[0.0; 3]; 3];
    let mut half_extents = [0.0; 3];
    for axis in 0..3 {
        let raw = [matrix[axis], matrix[axis + 4], matrix[axis + 8]];
        let length = (raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2]).sqrt().max(1.0e-6);
        axes[axis] = [raw[0] / length, raw[1] / length, raw[2] / length];
        half_extents[axis] = (local_max[axis] - local_min[axis]) * 0.5 * length;
    }
    let center = transform_point(&matrix, local_center);
    Some(make_bounds(id, center, half_extents, axes))
}

fn assimp_matrix(node: &serde_json::Value) -> Option<[f32; 16]> {
    let values = node.get("transformation")?.as_array()?;
    let mut matrix = [0.0; 16];
    for (index, value) in values.iter().take(16).enumerate() { matrix[index] = value.as_f64()? as f32; }
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
    for extent in &mut collider.half_extents { *extent = extent.max(0.01); }
    make_bounds(collider.id, collider.center, collider.half_extents, collider.axes)
}

fn combined_bounds(colliders: &[FieldCollider]) -> Option<FieldCollider> {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for collider in colliders {
        for axis in 0..3 { min[axis] = min[axis].min(collider.min[axis]); max[axis] = max[axis].max(collider.max[axis]); }
    }
    min[0].is_finite().then(|| {
        let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, (min[2] + max[2]) * 0.5];
        make_bounds("robot-envelope".into(), center, [(max[0]-min[0])*0.5, (max[1]-min[1])*0.5, (max[2]-min[2])*0.5], default_axes())
    })
}

fn make_bounds(id: String, center: [f32; 3], half_extents: [f32; 3], axes: [[f32; 3]; 3]) -> FieldCollider {
    let mut min = center;
    let mut max = center;
    for world_axis in 0..3 {
        let radius = (0..3).map(|local_axis| axes[local_axis][world_axis].abs() * half_extents[local_axis]).sum::<f32>();
        min[world_axis] -= radius;
        max[world_axis] += radius;
    }
    FieldCollider { id, min, max, center, half_extents, axes }
}
