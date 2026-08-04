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

use auth::verify_ticket;
use game::match_registry::{MatchInput, MatchRegistry, TEST_MATCH_ID};
use game::pack_loader::{GamePackMetadata, PackLoader};

#[derive(Clone)]
struct AppState {
    registry: Arc<MatchRegistry>,
    pack: Arc<GamePackMetadata>,
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
    let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    info!("Starting FGC 2026 Game Server (Engine v0.1.0)...");

    let loader = PackLoader::new("0.1.0");
    let pack = loader
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .expect("game pack metadata must compile before the server starts");

    let pack = Arc::new(pack);
    let registry = Arc::new(MatchRegistry::new(pack.clone()));
    registry.start_test_match().await;
    start_heartbeat(registry.clone());
    let state = AppState { registry, pack };
    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/pack/metadata", get(pack_metadata_handler))
        .route("/ws/match/{match_id}", get(ws_handler))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 3000)))
        .await
        .unwrap();
    info!("Listening on 0.0.0.0:3000; always-on match: {TEST_MATCH_ID}");
    axum::serve(listener, app).await.unwrap();
}

fn start_heartbeat(registry: Arc<MatchRegistry>) {
    let Some(control_plane) = std::env::var("CONTROL_PLANE_URL")
        .ok()
        .filter(|value| !value.is_empty())
    else {
        info!("CONTROL_PLANE_URL is not configured; game server heartbeat disabled");
        return;
    };
    let Some(key) = std::env::var("GAME_SERVER_KEY")
        .ok()
        .filter(|value| !value.is_empty())
    else {
        tracing::warn!("GAME_SERVER_KEY is not configured; game server heartbeat disabled");
        return;
    };
    let endpoint = format!(
        "{}/api/game-servers/heartbeat",
        control_plane.trim_end_matches('/')
    );
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let slots = std::env::var("GAME_SERVER_SLOTS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(10);
        loop {
            let active_matches = registry.match_count().await as u64;
            let result = client.post(&endpoint)
                .header("X-Game-Server-Key", &key)
                .json(&serde_json::json!({ "activeUsers": 0, "activeMatches": active_matches, "slots": slots, "version": env!("CARGO_PKG_VERSION") }))
                .send().await;
            if let Err(error) = result {
                tracing::warn!(%error, "game server heartbeat failed");
            }
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        }
    });
}

async fn pack_metadata_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::Json<game::error::ApiResponse<GamePackMetadata>> {
    axum::Json(game::error::ApiResponse::success((*state.pack).clone()))
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
    let secret = match std::env::var("JWT_SECRET") {
        Ok(secret) if !secret.is_empty() => secret,
        _ => {
            tracing::warn!(
                "Using the development JWT_SECRET fallback; configure JWT_SECRET before deployment"
            );
            "fgc26-local-development-jwt-secret-change-before-deploy".to_string()
        }
    };
    match verify_ticket(ticket, &secret) {
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
            team_name: claims.team_name,
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
