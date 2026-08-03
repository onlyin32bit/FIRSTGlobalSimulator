use rhai::{Engine, AST};
use std::fs;
use tracing::{info, error};

pub struct RhaiEngine {
    engine: Engine,
    scoring_ast: Option<AST>,
}

impl RhaiEngine {
    pub fn new() -> Self {
        let mut engine = Engine::new();
        
        engine.register_fn("add_score", |team: &str, category: &str, points: i64| {
            info!("Rhai added score! Team: {}, Category: {}, Points: {}", team, category, points);
        });

        Self {
            engine,
            scoring_ast: None,
        }
    }

    pub fn load_script(&mut self, path: &str) {
        match fs::read_to_string(path) {
            Ok(script) => {
                match self.engine.compile(&script) {
                    Ok(ast) => {
                        self.scoring_ast = Some(ast);
                        info!("Successfully loaded and compiled script: {}", path);
                    }
                    Err(e) => error!("Failed to compile script {}: {}", path, e),
                }
            }
            Err(e) => error!("Failed to read script file {}: {}", path, e),
        }
    }
}
