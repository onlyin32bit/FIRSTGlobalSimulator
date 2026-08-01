use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::{info, error, warn};
use super::error::GameError;

#[derive(Serialize, Deserialize, Debug)]
pub struct GamePackManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(rename = "engineVersion")]
    pub engine_version: String,
}

pub struct PackLoader {
    engine_version: Version,
}

impl PackLoader {
    pub fn new(current_engine_version: &str) -> Self {
        Self {
            engine_version: Version::parse(current_engine_version).unwrap(),
        }
    }

    pub fn load_manifest(&self, path: &str) -> Result<GamePackManifest, GameError> {
        let content = fs::read_to_string(path).map_err(|_| {
            error!("Could not find manifest at {}", path);
            GameError::ManifestNotFound
        })?;

        let manifest: GamePackManifest = serde_json::from_str(&content).map_err(|e| {
            error!("Failed to parse manifest json: {}", e);
            GameError::ManifestParseError(e.to_string())
        })?;

        let req = VersionReq::parse(&manifest.engine_version).unwrap();
        if !req.matches(&self.engine_version) {
            warn!("Engine version {} is NOT compatible with Pack requirement {}", self.engine_version, manifest.engine_version);
            return Err(GameError::IncompatibleEngineVersion {
                engine: self.engine_version.to_string(),
                pack: manifest.engine_version.clone(),
            });
        }

        info!("Successfully loaded compatible Game Pack: {} v{}", manifest.name, manifest.version);
        Ok(manifest)
    }
}
