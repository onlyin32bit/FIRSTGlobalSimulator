use axum::extract::Query;
use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    process::Command,
    sync::Arc,
    time::Instant,
};
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

use crate::auth::TicketClaims;
use crate::game::match_registry::{MatchBootstrap, MatchInput, MatchRegistry, MatchReport};
use crate::game::pack_loader::{GamePackRuntimeSnapshot, PackLoader};

#[derive(Clone)]
struct AppState {
    registry: Arc<MatchRegistry>,
    control_plane: ControlPlane,
}

#[derive(Clone)]
struct ControlPlane {
    api_url: String,
    game_server_key: String,
    max_users: u64,
    max_matches: u64,
    slots: u64,
    client: reqwest::Client,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FlyInstance {
    machine_id: String,
    app_name: Option<String>,
    region: Option<String>,
    private_ip: Option<String>,
}

#[derive(Clone)]
struct HostIdentity {
    platform: &'static str,
    hostname: String,
    machine_id: Option<String>,
    app_name: Option<String>,
    region: Option<String>,
    private_ip: Option<String>,
    instances: Vec<FlyInstance>,
    started_at: Instant,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HostRuntime<'a> {
    platform: &'a str,
    hostname: &'a str,
    machine_id: &'a Option<String>,
    app_name: &'a Option<String>,
    region: &'a Option<String>,
    private_ip: &'a Option<String>,
    os: &'static str,
    arch: &'static str,
    cpu_cores: usize,
    memory_total_bytes: u64,
    cpu_percent: f64,
    rss_bytes: u64,
    uptime_seconds: f64,
}

#[derive(Default)]
struct HostSampler {
    previous_process_ticks: u64,
    previous_system_ticks: u64,
}

impl HostSampler {
    fn sample<'a>(&mut self, identity: &'a HostIdentity) -> HostRuntime<'a> {
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
        let cpu_cores = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1);
        let rss_bytes = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|status| {
                status
                    .lines()
                    .find(|line| line.starts_with("VmRSS:"))
                    .and_then(|line| line.split_whitespace().nth(1))
                    .and_then(|value| value.parse::<u64>().ok())
            })
            .unwrap_or(0)
            * 1024;
        let memory_total_bytes = std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|status| {
                status
                    .lines()
                    .find(|line| line.starts_with("MemTotal:"))
                    .and_then(|line| line.split_whitespace().nth(1))
                    .and_then(|value| value.parse::<u64>().ok())
            })
            .unwrap_or(0)
            * 1024;
        HostRuntime {
            platform: identity.platform,
            hostname: &identity.hostname,
            machine_id: &identity.machine_id,
            app_name: &identity.app_name,
            region: &identity.region,
            private_ip: &identity.private_ip,
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            cpu_cores,
            memory_total_bytes,
            cpu_percent: if system_delta == 0 {
                0.0
            } else {
                process_delta as f64 / system_delta as f64 * cpu_cores as f64 * 100.0
            },
            rss_bytes,
            uptime_seconds: identity.started_at.elapsed().as_secs_f64(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HeartbeatCommand {
    id: String,
    r#type: String,
    match_id: Option<String>,
    user_id: Option<String>,
}

#[derive(Deserialize)]
struct HeartbeatResponse {
    commands: Vec<HeartbeatCommand>,
}

#[derive(Serialize)]
struct CommandResult {
    id: String,
    ok: bool,
    error: Option<String>,
}

impl ControlPlane {
    fn from_env() -> Result<Self, String> {
        let api_url = std::env::var("API_URL")
            .ok()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "API_URL must point to the control-plane API".to_string())?
            .trim_end_matches('/')
            .to_string();
        let game_server_key = std::env::var("GAME_SERVER_KEY")
            .ok()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                "GAME_SERVER_KEY must be the key generated in the admin dashboard".to_string()
            })?;
        let capacity = |name: &str, default: u64| {
            std::env::var(name)
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(default)
        };
        Ok(Self {
            api_url,
            game_server_key,
            max_users: capacity("GAME_SERVER_MAX_USERS", 50),
            max_matches: capacity("GAME_SERVER_MAX_MATCHES", 10),
            slots: capacity("GAME_SERVER_SLOTS", 10),
            client: reqwest::Client::new(),
        })
    }

    fn endpoint(&self, path: &str) -> String {
        let base = self.api_url.trim_end_matches('/');
        if base.ends_with("/api") {
            format!("{base}/{path}")
        } else {
            format!("{base}/api/{path}")
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Input {
        sequence: u64,
        move_x: f32,
        move_z: f32,
        #[serde(default)]
        intake_power: f32,
        #[serde(default)]
        outtake_power: f32,
        #[serde(default)]
        climb_power: f32,
    },
    RobotSpecs {
        #[serde(default)]
        capacity: Option<usize>,
        #[serde(default)]
        intake_rate_bps: Option<f32>,
        #[serde(default)]
        outtake_rate_bps: Option<f32>,
        #[serde(default)]
        outtake_velocity_mps: Option<f32>,
        #[serde(default)]
        outtake_angle_deg: Option<f32>,
        #[serde(default)]
        flywheel_width_m: Option<f32>,
    },
    ContinuePractice,
    EndPractice,
    Ping {
        nonce: u64,
    },
}

#[derive(Serialize)]
struct PongMessage {
    r#type: &'static str,
    nonce: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConnectionAccess {
    Driver,
    Observer,
}

fn authorize_connection(
    claims: &TicketClaims,
    bootstrap: &MatchBootstrap,
) -> Result<ConnectionAccess, String> {
    let role = claims.role.as_deref().unwrap_or("spectator");
    if matches!(role, "spectator" | "reviewer")
        || (role == "host" && claims.sub == bootstrap.host_id)
    {
        return Ok(ConnectionAccess::Observer);
    }
    if !bootstrap.open_arena {
        let participant = bootstrap
            .participants
            .iter()
            .find(|entry| entry.user_id == claims.sub)
            .ok_or_else(|| "User is not in the locked roster.".to_string())?;
        if participant.role != role
            || claims.slot_id.as_deref() != Some(participant.slot_id.as_str())
            || claims.alliance.as_deref() != Some(participant.alliance.as_str())
            || claims.robot_id.as_deref() != participant.robot_id.as_deref()
        {
            return Err("Ticket no longer matches the locked station and robot.".to_string());
        }
    }
    Ok(if role == "driver" {
        ConnectionAccess::Driver
    } else {
        ConnectionAccess::Observer
    })
}

struct InputGate {
    tokens: f32,
    last: Instant,
    malformed: u8,
    rejected: u8,
    last_sequence: Option<u64>,
}
impl InputGate {
    fn new() -> Self {
        Self {
            tokens: 90.0,
            last: Instant::now(),
            malformed: 0,
            rejected: 0,
            last_sequence: None,
        }
    }
    fn allow(&mut self) -> bool {
        let now = Instant::now();
        self.tokens = (self.tokens + now.duration_since(self.last).as_secs_f32() * 60.0).min(90.0);
        self.last = now;
        if self.tokens < 1.0 {
            self.rejected = self.rejected.saturating_add(1);
            false
        } else {
            self.tokens -= 1.0;
            true
        }
    }
    fn malformed(&mut self) -> bool {
        self.malformed = self.malformed.saturating_add(1);
        self.malformed >= 8
    }
    fn rejected(&mut self) -> bool {
        self.rejected = self.rejected.saturating_add(1);
        self.rejected >= 24
    }
    fn valid_input(&mut self, sequence: u64, values: &[f32]) -> bool {
        let valid = values
            .iter()
            .all(|value| value.is_finite() && (-1.0..=1.0).contains(value))
            && self.last_sequence.is_none_or(|last| sequence >= last);
        if valid {
            self.last_sequence = Some(sequence);
        } else {
            self.rejected = self.rejected.saturating_add(1);
        }
        valid
    }
}

pub async fn run() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    info!("Starting FGC 2026 Game Server (Engine v0.1.0)...");

    let control_plane = ControlPlane::from_env()
        .expect("API_URL and GAME_SERVER_KEY must be configured before the game server starts");
    let pack = fetch_api_pack(&PackLoader::new("0.1.0"), &control_plane)
        .await
        .expect("the API game-pack runtime snapshot must load before the server starts");

    let pack = Arc::new(pack);
    let (report_tx, report_rx) = tokio::sync::mpsc::unbounded_channel();
    let registry = Arc::new(MatchRegistry::with_reports(pack.clone(), report_tx));
    start_match_reporter(control_plane.clone(), report_rx);
    let host_identity = discover_host_identity();
    info!(machine_id = ?host_identity.machine_id, region = ?host_identity.region, instances = host_identity.instances.len(), "Discovered host inventory");
    start_heartbeat(registry.clone(), control_plane.clone(), host_identity);
    info!(pack = %pack.manifest.id, version = %pack.manifest.version, "Loaded game-pack runtime snapshot from API");
    let state = AppState {
        registry,
        control_plane,
    };
    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/ws/match/{match_id}", get(ws_handler))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 3000)))
        .await
        .unwrap();
    info!("Listening on 0.0.0.0:3000; matches require a control-plane bootstrap");
    axum::serve(listener, app).await.unwrap();
}

#[derive(Deserialize)]
struct ControlPlaneResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<ControlPlaneError>,
}

