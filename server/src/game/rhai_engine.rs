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

impl Default for RhaiEngine {
    fn default() -> Self {
        Self::new()
    }
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
        let bool_value = |key: &str| {
            value(key)?
                .as_bool()
                .map_err(|_| format!("arena_config.{key} must be a boolean"))
        };
        let map_value = |key: &str| {
            value(key)?
                .clone()
                .try_cast::<Map>()
                .ok_or_else(|| format!("arena_config.{key} must be a map"))
        };
        let nested_string = |map: &Map, section: &str, key: &str| {
            map.get(key)
                .ok_or_else(|| format!("arena_config.{section} is missing {key}"))?
                .clone()
                .into_string()
                .map_err(|_| format!("arena_config.{section}.{key} must be a string"))
        };
        let nested_number = |map: &Map, section: &str, key: &str| {
            map.get(key)
                .ok_or_else(|| format!("arena_config.{section} is missing {key}"))?
                .as_float()
                .map(|number| number as f32)
                .map_err(|_| format!("arena_config.{section}.{key} must be a number"))
        };
        let nested_bool = |map: &Map, section: &str, key: &str| {
            map.get(key)
                .ok_or_else(|| format!("arena_config.{section} is missing {key}"))?
                .as_bool()
                .map_err(|_| format!("arena_config.{section}.{key} must be a boolean"))
        };
        let nested_map = |map: &Map, section: &str, key: &str| {
            map.get(key)
                .ok_or_else(|| format!("arena_config.{section} is missing {key}"))?
                .clone()
                .try_cast::<Map>()
                .ok_or_else(|| format!("arena_config.{section}.{key} must be a map"))
        };
        let restitution_curve =
            |map: &Map,
             section: &str|
             -> Result<crate::game::pack_loader::RestitutionCurveConfig, String> {
                Ok(crate::game::pack_loader::RestitutionCurveConfig {
                    low_speed: nested_number(map, section, "low_speed")?,
                    high_speed: nested_number(map, section, "high_speed")?,
                    transition_speed_mps: nested_number(map, section, "transition_speed_mps")?,
                    exponent: nested_number(map, section, "exponent")?,
                })
            };
        let ball = map_value("ball")?;
        let floor = map_value("floor")?;
        let robot = map_value("robot")?;
        let solver = map_value("solver")?;
        let goal_wall = map_value("goal_wall")?;
        let metal_wall = map_value("metal_wall")?;
        let ramp = map_value("ramp")?;
        let ball_restitution = nested_map(&ball, "ball", "restitution_curve")?;
        let floor_restitution = nested_map(&floor, "floor", "restitution_curve")?;
        let robot_restitution = nested_map(&robot, "robot", "restitution_curve")?;
        let intake_restitution = nested_map(&robot, "robot", "intake_restitution_curve")?;
        let goal_restitution = nested_map(&goal_wall, "goal_wall", "restitution_curve")?;
        let metal_restitution = nested_map(&metal_wall, "metal_wall", "restitution_curve")?;
        let ramp_restitution = nested_map(&ramp, "ramp", "restitution_curve")?;
        let arena = crate::game::pack_loader::ArenaConfig {
            physics_backend: string_value("physics_backend")?,
            solver: crate::game::pack_loader::SolverConfig {
                position_iterations: nested_number(&solver, "solver", "position_iterations")?
                    as usize,
                velocity_iterations: nested_number(&solver, "solver", "velocity_iterations")?
                    as usize,
                contact_compliance: nested_number(&solver, "solver", "contact_compliance")?,
                max_depenetration_speed_mps: nested_number(
                    &solver,
                    "solver",
                    "max_depenetration_speed_mps",
                )?,
                max_ball_speed_mps: nested_number(&solver, "solver", "max_ball_speed_mps")?,
                max_ball_angular_speed_radps: nested_number(
                    &solver,
                    "solver",
                    "max_ball_angular_speed_radps",
                )?,
                sleep_linear_threshold_mps: nested_number(
                    &solver,
                    "solver",
                    "sleep_linear_threshold_mps",
                )?,
                sleep_angular_threshold_radps: nested_number(
                    &solver,
                    "solver",
                    "sleep_angular_threshold_radps",
                )?,
                sleep_after_seconds: nested_number(&solver, "solver", "sleep_after_seconds")?,
                restitution_velocity_threshold_mps: nested_number(
                    &solver,
                    "solver",
                    "restitution_velocity_threshold_mps",
                )?,
            },
            object_id: string_value("object_id")?,
            object_count: integer_value("object_count")?
                .try_into()
                .map_err(|_| "arena_config.object_count must be positive".to_string())?,
            spawn_radius: number_value("spawn_radius")? as f32,
            spawn_height: number_value("spawn_height")? as f32,
            gravity_scale: number_value("gravity_scale")? as f32,
            ball_to_ball_collisions: bool_value("ball_to_ball_collisions")?,
            color: string_value("color")?,
            ball: crate::game::pack_loader::BallPhysicsConfig {
                material: nested_string(&ball, "ball", "material")?,
                diameter_m: nested_number(&ball, "ball", "diameter_m")?,
                diameter_tolerance_m: nested_number(&ball, "ball", "diameter_tolerance_m")?,
                mass_kg: nested_number(&ball, "ball", "mass_kg")?,
                friction: nested_number(&ball, "ball", "friction")?,
                restitution: nested_number(&ball, "ball", "restitution")?,
                linear_damping: nested_number(&ball, "ball", "linear_damping")?,
                angular_damping: nested_number(&ball, "ball", "angular_damping")?,
                rolling_resistance_mps2: nested_number(&ball, "ball", "rolling_resistance_mps2")?,
                soft_ccd_prediction_m: nested_number(&ball, "ball", "soft_ccd_prediction_m")?,
                inertia_factor: nested_number(&ball, "ball", "inertia_factor")?,
                drag_coefficient: nested_number(&ball, "ball", "drag_coefficient")?,
                air_density_kg_m3: nested_number(&ball, "ball", "air_density_kg_m3")?,
                ball_friction: nested_number(&ball, "ball", "ball_friction")?,
                restitution_curve: restitution_curve(&ball_restitution, "ball.restitution_curve")?,
            },
            floor: crate::game::pack_loader::FloorPhysicsConfig {
                material: nested_string(&floor, "floor", "material")?,
                friction: nested_number(&floor, "floor", "friction")?,
                restitution: nested_number(&floor, "floor", "restitution")?,
                static_friction: nested_number(&floor, "floor", "static_friction")?,
                dynamic_friction: nested_number(&floor, "floor", "dynamic_friction")?,
                rolling_resistance_mps2: nested_number(&floor, "floor", "rolling_resistance_mps2")?,
                restitution_curve: restitution_curve(
                    &floor_restitution,
                    "floor.restitution_curve",
                )?,
            },
            robot: crate::game::pack_loader::RobotPhysicsConfig {
                mass_kg: nested_number(&robot, "robot", "mass_kg")?,
                width_m: nested_number(&robot, "robot", "width_m")?,
                height_m: nested_number(&robot, "robot", "height_m")?,
                length_m: nested_number(&robot, "robot", "length_m")?,
                track_width_m: nested_number(&robot, "robot", "track_width_m")?,
                traction_friction: nested_number(&robot, "robot", "traction_friction")?,
                surface_friction: nested_number(&robot, "robot", "surface_friction")?,
                restitution: nested_number(&robot, "robot", "restitution")?,
                rolling_resistance: nested_number(&robot, "robot", "rolling_resistance")?,
                max_speed_mps: nested_number(&robot, "robot", "max_speed_mps")?,
                max_acceleration_mps2: nested_number(&robot, "robot", "max_acceleration_mps2")?,
                max_deceleration_mps2: nested_number(&robot, "robot", "max_deceleration_mps2")?,
                max_drive_force_n: nested_number(&robot, "robot", "max_drive_force_n")?,
                max_drive_power_w: nested_number(&robot, "robot", "max_drive_power_w")?,
                max_brake_force_n: nested_number(&robot, "robot", "max_brake_force_n")?,
                max_turn_rate_radps: nested_number(&robot, "robot", "max_turn_rate_radps")?,
                max_angular_acceleration_radps2: nested_number(
                    &robot,
                    "robot",
                    "max_angular_acceleration_radps2",
                )?,
                lateral_grip_mps2: nested_number(&robot, "robot", "lateral_grip_mps2")?,
                restitution_curve: restitution_curve(
                    &robot_restitution,
                    "robot.restitution_curve",
                )?,
                intake_enabled: nested_bool(&robot, "robot", "intake_enabled")?,
                intake_width_m: nested_number(&robot, "robot", "intake_width_m")?,
                intake_radius_m: nested_number(&robot, "robot", "intake_radius_m")?,
                intake_forward_offset_m: nested_number(&robot, "robot", "intake_forward_offset_m")?,
                intake_center_height_m: nested_number(&robot, "robot", "intake_center_height_m")?,
                intake_surface_speed_mps: nested_number(
                    &robot,
                    "robot",
                    "intake_surface_speed_mps",
                )?,
                intake_friction: nested_number(&robot, "robot", "intake_friction")?,
                intake_normal_force_n: nested_number(&robot, "robot", "intake_normal_force_n")?,
                intake_restitution_curve: restitution_curve(
                    &intake_restitution,
                    "robot.intake_restitution_curve",
                )?,
            },
            goal_wall: crate::game::pack_loader::SurfacePhysicsConfig {
                material: nested_string(&goal_wall, "goal_wall", "material")?,
                static_friction: nested_number(&goal_wall, "goal_wall", "static_friction")?,
                dynamic_friction: nested_number(&goal_wall, "goal_wall", "dynamic_friction")?,
                restitution_curve: restitution_curve(
                    &goal_restitution,
                    "goal_wall.restitution_curve",
                )?,
            },
            metal_wall: crate::game::pack_loader::SurfacePhysicsConfig {
                material: nested_string(&metal_wall, "metal_wall", "material")?,
                static_friction: nested_number(&metal_wall, "metal_wall", "static_friction")?,
                dynamic_friction: nested_number(&metal_wall, "metal_wall", "dynamic_friction")?,
                restitution_curve: restitution_curve(
                    &metal_restitution,
                    "metal_wall.restitution_curve",
                )?,
            },
            ramp: crate::game::pack_loader::RampPhysicsConfig {
                enabled: nested_bool(&ramp, "ramp", "enabled")?,
                center_x: nested_number(&ramp, "ramp", "center_x")?,
                start_z: nested_number(&ramp, "ramp", "start_z")?,
                width_m: nested_number(&ramp, "ramp", "width_m")?,
                length_m: nested_number(&ramp, "ramp", "length_m")?,
                angle_deg: nested_number(&ramp, "ramp", "angle_deg")?,
                surface: crate::game::pack_loader::SurfacePhysicsConfig {
                    material: nested_string(&ramp, "ramp", "material")?,
                    static_friction: nested_number(&ramp, "ramp", "static_friction")?,
                    dynamic_friction: nested_number(&ramp, "ramp", "dynamic_friction")?,
                    restitution_curve: restitution_curve(
                        &ramp_restitution,
                        "ramp.restitution_curve",
                    )?,
                },
            },
        };
        for (name, value) in [
            ("ball.diameter_m", arena.ball.diameter_m),
            ("ball.mass_kg", arena.ball.mass_kg),
            ("robot.mass_kg", arena.robot.mass_kg),
            ("robot.width_m", arena.robot.width_m),
            ("robot.height_m", arena.robot.height_m),
            ("robot.length_m", arena.robot.length_m),
            ("robot.track_width_m", arena.robot.track_width_m),
            ("robot.max_speed_mps", arena.robot.max_speed_mps),
            ("robot.max_drive_force_n", arena.robot.max_drive_force_n),
            ("robot.max_drive_power_w", arena.robot.max_drive_power_w),
            ("robot.max_brake_force_n", arena.robot.max_brake_force_n),
            (
                "solver.max_depenetration_speed_mps",
                arena.solver.max_depenetration_speed_mps,
            ),
            ("solver.max_ball_speed_mps", arena.solver.max_ball_speed_mps),
            (
                "solver.max_ball_angular_speed_radps",
                arena.solver.max_ball_angular_speed_radps,
            ),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(format!("arena_config.{name} must be positive and finite"));
            }
        }
        if !(0.0..=1.0).contains(&arena.ball.restitution)
            || !(0.0..=1.0).contains(&arena.floor.restitution)
            || !(0.0..=1.0).contains(&arena.robot.restitution)
        {
            return Err("arena_config restitution values must be between 0 and 1".into());
        }
        if !matches!(arena.physics_backend.as_str(), "rapier" | "sphere_xpbd") {
            return Err("arena_config.physics_backend must be rapier or sphere_xpbd".into());
        }
        if !(1..=12).contains(&arena.solver.position_iterations) {
            return Err("arena_config.solver.position_iterations must be between 1 and 12".into());
        }
        if !(1..=12).contains(&arena.solver.velocity_iterations) {
            return Err("arena_config.solver.velocity_iterations must be between 1 and 12".into());
        }
        if arena.ball.diameter_tolerance_m < 0.0
            || arena.ball.friction < 0.0
            || arena.ball.linear_damping < 0.0
            || arena.ball.angular_damping < 0.0
            || arena.ball.rolling_resistance_mps2 < 0.0
            || arena.ball.soft_ccd_prediction_m < 0.0
            || arena.floor.friction < 0.0
            || arena.robot.traction_friction < 0.0
            || arena.robot.surface_friction < 0.0
        {
            return Err(
                "arena_config tolerances, damping, and friction values cannot be negative".into(),
            );
        }
        let non_negative = [
            ("ball.inertia_factor", arena.ball.inertia_factor),
            ("ball.drag_coefficient", arena.ball.drag_coefficient),
            ("ball.air_density_kg_m3", arena.ball.air_density_kg_m3),
            ("ball.ball_friction", arena.ball.ball_friction),
            ("floor.static_friction", arena.floor.static_friction),
            ("floor.dynamic_friction", arena.floor.dynamic_friction),
            (
                "floor.rolling_resistance_mps2",
                arena.floor.rolling_resistance_mps2,
            ),
            ("robot.intake_friction", arena.robot.intake_friction),
            (
                "robot.intake_normal_force_n",
                arena.robot.intake_normal_force_n,
            ),
            ("goal_wall.static_friction", arena.goal_wall.static_friction),
            (
                "goal_wall.dynamic_friction",
                arena.goal_wall.dynamic_friction,
            ),
            (
                "metal_wall.static_friction",
                arena.metal_wall.static_friction,
            ),
            (
                "metal_wall.dynamic_friction",
                arena.metal_wall.dynamic_friction,
            ),
            (
                "ramp.surface.static_friction",
                arena.ramp.surface.static_friction,
            ),
            (
                "ramp.surface.dynamic_friction",
                arena.ramp.surface.dynamic_friction,
            ),
        ];
        for (name, value) in non_negative {
            if !value.is_finite() || value < 0.0 {
                return Err(format!(
                    "arena_config.{name} must be non-negative and finite"
                ));
            }
        }
        for (name, value) in [
            ("robot.intake_width_m", arena.robot.intake_width_m),
            ("robot.intake_radius_m", arena.robot.intake_radius_m),
            (
                "robot.intake_surface_speed_mps",
                arena.robot.intake_surface_speed_mps,
            ),
            ("ramp.width_m", arena.ramp.width_m),
            ("ramp.length_m", arena.ramp.length_m),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(format!("arena_config.{name} must be positive and finite"));
            }
        }
        if !arena.ramp.angle_deg.is_finite() || !(0.0..=60.0).contains(&arena.ramp.angle_deg) {
            return Err("arena_config.ramp.angle_deg must be between 0 and 60".into());
        }
        for (name, curve) in [
            ("ball.restitution_curve", &arena.ball.restitution_curve),
            ("floor.restitution_curve", &arena.floor.restitution_curve),
            ("robot.restitution_curve", &arena.robot.restitution_curve),
            (
                "robot.intake_restitution_curve",
                &arena.robot.intake_restitution_curve,
            ),
            (
                "goal_wall.restitution_curve",
                &arena.goal_wall.restitution_curve,
            ),
            (
                "metal_wall.restitution_curve",
                &arena.metal_wall.restitution_curve,
            ),
            (
                "ramp.restitution_curve",
                &arena.ramp.surface.restitution_curve,
            ),
        ] {
            if !curve.low_speed.is_finite()
                || !curve.high_speed.is_finite()
                || !(0.0..=1.0).contains(&curve.low_speed)
                || !(0.0..=1.0).contains(&curve.high_speed)
                || curve.high_speed > curve.low_speed
                || !curve.transition_speed_mps.is_finite()
                || curve.transition_speed_mps <= 0.0
                || !curve.exponent.is_finite()
                || curve.exponent <= 0.0
            {
                return Err(format!(
                    "arena_config.{name} must have 0 <= high_speed <= low_speed <= 1 and positive transition/exponent"
                ));
            }
        }
        Ok(arena)
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
