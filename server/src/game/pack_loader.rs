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
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GamePackMetadata {
    pub manifest: GamePackManifest,
    pub scripts: Vec<RuleScriptMetadata>,
    pub arena: ArenaConfig,
    pub field_definition: FieldDefinition,
    pub robot_colliders: Vec<FieldCollider>,
    /// Authorized semantic zones pulled from `bot.semantics.json` (IntakeZone,
    /// TransferZone, OuttakeZone). Empty when the pack ships no semantics:
    /// the mechanism buttons then have no pull force.
    pub semantic_zones: Vec<RobotSemanticZone>,
    /// Raw Rhai source belongs to the API pack snapshot, not the filesystem.
    /// It stays process-local and is never sent to connected clients.
    #[serde(skip)]
    pub script_sources: BTreeMap<String, String>,
}

/// The kind of an authored robot semantic zone. The simulator treats every
/// zone with the exact same directional-conveyor mechanism; the kind only
/// decides which mechanism power channel gates it and which force config it
/// uses.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SemanticZoneKind {
    Intake,
    Transfer,
    Outtake,
}

/// An authored robot semantic zone: the feed geometry for a mechanism and the
/// kind that selects its power channel and force configuration.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RobotSemanticZone {
    pub kind: SemanticZoneKind,
    pub geometry: RobotIntakeGeometry,
}

/// Robot-local mechanism geometry described in the authored robot semantics.
/// All coordinates are in robot-local metres (yaw applied per player at
/// runtime), and the Z-help caller — an IntakeZone mesh, a TransferZone mesh
/// or an OuttakeZone mesh — is described identically by this OBB + feed
/// direction.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RobotIntakeGeometry {
    /// OBB centre of the authored zone mesh in robot-local space.
    pub zone_center: [f32; 3],
    /// Half extents of the OBB along each of `zone_axes`.
    pub zone_half_extents: [f32; 3],
    /// Normalised local axes of the OBB in robot-local space. The mouth is a
    /// thin slot that spans the robot width, so the touch test uses this OBB
    /// instead of a fat bounding sphere that would magnetise balls from afar.
    pub zone_axes: [[f32; 3]; 3],
    /// Robot-local unit vector pointing along the mechanism feed direction,
    /// derived from the zone transform (its local +Z column points out of the
    /// mechanism mouth, so the conveyor pushes along the negated column).
    /// Never hardcoded world space: it is rotated per player with the same
    /// yaw convention as the physics colliders.
    pub direction: [f32; 3],
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
    /// Optional authored robot semantics (e.g. IntakeZone) mirrored from the
    /// API runtime response. Old packs without one deserialize as empty.
    #[serde(default)]
    pub robot_semantics: serde_json::Value,
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActuatorConfig {
    #[serde(default = "default_spin_axis")]
    pub spin_axis: [f32; 3],
    #[serde(default = "default_input_channel")]
    pub input_channel: String,
    #[serde(default = "default_target_surface_speed")]
    pub target_surface_speed_mps: f32,
    #[serde(default = "default_max_torque")]
    pub max_torque: f32,
    #[serde(default = "default_actuator_mass")]
    pub mass_kg: f32,
    #[serde(default = "default_actuator_friction")]
    pub friction: f32,
    /// Rigid wheel core with a bounded compliant tread/contact layer.
    #[serde(default = "default_contact_stiffness")]
    pub contact_stiffness_n_per_m: f32,
    #[serde(default = "default_contact_damping")]
    pub contact_damping_n_s_per_m: f32,
    #[serde(default = "default_max_compression")]
    pub max_compression_m: f32,
}