#[derive(Deserialize)]
struct ControlPlaneError {
    message: Option<String>,
}

/// All game-pack authority is in the control-plane API. A host fetches and
/// compiles one immutable in-memory snapshot; it never reads or serves pkgs.
async fn fetch_api_pack(
    loader: &PackLoader,
    control_plane: &ControlPlane,
) -> Result<crate::game::pack_loader::GamePackMetadata, String> {
    let endpoint = control_plane.endpoint("game-packs/fgc-2026/runtime");
    info!(%endpoint, "Fetching game-pack runtime snapshot from API");
    let response = control_plane
        .client
        .get(&endpoint)
        .header("X-Game-Server-Key", &control_plane.game_server_key)
        .send()
        .await
        .map_err(|error| format!("could not reach pack API at {endpoint}: {error}"))?;
    let status = response.status();
    let payload: ControlPlaneResponse<GamePackRuntimeSnapshot> = response
        .json()
        .await
        .map_err(|error| format!("API runtime response was invalid JSON: {error}"))?;
    if !status.is_success() || !payload.success {
        let detail = payload
            .error
            .and_then(|error| error.message)
            .unwrap_or_else(|| status.to_string());
        return Err(format!("API rejected game-pack runtime request: {detail}"));
    }
    let snapshot = payload
        .data
        .ok_or_else(|| "API runtime response did not include a pack snapshot".to_string())?;
    loader
        .load_runtime_snapshot(snapshot)
        .map_err(|error| error.to_string())
}

