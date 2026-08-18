use axum::body::Bytes;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::info;

use super::match_runtime::{MatchRuntime, PlayerSnapshot, ScoreState};
use super::pack_loader::{ArenaConfig, GamePackMetadata};
use super::rhai_engine::RhaiEngine;
use super::sphere_runtime::{MechSpec, SphereRuntime, StepMetrics, TransferDebug, BallDebugFlag};

pub struct MatchRegistry {
    matches: RwLock<HashMap<String, MatchHandle>>,
    pack: Arc<GamePackMetadata>,
    reports: Option<mpsc::UnboundedSender<MatchReport>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapParticipant {
    pub user_id: String,
    pub role: String,
    pub slot_id: String,
    pub alliance: String,
    pub robot_id: Option<String>,
    pub robot_revision: Option<u64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchBootstrap {
    pub match_id: String,
    pub host_id: String,
    pub assigned_server_id: String,
    pub game_pack_id: String,
    pub game_pack_version: String,
    pub match_seed: u64,
    pub starts_at: u64,
    pub duration_seconds: u64,
    pub max_players: usize,
    pub scenario_id: String,
    pub visibility: String,
    #[serde(default)]
    pub open_arena: bool,
    #[serde(default)]
    pub participants: Vec<BootstrapParticipant>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchEventReport {
    pub match_id: String,
    pub event_id: String,
    pub tick: u64,
    pub kind: String,
    pub payload: serde_json::Value,
    pub game_pack_version: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchCompletionReport {
    pub match_id: String,
    pub completion_id: String,
    pub tick: u64,
    pub reason: String,
    pub result: serde_json::Value,
    pub game_pack_version: String,
}

#[derive(Debug, Clone)]
pub enum MatchReport {
    Event(MatchEventReport),
    Completion(MatchCompletionReport),
}

#[derive(Clone)]
pub struct MatchHandle {
    pub input_tx: mpsc::Sender<MatchInput>,
    pub state_tx: broadcast::Sender<Bytes>,
    shutdown: Arc<AtomicBool>,
    kicked_users: Arc<Mutex<HashSet<String>>>,
    telemetry: Arc<Mutex<RuntimeMatchTelemetry>>,
    pub bootstrap: Arc<MatchBootstrap>,
    reports: Option<mpsc::UnboundedSender<MatchReport>>,
}

impl MatchHandle {
    pub fn report_input_rejection(&self, user_id: &str, reason: &str) {
        if let Some(reports) = &self.reports {
            let _ = reports.send(MatchReport::Event(MatchEventReport {
                match_id: self.bootstrap.match_id.clone(),
                event_id: format!(
                    "{}:input:{}:{}",
                    self.bootstrap.match_id,
                    user_id,
                    uuid::Uuid::new_v4()
                ),
                tick: self
                    .telemetry
                    .lock()
                    .ok()
                    .map(|value| value.tick)
                    .unwrap_or_default(),
                kind: "input.rejected".to_string(),
                payload: serde_json::json!({ "userId": user_id, "reason": reason }),
                game_pack_version: self.bootstrap.game_pack_version.clone(),
            }));
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMatchTelemetry {
    pub id: String,
    pub players: usize,
    pub objects: usize,
    pub contacts: usize,
    pub tick: u64,
    pub tps: f64,
    pub physics_tick_ms: f64,
    pub physics_load_percent: f64,
    pub clock_drift_ms: f64,
}

#[derive(Debug, Clone)]
pub enum MatchInput {
    PlayerJoin {
        user_id: String,
        name: String,
        team_name: String,
        slot_id: Option<String>,
    },
    PlayerLeave {
        user_id: String,
    },
    PlayerInput {
        user_id: String,
        move_x: f32,
        move_z: f32,
        intake_power: f32,
        outtake_power: f32,
        climb_power: f32,
        sequence: u64,
    },
    PlayerMech {
        user_id: String,
        mech: MechSpec,
    },
    /// Keep simulating past the 150 s clock so teams can keep practising with
    /// the same field. Scoring stays disabled while practice continues.
    ContinuePractice,
    EndPractice,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectPositionsSync {
    pub count: u32,
    pub active_mask: Vec<u8>,
    pub moving_mask: Vec<u8>,
    pub quantized_positions: Vec<u16>,
}

#[derive(Debug, Clone)]
pub struct MatchStateSync {
    pub tick: u64,
    pub game_pack_id: String,
    pub game_pack_version: String,
    pub players: Vec<PlayerSnapshot>,
    pub object_id: String,
    pub object_radius: f32,
    pub object_color: String,
    pub object_positions: ObjectPositionsSync,
    pub contacts: usize,
    pub match_clock: f64,
    pub match_duration_seconds: f64,
    pub pre_match_remaining_seconds: f64,
    pub match_running: bool,
    pub simulation_clock: f64,
    pub physics_tick_ms: f64,
    pub physics_load_percent: f64,
    pub ticks_per_second: f64,
    pub target_ticks_per_second: f64,
    pub clock_drift_ms: f64,
    pub step_metrics: StepMetrics,
    pub physics: PhysicsSync,
    pub drive: DriveSync,
    pub semantic_events: Vec<String>,
    pub score: ScoreState,
    /// True while physics keeps running past the match clock for practice.
    pub practice_running: bool,
    /// Transfer mechanism debug info for each player (nearest ball conditions).
    pub transfer_debug: Vec<TransferDebug>,
    /// Lightweight debug state per ball.
    pub ball_debug: Vec<BallDebugFlag>,
    /// Authored robot collider ids each ball is touching, per ball.
    pub ball_contact_colliders: Vec<Vec<String>>,
}

/// Drivetrain model constants the client needs to reproduce the server's
/// `apply_player_drive` locally for client-side prediction. Sent once per
/// snapshot as its own protocol section so older readers skip it unchanged.
#[derive(Debug, Clone, Copy)]
pub struct DriveSync {
    pub max_acceleration_mps2: f32,
    pub max_deceleration_mps2: f32,
    pub max_turn_rate_radps: f32,
    pub max_angular_acceleration_radps2: f32,
    pub lateral_grip_mps2: f32,
    pub traction_friction: f32,
    pub track_width_m: f32,
}

impl Default for DriveSync {
    fn default() -> Self {
        Self {
            max_acceleration_mps2: 3.0,
            max_deceleration_mps2: 4.0,
            max_turn_rate_radps: 2.5,
            max_angular_acceleration_radps2: 6.0,
            lateral_grip_mps2: 6.0,
            traction_friction: 0.85,
            track_width_m: 0.4,
        }
    }
}

enum RuntimeBackend {
    Rapier(Box<MatchRuntime>),
    Sphere(Box<SphereRuntime>),
}

impl RuntimeBackend {
    fn new(match_id: String, pack: &GamePackMetadata, seed: u64) -> Self {
        if pack.arena.physics_backend == "sphere_xpbd" {
            let mut runtime = SphereRuntime::new(match_id, pack.manifest.id.clone(), seed);
            runtime.context.game_pack_version = pack.manifest.version.clone();
            runtime.create_field_arena(&pack.arena, &pack.field_definition);
            runtime.set_robot_definition(pack.default_robot.as_ref());
            Self::Sphere(Box::new(runtime))
        } else {
            let mut runtime = MatchRuntime::new(match_id, pack.manifest.id.clone(), seed);
            runtime.context.game_pack_version = pack.manifest.version.clone();
            runtime.create_test_arena(&pack.arena);
            Self::Rapier(Box::new(runtime))
        }
    }

    fn add_player(
        &mut self,
        id: String,
        name: String,
        team: String,
        slot_id: Option<String>,
        arena: &ArenaConfig,
    ) {
        match self {
            Self::Rapier(runtime) => runtime.add_player(id, name, team, arena),
            Self::Sphere(runtime) => runtime.add_player(id, name, team, slot_id.as_deref(), arena),
        }
    }

    fn remove_player(&mut self, id: &str) {
        match self {
            Self::Rapier(runtime) => runtime.remove_player(id),
            Self::Sphere(runtime) => runtime.remove_player(id),
        }
    }

    fn set_player_input(
        &mut self,
        id: &str,
        x: f32,
        z: f32,
        intake: f32,
        outtake: f32,
        climb: f32,
        sequence: u64,
    ) {
        match self {
            Self::Rapier(runtime) => runtime.set_player_input(id, x, z, sequence),
            Self::Sphere(runtime) => {
                runtime.set_player_input_with_climb(id, x, z, intake, outtake, climb, sequence)
            }
        }
    }

    fn step(&mut self, arena: &ArenaConfig, dt: f64) {
        match self {
            Self::Rapier(runtime) => {
                runtime.apply_player_drive(arena);
                runtime.tick(dt);
            }
            Self::Sphere(runtime) => {
                runtime.apply_player_drive(arena, dt as f32);
                runtime.tick(dt);
            }
        }
    }

    fn set_scoring_enabled(&mut self, enabled: bool) {
        if let Self::Sphere(runtime) = self {
            runtime.set_scoring_enabled(enabled);
        }
    }

    fn set_player_mech(&mut self, id: &str, mech: MechSpec) {
        if let Self::Sphere(runtime) = self {
            runtime.set_player_mech(id, mech);
        }
    }

    fn disable_player_controls(&mut self) {
        match self {
            Self::Rapier(runtime) => runtime.disable_player_controls(),
            Self::Sphere(runtime) => runtime.disable_player_controls(),
        }
    }

    /// Accumulate a single scored outcome into the runtime score ledger.
    fn apply_score(&mut self, team: &str, category: &str, points: i32) {
        let score = match self {
            Self::Rapier(runtime) => &mut runtime.score_state,
            Self::Sphere(runtime) => &mut runtime.score_state,
        };
        match team {
            "blue" => {
                score.blue_score += points;
                if category == "SU" {
                    score.blue_su_score += points;
                }
            }
            "red" => {
                score.red_score += points;
                if category == "SU" {
                    score.red_su_score += points;
                }
            }
            _ => score.global_score += points,
        }
        *score.breakdown.entry(category.to_string()).or_insert(0) += points;
    }

    fn score_state(&self) -> ScoreState {
        match self {
            Self::Rapier(runtime) => runtime.score_state.clone(),
            Self::Sphere(runtime) => runtime.score_state.clone(),
        }
    }

    fn begin_match(&mut self) {
        match self {
            Self::Rapier(runtime) => runtime.begin_match(),
            Self::Sphere(runtime) => runtime.begin_match(),
        }
    }

    fn simulation_clock(&self) -> f64 {
        match self {
            Self::Rapier(runtime) => runtime.context.clock,
            Self::Sphere(runtime) => runtime.context.clock,
        }
    }

    fn players(&self) -> Vec<PlayerSnapshot> {
        match self {
            Self::Rapier(runtime) => runtime.player_snapshots(),
            Self::Sphere(runtime) => runtime.player_snapshots(),
        }
    }

    fn players_with_brace_states(
        &self,
        triggers: &[super::pack_loader::FieldTrigger],
    ) -> Vec<PlayerSnapshot> {
        match self {
            Self::Rapier(runtime) => runtime.player_snapshots(),
            Self::Sphere(runtime) => runtime.player_snapshots_with_brace_states(triggers),
        }
    }

    fn brace_multipliers(&self, triggers: &[super::pack_loader::FieldTrigger]) -> (f32, f32) {
        match self {
            Self::Rapier(_) => (1.0, 1.0),
            Self::Sphere(runtime) => runtime.brace_multipliers(triggers),
        }
    }

    fn apply_endgame_brace_multipliers(&mut self, multipliers: (f32, f32)) {
        if let Self::Sphere(runtime) = self {
            runtime.apply_endgame_brace_multipliers(multipliers.0, multipliers.1);
        }
    }

    fn positions(&self) -> ObjectPositionsSync {
        match self {
            Self::Rapier(runtime) => runtime.field_object_positions(),
            Self::Sphere(runtime) => runtime.field_object_positions(),
        }
    }

    fn contacts(&self) -> usize {
        match self {
            Self::Rapier(runtime) => runtime.contact_count(),
            Self::Sphere(runtime) => runtime.contact_count(),
        }
    }

    fn step_metrics(&self) -> StepMetrics {
        match self {
            Self::Rapier(_) => StepMetrics::default(),
            Self::Sphere(runtime) => runtime.step_metrics(),
        }
    }

    fn drain_semantic_events(&mut self) -> Vec<super::sphere_runtime::SemanticEvent> {
        match self {
            Self::Rapier(_) => Vec::new(),
            Self::Sphere(runtime) => runtime.drain_semantic_events(),
        }
    }

    fn transfer_debug(&self) -> Vec<TransferDebug> {
        match self {
            Self::Rapier(_) => Vec::new(),
            Self::Sphere(runtime) => runtime.transfer_debug.clone(),
        }
    }

    fn ball_debug(&self) -> Vec<BallDebugFlag> {
        match self {
            Self::Rapier(_) => Vec::new(),
            Self::Sphere(runtime) => runtime.ball_debug.clone(),
        }
    }

    fn ball_contact_colliders(&self) -> Vec<Vec<String>> {
        match self {
            Self::Rapier(_) => Vec::new(),
            Self::Sphere(runtime) => runtime.ball_contact_collider_ids(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicsSync {
    pub ball_material: String,
    pub ball_diameter_m: f32,
    pub ball_diameter_tolerance_m: f32,
    pub ball_mass_kg: f32,
    pub ball_friction: f32,
    pub ball_restitution: f32,
    pub ball_rolling_resistance_mps2: f32,
    pub floor_material: String,
    pub floor_friction: f32,
    pub robot_mass_kg: f32,
    pub robot_width_m: f32,
    pub robot_height_m: f32,
    pub robot_length_m: f32,
    pub robot_max_speed_mps: f32,
    pub ball_inertia_factor: f32,
    pub ball_drag_coefficient: f32,
    pub air_density_kg_m3: f32,
    pub ball_ball_friction: f32,
    pub floor_static_friction: f32,
    pub floor_dynamic_friction: f32,
    pub floor_rolling_resistance_mps2: f32,
    pub intake_enabled: bool,
    pub intake_width_m: f32,
    pub intake_radius_m: f32,
    pub intake_forward_offset_m: f32,
    pub intake_center_height_m: f32,
    pub intake_surface_speed_mps: f32,
    pub ramp_enabled: bool,
    pub ramp_center_x: f32,
    pub ramp_start_z: f32,
    pub ramp_width_m: f32,
    pub ramp_length_m: f32,
    pub ramp_angle_deg: f32,
    pub solver_position_iterations: f32,
    pub solver_velocity_iterations: f32,
    pub max_depenetration_speed_mps: f32,
    pub max_ball_speed_mps: f32,
    pub max_ball_angular_speed_radps: f32,
    pub max_drive_force_n: f32,
    pub max_drive_power_w: f32,
    pub max_brake_force_n: f32,
    pub storage_capacity: f32,
    pub intake_rate_bps: f32,
    pub outtake_rate_bps: f32,
    pub outtake_velocity_mps: f32,
    pub outtake_angle_deg: f32,
    pub flywheel_width_m: f32,
    pub outtake_forward_offset_m: f32,
    pub outtake_height_m: f32,
}

impl From<&ArenaConfig> for PhysicsSync {
    fn from(arena: &ArenaConfig) -> Self {
        Self {
            ball_material: arena.ball.material.clone(),
            ball_diameter_m: arena.ball.diameter_m,
            ball_diameter_tolerance_m: arena.ball.diameter_tolerance_m,
            ball_mass_kg: arena.ball.mass_kg,
            ball_friction: arena.ball.friction,
            ball_restitution: arena.ball.restitution,
            ball_rolling_resistance_mps2: arena.ball.rolling_resistance_mps2,
            floor_material: arena.floor.material.clone(),
            floor_friction: arena.floor.friction,
            robot_mass_kg: arena.robot.mass_kg,
            robot_width_m: arena.robot.width_m,
            robot_height_m: arena.robot.height_m,
            robot_length_m: arena.robot.length_m,
            robot_max_speed_mps: arena.robot.max_speed_mps,
            ball_inertia_factor: arena.ball.inertia_factor,
            ball_drag_coefficient: arena.ball.drag_coefficient,
            air_density_kg_m3: arena.ball.air_density_kg_m3,
            ball_ball_friction: arena.ball.ball_friction,
            floor_static_friction: arena.floor.static_friction,
            floor_dynamic_friction: arena.floor.dynamic_friction,
            floor_rolling_resistance_mps2: arena.floor.rolling_resistance_mps2,
            intake_enabled: arena.robot.intake_enabled,
            intake_width_m: arena.robot.intake_width_m,
            intake_radius_m: arena.robot.intake_radius_m,
            intake_forward_offset_m: arena.robot.intake_forward_offset_m,
            intake_center_height_m: arena.robot.intake_center_height_m,
            intake_surface_speed_mps: arena.robot.intake_surface_speed_mps,
            ramp_enabled: arena.ramp.enabled,
            ramp_center_x: arena.ramp.center_x,
            ramp_start_z: arena.ramp.start_z,
            ramp_width_m: arena.ramp.width_m,
            ramp_length_m: arena.ramp.length_m,
            ramp_angle_deg: arena.ramp.angle_deg,
            solver_position_iterations: arena.solver.position_iterations as f32,
            solver_velocity_iterations: arena.solver.velocity_iterations as f32,
            max_depenetration_speed_mps: arena.solver.max_depenetration_speed_mps,
            max_ball_speed_mps: arena.solver.max_ball_speed_mps,
            max_ball_angular_speed_radps: arena.solver.max_ball_angular_speed_radps,
            max_drive_force_n: arena.robot.max_drive_force_n,
            max_drive_power_w: arena.robot.max_drive_power_w,
            max_brake_force_n: arena.robot.max_brake_force_n,
            storage_capacity: arena.robot.storage_capacity as f32,
            intake_rate_bps: arena.robot.intake_rate_bps,
            outtake_rate_bps: arena.robot.outtake_rate_bps,
            outtake_velocity_mps: arena.robot.outtake_velocity_mps,
            outtake_angle_deg: arena.robot.outtake_angle_deg,
            flywheel_width_m: arena.robot.flywheel_width_m,
            outtake_forward_offset_m: arena.robot.outtake_forward_offset_m,
            outtake_height_m: arena.robot.outtake_height_m,
        }
    }
}

impl MatchRegistry {
    pub fn new(pack: Arc<GamePackMetadata>) -> Self {
        Self {
            matches: RwLock::new(HashMap::new()),
            pack,
            reports: None,
        }
    }

    pub fn with_reports(
        pack: Arc<GamePackMetadata>,
        reports: mpsc::UnboundedSender<MatchReport>,
    ) -> Self {
        Self {
            matches: RwLock::new(HashMap::new()),
            pack,
            reports: Some(reports),
        }
    }

    pub async fn match_count(&self) -> usize {
        self.matches.read().await.len()
    }

    pub async fn telemetry(&self) -> Vec<RuntimeMatchTelemetry> {
        self.matches
            .read()
            .await
            .values()
            .filter_map(|handle| handle.telemetry.lock().ok().map(|value| value.clone()))
            .collect()
    }

    pub async fn active_user_count(&self) -> usize {
        self.telemetry()
            .await
            .into_iter()
            .map(|match_info| match_info.players)
            .sum()
    }

    pub async fn is_player_kicked(&self, match_id: &str, user_id: &str) -> bool {
        self.matches
            .read()
            .await
            .get(match_id)
            .and_then(|handle| {
                handle
                    .kicked_users
                    .lock()
                    .ok()
                    .map(|users| users.contains(user_id))
            })
            .unwrap_or(false)
    }

    pub async fn is_match_stopped(&self, match_id: &str) -> bool {
        self.matches
            .read()
            .await
            .get(match_id)
            .map(|handle| handle.shutdown.load(Ordering::Relaxed))
            .unwrap_or(true)
    }

    pub async fn kick_player(&self, match_id: &str, user_id: &str) -> Result<(), String> {
        let handle = self
            .matches
            .read()
            .await
            .get(match_id)
            .cloned()
            .ok_or_else(|| "Match is not running on this host.".to_string())?;
        handle
            .kicked_users
            .lock()
            .map_err(|_| "Match control lock is unavailable.".to_string())?
            .insert(user_id.to_string());
        let _ = handle
            .input_tx
            .send(MatchInput::PlayerLeave {
                user_id: user_id.to_string(),
            })
            .await;
        Ok(())
    }

    pub async fn stop_match(&self, match_id: &str) -> Result<(), String> {
        let handle = self
            .matches
            .write()
            .await
            .remove(match_id)
            .ok_or_else(|| "Match is not running on this host.".to_string())?;
        handle.shutdown.store(true, Ordering::Relaxed);
        self.report_completion(
            match_id,
            0,
            "cancelled",
            serde_json::json!({ "reason": "control_plane_stop" }),
        );
        Ok(())
    }

    pub async fn cleanup_idle(&self) -> usize {
        let ids = self
            .matches
            .read()
            .await
            .iter()
            .filter_map(|(id, handle)| {
                handle
                    .telemetry
                    .lock()
                    .ok()
                    .filter(|telemetry| telemetry.players == 0)
                    .map(|_| id.clone())
            })
            .collect::<Vec<_>>();
        for id in &ids {
            let _ = self.stop_match(id).await;
        }
        ids.len()
    }

    pub async fn reset_host(&self) -> usize {
        let ids = self
            .matches
            .read()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in &ids {
            let _ = self.stop_match(id).await;
        }
        ids.len()
    }

    pub fn report_event(&self, event: MatchEventReport) {
        if let Some(reports) = &self.reports {
            let _ = reports.send(MatchReport::Event(event));
        }
    }

    fn report_completion(
        &self,
        match_id: &str,
        tick: u64,
        reason: &str,
        result: serde_json::Value,
    ) {
        if let Some(reports) = &self.reports {
            let _ = reports.send(MatchReport::Completion(MatchCompletionReport {
                match_id: match_id.to_string(),
                completion_id: format!("{match_id}:complete:{tick}:{reason}"),
                tick,
                reason: reason.to_string(),
                result,
                game_pack_version: self.pack.manifest.version.clone(),
            }));
        }
    }

    pub async fn get_or_create_match(
        &self,
        bootstrap: MatchBootstrap,
    ) -> Result<MatchHandle, String> {
        if bootstrap.game_pack_id != self.pack.manifest.id
            || bootstrap.game_pack_version != self.pack.manifest.version
        {
            return Err("The assigned game-pack version is not loaded on this host.".to_string());
        }
        let match_id = bootstrap.match_id.clone();
        let mut matches = self.matches.write().await;
        if let Some(handle) = matches.get(&match_id) {
            if handle.bootstrap.match_seed != bootstrap.match_seed {
                return Err("The match bootstrap changed after startup.".to_string());
            }
            return Ok(handle.clone());
        }

        let (input_tx, mut input_rx) = mpsc::channel(256);
        let (state_tx, _) = broadcast::channel(4);
        let handle = MatchHandle {
            input_tx,
            state_tx: state_tx.clone(),
            shutdown: Arc::new(AtomicBool::new(false)),
            kicked_users: Arc::new(Mutex::new(HashSet::new())),
            telemetry: Arc::new(Mutex::new(RuntimeMatchTelemetry {
                id: match_id.clone(),
                ..Default::default()
            })),
            bootstrap: Arc::new(bootstrap.clone()),
            reports: self.reports.clone(),
        };
        matches.insert(match_id.clone(), handle.clone());
        let pack = self.pack.clone();
        let latest_state = Arc::new(Mutex::new(None::<Arc<MatchStateSync>>));
        let telemetry = handle.telemetry.clone();
        let publisher_shutdown = handle.shutdown.clone();
        let report_tx = self.reports.clone();

        let mut rules = RhaiEngine::new();
        for script in pack.manifest.scripts.values() {
            let Some(source) = pack.script_sources.get(script) else {
                tracing::error!(path = %script, "API runtime snapshot is missing a validated rule script");
                return Ok(handle);
            };
            if !rules.load_source(script, source) {
                tracing::error!(path = %script, "Unable to compile a validated API rule script into match runtime");
                return Ok(handle);
            }
        }
        let loaded_script_count = rules.loaded_script_count();
        // Rhai is configured without its `sync` feature. Validation happens
        // here, but each dedicated match thread owns its executable engine.
        drop(rules);
        info!(match_id = %match_id, pack = %pack.manifest.id, version = %pack.manifest.version, scripts = loaded_script_count, "Loaded game pack into match runtime");

        let publisher_state = latest_state.clone();
        let publisher_tx = state_tx.clone();
        std::thread::Builder::new()
            .name(format!("match-publisher-{match_id}"))
            .spawn(move || {
                let interval = Duration::from_millis(50);
                let mut next_publish = Instant::now();
                let mut next_process_sample = next_publish;
                let mut process_sampler = ProcessSampler::default();
                let mut process_metrics = ProcessMetrics::default();
                let mut publish_count = 0u64;
                while !publisher_shutdown.load(Ordering::Relaxed) {
                    publish_count += 1;
                    let now = Instant::now();
                    if now < next_publish {
                        std::thread::sleep(next_publish - now);
                    }
                    next_publish += interval;
                    if publisher_tx.receiver_count() == 0 {
                        continue;
                    }
                    if Instant::now() >= next_process_sample {
                        process_metrics = process_sampler.sample();
                        next_process_sample = Instant::now() + Duration::from_secs(1);
                    }
                    let state = publisher_state
                        .lock()
                        .ok()
                        .and_then(|state| state.as_ref().cloned());
                    if let Some(state) = state {
                        let include_physics = publish_count % 10 == 1;
                        let _ = publisher_tx.send(Bytes::from(encode_state(
                            &state,
                            process_metrics,
                            include_physics,
                        )));
                    }
                }
            })
            .expect("failed to start match publisher thread");

        let simulation_shutdown = handle.shutdown.clone();
        std::thread::Builder::new()
            .name(format!("match-{match_id}"))
            .spawn(move || {
                let mut runtime = RuntimeBackend::new(match_id.clone(), &pack, bootstrap.match_seed);
                let mut rules = RhaiEngine::new();
                for script in pack.manifest.scripts.values() {
                    let Some(source) = pack.script_sources.get(script) else {
                        tracing::error!(path = %script, "API runtime snapshot is missing a rule script in match thread");
                        continue;
                    };
                    if !rules.load_source(script, source) {
                        tracing::error!(path = %script, "Unable to compile API rule script into match thread");
                    }
                }
                let physics = PhysicsSync::from(&pack.arena);
                let tick_duration = Duration::from_secs_f64(1.0 / 60.0);
                let tick_budget_ms = tick_duration.as_secs_f64() * 1_000.0;
                let mut next_tick = Instant::now();
                let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
                let match_started = next_tick + Duration::from_millis(bootstrap.starts_at.saturating_sub(now_ms));
                let match_duration = Duration::from_secs(bootstrap.duration_seconds);
                let match_ends = match_started + match_duration;
                const POST_MATCH_SETTLE: Duration = Duration::from_millis(3_500);
                let match_finalizes = match_ends + POST_MATCH_SETTLE;
                let mut live_phase_entered = false;
                let mut practice_continue = false;
                let mut controls_disabled = false;
                let mut tps_window_started = next_tick;
                let mut ticks_in_tps_window = 0_u64;
                let mut ticks_per_second = 60.0;
                let mut tick = 0_u64;
                let mut event_sequence = 0_u64;
                let mut recent_semantic_events = VecDeque::<String>::with_capacity(16);
                let mut brace_score_applied = false;
                let mut endgame_brace_multipliers = None;

                while !simulation_shutdown.load(Ordering::Relaxed) {
                    let now = Instant::now();
                    if now < next_tick {
                        std::thread::sleep(next_tick - now);
                    } else if now.duration_since(next_tick) > tick_duration {
                        // Do not burst through missed physics ticks. Catch-up bursts
                        // starve websocket I/O precisely when the simulation is busy.
                        next_tick = now;
                    }
                    next_tick += tick_duration;

                    let clock_now = Instant::now();
                    let controls_locked = live_phase_entered && clock_now >= match_ends;
                    if controls_locked && !controls_disabled {
                        runtime.disable_player_controls();
                        controls_disabled = true;
                    }
                    while let Ok(input) = input_rx.try_recv() {
                        match input {
                            MatchInput::PlayerJoin {
                                user_id,
                                name,
                                team_name,
                                slot_id,
                            } if !controls_locked => runtime.add_player(user_id, name, team_name, slot_id, &pack.arena),
                            MatchInput::PlayerLeave { user_id } => runtime.remove_player(&user_id),
                            MatchInput::PlayerInput {
                                user_id,
                                move_x,
                                move_z,
                                intake_power,
                                outtake_power,
                                climb_power,
                                sequence,
                            } if !controls_locked => runtime.set_player_input(&user_id, move_x, move_z, intake_power, outtake_power, climb_power, sequence),
                            MatchInput::PlayerMech { user_id, mech } if !controls_locked => runtime.set_player_mech(&user_id, mech),
                            MatchInput::ContinuePractice if !controls_locked => practice_continue = true,
                            MatchInput::EndPractice if !controls_locked => practice_continue = false,
                            _ => {}
                        }
                    }

                    if !live_phase_entered && clock_now >= match_started {
                        runtime.begin_match();
                        live_phase_entered = true;
                        if let Some(reports) = &report_tx { let _ = reports.send(MatchReport::Event(MatchEventReport {
                            match_id: match_id.clone(), event_id: format!("{match_id}:started"), tick, kind: "match.started".to_string(),
                            payload: serde_json::json!({ "seed": bootstrap.match_seed, "scenario": bootstrap.scenario_id }), game_pack_version: pack.manifest.version.clone(),
                        })); }
                    }
                    let match_running = live_phase_entered && clock_now < match_ends;
                    let settling = live_phase_entered && clock_now >= match_ends && clock_now < match_finalizes;
                    if settling && endgame_brace_multipliers.is_none() {
                        // Lock the brace result at the final buzzer, then let
                        // balls keep settling and scoring until completion.
                        endgame_brace_multipliers =
                            Some(runtime.brace_multipliers(&pack.field_definition.triggers));
                    }
                    let scoring_active = match_running || settling;
                    let physics_started = Instant::now();
                    if scoring_active || practice_continue {
                        runtime.set_scoring_enabled(scoring_active);
                        runtime.step(&pack.arena, 1.0 / 60.0);
                    }
                    for event in runtime.drain_semantic_events() {
                        event_sequence += 1;
                        let mut label = format!("{} {} ← {}", event.kind, event.target_id, event.entity_id);
                        if let Some(reports) = &report_tx { let _ = reports.send(MatchReport::Event(MatchEventReport {
                            match_id: match_id.clone(), event_id: format!("{match_id}:{tick}:{event_sequence}"), tick,
                            kind: format!("semantic.{}", event.kind), payload: serde_json::json!({ "targetId": event.target_id, "entityId": event.entity_id }), game_pack_version: pack.manifest.version.clone(),
                        })); }
                        if scoring_active {
                            let outcomes = rules.on_trigger_enter(&event.target_id, &event.entity_id);
                            for outcome in outcomes {
                                event_sequence += 1;
                                label.push_str(&format!(" · {} {}/{} +{}", outcome.kind, outcome.team, outcome.category, outcome.points));
                                // Native scoring: the authored rule's outcome
                                // is the source of truth for team, category and
                                // points, so tweaking scoring.rhai rebalances a
                                // match without a rebuild.
                                runtime.apply_score(&outcome.team, &outcome.category, outcome.points as i32);
                                if let Some(reports) = &report_tx { let _ = reports.send(MatchReport::Event(MatchEventReport {
                                    match_id: match_id.clone(), event_id: format!("{match_id}:{tick}:{event_sequence}"), tick, kind: "score.awarded".to_string(),
                                    payload: serde_json::json!({ "team": outcome.team, "category": outcome.category, "points": outcome.points }), game_pack_version: pack.manifest.version.clone(),
                                })); }
                            }
                        }
                        recent_semantic_events.push_back(label);
                        while recent_semantic_events.len() > 16 {
                            recent_semantic_events.pop_front();
                        }
                    }
                    let physics_tick_ms = physics_started.elapsed().as_secs_f64() * 1_000.0;
                    let physics_load_percent = physics_tick_ms / tick_budget_ms * 100.0;
                    tick += 1;
                    ticks_in_tps_window += 1;
                    let tps_elapsed = physics_started.duration_since(tps_window_started);
                    if tps_elapsed >= Duration::from_secs(1) {
                        ticks_per_second = ticks_in_tps_window as f64 / tps_elapsed.as_secs_f64();
                        ticks_in_tps_window = 0;
                        tps_window_started = physics_started;
                    }
                    let match_clock = match_ends
                        .checked_duration_since(clock_now)
                        .unwrap_or_default()
                        .as_secs_f64();
                    let pre_match_remaining_seconds = match_started
                        .checked_duration_since(clock_now)
                        .unwrap_or_default()
                        .as_secs_f64();
                    let simulation_clock = runtime.simulation_clock();
                    let elapsed_live_seconds = clock_now
                        .checked_duration_since(match_started)
                        .unwrap_or_default()
                        .as_secs_f64()
                                .min(match_duration.as_secs_f64());
                    let clock_drift_ms = if match_running {
                        (simulation_clock - elapsed_live_seconds) * 1_000.0
                    } else {
                        0.0
                    };

                    if let Ok(mut current) = telemetry.lock() {
                        *current = RuntimeMatchTelemetry {
                            id: match_id.clone(),
                            players: runtime.players().len(),
                            objects: pack.arena.object_count,
                            contacts: runtime.contacts(),
                            tick,
                            tps: ticks_per_second,
                            physics_tick_ms,
                            physics_load_percent,
                            clock_drift_ms,
                        };
                    }

                    if state_tx.receiver_count() > 0 {
                        let state = MatchStateSync {
                            tick,
                            game_pack_id: pack.manifest.id.clone(),
                            game_pack_version: pack.manifest.version.clone(),
                            players: runtime.players_with_brace_states(&pack.field_definition.triggers),
                            object_id: pack.arena.object_id.clone(),
                            object_radius: pack.arena.ball.radius_m(),
                            object_color: pack.arena.color.clone(),
                            object_positions: runtime.positions(),
                            contacts: runtime.contacts(),
                            match_clock,
                            match_duration_seconds: match_duration.as_secs_f64(),
                            pre_match_remaining_seconds,
                            match_running,
                            simulation_clock,
                            physics_tick_ms,
                            physics_load_percent,
                            ticks_per_second,
                            target_ticks_per_second: 60.0,
                            clock_drift_ms,
                            step_metrics: runtime.step_metrics(),
                            physics: physics.clone(),
                            drive: DriveSync {
                                max_acceleration_mps2: pack.arena.robot.max_acceleration_mps2,
                                max_deceleration_mps2: pack.arena.robot.max_deceleration_mps2,
                                max_turn_rate_radps: pack.arena.robot.max_turn_rate_radps,
                                max_angular_acceleration_radps2: pack
                                    .arena
                                    .robot
                                    .max_angular_acceleration_radps2,
                                lateral_grip_mps2: pack.arena.robot.lateral_grip_mps2,
                                traction_friction: pack.arena.robot.traction_friction,
                                track_width_m: pack.arena.robot.track_width_m,
                            },
                            semantic_events: recent_semantic_events.iter().cloned().collect(),
                            score: runtime.score_state(),
                            practice_running: practice_continue,
                            transfer_debug: runtime.transfer_debug(),
                            ball_debug: runtime.ball_debug(),
                            ball_contact_colliders: runtime.ball_contact_colliders(),
                        };
                        if let Ok(mut slot) = latest_state.lock() {
                            *slot = Some(Arc::new(state));
                        }
                    }
                    if live_phase_entered && !practice_continue && clock_now >= match_finalizes && !brace_score_applied {
                        runtime.apply_endgame_brace_multipliers(
                            endgame_brace_multipliers.unwrap_or((1.0, 1.0)),
                        );
                        brace_score_applied = true;
                        recent_semantic_events.push_back("match_end · brace multipliers applied".into());
                        while recent_semantic_events.len() > 16 {
                            recent_semantic_events.pop_front();
                        }
                        continue;
                    }
                    if live_phase_entered && !practice_continue && clock_now >= match_finalizes {
                        let score = runtime.score_state();
                        if let Some(reports) = &report_tx { let _ = reports.send(MatchReport::Completion(MatchCompletionReport {
                            match_id: match_id.clone(), completion_id: format!("{match_id}:complete:{tick}:finished"), tick,
                            reason: "finished".to_string(), game_pack_version: pack.manifest.version.clone(),
                            result: serde_json::json!({ "redScore": score.red_score, "blueScore": score.blue_score, "globalScore": score.global_score, "breakdown": score.breakdown }),
                        })); }
                        simulation_shutdown.store(true, Ordering::Relaxed);
                    }
                }
            })
            .expect("failed to start match physics thread");

        Ok(handle)
    }
}

mod protocol;
use protocol::encode_state;

#[derive(Clone, Copy, Default)]
struct ProcessMetrics {
    cpu_percent: f64,
    rss_mib: f64,
}

#[derive(Default)]
struct ProcessSampler {
    previous_process_ticks: u64,
    previous_system_ticks: u64,
}

impl ProcessSampler {
    fn sample(&mut self) -> ProcessMetrics {
        let process_ticks = std::fs::read_to_string("/proc/self/stat")
            .ok()
            .and_then(|stat| stat.rsplit_once(')').map(|(_, fields)| fields.to_string()))
            .and_then(|fields| {
                let fields = fields.split_whitespace().collect::<Vec<_>>();
                Some(fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?)
            })
            .unwrap_or(self.previous_process_ticks);
        let system_ticks = std::fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|stat| stat.lines().next().map(str::to_string))
            .map(|line| {
                line.split_whitespace()
                    .skip(1)
                    .filter_map(|value| value.parse::<u64>().ok())
                    .sum()
            })
            .unwrap_or(self.previous_system_ticks);
        let process_delta = process_ticks.saturating_sub(self.previous_process_ticks);
        let system_delta = system_ticks.saturating_sub(self.previous_system_ticks);
        self.previous_process_ticks = process_ticks;
        self.previous_system_ticks = system_ticks;
        let cpu_count = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1) as f64;
        let cpu_percent = if system_delta == 0 {
            0.0
        } else {
            process_delta as f64 / system_delta as f64 * cpu_count * 100.0
        };
        let rss_mib = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| {
                status
                    .lines()
                    .find(|line| line.starts_with("VmRSS:"))
                    .and_then(|line| line.split_whitespace().nth(1))
                    .and_then(|value| value.parse::<f64>().ok())
            })
            .map(|kib| kib / 1024.0)
            .unwrap_or(0.0);
        ProcessMetrics {
            cpu_percent,
            rss_mib,
        }
    }
}

fn section(output: &mut Vec<u8>, tag: u16, write: impl FnOnce(&mut Vec<u8>)) {
    put_u16(output, tag);
    put_u16(output, 0);
    let length_offset = output.len();
    put_u32(output, 0);
    let start = output.len();
    write(output);
    let length = (output.len() - start) as u32;
    output[length_offset..length_offset + 4].copy_from_slice(&length.to_le_bytes());
}

fn put_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}
fn put_u8(output: &mut Vec<u8>, value: u8) {
    output.push(value);
}
fn put_i32(output: &mut Vec<u8>, value: i32) {
    output.extend_from_slice(&value.to_le_bytes());
}
fn put_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}
fn put_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}
fn put_f32(output: &mut Vec<u8>, value: f32) {
    output.extend_from_slice(&value.to_le_bytes());
}
fn put_f64(output: &mut Vec<u8>, value: f64) {
    output.extend_from_slice(&value.to_le_bytes());
}
fn put_string(output: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    put_u16(output, bytes.len().min(u16::MAX as usize) as u16);
    output.extend_from_slice(&bytes[..bytes.len().min(u16::MAX as usize)]);
}

#[cfg(test)]
#[path = "match_registry/tests.rs"]
mod protocol_tests;
