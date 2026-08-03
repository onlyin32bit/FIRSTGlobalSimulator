use std::collections::HashMap;
use tokio::sync::{broadcast, mpsc, RwLock};

use super::match_runtime::{MatchRuntime, PlayerSnapshot};

pub const TEST_MATCH_ID: &str = "test-match";

pub struct MatchRegistry {
    matches: RwLock<HashMap<String, MatchHandle>>,
}

#[derive(Clone)]
pub struct MatchHandle {
    pub input_tx: mpsc::Sender<MatchInput>,
    pub state_tx: broadcast::Sender<MatchStateSync>,
}

#[derive(Debug, Clone)]
pub enum MatchInput {
    PlayerJoin { user_id: String, name: String, team_name: String },
    PlayerLeave { user_id: String },
    PlayerInput { user_id: String, move_x: f32, move_z: f32, sequence: u64 },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MatchStateSync {
    pub r#type: &'static str,
    pub tick: u64,
    pub players: Vec<PlayerSnapshot>,
}

impl MatchRegistry {
    pub fn new() -> Self {
        Self { matches: RwLock::new(HashMap::new()) }
    }

    pub async fn start_test_match(&self) {
        self.get_or_create_match(TEST_MATCH_ID).await;
    }

    pub async fn match_count(&self) -> usize {
        self.matches.read().await.len()
    }

    pub async fn get_or_create_match(&self, match_id: &str) -> MatchHandle {
        let mut matches = self.matches.write().await;
        if let Some(handle) = matches.get(match_id) {
            return handle.clone();
        }

        let (input_tx, mut input_rx) = mpsc::channel(256);
        let (state_tx, _) = broadcast::channel(32);
        let handle = MatchHandle { input_tx, state_tx: state_tx.clone() };
        matches.insert(match_id.to_string(), handle.clone());
        let match_id = match_id.to_string();

        tokio::spawn(async move {
            let mut runtime = MatchRuntime::new(match_id.clone(), "fgc-2026".into(), 0);
            runtime.create_test_arena();
            let mut interval = tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / 60.0));
            let mut tick = 0_u64;
            let mut broadcast_counter = 0_u8;

            loop {
                interval.tick().await;
                while let Ok(input) = input_rx.try_recv() {
                    match input {
                        MatchInput::PlayerJoin { user_id, name, team_name } => runtime.add_player(user_id, name, team_name),
                        MatchInput::PlayerLeave { user_id } => runtime.remove_player(&user_id),
                        MatchInput::PlayerInput { user_id, move_x, move_z, sequence } => runtime.set_player_input(&user_id, move_x, move_z, sequence),
                    }
                }

                runtime.apply_player_drive();
                runtime.tick(1.0 / 60.0);
                tick += 1;
                broadcast_counter = broadcast_counter.wrapping_add(1);

                if broadcast_counter >= 3 && state_tx.receiver_count() > 0 {
                    broadcast_counter = 0;
                    let _ = state_tx.send(MatchStateSync { r#type: "state", tick, players: runtime.player_snapshots() });
                }
            }
        });

        handle
    }
}
