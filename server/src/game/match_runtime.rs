use std::collections::HashMap;
use rapier3d::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPhase {
    PreMatch,
    Autonomous,
    Teleop,
    Endgame,
    PostMatch,
}

#[derive(Debug, Clone)]
pub struct MatchContext {
    pub match_id: String,
    pub game_pack_id: String,
    pub game_pack_version: String,
    pub engine_version: String,
    pub match_seed: u64,
    pub phase: MatchPhase,
    pub clock: f64,
}

#[derive(Debug, Clone, Default)]
pub struct ScoreState {
    pub blue_score: i32,
    pub red_score: i32,
    pub global_score: i32,
    pub breakdown: HashMap<String, i32>,
}

pub struct MatchRuntime {
    pub context: MatchContext,
    pub score_state: ScoreState,
    pub rigid_body_set: RigidBodySet,
    pub collider_set: ColliderSet,
    pub gravity: Vector,
    pub integration_parameters: IntegrationParameters,
    pub physics_pipeline: PhysicsPipeline,
    pub island_manager: IslandManager,
    pub broad_phase: BroadPhaseBvh,
    pub narrow_phase: NarrowPhase,
    pub impulse_joint_set: ImpulseJointSet,
    pub multibody_joint_set: MultibodyJointSet,
    pub ccd_solver: CCDSolver,
}

impl MatchRuntime {
    pub fn new(match_id: String, game_pack_id: String, match_seed: u64) -> Self {
        Self {
            context: MatchContext {
                match_id,
                game_pack_id,
                game_pack_version: "1.0.0".to_string(),
                engine_version: "0.1.0".to_string(),
                match_seed,
                phase: MatchPhase::PreMatch,
                clock: 0.0,
            },
            score_state: ScoreState::default(),
            rigid_body_set: RigidBodySet::new(),
            collider_set: ColliderSet::new(),
            gravity: vector![0.0, -9.81, 0.0].into(),
            integration_parameters: IntegrationParameters::default(),
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: BroadPhaseBvh::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joint_set: ImpulseJointSet::new(),
            multibody_joint_set: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
        }
    }

    pub fn tick(&mut self, dt: f64) {
        self.context.clock += dt;
        
        self.integration_parameters.dt = dt as f32;

        self.physics_pipeline.step(
            self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.ccd_solver,
            &(),
            &(),
        );
    }
}
