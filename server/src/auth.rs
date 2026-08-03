use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct TicketClaims {
    pub sub: String,          // User ID
    pub match_id: String,     // Match ID
    pub team_name: String,    // Team Name
    pub display_name: String, // Player-facing name
    pub robot_data: String,   // JSON string of their robot build
    pub exp: usize,
}

pub fn verify_ticket(token: &str, secret: &str) -> Result<TicketClaims, jsonwebtoken::errors::Error> {
    let decoding_key = DecodingKey::from_secret(secret.as_bytes());
    let validation = Validation::new(Algorithm::HS256);

    let token_data = decode::<TicketClaims>(token, &decoding_key, &validation)?;
    Ok(token_data.claims)
}