fn discover_host_identity() -> HostIdentity {
    let app_name = std::env::var("FLY_APP_NAME")
        .ok()
        .filter(|value| !value.is_empty());
    let instances = if app_name.is_some() {
        Command::new("dig")
            .args(["+short", "TXT", "_instances.internal"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .filter_map(|line| {
                        let parts = line
                            .trim()
                            .trim_matches('"')
                            .split(',')
                            .map(str::trim)
                            .collect::<Vec<_>>();
                        (parts.len() >= 4 && !parts[0].is_empty()).then(|| FlyInstance {
                            machine_id: parts[0].to_string(),
                            app_name: (!parts[1].is_empty()).then(|| parts[1].to_string()),
                            private_ip: (!parts[2].is_empty()).then(|| parts[2].to_string()),
                            region: (!parts[3].is_empty()).then(|| parts[3].to_string()),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    HostIdentity {
        platform: if app_name.is_some() { "fly" } else { "unknown" },
        hostname: std::env::var("HOSTNAME").unwrap_or_else(|_| "unknown".to_string()),
        machine_id: std::env::var("FLY_MACHINE_ID")
            .ok()
            .filter(|value| !value.is_empty()),
        app_name,
        region: std::env::var("FLY_REGION")
            .ok()
            .filter(|value| !value.is_empty()),
        private_ip: std::env::var("FLY_PRIVATE_IP")
            .ok()
            .filter(|value| !value.is_empty()),
        instances,
        started_at: Instant::now(),
    }
}

async fn execute_command(
    registry: &MatchRegistry,
    control_plane: &ControlPlane,
    command: HeartbeatCommand,
) -> CommandResult {
    let result = match command.r#type.as_str() {
        "bootstrap_match" => match command.match_id.as_deref() {
            Some(match_id) => match fetch_bootstrap(control_plane, match_id).await {
                Ok(bootstrap) => registry.get_or_create_match(bootstrap).await.map(|_| ()),
                Err(error) => Err(error),
            },
            None => Err("bootstrap_match requires matchId".to_string()),
        },
        "kick_player" => match (command.match_id.as_deref(), command.user_id.as_deref()) {
            (Some(match_id), Some(user_id)) => registry.kick_player(match_id, user_id).await,
            _ => Err("kick_player requires matchId and userId".to_string()),
        },
        "stop_match" | "clear_match" => match command.match_id.as_deref() {
            Some(match_id) => registry.stop_match(match_id).await,
            None => Err("This command requires matchId".to_string()),
        },
        "cleanup_idle" => {
            registry.cleanup_idle().await;
            Ok(())
        }
        "reset_host" => {
            registry.reset_host().await;
            Ok(())
        }
        _ => Err("Unknown control command".to_string()),
    };
    CommandResult {
        id: command.id,
        ok: result.is_ok(),
        error: result.err(),
    }
}

fn start_heartbeat(
    registry: Arc<MatchRegistry>,
    control_plane: ControlPlane,
    host_identity: HostIdentity,
) {
    let endpoint = control_plane.endpoint("game-servers/heartbeat");
    tokio::spawn(async move {
        let mut sampler = HostSampler::default();
        let mut results = Vec::<CommandResult>::new();
        loop {
            let active_matches = registry.match_count().await as u64;
            let active_users = registry.active_user_count().await as u64;
            let matches = registry.telemetry().await;
            let runtime = sampler.sample(&host_identity);
            let result = control_plane.client.post(&endpoint)
                .header("X-Game-Server-Key", &control_plane.game_server_key)
                .json(&serde_json::json!({ "activeUsers": active_users, "activeMatches": active_matches, "maxUsers": control_plane.max_users, "maxMatches": control_plane.max_matches, "slots": control_plane.slots, "version": env!("CARGO_PKG_VERSION"), "runtime": runtime, "instances": &host_identity.instances, "matches": &matches, "commandResults": &results }))
                .send().await;
            match result {
                Ok(response) => match response
                    .json::<ControlPlaneResponse<HeartbeatResponse>>()
                    .await
                {
                    Ok(payload) if payload.success => {
                        results = Vec::new();
                        for command in payload.data.map(|data| data.commands).unwrap_or_default() {
                            results.push(execute_command(&registry, &control_plane, command).await);
                        }
                    }
                    Ok(payload) => {
                        tracing::warn!(error = ?payload.error.and_then(|error| error.message), "game server heartbeat rejected")
                    }
                    Err(error) => {
                        tracing::warn!(%error, "game server heartbeat response was invalid")
                    }
                },
                Err(error) => tracing::warn!(%error, "game server heartbeat failed"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    });
}

#[derive(Deserialize)]
struct TicketVerification {
    claims: TicketClaims,
}

async fn verify_ticket(control_plane: &ControlPlane, ticket: &str) -> Result<TicketClaims, String> {
    let endpoint = control_plane.endpoint("game-servers/tickets/verify");
    let response = control_plane
        .client
        .post(&endpoint)
        .header("X-Game-Server-Key", &control_plane.game_server_key)
        .json(&serde_json::json!({ "ticket": ticket }))
        .send()
        .await
        .map_err(|error| format!("ticket verification request failed: {error}"))?;
    let status = response.status();
    let payload: ControlPlaneResponse<TicketVerification> = response
        .json()
        .await
        .map_err(|error| format!("ticket verification response was invalid JSON: {error}"))?;
    if !status.is_success() || !payload.success {
        return Err(payload
            .error
            .and_then(|error| error.message)
            .unwrap_or_else(|| status.to_string()));
    }
    payload
        .data
        .map(|data| data.claims)
        .ok_or_else(|| "ticket verification did not return claims".to_string())
}

#[derive(Deserialize)]
struct BootstrapResponse {
    bootstrap: MatchBootstrap,
}

async fn fetch_bootstrap(
    control_plane: &ControlPlane,
    match_id: &str,
) -> Result<MatchBootstrap, String> {
    let endpoint = control_plane.endpoint(&format!("game-servers/matches/{match_id}/bootstrap"));
    let response = control_plane
        .client
        .get(&endpoint)
        .header("X-Game-Server-Key", &control_plane.game_server_key)
        .send()
        .await
        .map_err(|error| format!("bootstrap request failed: {error}"))?;
    let status = response.status();
    let payload: ControlPlaneResponse<BootstrapResponse> = response
        .json()
        .await
        .map_err(|error| format!("bootstrap response was invalid JSON: {error}"))?;
    if !status.is_success() || !payload.success {
        return Err(payload
            .error
            .and_then(|error| error.message)
            .unwrap_or_else(|| status.to_string()));
    }
    payload
        .data
        .map(|data| data.bootstrap)
        .ok_or_else(|| "bootstrap response was empty".to_string())
}

fn start_match_reporter(
    control_plane: ControlPlane,
    mut reports: tokio::sync::mpsc::UnboundedReceiver<MatchReport>,
) {
    tokio::spawn(async move {
        let mut pending =
            HashMap::<String, VecDeque<crate::game::match_registry::MatchEventReport>>::new();
        let mut flush = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = flush.tick() => flush_event_batches(&control_plane, &mut pending).await,
                report = reports.recv() => match report {
                    Some(MatchReport::Event(event)) => {
                        let queue = pending.entry(event.match_id.clone()).or_default();
                        if !queue.iter().any(|queued| queued.event_id == event.event_id) { queue.push_back(event); }
                        if queue.len() >= 64 { flush_event_batches(&control_plane, &mut pending).await; }
                    }
                    Some(MatchReport::Completion(completion)) => {
                        flush_event_batches(&control_plane, &mut pending).await;
                        send_completion(&control_plane, completion).await;
                    }
                    None => { flush_event_batches(&control_plane, &mut pending).await; break; }
                }
            }
        }
    });
}

async fn flush_event_batches(
    control_plane: &ControlPlane,
    pending: &mut HashMap<String, VecDeque<crate::game::match_registry::MatchEventReport>>,
) {
    for (match_id, queue) in pending.iter_mut() {
        while !queue.is_empty() {
            let batch = queue
                .iter()
                .take(100)
                .map(|event| {
                    serde_json::json!({
                        "eventId": event.event_id, "tick": event.tick, "kind": event.kind,
                        "payload": event.payload, "gamePackVersion": event.game_pack_version,
                    })
                })
                .collect::<Vec<_>>();
            let endpoint =
                control_plane.endpoint(&format!("game-servers/matches/{match_id}/events"));
            let response = control_plane
                .client
                .post(&endpoint)
                .header("X-Game-Server-Key", &control_plane.game_server_key)
                .json(&serde_json::json!({ "events": batch }))
                .send()
                .await;
            match response {
                Ok(response) if response.status().is_success() => {
                    for _ in 0..batch.len() {
                        queue.pop_front();
                    }
                }
                Ok(response) => {
                    let status = response.status();
                    let detail = response.text().await.unwrap_or_default();
                    tracing::warn!(%match_id, %status, %detail, "event batch deferred");
                    break;
                }
                Err(error) => {
                    tracing::warn!(%match_id, %error, "event batch deferred");
                    break;
                }
            }
        }
    }
    pending.retain(|_, queue| !queue.is_empty());
}

async fn send_completion(
    control_plane: &ControlPlane,
    completion: crate::game::match_registry::MatchCompletionReport,
) {
    let match_id = completion.match_id;
    let endpoint = control_plane.endpoint(&format!("game-servers/matches/{match_id}/complete"));
    let body = serde_json::json!({ "completionId": completion.completion_id, "tick": completion.tick, "reason": completion.reason, "result": completion.result, "gamePackVersion": completion.game_pack_version });
    match control_plane
        .client
        .post(&endpoint)
        .header("X-Game-Server-Key", &control_plane.game_server_key)
        .json(&body)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => {}
        Ok(response) => {
            let status = response.status();
            let detail = response.text().await.unwrap_or_default();
            tracing::error!(%match_id, %status, %detail, "match completion rejected")
        }
        Err(error) => tracing::error!(%match_id, %error, "match completion failed"),
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Path(match_id): axum::extract::Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let Some(ticket) = params.get("ticket") else {
        return Response::builder()
            .status(401)
            .body("Missing ticket".into())
            .unwrap();
    };
    match verify_ticket(&state.control_plane, ticket).await {
        Ok(claims) if claims.match_id == match_id => {
            match fetch_bootstrap(&state.control_plane, &match_id)
                .await
                .and_then(|bootstrap| {
                    authorize_connection(&claims, &bootstrap).map(|access| (bootstrap, access))
                }) {
                Ok((bootstrap, access)) => {
                    match state.registry.get_or_create_match(bootstrap).await {
                        Ok(registry) => ws.on_upgrade(move |socket| {
                            handle_socket(socket, claims, registry, access)
                        }),
                        Err(error) => Response::builder().status(409).body(error.into()).unwrap(),
                    }
                }
                Err(error) => Response::builder().status(403).body(error.into()).unwrap(),
            }
        }
        Ok(_) => Response::builder()
            .status(403)
            .body("Invalid match ID".into())
            .unwrap(),
        Err(_) => Response::builder()
            .status(401)
            .body("Invalid ticket".into())
            .unwrap(),
    }
}

async fn handle_socket(
    socket: WebSocket,
    claims: TicketClaims,
    match_handle: crate::game::match_registry::MatchHandle,
    access: ConnectionAccess,
) {
    if access == ConnectionAccess::Driver {
        let _ = match_handle
            .input_tx
            .send(MatchInput::PlayerJoin {
                user_id: claims.sub.clone(),
                name: claims.display_name,
                team_name: claims.alliance.unwrap_or(claims.team_name),
                slot_id: claims.slot_id,
            })
            .await;
    }
    let mut state_rx = match_handle.state_tx.subscribe();
    let (mut sender, mut receiver) = socket.split();
    let mut input_gate = InputGate::new();
    loop {
        tokio::select! {
            message = receiver.next() => match message {
                Some(Ok(Message::Binary(bin))) => {
                    if access != ConnectionAccess::Driver { match_handle.report_input_rejection(&claims.sub, "role_denied"); if input_gate.rejected() { break; } continue; }
                    if matches!((bin.len(), bin[0]), (25, 1) | (29, 2)) {
                        let mut arr8 = [0u8; 8];
                        arr8.copy_from_slice(&bin[1..9]);
                        let sequence = u64::from_le_bytes(arr8);

                        let mut arr4 = [0u8; 4];
                        arr4.copy_from_slice(&bin[9..13]);
                        let move_x = f32::from_le_bytes(arr4);

                        arr4.copy_from_slice(&bin[13..17]);
                        let move_z = f32::from_le_bytes(arr4);

                        arr4.copy_from_slice(&bin[17..21]);
                        let intake_power = f32::from_le_bytes(arr4);

                        arr4.copy_from_slice(&bin[21..25]);
                        let outtake_power = f32::from_le_bytes(arr4);

                        let climb_power = if bin.len() == 29 {
                            arr4.copy_from_slice(&bin[25..29]);
                            f32::from_le_bytes(arr4)
                        } else {
                            0.0
                        };

                        if !input_gate.allow() { match_handle.report_input_rejection(&claims.sub, "rate_limited"); if input_gate.rejected() { break; } continue; }
                        if !input_gate.valid_input(sequence, &[move_x, move_z, intake_power, outtake_power, climb_power]) { match_handle.report_input_rejection(&claims.sub, "invalid_frame"); if input_gate.rejected() { break; } continue; }
                        let _ = match_handle.input_tx.send(MatchInput::PlayerInput { user_id: claims.sub.clone(), move_x, move_z, intake_power, outtake_power, climb_power, sequence }).await;
                    } else { match_handle.report_input_rejection(&claims.sub, "malformed_frame"); if input_gate.malformed() { break; } }
                }
                Some(Ok(Message::Text(text))) => match serde_json::from_str(&text) {
                    Ok(ClientMessage::Input { sequence, move_x, move_z, intake_power, outtake_power, climb_power }) => {
                        if access != ConnectionAccess::Driver { match_handle.report_input_rejection(&claims.sub, "role_denied"); if input_gate.rejected() { break; } continue; }
                        if !input_gate.allow() { match_handle.report_input_rejection(&claims.sub, "rate_limited"); if input_gate.rejected() { break; } continue; }
                        if !input_gate.valid_input(sequence, &[move_x, move_z, intake_power, outtake_power, climb_power]) { match_handle.report_input_rejection(&claims.sub, "invalid_frame"); if input_gate.rejected() { break; } continue; }
                        let _ = match_handle.input_tx.send(MatchInput::PlayerInput { user_id: claims.sub.clone(), move_x, move_z, intake_power, outtake_power, climb_power, sequence }).await;
                    }
                    Ok(ClientMessage::RobotSpecs { capacity, intake_rate_bps, outtake_rate_bps, outtake_velocity_mps, outtake_angle_deg, flywheel_width_m }) => {
                        if access != ConnectionAccess::Driver || !input_gate.allow() { match_handle.report_input_rejection(&claims.sub, "role_or_rate_denied"); if input_gate.rejected() { break; } continue; }
                        let _ = match_handle.input_tx.send(MatchInput::PlayerMech { user_id: claims.sub.clone(), mech: crate::game::sphere_runtime::MechSpec {
                            capacity,
                            intake_rate_bps,
                            outtake_rate_bps,
                            outtake_velocity_mps,
                            outtake_angle_deg,
                            flywheel_width_m,
                            ..Default::default()
                        } }).await;
                    }
                    Ok(ClientMessage::ContinuePractice) => {
                        if access != ConnectionAccess::Driver { if input_gate.rejected() { break; } continue; }
                        let _ = match_handle.input_tx.send(MatchInput::ContinuePractice).await;
                    }
                    Ok(ClientMessage::EndPractice) => {
                        if access != ConnectionAccess::Driver { if input_gate.rejected() { break; } continue; }
                        let _ = match_handle.input_tx.send(MatchInput::EndPractice).await;
                    }
                    Ok(ClientMessage::Ping { nonce }) => {
                        if let Ok(message) = serde_json::to_string(&PongMessage { r#type: "pong", nonce })
                            && sender.send(Message::Text(message.into())).await.is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => { match_handle.report_input_rejection(&claims.sub, "malformed_frame"); if input_gate.malformed() { break; } }
                },
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
            Ok(state) = state_rx.recv() => {
                if sender.send(Message::Binary(state)).await.is_err() { break; }
            }
        }
    }
    if access == ConnectionAccess::Driver {
        let _ = match_handle
            .input_tx
            .send(MatchInput::PlayerLeave {
                user_id: claims.sub,
            })
            .await;
    }
}
