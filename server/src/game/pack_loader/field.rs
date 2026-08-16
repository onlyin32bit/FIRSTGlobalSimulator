use super::*;

pub(super) fn load_field_definition(
    physics: &serde_json::Value,
    semantics: &serde_json::Value,
    scoring: &ScoringConfig,
) -> Result<FieldDefinition, GameError> {
    let mut definition = FieldDefinition::default();
    // Keep planar collision surfaces too. Most authored field collision
    // meshes are planes; they are extruded into thin server-side volumes
    // below so a robot cannot drive through the visible field structure.
    let authored_bounds = assimp_bounds(&physics, false);
    if let Some((_, min, max)) = authored_bounds.iter().find(|(id, _, _)| id == "RISER.001") {
        definition.boundary = FieldBoundary {
            min: *min,
            max: *max,
        };
        definition.floor_height_m = max[1];
    }
    definition.colliders = assimp_colliders(physics)
        .into_iter()
        .filter(|(id, min, max, _, _, _)| {
            // The guard rail and riser provide the boundary/floor. Their
            // visual bounds must not become solid cuboids. Likewise, broad
            // cross-field assemblies are render geometry, not local blocks.
            let brace = matches!(
                id.as_str(),
                "Cylinder" | "Cylinder.001" | "Cylinder.002" | "Cylinder.003"
            );
            !matches!(id.as_str(), "GUARD_RAIL.001" | "RISER.001")
                && (brace || (max[0] - min[0] <= 2.5 && max[2] - min[2] <= 2.5))
        })
        .map(|(id, _min, _max, center, mut half_extents, axes)| {
            // Rapier can contact a zero-thickness triangle mesh, but the
            // lightweight host solver needs a small volume to prevent a
            // moving robot from tunnelling through an authored panel. Keep
            // the authored OBB orientation and add thickness in its local
            // frame, rather than inflating the world AABB.
            const MIN_THICKNESS_M: f32 = 0.05;
            for extent in &mut half_extents {
                *extent = extent.max(MIN_THICKNESS_M * 0.5);
            }
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
        })
        .collect();
    let mut semantic_bounds = BTreeMap::new();
    for child in assimp_children(&semantics) {
        let Some(id) = child.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if child.get("meshes").is_some() {
            if let Some((_, min, max)) = assimp_bound(child, &semantics, false) {
                semantic_bounds.insert(id.to_string(), (min, max));
                definition.triggers.push(FieldTrigger {
                    id: id.into(),
                    min,
                    max,
                });
            }
        } else if let Some(position) = assimp_translation(child) {
            definition.anchors.insert(id.into(), position);
        }
    }
    definition.scoring_targets = scoring
        .targets
        .iter()
        .filter_map(|target| {
            let Some((min, max)) = semantic_bounds.get(&target.semantic_id) else {
                warn!(target = %target.id, semantic = %target.semantic_id, "Scoring target has no semantic bounds");
                return None;
            };
            let (min, max) = target
                .area
                .as_ref()
                .map(|area| (area.min, area.max))
                .unwrap_or((*min, *max));
            Some(FieldScoringTarget {
                id: target.id.clone(),
                kind: target.kind.clone(),
                alliance: target.alliance.clone(),
                points: target.points,
                enabled: target.enabled,
                requires_robot_outtake: target.requires_robot_outtake,
                min,
                max,
                retention: target.retention.clone(),
            })
        })
        .collect();
    info!(
        colliders = definition.colliders.len(),
        anchors = definition.anchors.len(),
        triggers = definition.triggers.len(),
        boundary_min = ?definition.boundary.min,
        boundary_max = ?definition.boundary.max,
        floor_height_m = definition.floor_height_m,
        "Loaded field physics and semantics"
    );
    Ok(definition)
}

fn assimp_children(scene: &serde_json::Value) -> Vec<&serde_json::Value> {
    scene
        .get("rootnode")
        .and_then(|node| node.get("children"))
        .and_then(serde_json::Value::as_array)
        .map(|children| children.iter().collect())
        .unwrap_or_default()
}

