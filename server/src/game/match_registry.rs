use axum::body::Bytes;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::info;

use super::match_runtime::{MatchRuntime, PlayerSnapshot, ScoreState};
use super::pack_loader::{ArenaConfig, GamePackMetadata};
use super::rhai_engine::RhaiEngine;
use super::sphere_runtime::{MechSpec, SphereRuntime, StepMetrics};

pub struct MatchRegistry {
    matches: RwLock<HashMap<String, MatchHandle>>,
    pack: Arc<GamePackMetadata>,
}

#[derive(Clone)]
pub struct MatchHandle {
    pub input_tx: mpsc::Sender<MatchInput>,
    pub state_tx: broadcast::Sender<Bytes>,
    shutdown: Arc<AtomicBool>,
    kicked_users: Arc<Mutex<HashSet<String>>>,
    telemetry: Arc<Mutex<RuntimeMatchTelemetry>>,
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

#[derive(Debug, Clone)]
pub struct MatchStateSync {
    pub tick: u64,
    pub game_pack_id: String,
    pub game_pack_version: String,
    pub players: Vec<PlayerSnapshot>,
    pub object_id: String,
    pub object_radius: f32,
    pub object_color: String,
    pub object_positions: Vec<[f32; 3]>,
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
    fn new(match_id: String, pack: &GamePackMetadata) -> Self {
        if pack.arena.physics_backend == "sphere_xpbd" {
            let mut runtime = SphereRuntime::new(match_id, pack.manifest.id.clone(), 0);
            runtime.context.game_pack_version = pack.manifest.version.clone();
            runtime.create_field_arena(&pack.arena, &pack.field_definition);
            Self::Sphere(Box::new(runtime))
        } else {
            let mut runtime = MatchRuntime::new(match_id, pack.manifest.id.clone(), 0);
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
        sequence: u64,
    ) {
        match self {
            Self::Rapier(runtime) => runtime.set_player_input(id, x, z, sequence),
            Self::Sphere(runtime) => {
                runtime.set_player_input(id, x, z, intake, outtake, sequence)
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

    fn set_player_mech(&mut self, id: &str, mech: MechSpec) {
        if let Self::Sphere(runtime) = self {
            runtime.set_player_mech(id, mech);
        }
    }

    /// Remove a game piece from play after a scoring trigger. The ball is
    /// deactivated so it can never be scored twice.
    fn contain_ball(&mut self, entity_id: &str) -> bool {
        match self {
            Self::Rapier(_) => false,
            Self::Sphere(runtime) => runtime.contain_ball(entity_id),
        }
    }

    /// Accumulate a single scored outcome into the runtime score ledger.
    fn apply_score(&mut self, team: &str, category: &str, points: i32) {
        let score = match self {
            Self::Rapier(runtime) => &mut runtime.score_state,
            Self::Sphere(runtime) => &mut runtime.score_state,
        };
        match team {
            "blue" => score.blue_score += points,
            "red" => score.red_score += points,
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

    fn positions(&self) -> Vec<[f32; 3]> {
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

    pub async fn get_or_create_match(&self, match_id: &str) -> MatchHandle {
        let mut matches = self.matches.write().await;
        if let Some(handle) = matches.get(match_id) {
            return handle.clone();
        }

        let (input_tx, mut input_rx) = mpsc::channel(256);
        let (state_tx, _) = broadcast::channel(4);
        let handle = MatchHandle {
            input_tx,
            state_tx: state_tx.clone(),
            shutdown: Arc::new(AtomicBool::new(false)),
            kicked_users: Arc::new(Mutex::new(HashSet::new())),
            telemetry: Arc::new(Mutex::new(RuntimeMatchTelemetry {
                id: match_id.to_string(),
                ..Default::default()
            })),
        };
        matches.insert(match_id.to_string(), handle.clone());
        let match_id = match_id.to_string();
        let pack = self.pack.clone();
        let latest_state = Arc::new(Mutex::new(None::<Arc<MatchStateSync>>));
        let telemetry = handle.telemetry.clone();
        let publisher_shutdown = handle.shutdown.clone();

        let mut rules = RhaiEngine::new();
        for script in pack.manifest.scripts.values() {
            let Some(source) = pack.script_sources.get(script) else {
                tracing::error!(path = %script, "API runtime snapshot is missing a validated rule script");
                return handle;
            };
            if !rules.load_source(script, source) {
                tracing::error!(path = %script, "Unable to compile a validated API rule script into match runtime");
                return handle;
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
                while !publisher_shutdown.load(Ordering::Relaxed) {
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
                        let _ =
                            publisher_tx.send(Bytes::from(encode_state(&state, process_metrics)));
                    }
                }
            })
            .expect("failed to start match publisher thread");

        let simulation_shutdown = handle.shutdown.clone();
        std::thread::Builder::new()
            .name(format!("match-{match_id}"))
            .spawn(move || {
                let mut runtime = RuntimeBackend::new(match_id.clone(), &pack);
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
                // A short, server-owned staging phase gives every redirected
                // client the same 5→1 presentation and prevents movement or
                // game-piece release before the match actually starts.
                const PRE_MATCH_COUNTDOWN: Duration = Duration::from_secs(5);
                const MATCH_DURATION: Duration = Duration::from_secs(150);
                let match_created = next_tick;
                let match_started = match_created + PRE_MATCH_COUNTDOWN;
                let match_ends = match_started + MATCH_DURATION;
                let mut live_phase_entered = false;
                let mut practice_continue = false;
                let mut tps_window_started = next_tick;
                let mut ticks_in_tps_window = 0_u64;
                let mut ticks_per_second = 60.0;
                let mut tick = 0_u64;
                let mut recent_semantic_events = VecDeque::<String>::with_capacity(16);

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

                    while let Ok(input) = input_rx.try_recv() {
                        match input {
                            MatchInput::PlayerJoin {
                                user_id,
                                name,
                                team_name,
                                slot_id,
                            } => runtime.add_player(user_id, name, team_name, slot_id, &pack.arena),
                            MatchInput::PlayerLeave { user_id } => runtime.remove_player(&user_id),
                            MatchInput::PlayerInput {
                                user_id,
                                move_x,
                                move_z,
                                intake_power,
                                outtake_power,
                                sequence,
                            } => runtime.set_player_input(
                                &user_id,
                                move_x,
                                move_z,
                                intake_power,
                                outtake_power,
                                sequence,
                            ),
                            MatchInput::PlayerMech { user_id, mech } => {
                                runtime.set_player_mech(&user_id, mech)
                            }
                            MatchInput::ContinuePractice => practice_continue = true,
                            MatchInput::EndPractice => practice_continue = false,
                        }
                    }

                    let clock_now = Instant::now();
                    if !live_phase_entered && clock_now >= match_started {
                        runtime.begin_match();
                        live_phase_entered = true;
                    }
                    let match_running = live_phase_entered && clock_now < match_ends;
                    let physics_started = Instant::now();
                    if match_running || practice_continue {
                        runtime.step(&pack.arena, 1.0 / 60.0);
                    }
                    for event in runtime.drain_semantic_events() {
                        let mut label = format!("{} {} ← {}", event.kind, event.target_id, event.entity_id);
                        if match_running {
                            let outcomes = rules.on_trigger_enter(&event.target_id, &event.entity_id);
                            for outcome in outcomes {
                                label.push_str(&format!(" · {} {}/{} +{}", outcome.kind, outcome.team, outcome.category, outcome.points));
                                // Native scoring: the authored rule's outcome
                                // is the source of truth for team, category and
                                // points, so tweaking scoring.rhai rebalances a
                                // match without a rebuild.
                                runtime.apply_score(&outcome.team, &outcome.category, outcome.points as i32);
                                // SU containment and EXT extinguishing remove the
                                // piece from the field entirely (never re-scored).
                                if outcome.category == "SU" || outcome.category == "EXT" {
                                    runtime.contain_ball(&event.entity_id);
                                }
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
                        .min(MATCH_DURATION.as_secs_f64());
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
                            players: runtime.players(),
                            object_id: pack.arena.object_id.clone(),
                            object_radius: pack.arena.ball.radius_m(),
                            object_color: pack.arena.color.clone(),
                            object_positions: runtime.positions(),
                            contacts: runtime.contacts(),
                            match_clock,
                            match_duration_seconds: MATCH_DURATION.as_secs_f64(),
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
                        };
                        if let Ok(mut slot) = latest_state.lock() {
                            *slot = Some(Arc::new(state));
                        }
                    }
                }
            })
            .expect("failed to start match physics thread");

        handle
    }
}

// FGS1 is a little-endian, sectioned WebSocket protocol. Every section is
// [tag:u16, flags:u16, byte_length:u32, payload]. Readers must skip unknown
// tags, which makes compatible additions possible without changing old fields.
fn encode_state(state: &MatchStateSync, process: ProcessMetrics) -> Vec<u8> {
    const METADATA: u16 = 1;
    const CLOCKS: u16 = 2;
    const METRICS: u16 = 3;
    const PLAYERS: u16 = 4;
    const OBJECTS: u16 = 5;
    const PHYSICS: u16 = 6;
    const SEMANTIC_EVENTS: u16 = 7;
    const DRIVE: u16 = 8;
    const SCORE: u16 = 9;
    let mut output = Vec::with_capacity(1024 + state.object_positions.len() * 12);
    output.extend_from_slice(b"FGS1");
    put_u16(&mut output, 1);
    put_u16(&mut output, 4);
    put_u16(&mut output, 1); // StateSnapshot
    put_u16(&mut output, 0);
    put_u32(&mut output, 0);

    section(&mut output, METADATA, |bytes| {
        put_string(bytes, &state.game_pack_id);
        put_string(bytes, &state.game_pack_version);
        put_string(bytes, &state.object_id);
        put_string(bytes, &state.object_color);
        put_f32(bytes, state.object_radius);
    });
    section(&mut output, CLOCKS, |bytes| {
        put_u64(bytes, state.tick);
        put_f64(bytes, state.match_clock);
        put_f64(bytes, state.simulation_clock);
        put_f64(bytes, state.clock_drift_ms);
        put_f64(bytes, state.match_duration_seconds);
        put_f64(bytes, state.pre_match_remaining_seconds);
        put_u8(bytes, u8::from(state.match_running));
        put_u8(bytes, u8::from(state.practice_running));
    });
    section(&mut output, METRICS, |bytes| {
        put_f64(bytes, state.physics_tick_ms);
        put_f64(bytes, state.physics_load_percent);
        put_f64(bytes, state.ticks_per_second);
        put_f64(bytes, state.target_ticks_per_second);
        put_u32(bytes, state.contacts as u32);
        put_f64(bytes, state.step_metrics.integrate_ms);
        put_f64(bytes, state.step_metrics.broad_phase_ms);
        put_f64(bytes, state.step_metrics.solve_ms);
        put_u32(bytes, state.step_metrics.candidate_pairs as u32);
        put_u32(bytes, state.step_metrics.active_balls as u32);
        put_u32(bytes, state.step_metrics.sleeping_balls as u32);
        put_f64(bytes, process.cpu_percent);
        put_f64(bytes, process.rss_mib);
    });
    section(&mut output, PLAYERS, |bytes| {
        put_u32(bytes, state.players.len() as u32);
        for player in &state.players {
            put_string(bytes, &player.id);
            put_string(bytes, &player.name);
            put_string(bytes, &player.team_name);
            put_string(bytes, &player.color);
            for value in [
                player.x,
                player.y,
                player.z,
                player.yaw,
                player.heading_deg,
                player.velocity_x,
                player.velocity_y,
                player.velocity_z,
                player.angular_velocity_y,
            ] {
                put_f32(bytes, value);
            }
            put_u32(bytes, player.stored_balls as u32);
            put_u32(bytes, player.capacity as u32);
        }
    });
    section(&mut output, OBJECTS, |bytes| {
        put_u32(bytes, state.object_positions.len() as u32);
        for position in &state.object_positions {
            for value in position {
                put_f32(bytes, *value);
            }
        }
    });
    section(&mut output, PHYSICS, |bytes| {
        put_string(bytes, &state.physics.ball_material);
        put_string(bytes, &state.physics.floor_material);
        for value in [
            state.physics.ball_diameter_m,
            state.physics.ball_diameter_tolerance_m,
            state.physics.ball_mass_kg,
            state.physics.ball_friction,
            state.physics.ball_restitution,
            state.physics.ball_rolling_resistance_mps2,
            state.physics.floor_friction,
            state.physics.robot_mass_kg,
            state.physics.robot_width_m,
            state.physics.robot_height_m,
            state.physics.robot_length_m,
            state.physics.robot_max_speed_mps,
        ] {
            put_f32(bytes, value);
        }
        put_u8(bytes, u8::from(state.physics.intake_enabled));
        put_u8(bytes, u8::from(state.physics.ramp_enabled));
        for value in [
            state.physics.ball_inertia_factor,
            state.physics.ball_drag_coefficient,
            state.physics.air_density_kg_m3,
            state.physics.ball_ball_friction,
            state.physics.floor_static_friction,
            state.physics.floor_dynamic_friction,
            state.physics.floor_rolling_resistance_mps2,
            state.physics.intake_width_m,
            state.physics.intake_radius_m,
            state.physics.intake_forward_offset_m,
            state.physics.intake_center_height_m,
            state.physics.intake_surface_speed_mps,
            state.physics.ramp_center_x,
            state.physics.ramp_start_z,
            state.physics.ramp_width_m,
            state.physics.ramp_length_m,
            state.physics.ramp_angle_deg,
            state.physics.solver_position_iterations,
            state.physics.solver_velocity_iterations,
            state.physics.max_depenetration_speed_mps,
            state.physics.max_ball_speed_mps,
            state.physics.max_ball_angular_speed_radps,
            state.physics.max_drive_force_n,
            state.physics.max_drive_power_w,
            state.physics.max_brake_force_n,
        ] {
            put_f32(bytes, value);
        }
        for value in [
            state.physics.storage_capacity,
            state.physics.intake_rate_bps,
            state.physics.outtake_rate_bps,
            state.physics.outtake_velocity_mps,
            state.physics.outtake_angle_deg,
            state.physics.flywheel_width_m,
            state.physics.outtake_forward_offset_m,
            state.physics.outtake_height_m,
        ] {
            put_f32(bytes, value);
        }
    });
    section(&mut output, SCORE, |bytes| {
        put_i32(bytes, state.score.blue_score);
        put_i32(bytes, state.score.red_score);
        put_i32(bytes, state.score.global_score);
        put_u32(bytes, state.score.breakdown.len() as u32);
        for (category, points) in &state.score.breakdown {
            put_string(bytes, category);
            put_i32(bytes, *points);
        }
    });
    section(&mut output, SEMANTIC_EVENTS, |bytes| {
        put_u16(bytes, state.semantic_events.len() as u16);
        for event in &state.semantic_events {
            put_string(bytes, event);
        }
    });
    section(&mut output, DRIVE, |bytes| {
        for value in [
            state.drive.max_acceleration_mps2,
            state.drive.max_deceleration_mps2,
            state.drive.max_turn_rate_radps,
            state.drive.max_angular_acceleration_radps2,
            state.drive.lateral_grip_mps2,
            state.drive.traction_friction,
            state.drive.track_width_m,
        ] {
            put_f32(bytes, value);
        }
    });
    let payload_len = (output.len() - 16) as u32;
    output[12..16].copy_from_slice(&payload_len.to_le_bytes());
    output
}

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
mod protocol_tests {
    use super::*;

    #[test]
    fn binary_state_has_versioned_header_and_compact_positions() {
        let state = MatchStateSync {
            tick: 7,
            game_pack_id: "pack".into(),
            game_pack_version: "1".into(),
            players: Vec::new(),
            object_id: "ball".into(),
            object_radius: 0.05,
            object_color: "#fff".into(),
            object_positions: vec![[1.0, 2.0, 3.0]; 1000],
            contacts: 2,
            match_clock: 1.0,
            match_duration_seconds: 150.0,
            pre_match_remaining_seconds: 0.0,
            match_running: true,
            simulation_clock: 1.0,
            physics_tick_ms: 2.0,
            physics_load_percent: 12.0,
            ticks_per_second: 60.0,
            target_ticks_per_second: 60.0,
            clock_drift_ms: 0.0,
            step_metrics: StepMetrics::default(),
            physics: PhysicsSync::from(
                &crate::game::pack_loader::PackLoader::new("0.1.0")
                    .load_pack("../pkgs/games/fgc-2026/manifest.json")
                    .unwrap()
                    .arena,
            ),
            drive: DriveSync::default(),
            semantic_events: Vec::new(),
            score: ScoreState::default(),
            practice_running: false,
        };
        let encoded = encode_state(&state, ProcessMetrics::default());
        assert_eq!(&encoded[..4], b"FGS1");
        assert_eq!(u16::from_le_bytes(encoded[4..6].try_into().unwrap()), 1);
        assert_eq!(u16::from_le_bytes(encoded[6..8].try_into().unwrap()), 4);
        assert_eq!(u16::from_le_bytes(encoded[8..10].try_into().unwrap()), 1);
        assert_eq!(
            u32::from_le_bytes(encoded[12..16].try_into().unwrap()) as usize,
            encoded.len() - 16
        );
        assert!(
            encoded.len() < 14_000,
            "snapshot was {} bytes",
            encoded.len()
        );
    }
}
