use super::error::GameError;
use super::rhai_engine::{RhaiEngine, RuleScriptMetadata};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tracing::{info, warn};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GamePackManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(rename = "engineVersion")]
    pub engine_version: String,
    #[serde(default)]
    pub field: serde_json::Value,
    #[serde(default)]
    pub objects: Vec<serde_json::Value>,
    #[serde(default)]
    pub phases: Vec<serde_json::Value>,
    #[serde(default)]
    pub scripts: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub robots: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GamePackMetadata {
    pub manifest: GamePackManifest,
    pub scripts: Vec<RuleScriptMetadata>,
    pub arena: ArenaConfig,
    pub field_definition: FieldDefinition,
    /// Raw Rhai source belongs to the API pack snapshot, not the filesystem.
    /// It stays process-local and is never sent to connected clients.
    #[serde(skip)]
    pub script_sources: BTreeMap<String, String>,
}

/// The API-owned source of truth a match host fetches before it accepts users.
/// Visual GLB data is intentionally excluded: a host only needs physics,
/// semantic data, and executable rules.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GamePackRuntimeSnapshot {
    pub manifest: GamePackManifest,
    pub field_physics: serde_json::Value,
    pub field_semantics: serde_json::Value,
    pub robot_physics: serde_json::Value,
    pub scripts: BTreeMap<String, String>,
}

/// Server-ready subset of the authored Assimp field files. The GLB is only a
/// renderer asset; these collision volumes and anchors are the authoritative
/// simulation inputs shared by every match using this pack.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct FieldDefinition {
    pub colliders: Vec<FieldCollider>,
    pub anchors: BTreeMap<String, [f32; 3]>,
    pub triggers: Vec<FieldTrigger>,
    /// Top surface of the authored playable floor/riser in metres.
    pub floor_height_m: f32,
    /// The playable X/Z envelope, derived from the riser's inner footprint.
    /// The guard rail stays visual geometry rather than a giant solid AABB.
    pub boundary: FieldBoundary,
    /// Robot-local collision volumes authored in bot.physics.json. This is
    /// kept with the loaded field definition because the runtime already
    /// receives both sets of authored geometry together.
    #[serde(skip)]
    pub robot_colliders: Vec<FieldCollider>,
}

