use rhai::{AST, Engine, Map, Scope};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use tracing::{error, info};

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleFunctionMetadata {
    pub name: String,
    pub parameters: Vec<String>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleScriptMetadata {
    pub path: String,
    pub functions: Vec<RuleFunctionMetadata>,
    pub engine_calls: Vec<String>,
}

pub struct RhaiEngine {
    engine: Engine,
    scoring_ast: Option<AST>,
    scripts: HashMap<String, AST>,
}

impl RhaiEngine {
    pub fn new() -> Self {
        let mut engine = Engine::new();

        engine.register_fn("add_score", |team: &str, category: &str, points: i64| {
            info!(
                "Rhai added score! Team: {}, Category: {}, Points: {}",
                team, category, points
            );
        });

        Self {
            engine,
            scoring_ast: None,
            scripts: HashMap::new(),
        }
    }

    pub fn load_script(&mut self, path: &str) -> bool {
        match fs::read_to_string(path) {
            Ok(script) => match self.compile_script(&script) {
                Ok(ast) => {
                    if path.ends_with("scoring.rhai") {
                        self.scoring_ast = Some(ast.clone());
                    }
                    self.scripts.insert(path.to_string(), ast);
                    info!("Successfully loaded and compiled script: {}", path);
                    true
                }
                Err(e) => {
                    error!("Failed to compile script {}: {}", path, e);
                    false
                }
            },
            Err(e) => {
                error!("Failed to read script file {}: {}", path, e);
                false
            }
        }
    }

    pub fn loaded_script_count(&self) -> usize {
        self.scripts.len()
    }

    fn compile_script(&self, source: &str) -> Result<AST, rhai::ParseError> {
        self.engine.compile(source)
    }

    pub fn inspect_source(
        &self,
        path: impl Into<String>,
        source: &str,
    ) -> Result<RuleScriptMetadata, String> {
        let ast = self
            .compile_script(source)
            .map_err(|error| error.to_string())?;
        let functions = ast
            .iter_functions()
            .map(|function| RuleFunctionMetadata {
                name: function.name.to_string(),
                parameters: function
                    .params
                    .iter()
                    .map(|parameter| parameter.to_string())
                    .collect(),
            })
            .collect();

        let mut engine_calls = source
            .split(['{', '}', ';'])
            .filter(|fragment| !fragment.trim_start().starts_with("fn "))
            .filter_map(|fragment| fragment.split_once('(').map(|(name, _)| name.trim()))
            .map(|name| name.split_whitespace().last().unwrap_or(name))
            .filter(|name| {
                !name.is_empty()
                    && !name.starts_with("//")
                    && !matches!(*name, "if" | "while" | "for")
            })
            .map(str::to_string)
            .collect::<Vec<_>>();
        engine_calls.sort();
        engine_calls.dedup();

        Ok(RuleScriptMetadata {
            path: path.into(),
            functions,
            engine_calls,
        })
    }

    pub fn inspect_script(&self, path: &str) -> Result<RuleScriptMetadata, String> {
        let source = fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
        self.inspect_source(path, &source)
    }

    pub fn load_arena_config(
        &self,
        path: &str,
    ) -> Result<crate::game::pack_loader::ArenaConfig, String> {
        let source = fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
        let ast = self
            .compile_script(&source)
            .map_err(|error| error.to_string())?;
        let mut scope = Scope::new();
        let config: Map = self
            .engine
            .call_fn(&mut scope, &ast, "arena_config", ())
            .map_err(|error| error.to_string())?;
        let value = |key: &str| {
            config
                .get(key)
                .ok_or_else(|| format!("arena_config is missing {key}"))
        };
        let string_value = |key: &str| {
            value(key)?
                .clone()
                .into_string()
                .map_err(|_| format!("arena_config.{key} must be a string"))
        };
        let number_value = |key: &str| {
            value(key)?
                .as_float()
                .map_err(|_| format!("arena_config.{key} must be a number"))
        };
        let integer_value = |key: &str| {
            value(key)?
                .as_int()
                .map_err(|_| format!("arena_config.{key} must be an integer"))
        };
        Ok(crate::game::pack_loader::ArenaConfig {
            object_id: string_value("object_id")?,
            object_count: integer_value("object_count")?
                .try_into()
                .map_err(|_| "arena_config.object_count must be positive".to_string())?,
            object_radius: number_value("object_radius")? as f32,
            spawn_radius: number_value("spawn_radius")? as f32,
            spawn_height: number_value("spawn_height")? as f32,
            restitution: number_value("restitution")? as f32,
            gravity_scale: number_value("gravity_scale")? as f32,
            color: string_value("color")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::RhaiEngine;

    #[test]
    fn inspects_functions_and_engine_calls() {
        let engine = RhaiEngine::new();
        let metadata = engine
            .inspect_source(
                "rules/example.rhai",
                "fn on_tick(state) { add_score(\"blue\", \"SU\", 1); }",
            )
            .unwrap();
        assert_eq!(metadata.functions[0].name, "on_tick");
        assert_eq!(metadata.functions[0].parameters, vec!["state"]);
        assert_eq!(metadata.engine_calls, vec!["add_score"]);
    }

    #[test]
    fn rejects_invalid_rhai() {
        let engine = RhaiEngine::new();
        assert!(
            engine
                .inspect_source("rules/broken.rhai", "fn broken( {")
                .is_err()
        );
    }
}