fn assimp_bounds(scene: &serde_json::Value, solid_only: bool) -> Vec<(String, [f32; 3], [f32; 3])> {
    assimp_children(scene)
        .into_iter()
        .filter_map(|child| assimp_bound(child, scene, solid_only))
        .collect()
}

fn assimp_colliders(
    scene: &serde_json::Value,
) -> Vec<(
    String,
    [f32; 3],
    [f32; 3],
    [f32; 3],
    [f32; 3],
    [[f32; 3]; 3],
)> {
    assimp_children(scene)
        .into_iter()
        .filter_map(|child| {
            let id = child.get("name")?.as_str()?.to_string();
            let mesh_index = child.get("meshes")?.as_array()?.first()?.as_u64()? as usize;
            let vertices = scene
                .get("meshes")?
                .as_array()?
                .get(mesh_index)?
                .get("vertices")?
                .as_array()?;
            let matrix = assimp_matrix(child)?;
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
            let local_half = [
                (local_max[0] - local_min[0]) * 0.5,
                (local_max[1] - local_min[1]) * 0.5,
                (local_max[2] - local_min[2]) * 0.5,
            ];
            let center = transform_point(&matrix, local_center);
            let raw_axes = [
                [matrix[0], matrix[4], matrix[8]],
                [matrix[1], matrix[5], matrix[9]],
                [matrix[2], matrix[6], matrix[10]],
            ];
            let mut axes = [[0.0; 3]; 3];
            let mut half_extents = [0.0; 3];
            for axis in 0..3 {
                let length = (raw_axes[axis][0] * raw_axes[axis][0]
                    + raw_axes[axis][1] * raw_axes[axis][1]
                    + raw_axes[axis][2] * raw_axes[axis][2])
                    .sqrt()
                    .max(1.0e-6);
                axes[axis] = [
                    raw_axes[axis][0] / length,
                    raw_axes[axis][1] / length,
                    raw_axes[axis][2] / length,
                ];
                half_extents[axis] = local_half[axis] * length;
            }
            let mut min = center;
            let mut max = center;
            for world_axis in 0..3 {
                let radius = (0..3)
                    .map(|local_axis| axes[local_axis][world_axis].abs() * half_extents[local_axis])
                    .sum::<f32>();
                min[world_axis] -= radius;
                max[world_axis] += radius;
            }
            Some((id, min, max, center, half_extents, axes))
        })
        .collect()
}

fn assimp_bound(
    child: &serde_json::Value,
    scene: &serde_json::Value,
    solid_only: bool,
) -> Option<(String, [f32; 3], [f32; 3])> {
    let id = child.get("name")?.as_str()?.to_string();
    let mesh_index = child.get("meshes")?.as_array()?.first()?.as_u64()? as usize;
    let vertices = scene
        .get("meshes")?
        .as_array()?
        .get(mesh_index)?
        .get("vertices")?
        .as_array()?;
    let matrix = assimp_matrix(child)?;
    let mut local_min = [f32::INFINITY; 3];
    let mut local_max = [f32::NEG_INFINITY; 3];
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for xyz in vertices.chunks_exact(3) {
        let local = [
            xyz[0].as_f64()? as f32,
            xyz[1].as_f64()? as f32,
            xyz[2].as_f64()? as f32,
        ];
        for axis in 0..3 {
            local_min[axis] = local_min[axis].min(local[axis]);
            local_max[axis] = local_max[axis].max(local[axis]);
        }
        let point = transform_point(&matrix, local);
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    if solid_only && (0..3).any(|axis| local_max[axis] - local_min[axis] < 0.01) {
        return None;
    }
    min[0].is_finite().then_some((id, min, max))
}

fn assimp_translation(child: &serde_json::Value) -> Option<[f32; 3]> {
    let matrix = assimp_matrix(child)?;
    Some([matrix[3], matrix[7], matrix[11]])
}

fn assimp_matrix(child: &serde_json::Value) -> Option<[f32; 16]> {
    let values = child.get("transformation")?.as_array()?;
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
