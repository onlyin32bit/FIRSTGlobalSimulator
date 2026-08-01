use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, broadcast, RwLock};
use uuid::Uuid;

use super::match_runtime::MatchRuntime;

pub struct MatchRegistry {
    matches: RwLock<HashMap<String, MatchHandle>>,
}

#[derive(Clone)]
pub struct MatchHandle {
    pub match_id: String,
    // Channel to send inputs to the match loop
    pub input_tx: mpsc::Sender<MatchInput>,
    // Channel to receive physics state updates
    pub state_rx: broadcast::Sender<MatchStateSync>,
}

#[derive(Debug, Clone)]
pub enum MatchInput {
    PlayerJoin { user_id: String, robot_data: String },
    PlayerLeave { user_id: String },
    PlayerInput { user_id: String, gamepad_data: Vec<u8> },
}

#[derive(Debug, Clone)]
pub struct MatchStateSync {
    pub tick: u64,
    // Simplified: in a real app, this would be a serialized list of body transforms
    pub transforms: Vec<u8>, 
}

impl MatchRegistry {
    pub fn new() -> Self {
        Self {
            matches: RwLock::new(HashMap::new()),
        }
    }

    pub async fn get_or_create_match(&self, match_id: &str) -> MatchHandle {
        let mut matches = self.matches.write().await;
        if let Some(handle) = matches.get(match_id) {
            return handle.clone();
        }

        // Create new match
        let (input_tx, mut input_rx) = mpsc::channel(100);
        let (state_tx, _) = broadcast::channel(100);
        let state_tx_clone = state_tx.clone();
        
        let match_id_owned = match_id.to_string();

        let handle = MatchHandle {
            match_id: match_id.to_string(),
            input_tx,
            state_rx: state_tx,
        };

        matches.insert(match_id.to_string(), handle.clone());

        // Spawn match loop
        tokio::spawn(async move {
            let mut runtime = MatchRuntime::new(match_id_owned.clone(), "fgc-2026".into(), 0);
            let mut interval = tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / 60.0));
            let mut tick = 0;

            loop {
                interval.tick().await;

                // Process inputs
                while let Ok(input) = input_rx.try_recv() {
                    match input {
                        MatchInput::PlayerJoin { user_id, robot_data } => {
                            tracing::info!("Match {}: Player {} joined with robot {}", match_id_owned, user_id, robot_data.len());
                            // TODO: parse robot data and add rigidbodies
                        }
                        MatchInput::PlayerLeave { user_id } => {
                            tracing::info!("Match {}: Player {} left", match_id_owned, user_id);
                        }
                        MatchInput::PlayerInput { user_id, gamepad_data } => {
                            // Apply force/torque to robot based on input
                        }
                    }
                }

                // Tick physics
                runtime.tick(1.0 / 60.0);
                tick += 1;

                // Broadcast state
                if state_tx_clone.receiver_count() > 0 {
                    let _ = state_tx_clone.send(MatchStateSync {
                        tick,
                        transforms: vec![], // TODO: Serialize rigid body positions
                    });
                }
            }
        });

        handle
    }
}
