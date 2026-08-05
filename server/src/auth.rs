use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TicketClaims {
    pub sub: String,          // User ID
    pub match_id: String,     // Match ID
    pub team_name: String,    // Team Name
    pub display_name: String, // Player-facing name
    pub robot_data: String,   // JSON string of their robot build
    #[serde(default)]
    pub slot_id: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub alliance: Option<String>,
    pub exp: usize,
}
