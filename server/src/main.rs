use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::Response,
    routing::get,
    Router,
};
use std::net::SocketAddr;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

pub mod game;
pub mod auth;

use game::pack_loader::PackLoader;
use game::rhai_engine::RhaiEngine;
use auth::verify_ticket;
use axum::extract::Query;
use std::collections::HashMap;

#[tokio::main]
async fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    info!("Starting FGC 2026 Game Server (Engine v0.1.0)...");

    let loader = PackLoader::new("0.1.0");
    let manifest = loader.load_manifest("../pkgs/games/fgc-2026/manifest.json");

    if let Ok(manifest) = manifest {
        let mut rhai = RhaiEngine::new();
        rhai.load_script("../pkgs/games/fgc-2026/rules/scoring.rhai");
    } else {
        tracing::error!("Failed to load game pack: {:?}", manifest.unwrap_err());
    }

    let registry = std::sync::Arc::new(game::match_registry::MatchRegistry::new());

    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/ws/match/:match_id", get(ws_handler))
        .with_state(registry);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    info!("Listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(registry): axum::extract::State<std::sync::Arc<game::match_registry::MatchRegistry>>,
    axum::extract::Path(match_id): axum::extract::Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let ticket = match params.get("ticket") {
        Some(t) => t,
        None => return Response::builder().status(401).body("Missing ticket".into()).unwrap(),
    };

    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());
    
    match verify_ticket(ticket, &secret) {
        Ok(claims) => {
            if claims.match_id != match_id {
                return Response::builder().status(403).body("Invalid match ID".into()).unwrap();
            }
            ws.on_upgrade(move |socket| handle_socket(socket, claims, registry))
        }
        Err(e) => {
            tracing::error!("Ticket verification failed: {:?}", e);
            Response::builder().status(401).body("Invalid ticket".into()).unwrap()
        }
    }
}

async fn handle_socket(mut socket: WebSocket, claims: auth::TicketClaims, registry: std::sync::Arc<game::match_registry::MatchRegistry>) {
    info!("New WebSocket connection from user {} (Team: {}) for match {}", claims.sub, claims.team_name, claims.match_id);

    let match_handle = registry.get_or_create_match(&claims.match_id).await;
    
    // Notify match that a player joined
    let _ = match_handle.input_tx.send(game::match_registry::MatchInput::PlayerJoin {
        user_id: claims.sub.clone(),
        robot_data: claims.robot_data.clone(),
    }).await;

    let mut state_rx = match_handle.state_rx.subscribe();

    loop {
        tokio::select! {
            msg = socket.recv() => {
                let msg = match msg {
                    Some(Ok(msg)) => msg,
                    _ => break, // Disconnected or error
                };

                match msg {
                    Message::Binary(data) => {
                        // Forward inputs to game loop
                        let _ = match_handle.input_tx.send(game::match_registry::MatchInput::PlayerInput {
                            user_id: claims.sub.clone(),
                            gamepad_data: data.to_vec(),
                        }).await;
                    }
                    Message::Text(t) => {
                        info!("Received from {}: {}", claims.sub, t);
                    }
                    _ => {}
                }
            }
            Ok(state) = state_rx.recv() => {
                // Send physics state to client
                // Here we would encode state.transforms into binary and send it
                // let _ = socket.send(Message::Binary(state.transforms)).await;
            }
        }
    }

    // Notify match that a player left
    let _ = match_handle.input_tx.send(game::match_registry::MatchInput::PlayerLeave {
        user_id: claims.sub.clone(),
    }).await;
    
    info!("User {} disconnected from match {}", claims.sub, claims.match_id);
}
