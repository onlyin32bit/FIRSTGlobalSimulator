use std::collections::HashMap;
use rapier3d::prelude::*;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPhase { PreMatch, Autonomous, Teleop, Endgame, PostMatch }

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
pub struct ScoreState { pub blue_score: i32, pub red_score: i32, pub global_score: i32, pub breakdown: HashMap<String, i32> }

#[derive(Debug, Clone, Serialize)]
pub struct PlayerSnapshot {
    pub id: String,
    pub name: String,
    #[serde(rename = "teamName")]
    pub team_name: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub color: String,
}

struct PlayerBody {
    name: String,
    team_name: String,
    body: RigidBodyHandle,
    collider: ColliderHandle,
    move_x: f32,
    move_z: f32,
    sequence: u64,
    color: &'static str,
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
    players: HashMap<String, PlayerBody>,
}

impl MatchRuntime {
    pub fn new(match_id: String, game_pack_id: String, match_seed: u64) -> Self {
        Self {
            context: MatchContext { match_id, game_pack_id, game_pack_version: "1.0.0".to_string(), engine_version: "0.1.0".to_string(), match_seed, phase: MatchPhase::Teleop, clock: 0.0 },
            score_state: ScoreState::default(), rigid_body_set: RigidBodySet::new(), collider_set: ColliderSet::new(), gravity: vector![0.0, -9.81, 0.0].into(), integration_parameters: IntegrationParameters::default(), physics_pipeline: PhysicsPipeline::new(), island_manager: IslandManager::new(), broad_phase: BroadPhaseBvh::new(), narrow_phase: NarrowPhase::new(), impulse_joint_set: ImpulseJointSet::new(), multibody_joint_set: MultibodyJointSet::new(), ccd_solver: CCDSolver::new(), players: HashMap::new(),
        }
    }

    pub fn create_test_arena(&mut self) {
        let floor = RigidBodyBuilder::fixed().translation(vector![0.0, -0.25, 0.0].into()).build();
        let floor_handle = self.rigid_body_set.insert(floor);
        self.collider_set.insert_with_parent(ColliderBuilder::cuboid(8.0, 0.25, 8.0).friction(0.9).build(), floor_handle, &mut self.rigid_body_set);
        for (x, z, hx, hz) in [(0.0, -8.0, 8.25, 0.25), (0.0, 8.0, 8.25, 0.25), (-8.0, 0.0, 0.25, 8.25), (8.0, 0.0, 0.25, 8.25)] {
            let body = self.rigid_body_set.insert(RigidBodyBuilder::fixed().translation(vector![x, 0.5, z].into()).build());
            self.collider_set.insert_with_parent(ColliderBuilder::cuboid(hx, 0.75, hz).restitution(0.2).build(), body, &mut self.rigid_body_set);
        }
    }

    pub fn add_player(&mut self, user_id: String, name: String, team_name: String) {
        if self.players.contains_key(&user_id) { return; }
        let slot = self.players.len();
        let angle = slot as f32 * std::f32::consts::TAU / 8.0;
        let body = RigidBodyBuilder::dynamic().translation(vector![angle.cos() * 4.0, 0.4, angle.sin() * 4.0].into()).linear_damping(4.0).angular_damping(5.0).enabled_rotations(false, true, false).ccd_enabled(true).build();
        let body_handle = self.rigid_body_set.insert(body);
        let collider_handle = self.collider_set.insert_with_parent(ColliderBuilder::cuboid(0.38, 0.38, 0.38).density(20.0).friction(0.8).restitution(0.1).build(), body_handle, &mut self.rigid_body_set);
        let colors = ["#f97316", "#2563eb", "#16a34a", "#9333ea", "#dc2626", "#0891b2", "#ca8a04", "#db2777"];
        self.players.insert(user_id, PlayerBody { name, team_name, body: body_handle, collider: collider_handle, move_x: 0.0, move_z: 0.0, sequence: 0, color: colors[slot % colors.len()] });
    }

    pub fn remove_player(&mut self, user_id: &str) {
        if let Some(player) = self.players.remove(user_id) {
            self.collider_set.remove(player.collider, &mut self.island_manager, &mut self.rigid_body_set, true);
            self.rigid_body_set.remove(player.body, &mut self.island_manager, &mut self.collider_set, &mut self.impulse_joint_set, &mut self.multibody_joint_set, true);
        }
    }

    pub fn set_player_input(&mut self, user_id: &str, move_x: f32, move_z: f32, sequence: u64) {
        if let Some(player) = self.players.get_mut(user_id) {
            if sequence >= player.sequence { player.sequence = sequence; player.move_x = move_x.clamp(-1.0, 1.0); player.move_z = move_z.clamp(-1.0, 1.0); }
        }
    }

    pub fn apply_player_drive(&mut self) {
        for player in self.players.values() {
            if let Some(body) = self.rigid_body_set.get_mut(player.body) {
                let rotation = body.rotation();
                let forward_x = -2.0 * (rotation.x * rotation.z + rotation.w * rotation.y);
                let forward_z = -1.0 + 2.0 * (rotation.x * rotation.x + rotation.y * rotation.y);
                let desired_speed = player.move_z * 7.5;
                let desired = vector![forward_x * desired_speed, 0.0, forward_z * desired_speed];
                let velocity = body.linvel();
                let impulse = vector![(desired.x - velocity.x) * 0.6, 0.0, (desired.z - velocity.z) * 0.6];
                body.apply_impulse(impulse.into(), true);
                // Use an explicit yaw velocity for responsive arcade-style steering.
                // Rapier still owns all translation, collision, and contact resolution.
                body.set_angvel(vector![0.0, -player.move_x * 3.5, 0.0].into(), true);
            }
        }
    }

    pub fn player_snapshots(&self) -> Vec<PlayerSnapshot> {
        self.players.iter().filter_map(|(id, player)| self.rigid_body_set.get(player.body).map(|body| {
            let p = body.translation(); let r = body.rotation();
            let yaw = 2.0 * (r.w * r.y + r.x * r.z).atan2(1.0 - 2.0 * (r.y * r.y + r.z * r.z));
            PlayerSnapshot { id: id.clone(), name: player.name.clone(), team_name: player.team_name.clone(), x: p.x, y: p.y, z: p.z, yaw, color: player.color.to_string() }
        })).collect()
    }

    pub fn tick(&mut self, dt: f64) {
        self.context.clock += dt; self.integration_parameters.dt = dt as f32;
        self.physics_pipeline.step(self.gravity, &self.integration_parameters, &mut self.island_manager, &mut self.broad_phase, &mut self.narrow_phase, &mut self.rigid_body_set, &mut self.collider_set, &mut self.impulse_joint_set, &mut self.multibody_joint_set, &mut self.ccd_solver, &(), &());
    }
}
