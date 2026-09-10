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
    pub scoring: ScoringConfig,
    #[serde(default, rename = "defaultRobot")]
    pub default_robot: Option<String>,
    #[serde(default)]
    pub robots: BTreeMap<String, PackRobotManifest>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PackRobotManifest {
    pub name: String,
    pub visual: String,
    pub physics: Option<String>,
    pub semantics: Option<String>,
    pub behavior: Option<String>,
    pub climber: Option<RobotClimberConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RobotClimberConfig {
    pub wheel_parts: Vec<String>,
    #[serde(default)]
    pub support_parts: Vec<String>,
    pub axle: [f32; 3],
    pub wheel_mass_kg: f32,
    pub groove_root_radius_m: f32,
    pub groove_outer_radius_m: f32,
    pub max_climb_speed_mps: f32,
    pub free_speed_radps: f32,
    pub stall_torque_nm: f32,
    pub brake_torque_nm: f32,
    pub static_friction: f32,
    pub dynamic_friction: f32,
    pub contact_skin_m: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ScoringConfig {
    #[serde(default)]
    pub targets: Vec<ScoringTargetConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScoringTargetConfig {
    pub id: String,
    pub semantic_id: String,
    pub kind: String,
    pub alliance: Option<String>,
    pub points: i32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub requires_robot_outtake: bool,
    pub area: Option<ScoringAreaConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScoringAreaConfig {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GamePackMetadata {
    pub manifest: GamePackManifest,
    pub scripts: Vec<RuleScriptMetadata>,
    pub arena: ArenaConfig,
    pub field_definition: FieldDefinition,
    pub default_robot: Option<RobotDefinition>,
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
    pub scripts: BTreeMap<String, String>,
    #[serde(default)]
    pub robots: BTreeMap<String, RobotRuntimeAssets>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RobotRuntimeAssets {
    pub physics: serde_json::Value,
    pub semantics: serde_json::Value,
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
    pub scoring_targets: Vec<FieldScoringTarget>,
    /// Top surface of the authored playable floor/riser in metres.
    pub floor_height_m: f32,
    /// The playable X/Z envelope, derived from the riser's inner footprint.
    /// The guard rail stays visual geometry rather than a giant solid AABB.
    pub boundary: FieldBoundary,
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
pub struct FieldScoringTarget {
    pub id: String,
    pub kind: String,
    pub alliance: Option<String>,
    pub points: i32,
    pub enabled: bool,
    pub requires_robot_outtake: bool,
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
    pub transfer_surface_speed_mps: f32,
    pub transfer_normal_force_n: f32,
    /// Ball storage capacity of the on-robot hopper (0 = no storage, balls
    /// simply deflect off the chassis as before).
    pub storage_capacity: usize,
    /// Max balls pulled into storage per second while intake is powered.
    pub intake_rate_bps: f32,
    /// Max balls ejected per second while outtake is powered.
    pub outtake_rate_bps: f32,
    /// Flywheel launch speed in metres per second.
    pub outtake_velocity_mps: f32,
    pub outtake_normal_force_n: f32,
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
        let field_definition = load_field_definition(
            &snapshot.field_physics,
            &snapshot.field_semantics,
            &snapshot.manifest.scoring,
        )?;
        let default_robot = snapshot
            .manifest
            .default_robot
            .as_deref()
            .map(|id| {
                let assets = snapshot.robots.get(id).ok_or_else(|| {
                    GameError::ManifestParseError(format!(
                        "Runtime snapshot is missing collision or semantics for default robot {id}"
                    ))
                })?;
                let climber = snapshot
                    .manifest
                    .robots
                    .get(id)
                    .and_then(|robot| robot.climber.clone());
                load_robot_definition(id, &assets.physics, &assets.semantics, climber)
            })
            .transpose()?;
        Ok(GamePackMetadata {
            manifest: snapshot.manifest,
            scripts,
            arena,
            field_definition,
            default_robot,
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
        let scripts = manifest
            .scripts
            .values()
            .map(|path| {
                std::fs::read_to_string(root.join(path))
                    .map(|source| (path.clone(), source))
                    .map_err(|error| GameError::ManifestParseError(error.to_string()))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let robots = manifest
            .robots
            .iter()
            .filter_map(|(id, robot)| match (&robot.physics, &robot.semantics) {
                (Some(physics), Some(semantics)) => Some((id, physics, semantics)),
                _ => None,
            })
            .map(|(id, physics, semantics)| {
                let physics = serde_json::from_str(
                    &std::fs::read_to_string(root.join(physics))
                        .map_err(|error| GameError::ManifestParseError(error.to_string()))?,
                )
                .map_err(|error| GameError::ManifestParseError(error.to_string()))?;
                let semantics = serde_json::from_str(
                    &std::fs::read_to_string(root.join(semantics))
                        .map_err(|error| GameError::ManifestParseError(error.to_string()))?,
                )
                .map_err(|error| GameError::ManifestParseError(error.to_string()))?;
                Ok((id.clone(), RobotRuntimeAssets { physics, semantics }))
            })
            .collect::<Result<BTreeMap<_, _>, GameError>>()?;
        self.load_runtime_snapshot(GamePackRuntimeSnapshot {
            manifest,
            field_physics,
            field_semantics,
            scripts,
            robots,
        })
    }
}

mod field;
use field::load_field_definition;
mod robot;
use robot::load_robot_definition;
pub use robot::{RobotDefinition, RobotSemanticKind};

#[cfg(test)]
#[path = "pack_loader/tests.rs"]
mod tests;
