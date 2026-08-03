use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast, mpsc};
use tracing::info;

use super::match_runtime::{FieldObjectSnapshot, MatchRuntime, PlayerSnapshot};
use super::pack_loader::GamePackMetadata;
use super::rhai_engine::RhaiEngine;

pub const TEST_MATCH_ID: &str = "test-match";

pub struct MatchRegistry {
    matches: RwLock<HashMap<String, MatchHandle>>,
    pack: Arc<GamePackMetadata>,
}

#[derive(Clone)]
pub struct MatchHandle {
    pub input_tx: mpsc::Sender<MatchInput>,
    pub state_tx: broadcast::Sender<MatchStateSync>,
}

#[derive(Debug, Clone)]
pub enum MatchInput {
    PlayerJoin {
        user_id: String,
        name: String,
        team_name: String,
    },
    PlayerLeave {
        user_id: String,
    },
    PlayerInput {
        user_id: String,
        move_x: f32,
        move_z: f32,
        sequence: u64,
    },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MatchStateSync {
    pub r#type: &'static str,
    pub tick: u64,
    #[serde(rename = "gamePackId")]
    pub game_pack_id: String,
    #[serde(rename = "gamePackVersion")]
    pub game_pack_version: String,
    pub players: Vec<PlayerSnapshot>,
    pub objects: Vec<FieldObjectSnapshot>,
}

impl MatchRegistry {
    pub fn new(pack: Arc<GamePackMetadata>) -> Self {
        Self {
            matches: RwLock::new(HashMap::new()),
            pack,
        }
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
        let handle = MatchHandle {
            input_tx,
            state_tx: state_tx.clone(),
        };
        matches.insert(match_id.to_string(), handle.clone());
        let match_id = match_id.to_string();
        let pack = self.pack.clone();

        let mut rules = RhaiEngine::new();
        for script in &pack.scripts {
            if !rules.load_script(&script.path) {
                tracing::error!(path = %script.path, "Unable to load a validated rule script into match runtime");
                return handle;
            }
        }
        let loaded_script_count = rules.loaded_script_count();
        drop(rules);
        info!(match_id = %match_id, pack = %pack.manifest.id, version = %pack.manifest.version, scripts = loaded_script_count, "Loaded game pack into match runtime");

        tokio::spawn(async move {
            let mut runtime = MatchRuntime::new(match_id.clone(), pack.manifest.id.clone(), 0);
            runtime.context.game_pack_version = pack.manifest.version.clone();
            runtime.create_test_arena(&pack.arena);
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / 60.0));
            let mut tick = 0_u64;
            let mut broadcast_counter = 0_u8;

            loop {
                interval.tick().await;
                while let Ok(input) = input_rx.try_recv() {
                    match input {
                        MatchInput::PlayerJoin {
                            user_id,
                            name,
                            team_name,
                        } => runtime.add_player(user_id, name, team_name),
                        MatchInput::PlayerLeave { user_id } => runtime.remove_player(&user_id),
                        MatchInput::PlayerInput {
                            user_id,
                            move_x,
                            move_z,
                            sequence,
                        } => runtime.set_player_input(&user_id, move_x, move_z, sequence),
                    }
                }

                runtime.apply_player_drive();
                runtime.tick(1.0 / 60.0);
                tick += 1;
                broadcast_counter = broadcast_counter.wrapping_add(1);

                if broadcast_counter >= 3 && state_tx.receiver_count() > 0 {
                    broadcast_counter = 0;
                    let _ = state_tx.send(MatchStateSync {
                        r#type: "state",
                        tick,
                        game_pack_id: runtime.context.game_pack_id.clone(),
                        game_pack_version: runtime.context.game_pack_version.clone(),
                        players: runtime.player_snapshots(),
                        objects: runtime.field_object_snapshots(),
                    });
                }
            }
        });

        handle
    }
}