/// Axis-aligned playable footprint of a game-pack field. The server uses this
/// for its cheap perimeter constraint, while authored local obstacles remain
/// separate collision volumes.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FieldBoundary {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Default for FieldBoundary {
    fn default() -> Self {
        Self {
            min: [-8.0, 0.0, -8.0],
            max: [8.0, 0.0, 8.0],
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FieldCollider {
    pub id: String,
    pub min: [f32; 3],
    pub max: [f32; 3],
    #[serde(default)]
    pub center: [f32; 3],
    #[serde(default)]
    pub half_extents: [f32; 3],
    #[serde(default = "default_axes")]
    pub axes: [[f32; 3]; 3],
}

fn default_axes() -> [[f32; 3]; 3] {
    [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FieldTrigger {
    pub id: String,
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArenaConfig {
    pub physics_backend: String,
    pub solver: SolverConfig,
    pub object_id: String,
    pub object_count: usize,
    /// Radius of the EXT dispenser nozzle, not a pre-spawn field scatter.
    pub spawn_radius: f32,
    pub spawn_height: f32,
    /// Temporary pack-space correction for an authored semantic anchor.
    /// Delete once the Blender EXTballspawn is placed correctly.
    pub spawn_offset_y_m: f32,
    pub spawn_release_seconds: f32,
    pub spawn_fountain_vertical_speed_mps: f32,
    pub spawn_fountain_forward_speed_mps: f32,
    pub spawn_fountain_spread_mps: f32,
    pub gravity_scale: f32,
    pub ball_to_ball_collisions: bool,
    pub color: String,
    pub ball: BallPhysicsConfig,
    pub floor: FloorPhysicsConfig,
    pub robot: RobotPhysicsConfig,
    pub goal_wall: SurfacePhysicsConfig,
    pub metal_wall: SurfacePhysicsConfig,
    pub ramp: RampPhysicsConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RestitutionCurveConfig {
    pub low_speed: f32,
    pub high_speed: f32,
    pub transition_speed_mps: f32,
    pub exponent: f32,
}

impl RestitutionCurveConfig {
    pub fn at_speed(&self, speed: f32) -> f32 {
        let normalized =
            (speed.max(0.0) / self.transition_speed_mps.max(0.001)).powf(self.exponent.max(0.05));
        (self.high_speed + (self.low_speed - self.high_speed) * (-normalized).exp()).clamp(0.0, 1.0)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SolverConfig {
    pub position_iterations: usize,
    pub velocity_iterations: usize,
    pub contact_compliance: f32,
    pub max_depenetration_speed_mps: f32,
    pub max_ball_speed_mps: f32,
    pub max_ball_angular_speed_radps: f32,
    pub sleep_linear_threshold_mps: f32,
    pub sleep_angular_threshold_radps: f32,
    pub sleep_after_seconds: f32,
    pub restitution_velocity_threshold_mps: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BallPhysicsConfig {
    pub material: String,
    pub diameter_m: f32,
    pub diameter_tolerance_m: f32,
    pub mass_kg: f32,
    pub friction: f32,
    pub restitution: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub rolling_resistance_mps2: f32,
    pub soft_ccd_prediction_m: f32,
    pub inertia_factor: f32,
    pub drag_coefficient: f32,
    pub air_density_kg_m3: f32,
    pub ball_friction: f32,
    pub restitution_curve: RestitutionCurveConfig,
}

impl BallPhysicsConfig {
    pub fn radius_m(&self) -> f32 {
        self.diameter_m * 0.5
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FloorPhysicsConfig {
    pub material: String,
    pub friction: f32,
    pub restitution: f32,
    pub static_friction: f32,
    pub dynamic_friction: f32,
    pub rolling_resistance_mps2: f32,
    pub restitution_curve: RestitutionCurveConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SurfacePhysicsConfig {
    pub material: String,
    pub static_friction: f32,
    pub dynamic_friction: f32,
    pub restitution_curve: RestitutionCurveConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RampPhysicsConfig {
    pub enabled: bool,
    pub center_x: f32,
    pub start_z: f32,
    pub width_m: f32,
    pub length_m: f32,
    pub angle_deg: f32,
    pub surface: SurfacePhysicsConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RobotPhysicsConfig {
    pub mass_kg: f32,
    pub width_m: f32,
    pub height_m: f32,
    pub length_m: f32,
    pub track_width_m: f32,
    pub traction_friction: f32,
    pub surface_friction: f32,
    pub restitution: f32,
    pub rolling_resistance: f32,
    pub max_speed_mps: f32,
    pub max_acceleration_mps2: f32,
    pub max_deceleration_mps2: f32,
    pub max_drive_force_n: f32,
    pub max_drive_power_w: f32,
    pub max_brake_force_n: f32,
    pub max_turn_rate_radps: f32,
    pub max_angular_acceleration_radps2: f32,
    pub lateral_grip_mps2: f32,
    pub restitution_curve: RestitutionCurveConfig,
    pub intake_enabled: bool,
    pub intake_width_m: f32,
    pub intake_radius_m: f32,
    pub intake_forward_offset_m: f32,
    pub intake_center_height_m: f32,
    pub intake_surface_speed_mps: f32,
    pub intake_friction: f32,
    pub intake_normal_force_n: f32,
    pub intake_restitution_curve: RestitutionCurveConfig,
    /// Ball storage capacity of the on-robot hopper (0 = no storage, balls
    /// simply deflect off the chassis as before).
    pub storage_capacity: usize,
    /// Max balls pulled into storage per second while intake is powered.
    pub intake_rate_bps: f32,
    /// Max balls ejected per second while outtake is powered.
    pub outtake_rate_bps: f32,
    /// Flywheel launch speed in metres per second.
    pub outtake_velocity_mps: f32,
    /// Flywheel launch pitch angle above horizontal, in degrees.
    pub outtake_angle_deg: f32,
    /// Width of the flywheel mouth. 3–4 WILDFIRE (100 mm) wide ≈ 0.30–0.40 m.
    pub flywheel_width_m: f32,
    /// Forward offset of the flywheel exit from the chassis centre.
    pub outtake_forward_offset_m: f32,
    /// Height of the flywheel exit above the floor.
    pub outtake_height_m: f32,
}

pub struct PackLoader {
    engine_version: Version,
}

impl PackLoader {
    pub fn new(current_engine_version: &str) -> Self {
        Self {
            engine_version: Version::parse(current_engine_version).unwrap(),
        }
    }

    fn validate_manifest(&self, manifest: &GamePackManifest) -> Result<(), GameError> {
        let req = VersionReq::parse(&manifest.engine_version)
            .map_err(|error| GameError::ManifestParseError(error.to_string()))?;
        if !req.matches(&self.engine_version) {
            warn!(
                "Engine version {} is NOT compatible with Pack requirement {}",
                self.engine_version, manifest.engine_version
            );
            return Err(GameError::IncompatibleEngineVersion {
                engine: self.engine_version.to_string(),
                pack: manifest.engine_version.clone(),
            });
        }

        info!(
            "Successfully loaded compatible Game Pack: {} v{}",
            manifest.name, manifest.version
        );
        Ok(())
    }

    /// Compile a snapshot fetched from the control-plane API. There is no
    /// pack path here by design: a game-server node must not own pack files.
    pub fn load_runtime_snapshot(
        &self,
        snapshot: GamePackRuntimeSnapshot,
    ) -> Result<GamePackMetadata, GameError> {
        self.validate_manifest(&snapshot.manifest)?;
        let engine = RhaiEngine::new();
        let mut scripts = Vec::with_capacity(snapshot.manifest.scripts.len());
        let mut arena = None;
        for script_path in snapshot.manifest.scripts.values() {
            let source = snapshot.scripts.get(script_path).ok_or_else(|| {
                GameError::ManifestParseError(format!(
                    "Runtime snapshot is missing rule source {script_path}"
                ))
            })?;
            let metadata = engine
                .inspect_source(script_path, source)
                .map_err(GameError::ScriptCompilationError)?;
            info!(
                "Loaded Rhai rule script {} ({} functions)",
                script_path,
                metadata.functions.len()
            );
            if script_path.ends_with("arena.rhai") {
                arena = Some(
                    engine
                        .load_arena_config_source(source)
                        .map_err(GameError::ScriptCompilationError)?,
                );
            }
            scripts.push(metadata);
        }
        let arena = arena.ok_or_else(|| {
            GameError::ScriptCompilationError("The pack must define an arena.rhai script".into())
        })?;
        let field_definition =
            load_field_definition(&snapshot.field_physics, &snapshot.field_semantics)?;
        let mut field_definition = field_definition;
        field_definition.robot_colliders = load_robot_colliders(&snapshot.robot_physics)?;
        Ok(GamePackMetadata {
            manifest: snapshot.manifest,
            scripts,
            arena,
            field_definition,
            script_sources: snapshot.scripts,
        })
    }

    /// Test-only fixture adapter. Production code must use
    /// `load_runtime_snapshot` after fetching the API-owned pack.
    #[cfg(test)]
    pub fn load_pack(&self, manifest_path: &str) -> Result<GamePackMetadata, GameError> {
        let root = std::path::Path::new(manifest_path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let manifest: GamePackManifest = serde_json::from_str(
            &std::fs::read_to_string(manifest_path).map_err(|_| GameError::ManifestNotFound)?,
        )
        .map_err(|error| GameError::ManifestParseError(error.to_string()))?;
        let physics_path = manifest
            .field
            .get("physics")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| GameError::ManifestParseError("Pack field.physics is missing".into()))?;
        let semantics_path = manifest
            .field
            .get("semantics")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                GameError::ManifestParseError("Pack field.semantics is missing".into())
            })?;
        let field_physics = serde_json::from_str(
            &std::fs::read_to_string(root.join(physics_path))
                .map_err(|error| GameError::ManifestParseError(error.to_string()))?,
        )
        .map_err(|error| GameError::ManifestParseError(error.to_string()))?;
        let field_semantics = serde_json::from_str(
            &std::fs::read_to_string(root.join(semantics_path))
                .map_err(|error| GameError::ManifestParseError(error.to_string()))?,
        )
        .map_err(|error| GameError::ManifestParseError(error.to_string()))?;
        let robot_physics_path = manifest
            .robots
            .get("StarterBot")
            .and_then(|robot| robot.get("physics"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| GameError::ManifestParseError("StarterBot physics is missing".into()))?;
        let robot_physics = serde_json::from_str(
            &std::fs::read_to_string(
                root.parent()
                    .and_then(std::path::Path::parent)
                    .unwrap_or(root)
                    .join(robot_physics_path),
            )
            .map_err(|error| GameError::ManifestParseError(error.to_string()))?,
        )
        .map_err(|error| GameError::ManifestParseError(error.to_string()))?;
        let scripts = manifest
            .scripts
            .values()
            .map(|path| {
                let source_path = if path.starts_with("robots/") {
                    root.parent()
                        .and_then(std::path::Path::parent)
                        .unwrap_or(root)
                        .join(path)
                } else {
                    root.join(path)
                };
                std::fs::read_to_string(source_path)
                    .map(|source| (path.clone(), source))
                    .map_err(|error| GameError::ManifestParseError(error.to_string()))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        self.load_runtime_snapshot(GamePackRuntimeSnapshot {
            manifest,
            field_physics,
            field_semantics,
            robot_physics,
            scripts,
        })
    }
}

fn load_robot_colliders(physics: &serde_json::Value) -> Result<Vec<FieldCollider>, GameError> {
    const MIN_THICKNESS_M: f32 = 0.01;
    Ok(assimp_colliders(physics)
        .into_iter()
        .map(|(id, _min, _max, center, mut half_extents, axes)| {
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
        .collect())
}

fn load_field_definition(
    physics: &serde_json::Value,
    semantics: &serde_json::Value,
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
            !matches!(id.as_str(), "GUARD_RAIL.001" | "RISER.001")
                && max[0] - min[0] <= 2.5
                && max[2] - min[2] <= 2.5
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
    for child in assimp_children(&semantics) {
        let Some(id) = child.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if child.get("meshes").is_some() {
            if let Some((_, min, max)) = assimp_bound(child, &semantics, false) {
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

#[cfg(test)]
mod tests {
    use super::{GamePackRuntimeSnapshot, PackLoader};
    use std::collections::BTreeMap;

    #[test]
    fn loads_manifest_and_all_rhai_rules() {
        let loader = PackLoader::new("0.1.0");
        let root = std::path::Path::new("../pkgs/games/fgc-2026");
        let manifest =
            serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).unwrap())
                .unwrap();
        let field_physics = serde_json::from_str(
            &std::fs::read_to_string(root.join("field.physics.json")).unwrap(),
        )
        .unwrap();
        let field_semantics = serde_json::from_str(
            &std::fs::read_to_string(root.join("field.semantics.json")).unwrap(),
        )
        .unwrap();
        let robot_physics = serde_json::from_str(
            &std::fs::read_to_string("../pkgs/robots/StarterBot/bot.physics.json").unwrap(),
        )
        .unwrap();
        let scripts = [
            "rules/arena.rhai",
            "rules/penalties.rhai",
            "rules/robot.rhai",
            "rules/scoring.rhai",
            "robots/StarterBot/robot.rhai",
        ]
        .into_iter()
        .map(|path| {
            let source_path = if path.starts_with("robots/") {
                std::path::Path::new("../pkgs").join(path)
            } else {
                root.join(path)
            };
            (
                path.to_string(),
                std::fs::read_to_string(source_path).unwrap(),
            )
        })
        .collect::<BTreeMap<_, _>>();
        let metadata = loader
            .load_runtime_snapshot(GamePackRuntimeSnapshot {
                manifest,
                field_physics,
                field_semantics,
                robot_physics,
                scripts,
            })
            .unwrap();
        assert_eq!(metadata.manifest.id, "fgc-2026");
        assert_eq!(metadata.scripts.len(), 5);
        assert_eq!(metadata.arena.object_count, 500);
        assert_eq!(metadata.arena.ball.diameter_m, 0.100);
        assert_eq!(metadata.arena.ball.mass_kg, 0.062);
        assert_eq!(metadata.arena.ball.inertia_factor, 0.4);
        assert_eq!(metadata.arena.ball.drag_coefficient, 0.47);
        assert_eq!(metadata.arena.floor.material, "low-pile carpet");
        assert!(metadata.arena.floor.rolling_resistance_mps2 > 0.0);
        assert!(!metadata.arena.robot.intake_enabled);
        assert_eq!(metadata.arena.robot.mass_kg, 18.0);
        assert_eq!(metadata.arena.robot.width_m, 0.50);
        assert_eq!(metadata.arena.robot.height_m, 0.50);
        assert_eq!(metadata.arena.robot.length_m, 0.50);
        assert!(metadata.arena.ramp.enabled);
        assert!(metadata.field_definition.colliders.len() >= 70);
        assert_eq!(metadata.field_definition.robot_colliders.len(), 32);
        let front_wall = metadata
            .field_definition
            .colliders
            .iter()
            .find(|collider| collider.id == "blueSUfront")
            .expect("authored planar wall must be loaded");
        assert!(front_wall.max[2] - front_wall.min[2] >= 0.05 - f32::EPSILON);
        assert!(
            front_wall
                .half_extents
                .iter()
                .any(|extent| *extent >= 0.025)
        );
        assert!(metadata.field_definition.anchors.contains_key("redSpawn1"));
        assert!(metadata.field_definition.anchors.contains_key("blueSpawn3"));
        assert!(
            metadata
                .field_definition
                .anchors
                .contains_key("EXTballspawn")
        );
        assert!(
            metadata
                .field_definition
                .triggers
                .iter()
                .any(|trigger| trigger.id == "EXTscore")
        );
        assert!((metadata.field_definition.boundary.min[0] + 3.5).abs() < 0.01);
        assert!((metadata.field_definition.boundary.max[0] - 3.5).abs() < 0.01);
        assert!(metadata.field_definition.floor_height_m > 0.65);
        assert!(
            metadata
                .scripts
                .iter()
                .any(|script| script.path.ends_with("scoring.rhai"))
        );
        assert!(
            metadata
                .scripts
                .iter()
                .flat_map(|script| script.functions.iter())
                .any(|function| function.name == "validate_build")
        );
    }
}
