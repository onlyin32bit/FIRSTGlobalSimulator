use axum::extract::Query;
use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, process::Command, sync::Arc, time::Instant};
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

pub mod auth;
pub mod game;

use auth::TicketClaims;
use game::match_registry::{MatchInput, MatchRegistry};
use game::pack_loader::{GamePackRuntimeSnapshot, PackLoader};

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

#[tokio::main]
async fn main() {
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
    let registry = Arc::new(MatchRegistry::new(pack.clone()));
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
    info!("Listening on 0.0.0.0:3000; matches are created on demand");
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
) -> Result<game::pack_loader::GamePackMetadata, String> {
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

async fn execute_command(registry: &MatchRegistry, command: HeartbeatCommand) -> CommandResult {
    let result = match command.r#type.as_str() {
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
                            results.push(execute_command(&registry, command).await);
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
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
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
            ws.on_upgrade(move |socket| handle_socket(socket, claims, state.registry))
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
    claims: auth::TicketClaims,
    registry: Arc<MatchRegistry>,
) {
    if registry
        .is_player_kicked(&claims.match_id, &claims.sub)
        .await
    {
        return;
    }
    let match_handle = registry.get_or_create_match(&claims.match_id).await;
    let _ = match_handle
        .input_tx
        .send(MatchInput::PlayerJoin {
            user_id: claims.sub.clone(),
            name: claims.display_name,
            // Lobby tickets bind a player to an alliance. Older development
            // tickets retain their team name and remain backward compatible.
            team_name: claims.alliance.unwrap_or(claims.team_name),
            slot_id: claims.slot_id,
        })
        .await;
    let mut state_rx = match_handle.state_tx.subscribe();
    let (mut sender, mut receiver) = socket.split();
    let mut control_check = tokio::time::interval(std::time::Duration::from_millis(250));
    loop {
        tokio::select! {
            _ = control_check.tick() => {
                if registry.is_player_kicked(&claims.match_id, &claims.sub).await || registry.is_match_stopped(&claims.match_id).await { break; }
            },
            message = receiver.next() => match message {
                Some(Ok(Message::Binary(bin))) => {
                    if bin.len() == 25 && bin[0] == 1 {
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
                        
                        let _ = match_handle.input_tx.send(MatchInput::PlayerInput { user_id: claims.sub.clone(), move_x, move_z, intake_power, outtake_power, sequence }).await;
                    }
                }
                Some(Ok(Message::Text(text))) => match serde_json::from_str(&text) {
                    Ok(ClientMessage::Input { sequence, move_x, move_z, intake_power, outtake_power }) => {
                        let _ = match_handle.input_tx.send(MatchInput::PlayerInput { user_id: claims.sub.clone(), move_x, move_z, intake_power, outtake_power, sequence }).await;
                    }
                    Ok(ClientMessage::RobotSpecs { capacity, intake_rate_bps, outtake_rate_bps, outtake_velocity_mps, outtake_angle_deg, flywheel_width_m }) => {
                        let _ = match_handle.input_tx.send(MatchInput::PlayerMech { user_id: claims.sub.clone(), mech: game::sphere_runtime::MechSpec {
                            capacity,
                            intake_rate_bps,
                            outtake_rate_bps,
                            outtake_velocity_mps,
                            outtake_angle_deg,
                            flywheel_width_m,
                            intake_surface_speed_mps: None,
                        } }).await;
                    }
                    Ok(ClientMessage::ContinuePractice) => {
                        let _ = match_handle.input_tx.send(MatchInput::ContinuePractice).await;
                    }
                    Ok(ClientMessage::EndPractice) => {
                        let _ = match_handle.input_tx.send(MatchInput::EndPractice).await;
                    }
                    Ok(ClientMessage::Ping { nonce }) => {
                        if let Ok(message) = serde_json::to_string(&PongMessage { r#type: "pong", nonce })
                            && sender.send(Message::Text(message.into())).await.is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => {}
                },
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
            Ok(state) = state_rx.recv() => {
                if sender.send(Message::Binary(state)).await.is_err() { break; }
            }
        }
    }
    let _ = match_handle
        .input_tx
        .send(MatchInput::PlayerLeave {
            user_id: claims.sub,
        })
        .await;
}
