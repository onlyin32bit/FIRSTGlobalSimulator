import re

with open('server/src/game/sphere_runtime.rs', 'r') as f:
    text = f.read()

# 1. Add sync_rollers to SphereRuntime
sync_rollers_code = """
    fn sync_rollers(&mut self, colliders: &[FieldCollider]) {
        for player in self.players.values_mut() {
            for collider in colliders {
                if let Some(actuator) = &collider.actuator {
                    if !player.rollers.contains_key(&collider.id) {
                        player.rollers.insert(
                            collider.id.clone(),
                            RollerState {
                                config: actuator.clone(),
                                angular_velocity: 0.0,
                                angle: 0.0,
                                radius: collider.half_extents[1].max(collider.half_extents[2]),
                                inertia: actuator.mass_kg * (collider.half_extents[1].max(collider.half_extents[2])) * (collider.half_extents[1].max(collider.half_extents[2])) * 0.5,
                            },
                        );
                    }
                }
            }
        }
    }
"""
text = text.replace("pub fn set_robot_colliders(&mut self, colliders: &[FieldCollider]) {", sync_rollers_code + "\n    pub fn set_robot_colliders(&mut self, colliders: &[FieldCollider]) {")

# 2. Update set_robot_colliders to call sync_rollers
text = text.replace("    pub fn set_robot_colliders(&mut self, colliders: &[FieldCollider]) {\n        self.robot_colliders = colliders.to_vec();\n    }", "    pub fn set_robot_colliders(&mut self, colliders: &[FieldCollider]) {\n        self.robot_colliders = colliders.to_vec();\n        self.sync_rollers(colliders);\n    }")

# 3. Update add_player to call sync_rollers
text = text.replace("self.players.insert(\n            user_id,\n            PlayerBody {", "self.players.insert(\n            user_id.clone(),\n            PlayerBody {")
# we need to append self.sync_rollers(&self.robot_colliders.clone()) inside add_player after insert
# wait, better to just call it at the end of add_player
text = re.sub(r'(pub fn add_player.*?self\.players\.insert.*?\}\);\n    \})', r'\1\n        let colls = self.robot_colliders.clone();\n        self.sync_rollers(&colls);', text, flags=re.DOTALL)

# 4. Inject step_mechanics logic
step_mechanics_code = """
    fn step_mechanics(&mut self, arena: &ArenaConfig, dt: f32) {
        for player in self.players.values_mut() {
            let robot_config = effective_robot(&arena.robot, &player.mech);
            for (_id, roller) in &mut player.rollers {
                let channel_power = match roller.config.input_channel.as_str() {
                    "outtake" => -player.outtake_power,
                    "climb" => player.climb_power,
                    _ => player.intake_power,
                };
                let speed_scale = match roller.config.input_channel.as_str() {
                    "outtake" => robot_config.outtake_velocity_mps,
                    "climb" => 3.0,
                    _ => robot_config.intake_surface_speed_mps,
                };
                let target_v = channel_power * speed_scale;
                let target_w = target_v / roller.radius.max(0.001);
                let speed_err = target_w - roller.angular_velocity;
                let motor_torque = (speed_err * 50.0 * roller.inertia)
                    .clamp(-roller.config.max_torque, roller.config.max_torque);
                roller.angular_velocity += (motor_torque / roller.inertia) * dt;
                roller.angle = wrap_angle(roller.angle + roller.angular_velocity * dt);
            }
        }
    }
"""
text = re.sub(r'fn step_mechanics\(&mut self, _arena: &ArenaConfig, _dt: f32\) \{\s*\}', step_mechanics_code.strip(), text)

with open('server/src/game/sphere_runtime.rs', 'w') as f:
    f.write(text)