fn default_spin_axis() -> [f32; 3] {
    [1.0, 0.0, 0.0]
}
fn default_input_channel() -> String {
    "intake".into()
}
fn default_target_surface_speed() -> f32 {
    6.0
}
fn default_max_torque() -> f32 {
    15.0
}
fn default_actuator_mass() -> f32 {
    0.8
}
fn default_actuator_friction() -> f32 {
    0.9
}
fn default_contact_stiffness() -> f32 {
    500.0
}
fn default_contact_damping() -> f32 {
    5.0
}
fn default_max_compression() -> f32 {
    0.010
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
    #[serde(default)]
    pub actuator: Option<ActuatorConfig>,
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
    /// Maximum horizontal acceleration (m/s²) the directional IntakeZone
    /// conveyor applies to a ball near the mouth, along the authored feed
    /// direction. The ball stays a normal dynamic body — this is an ordinary
    /// acceleration integrated with gravity, not a teleport or set-position.
    pub intake_force_mps2: f32,
    /// Same directional-conveyor acceleration for the TransferZone. Matches
    /// `intake_force_mps2` semantics: the mechanism pushes a ball along the
    /// zone's authored feed direction as a normal dynamic body.
    pub transfer_force_mps2: f32,
    /// Same directional-conveyor acceleration for the OuttakeZone. Pushes a
    /// ball along the zone's authored feed direction (outward) as a normal
    /// dynamic body.
    pub outtake_force_mps2: f32,
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
        let robot_colliders = load_robot_colliders(&snapshot.robot_physics, &arena.robot);
        if robot_colliders.is_empty() {
            return Err(GameError::ManifestParseError(
                "The pack robot physics asset contains no collision volumes".into(),
            ));
        } else {
            info!(
                colliders = robot_colliders.len(),
                "Loaded authored robot collision volumes"
            );
        }
        Ok(GamePackMetadata {
            manifest: snapshot.manifest,
            scripts,
            arena,
            field_definition,
            robot_colliders: robot_colliders.clone(),
            semantic_zones: robot_semantic_zones(&snapshot.robot_semantics),
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
        let robot_physics = std::fs::read_to_string(
            root.parent()
                .and_then(std::path::Path::parent)
                .unwrap_or(root)
                .join("robots/StarterBot/bot.physics.json"),
        )
        .ok()
        .and_then(|source| serde_json::from_str(&source).ok());
        let robot_semantics = std::fs::read_to_string(
            root.parent()
                .and_then(std::path::Path::parent)
                .unwrap_or(root)
                .join("robots/StarterBot/bot.semantics.json"),
        )
        .ok()
        .and_then(|source| serde_json::from_str(&source).ok());
        self.load_runtime_snapshot(GamePackRuntimeSnapshot {
            manifest,
            field_physics,
            field_semantics,
            robot_physics: robot_physics.unwrap_or_else(|| serde_json::json!({})),
            robot_semantics: robot_semantics.unwrap_or_else(|| serde_json::json!({})),
            scripts,
        })
    }
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
        .filter(|(id, _min, _max, _center, half_extents, _axes)| {
            // The guard rail and riser provide the boundary/floor. Their
            // visual bounds must not become solid cuboids. Likewise, broad
            // cross-field assemblies are render geometry, not local blocks.
            if matches!(id.as_str(), "GUARD_RAIL.001" | "RISER.001") {
                return false;
            }
            let mut sorted = *half_extents;
            sorted.sort_by(|a, b| a.total_cmp(b));
            sorted[0] <= 1.25 && sorted[1] <= 1.25
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
                actuator: None,
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

fn load_robot_colliders(
    physics: &serde_json::Value,
    robot: &RobotPhysicsConfig,
) -> Vec<FieldCollider> {
    let authored = assimp_colliders(physics);
    let floor_y = authored
        .iter()
        .map(|(_, _, _, center, half_extents, axes)| {
            let extent_y = (0..3)
                .map(|axis| axes[axis][1].abs() * half_extents[axis])
                .sum::<f32>();
            center[1] - extent_y
        })
        .fold(f32::INFINITY, f32::min);
    if !floor_y.is_finite() {
        return Vec::new();
    }
    authored
        .into_iter()
        .map(|(id, _min, _max, mut center, half_extents, axes)| {
            center[1] -= floor_y + robot.height_m * 0.5;
            let mut min = center;
            let mut max = center;
            for world_axis in 0..3 {
                let extent = (0..3)
                    .map(|local_axis| axes[local_axis][world_axis].abs() * half_extents[local_axis])
                    .sum::<f32>();
                min[world_axis] -= extent;
                max[world_axis] += extent;
            }
            let actuator = infer_actuator_config(&id, robot);
            FieldCollider {
                id,
                min,
                max,
                center,
                half_extents,
                axes,
                actuator,
            }
        })
        .collect()
}

fn infer_actuator_config(id: &str, robot: &RobotPhysicsConfig) -> Option<ActuatorConfig> {
    let is_intake = id == "IntakeRoller";
    let is_outtake = id == "OuttakeRoller" || id == "TransferFlap" || id == "Transfer Flap";
    let is_climb = id == "ClimbWheel1" || id == "ClimbWheel2";

    if !is_intake && !is_outtake && !is_climb {
        return None;
    }

    let (channel, speed, spin_axis, friction) = if is_intake {
        (
            "intake".to_string(),
            robot.intake_surface_speed_mps,
            [0.0, 1.0, 0.0],
            robot.intake_friction,
        )
    } else if is_outtake {
        if id == "OuttakeRoller" {
            (
                "outtake".to_string(),
                robot.outtake_velocity_mps,
                [0.0, 1.0, 0.0],
                robot.surface_friction,
            )
        } else {
            (
                "outtake".to_string(),
                robot.outtake_velocity_mps,
                [0.0, 1.0, 0.0],
                robot.surface_friction,
            )
        }
    } else {
        (
            "climb".to_string(),
            3.0,
            [1.0, 0.0, 0.0],
            robot.surface_friction,
        )
    };

    Some(ActuatorConfig {
        spin_axis,
        input_channel: channel,
        target_surface_speed_mps: speed,
        max_torque: 15.0,
        mass_kg: 0.8,
        friction,
        contact_stiffness_n_per_m: default_contact_stiffness(),
        contact_damping_n_s_per_m: default_contact_damping(),
        max_compression_m: default_max_compression(),
    })
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

fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for row in 0..4 {
        for col in 0..4 {
            out[row * 4 + col] = (0..4).map(|k| a[row * 4 + k] * b[k * 4 + col]).sum();
        }
    }
    out
}

fn identity_mat4() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn get_mechanism_id(name: &str) -> Option<&str> {
    if name == "IntakeRoller" || name.starts_with("IntakeRoller") {
        Some("IntakeRoller")
    } else if name == "OuttakeRoller" || name.starts_with("OuttakeRoller") {
        Some("OuttakeRoller")
    } else if name == "TransferFlap" || name.starts_with("TransferFlap") || name == "Transfer Flap"
    {
        Some("TransferFlap")
    } else if name == "ClimbWheel1" || name.starts_with("ClimbWheel1") {
        Some("ClimbWheel1")
    } else if name == "ClimbWheel2" || name.starts_with("ClimbWheel2") {
        Some("ClimbWheel2")
    } else {
        None
    }
}

/// Recursively walk a scene node tree, yielding one OBB entry per mesh found.
/// `inherited_id` is the nearest ancestor name — so child cylinders of an
/// empty "IntakeRoller" parent all get the id "IntakeRoller".
/// `parent_mat` is the accumulated world transform of all ancestors.
fn collect_colliders(
    node: &serde_json::Value,
    scene: &serde_json::Value,
    parent_mat: &[f32; 16],
    inherited_id: Option<&str>,
    out: &mut Vec<(
        String,
        [f32; 3],
        [f32; 3],
        [f32; 3],
        [f32; 3],
        [[f32; 3]; 3],
    )>,
) {
    let node_name = node.get("name").and_then(|v| v.as_str()).unwrap_or("");
    // Compose this node's local transform with the parent's.
    let local_mat = assimp_matrix(node).unwrap_or_else(identity_mat4);
    let world_mat = mat4_mul(parent_mat, &local_mat);

    let direct_mech = get_mechanism_id(node_name);
    let inherited_mech = inherited_id.and_then(get_mechanism_id);
    let effective_mech = direct_mech.or(inherited_mech);

    // The ID to use for any mesh found at or below this node.
    let effective_id: &str = if let Some(mech) = effective_mech {
        mech
    } else if !node_name.is_empty() {
        node_name
    } else if let Some(id) = inherited_id {
        id
    } else {
        return; // no name anywhere — skip
    };

    // If this node has a mesh, emit a collider for it.
    if let Some(mesh_index) = node
        .get("meshes")
        .and_then(|m| m.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_u64())
    {
        let vertices_opt = scene
            .get("meshes")
            .and_then(|m| m.as_array())
            .and_then(|a| a.get(mesh_index as usize))
            .and_then(|mesh| mesh.get("vertices"))
            .and_then(|v| v.as_array());

        if let Some(vertices) = vertices_opt {
            let mut local_min = [f32::INFINITY; 3];
            let mut local_max = [f32::NEG_INFINITY; 3];
            let mut ok = true;
            for xyz in vertices.chunks_exact(3) {
                for axis in 0..3 {
                    if let Some(v) = xyz[axis].as_f64() {
                        let val = v as f32;
                        local_min[axis] = local_min[axis].min(val);
                        local_max[axis] = local_max[axis].max(val);
                    } else {
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
            }
            if ok && local_min.iter().all(|v| v.is_finite()) {
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
                let center = transform_point(&world_mat, local_center);
                let raw_axes = [
                    [world_mat[0], world_mat[4], world_mat[8]],
                    [world_mat[1], world_mat[5], world_mat[9]],
                    [world_mat[2], world_mat[6], world_mat[10]],
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
                        .map(|la| axes[la][world_axis].abs() * half_extents[la])
                        .sum::<f32>();
                    min[world_axis] -= radius;
                    max[world_axis] += radius;
                }
                out.push((
                    effective_id.to_string(),
                    min,
                    max,
                    center,
                    half_extents,
                    axes,
                ));
            }
        }
    }

    // Recurse into children, passing this node's name as the inherited id.
    if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        for child in children {
            collect_colliders(child, scene, &world_mat, Some(effective_id), out);
        }
    }
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
    let root_children = assimp_children(scene);
    let identity = identity_mat4();
    let mut out = Vec::new();
    for child in root_children {
        collect_colliders(child, scene, &identity, None, &mut out);
    }
    out
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

/// Extract every authored mechanism zone from `bot.semantics.json`: the
/// IntakeZone, TransferZone and OuttakeZone nodes. Each node carries its
/// mechanism mesh; the mesh OBB becomes the touch volume for the conveyor,
/// and the node's local Z axis (negated) gives the feed direction. The feed
/// direction is never hardcoded world space — it comes from each zone's own
/// authored transform. Coordinates stay robot-local and are rotated per-player
/// with the same convention as the physics colliders.
fn robot_semantic_zones(semantics: &serde_json::Value) -> Vec<RobotSemanticZone> {
    if semantics.is_null() {
        return Vec::new();
    }
    let mut zones = Vec::new();
    let mut candidates = assimp_children(semantics);
    if let Some(root) = semantics.get("rootnode") {
        candidates.push(root);
    }
    for node in candidates {
        let Some(name) = node
            .get("name")
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        let kind = match name {
            "IntakeZone" => SemanticZoneKind::Intake,
            "TransferZone" => SemanticZoneKind::Transfer,
            "OuttakeZone" => SemanticZoneKind::Outtake,
            _ => continue,
        };
        if let Some(geometry) = robot_zone_geometry(node, semantics) {
            zones.push(RobotSemanticZone { kind, geometry });
        }
    }
    zones
}

/// Compute the OBB + feed direction of one authored mechanism zone node.
fn robot_zone_geometry(
    node: &serde_json::Value,
    semantics: &serde_json::Value,
) -> Option<RobotIntakeGeometry> {
    let matrix = assimp_matrix(node)?;
    let mesh_index = node.get("meshes")?.as_array()?.first()?.as_u64()? as usize;
    let vertices = semantics
        .get("meshes")?
        .as_array()?
        .get(mesh_index)?
        .get("vertices")?
        .as_array()?;
    let mut local_min = [f32::INFINITY; 3];
    let mut local_max = [f32::NEG_INFINITY; 3];
    for xyz in vertices.chunks_exact(3) {
        for axis in 0..3 {
            let value = xyz[axis].as_f64()? as f32;
            local_min[axis] = local_min[axis].min(value);
            local_max[axis] = local_max[axis].max(value);
        }
    }
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
        half_extents[axis] = 0.5 * (local_max[axis] - local_min[axis]) * length;
    }
    let local_center = [
        0.5 * (local_min[0] + local_max[0]),
        0.5 * (local_min[1] + local_max[1]),
        0.5 * (local_min[2] + local_max[2]),
    ];
    let zone_center = transform_point(&matrix, local_center);
    // The mechanism mouth is authored facing out: its local +Z column points
    // outward, so the conveyor pushes balls toward local -Z. This is the same
    // orientation convention the collision solver already uses.
    let depth_len = (raw_axes[2][0] * raw_axes[2][0]
        + raw_axes[2][1] * raw_axes[2][1]
        + raw_axes[2][2] * raw_axes[2][2])
        .sqrt()
        .max(1.0e-6);
    let direction = [
        -raw_axes[2][0] / depth_len,
        -raw_axes[2][1] / depth_len,
        -raw_axes[2][2] / depth_len,
    ];
    Some(RobotIntakeGeometry {
        zone_center,
        zone_half_extents: half_extents,
        zone_axes: axes,
        direction,
    })
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
                robot_physics: serde_json::from_str(
                    &std::fs::read_to_string(
                        root.parent()
                            .and_then(std::path::Path::parent)
                            .unwrap()
                            .join("robots/StarterBot/bot.physics.json"),
                    )
                    .unwrap(),
                )
                .unwrap(),
                robot_semantics: serde_json::from_str(
                    &std::fs::read_to_string(
                        root.parent()
                            .and_then(std::path::Path::parent)
                            .unwrap()
                            .join("robots/StarterBot/bot.semantics.json"),
                    )
                    .unwrap(),
                )
                .unwrap(),
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
        assert!(metadata.arena.robot.intake_enabled);
        assert_eq!(metadata.arena.robot.mass_kg, 18.0);
        assert_eq!(metadata.arena.robot.width_m, 0.50);
        assert_eq!(metadata.arena.robot.height_m, 0.50);
        assert_eq!(metadata.arena.robot.length_m, 0.50);
        assert!(metadata.arena.ramp.enabled);
        assert!(metadata.field_definition.colliders.len() >= 70);
        assert!(!metadata.robot_colliders.is_empty());
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
