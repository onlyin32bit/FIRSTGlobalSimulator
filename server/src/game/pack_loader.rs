use super::error::GameError;
use super::rhai_engine::{RhaiEngine, RuleScriptMetadata};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::fs;
use std::collections::BTreeMap;
use tracing::{error, info, warn};

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
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GamePackMetadata {
    pub manifest: GamePackManifest,
    pub scripts: Vec<RuleScriptMetadata>,
    pub arena: ArenaConfig,
    pub field_definition: FieldDefinition,
}

/// Server-ready subset of the authored Assimp field files. The GLB is only a
/// renderer asset; these bounds and anchors are the authoritative simulation
/// inputs shared by every match using this pack.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct FieldDefinition {
    pub colliders: Vec<FieldCollider>,
    pub anchors: BTreeMap<String, [f32; 3]>,
    pub triggers: Vec<FieldTrigger>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FieldCollider {
    pub id: String,
    pub min: [f32; 3],
    pub max: [f32; 3],
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
    pub spawn_radius: f32,
    pub spawn_height: f32,
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

    pub fn load_manifest(&self, path: &str) -> Result<GamePackManifest, GameError> {
        let content = fs::read_to_string(path).map_err(|_| {
            error!("Could not find manifest at {}", path);
            GameError::ManifestNotFound
        })?;

        let manifest: GamePackManifest = serde_json::from_str(&content).map_err(|e| {
            error!("Failed to parse manifest json: {}", e);
            GameError::ManifestParseError(e.to_string())
        })?;

        let req = VersionReq::parse(&manifest.engine_version).unwrap();
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
        Ok(manifest)
    }

    pub fn load_pack(&self, manifest_path: &str) -> Result<GamePackMetadata, GameError> {
        let manifest = self.load_manifest(manifest_path)?;
        let root = std::path::Path::new(manifest_path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let engine = RhaiEngine::new();
        let mut scripts = Vec::with_capacity(manifest.scripts.len());
        let mut arena = None;
        for script_path in manifest.scripts.values() {
            let path = root.join(script_path);
            let path_string = path.to_string_lossy().to_string();
            let metadata = engine
                .inspect_script(&path_string)
                .map_err(GameError::ScriptCompilationError)?;
            info!(
                "Loaded Rhai rule script {} ({} functions)",
                path_string,
                metadata.functions.len()
            );
            if script_path.ends_with("arena.rhai") {
                arena = Some(
                    engine
                        .load_arena_config(&path_string)
                        .map_err(GameError::ScriptCompilationError)?,
                );
            }
            scripts.push(metadata);
        }
        let arena = arena.ok_or_else(|| {
            GameError::ScriptCompilationError("The pack must define an arena.rhai script".into())
        })?;
        let field_definition = load_field_definition(root, &manifest.field)?;
        Ok(GamePackMetadata {
            manifest,
            scripts,
            arena,
            field_definition,
        })
    }
}

fn load_field_definition(root: &std::path::Path, field: &serde_json::Value) -> Result<FieldDefinition, GameError> {
    let physics_name = field.get("physics").and_then(serde_json::Value::as_str)
        .ok_or_else(|| GameError::ManifestParseError("Pack field.physics is missing".into()))?;
    let semantics_name = field.get("semantics").and_then(serde_json::Value::as_str)
        .ok_or_else(|| GameError::ManifestParseError("Pack field.semantics is missing".into()))?;
    let physics: serde_json::Value = serde_json::from_str(&fs::read_to_string(root.join(physics_name))
        .map_err(|error| GameError::ManifestParseError(error.to_string()))?)
        .map_err(|error| GameError::ManifestParseError(error.to_string()))?;
    let semantics: serde_json::Value = serde_json::from_str(&fs::read_to_string(root.join(semantics_name))
        .map_err(|error| GameError::ManifestParseError(error.to_string()))?)
        .map_err(|error| GameError::ManifestParseError(error.to_string()))?;

    let mut definition = FieldDefinition::default();
    definition.colliders = assimp_bounds(&physics, true)
        .into_iter()
        .map(|(id, min, max)| FieldCollider { id, min, max })
        .collect();
    for child in assimp_children(&semantics) {
        let Some(id) = child.get("name").and_then(serde_json::Value::as_str) else { continue; };
        if child.get("meshes").is_some() {
            if let Some((_, min, max)) = assimp_bound(child, &semantics, false) {
                definition.triggers.push(FieldTrigger { id: id.into(), min, max });
            }
        } else if let Some(position) = assimp_translation(child) {
            definition.anchors.insert(id.into(), position);
        }
    }
    info!(colliders = definition.colliders.len(), anchors = definition.anchors.len(), triggers = definition.triggers.len(), "Loaded field physics and semantics");
    Ok(definition)
}

fn assimp_children(scene: &serde_json::Value) -> Vec<&serde_json::Value> {
    scene.get("rootnode").and_then(|node| node.get("children"))
        .and_then(serde_json::Value::as_array).map(|children| children.iter().collect()).unwrap_or_default()
}

fn assimp_bounds(scene: &serde_json::Value, solid_only: bool) -> Vec<(String, [f32; 3], [f32; 3])> {
    assimp_children(scene).into_iter().filter_map(|child| assimp_bound(child, scene, solid_only)).collect()
}

fn assimp_bound(child: &serde_json::Value, scene: &serde_json::Value, solid_only: bool) -> Option<(String, [f32; 3], [f32; 3])> {
    let id = child.get("name")?.as_str()?.to_string();
    let mesh_index = child.get("meshes")?.as_array()?.first()?.as_u64()? as usize;
    let vertices = scene.get("meshes")?.as_array()?.get(mesh_index)?.get("vertices")?.as_array()?;
    let matrix = assimp_matrix(child)?;
    let mut local_min = [f32::INFINITY; 3];
    let mut local_max = [f32::NEG_INFINITY; 3];
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for xyz in vertices.chunks_exact(3) {
        let local = [xyz[0].as_f64()? as f32, xyz[1].as_f64()? as f32, xyz[2].as_f64()? as f32];
        for axis in 0..3 { local_min[axis] = local_min[axis].min(local[axis]); local_max[axis] = local_max[axis].max(local[axis]); }
        let point = transform_point(&matrix, local);
        for axis in 0..3 { min[axis] = min[axis].min(point[axis]); max[axis] = max[axis].max(point[axis]); }
    }
    if solid_only && (0..3).any(|axis| local_max[axis] - local_min[axis] < 0.01) { return None; }
    min[0].is_finite().then_some((id, min, max))
}

fn assimp_translation(child: &serde_json::Value) -> Option<[f32; 3]> {
    let matrix = assimp_matrix(child)?;
    Some([matrix[3], matrix[7], matrix[11]])
}

fn assimp_matrix(child: &serde_json::Value) -> Option<[f32; 16]> {
    let values = child.get("transformation")?.as_array()?;
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

#[cfg(test)]
mod tests {
    use super::PackLoader;

    #[test]
    fn loads_manifest_and_all_rhai_rules() {
        let loader = PackLoader::new("0.1.0");
        let metadata = loader
            .load_pack("../pkgs/games/fgc-2026/manifest.json")
            .unwrap();
        assert_eq!(metadata.manifest.id, "fgc-2026");
        assert_eq!(metadata.scripts.len(), 4);
        assert!(metadata.arena.object_count >= 1000);
        assert_eq!(metadata.arena.ball.diameter_m, 0.100);
        assert_eq!(metadata.arena.ball.mass_kg, 0.062);
        assert_eq!(metadata.arena.ball.inertia_factor, 0.4);
        assert_eq!(metadata.arena.ball.drag_coefficient, 0.47);
        assert_eq!(metadata.arena.floor.material, "low-pile carpet");
        assert!(metadata.arena.floor.rolling_resistance_mps2 > 0.0);
        assert!(metadata.arena.robot.intake_enabled);
        assert!(metadata.arena.ramp.enabled);
        assert!(!metadata.field_definition.colliders.is_empty());
        assert!(metadata.field_definition.anchors.contains_key("redSpawn1"));
        assert!(metadata.field_definition.anchors.contains_key("blueSpawn3"));
        assert!(metadata.field_definition.anchors.contains_key("EXTballspawn"));
        assert!(metadata.field_definition.triggers.iter().any(|trigger| trigger.id == "EXTscore"));
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
