use super::error::GameError;
use super::rhai_engine::{RhaiEngine, RuleScriptMetadata};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::{error, info, warn};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GamePackManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(rename = "engineVersion")]
    pub engine_version: String,
    #[serde(default)]
    pub field: serde_json::Value,
    #[serde(default)]
    pub objects: Vec<serde_json::Value>,
    #[serde(default)]
    pub phases: Vec<serde_json::Value>,
    #[serde(default)]
    pub scripts: std::collections::BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GamePackMetadata {
    pub manifest: GamePackManifest,
    pub scripts: Vec<RuleScriptMetadata>,
    pub arena: ArenaConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ArenaConfig {
    pub object_id: String,
    pub object_count: usize,
    pub object_radius: f32,
    pub spawn_radius: f32,
    pub spawn_height: f32,
    pub restitution: f32,
    pub gravity_scale: f32,
    pub color: String,
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
            warn!(
                "Engine version {} is NOT compatible with Pack requirement {}",
                self.engine_version, manifest.engine_version
            );
            return Err(GameError::IncompatibleEngineVersion {
                engine: self.engine_version.to_string(),
                pack: manifest.engine_version.clone(),
            });
        }

        info!(
            "Successfully loaded compatible Game Pack: {} v{}",
            manifest.name, manifest.version
        );
        Ok(manifest)
    }

    pub fn load_pack(&self, manifest_path: &str) -> Result<GamePackMetadata, GameError> {
        let manifest = self.load_manifest(manifest_path)?;
        let root = std::path::Path::new(manifest_path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let engine = RhaiEngine::new();
        let mut scripts = Vec::with_capacity(manifest.scripts.len());
        let mut arena = None;
        for script_path in manifest.scripts.values() {
            let path = root.join(script_path);
            let path_string = path.to_string_lossy().to_string();
            let metadata = engine
                .inspect_script(&path_string)
                .map_err(GameError::ScriptCompilationError)?;
            info!(
                "Loaded Rhai rule script {} ({} functions)",
                path_string,
                metadata.functions.len()
            );
            if script_path.ends_with("arena.rhai") {
                arena = Some(
                    engine
                        .load_arena_config(&path_string)
                        .map_err(GameError::ScriptCompilationError)?,
                );
            }
            scripts.push(metadata);
        }
        let arena = arena.ok_or_else(|| {
            GameError::ScriptCompilationError("The pack must define an arena.rhai script".into())
        })?;
        Ok(GamePackMetadata {
            manifest,
            scripts,
            arena,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::PackLoader;

    #[test]
    fn loads_manifest_and_all_rhai_rules() {
        let loader = PackLoader::new("0.1.0");
        let metadata = loader
            .load_pack("../pkgs/games/fgc-2026/manifest.json")
            .unwrap();
        assert_eq!(metadata.manifest.id, "fgc-2026");
        assert_eq!(metadata.scripts.len(), 4);
        assert_eq!(metadata.arena.object_count, 24);
        assert!(
            metadata
                .scripts
                .iter()
                .any(|script| script.path.ends_with("scoring.rhai"))
        );
        assert!(
            metadata
                .scripts
                .iter()
                .flat_map(|script| script.functions.iter())
                .any(|function| function.name == "validate_build")
        );
    }
}
