use rhai::{AST, Dynamic, Engine, Map, Scope};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
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

#[derive(Debug, Clone)]
pub struct RuleOutcome {
    pub kind: &'static str,
    pub team: String,
    pub category: String,
    pub points: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct RobotInput {
    pub move_x: f32,
    pub move_z: f32,
    pub intake_power: f32,
    pub outtake_power: f32,
}

pub struct RhaiEngine {
    engine: Engine,
    scoring_ast: Option<AST>,
    robot_input_ast: Option<AST>,
    scripts: HashMap<String, AST>,
    outcomes: Arc<Mutex<Vec<RuleOutcome>>>,
}

impl Default for RhaiEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RhaiEngine {
    pub fn new() -> Self {
        let mut engine = Engine::new();
        let outcomes = Arc::new(Mutex::new(Vec::<RuleOutcome>::new()));
        let scoring_outcomes = outcomes.clone();

        engine.register_fn(
            "add_score",
            move |team: &str, category: &str, points: i64| {
                info!(
                    "Rhai added score! Team: {}, Category: {}, Points: {}",
                    team, category, points
                );
                if let Ok(mut events) = scoring_outcomes.lock() {
                    events.push(RuleOutcome {
                        kind: "score",
                        team: team.to_string(),
                        category: category.to_string(),
                        points,
                    });
                }
            },
        );

        Self {
            engine,
            scoring_ast: None,
            robot_input_ast: None,
            scripts: HashMap::new(),
            outcomes,
        }
    }

    pub fn load_source(&mut self, path: &str, source: &str) -> bool {
        match self.compile_script(source) {
            Ok(ast) => {
                if path.ends_with("scoring.rhai") {
                    self.scoring_ast = Some(ast.clone());
                }
                if ast
                    .iter_functions()
                    .any(|function| function.name == "robot_input" && function.params.len() == 1)
                {
                    self.robot_input_ast = Some(ast.clone());
                }
                self.scripts.insert(path.to_string(), ast);
                info!("Successfully loaded and compiled API rule source: {}", path);
                true
            }
            Err(error) => {
                error!("Failed to compile API rule source {}: {}", path, error);
                false
            }
        }
    }

    pub fn loaded_script_count(&self) -> usize {
        self.scripts.len()
    }

    /// Apply the robot package's authored input contract before simulation.
    /// A failed or absent hook is fail-open so a malformed optional behavior
    /// script cannot strand a connected driver.
    pub fn process_robot_input(&self, input: RobotInput) -> RobotInput {
        let Some(ast) = &self.robot_input_ast else {
            return input;
        };
        let mut scope = Scope::new();
        let mut authored = Map::new();
        authored.insert("move_x".into(), Dynamic::from_float(input.move_x as f64));
        authored.insert("move_z".into(), Dynamic::from_float(input.move_z as f64));
        authored.insert(
            "intake_power".into(),
            Dynamic::from_float(input.intake_power as f64),
        );
        authored.insert(
            "outtake_power".into(),
            Dynamic::from_float(input.outtake_power as f64),
        );
        let result: Result<Map, _> =
            self.engine.call_fn(&mut scope, ast, "robot_input", (authored,));
        let Ok(result) = result else {
            error!("Rhai robot_input hook failed; using raw driver input");
            return input;
        };
        let number = |name: &str, fallback: f32| {
            result
                .get(name)
                .and_then(|value| value.as_float().ok())
                .map(|value| value as f32)
                .unwrap_or(fallback)
        };
        RobotInput {
            move_x: number("move_x", input.move_x),
            move_z: number("move_z", input.move_z),
            intake_power: number("intake_power", input.intake_power),
            outtake_power: number("outtake_power", input.outtake_power),
        }
    }

    /// Execute the authored semantic hook, if the scoring script defines it.
    /// The simulator intentionally passes a stable entity id here; richer
    /// entity maps can be added without changing the pack hook signature.
    pub fn on_trigger_enter(&self, trigger_id: &str, entity_id: &str) -> Vec<RuleOutcome> {
        let Some(ast) = &self.scoring_ast else {
            return Vec::new();
        };
        if !ast
            .iter_functions()
            .any(|function| function.name == "on_trigger_enter" && function.params.len() == 2)
        {
            return Vec::new();
        }
        let mut scope = Scope::new();
        if let Err(error) = self.engine.call_fn::<()>(
            &mut scope,
            ast,
            "on_trigger_enter",
            (trigger_id.to_string(), entity_id.to_string()),
        ) {
            error!(%error, %trigger_id, %entity_id, "Rhai trigger hook failed");
        }
        self.outcomes
            .lock()
            .map(|mut outcomes| std::mem::take(&mut *outcomes))
            .unwrap_or_default()
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

    pub fn load_arena_config_source(
        &self,
        source: &str,
    ) -> Result<crate::game::pack_loader::ArenaConfig, String> {
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
            spawn_offset_y_m: number_value("spawn_offset_y_m")? as f32,
            spawn_release_seconds: number_value("spawn_release_seconds")? as f32,
            spawn_fountain_vertical_speed_mps: number_value("spawn_fountain_vertical_speed_mps")?
                as f32,
            spawn_fountain_forward_speed_mps: number_value("spawn_fountain_forward_speed_mps")?
                as f32,
            spawn_fountain_spread_mps: number_value("spawn_fountain_spread_mps")? as f32,
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
                storage_capacity: nested_number(&robot, "robot", "storage_capacity")?.max(0.0)
                    as usize,
                intake_rate_bps: nested_number(&robot, "robot", "intake_rate_bps")?,
                outtake_rate_bps: nested_number(&robot, "robot", "outtake_rate_bps")?,
                outtake_velocity_mps: nested_number(&robot, "robot", "outtake_velocity_mps")?,
                outtake_angle_deg: nested_number(&robot, "robot", "outtake_angle_deg")?,
                flywheel_width_m: nested_number(&robot, "robot", "flywheel_width_m")?,
                outtake_forward_offset_m: nested_number(&robot, "robot", "outtake_forward_offset_m")?,
                outtake_height_m: nested_number(&robot, "robot", "outtake_height_m")?,
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
    use super::{RhaiEngine, RobotInput};

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

    #[test]
    fn executes_authored_trigger_hook_and_captures_score() {
        let mut engine = RhaiEngine::new();
        assert!(engine.load_source(
            "rules/scoring.rhai",
            r#"fn on_trigger_enter(trigger_id, entity_id) { add_score("blue", "SU", 1); }"#
        ));
        let outcomes = engine.on_trigger_enter("blueSUscore", "ball:42");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].team, "blue");
        assert_eq!(outcomes[0].category, "SU");
        assert_eq!(outcomes[0].points, 1);
    }

    #[test]
    fn executes_authored_robot_input_hook() {
        let mut engine = RhaiEngine::new();
        assert!(engine.load_source(
            "robots/StarterBot/robot.rhai",
            "fn robot_input(input) { #{ move_x: input.move_x, move_z: input.move_z, intake_power: 0.0, outtake_power: input.outtake_power } }"
        ));
        let input = engine.process_robot_input(RobotInput {
            move_x: 0.2,
            move_z: -0.4,
            intake_power: 1.0,
            outtake_power: 0.5,
        });
        assert_eq!(input.intake_power, 0.0);
        assert_eq!(input.outtake_power, 0.5);
    }
}
