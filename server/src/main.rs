use axum::extract::Query;
use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;

pub mod auth;
pub mod game;

use auth::TicketClaims;
use game::match_registry::{MatchInput, MatchRegistry, TEST_MATCH_ID};
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
            .ok_or_else(|| "GAME_SERVER_KEY must be the key generated in the admin dashboard".to_string())?;
        let capacity = |name: &str, default: u64| {
            std::env::var(name).ok().and_then(|value| value.parse::<u64>().ok()).unwrap_or(default)
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
    },
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
    registry.start_test_match().await;
    start_heartbeat(registry.clone(), control_plane.clone());
    info!(pack = %pack.manifest.id, version = %pack.manifest.version, "Loaded game-pack runtime snapshot from API");
    let state = AppState { registry, control_plane };
    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/ws/match/{match_id}", get(ws_handler))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 3000)))
        .await
        .unwrap();
    info!("Listening on 0.0.0.0:3000; always-on match: {TEST_MATCH_ID}");
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
async fn fetch_api_pack(loader: &PackLoader, control_plane: &ControlPlane) -> Result<game::pack_loader::GamePackMetadata, String> {
    let endpoint = control_plane.endpoint("game-packs/fgc-2026/runtime");
    info!(%endpoint, "Fetching game-pack runtime snapshot from API");
    let response = control_plane.client
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
        let detail = payload.error.and_then(|error| error.message).unwrap_or_else(|| status.to_string());
        return Err(format!("API rejected game-pack runtime request: {detail}"));
    }
    let snapshot = payload.data.ok_or_else(|| "API runtime response did not include a pack snapshot".to_string())?;
    loader.load_runtime_snapshot(snapshot).map_err(|error| error.to_string())
}

fn start_heartbeat(registry: Arc<MatchRegistry>, control_plane: ControlPlane) {
    let endpoint = control_plane.endpoint("game-servers/heartbeat");
    tokio::spawn(async move {
        loop {
            let active_matches = registry.match_count().await as u64;
            let result = control_plane.client.post(&endpoint)
                .header("X-Game-Server-Key", &control_plane.game_server_key)
                .json(&serde_json::json!({ "activeUsers": 0, "activeMatches": active_matches, "maxUsers": control_plane.max_users, "maxMatches": control_plane.max_matches, "slots": control_plane.slots, "version": env!("CARGO_PKG_VERSION") }))
                .send().await;
            if let Err(error) = result {
                tracing::warn!(%error, "game server heartbeat failed");
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
    let response = control_plane.client
        .post(&endpoint)
        .header("X-Game-Server-Key", &control_plane.game_server_key)
        .json(&serde_json::json!({ "ticket": ticket }))
        .send()
        .await
        .map_err(|error| format!("ticket verification request failed: {error}"))?;
    let status = response.status();
    let payload: ControlPlaneResponse<TicketVerification> = response.json().await
        .map_err(|error| format!("ticket verification response was invalid JSON: {error}"))?;
    if !status.is_success() || !payload.success {
        return Err(payload.error.and_then(|error| error.message).unwrap_or_else(|| status.to_string()));
    }
    payload.data.map(|data| data.claims).ok_or_else(|| "ticket verification did not return claims".to_string())
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
    loop {
        tokio::select! {
            message = receiver.next() => match message {
                Some(Ok(Message::Text(text))) => match serde_json::from_str(&text) {
                    Ok(ClientMessage::Input { sequence, move_x, move_z, intake_power }) => {
                        let _ = match_handle.input_tx.send(MatchInput::PlayerInput { user_id: claims.sub.clone(), move_x, move_z, intake_power, sequence }).await;
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
