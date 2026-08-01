use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug, Serialize)]
#[serde(tag = "code", content = "message")]
pub enum GameError {
    #[error("Game pack manifest not found")]
    ManifestNotFound,
    #[error("Failed to parse game pack manifest: {0}")]
    ManifestParseError(String),
    #[error("Engine version {engine} is incompatible with pack requirement {pack}")]
    IncompatibleEngineVersion {
        engine: String,
        pack: String,
    },
    #[error("Script failed to compile: {0}")]
    ScriptCompilationError(String),
    #[error("Script failed to execute: {0}")]
    ScriptExecutionError(String),
}

// Wrapper for API responses in Rust
#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<GameError>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }
    pub fn error(error: GameError) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
        }
    }
}
