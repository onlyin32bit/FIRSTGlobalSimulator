with open('server/src/game/sphere_runtime.rs', 'r') as f:
    lines = f.readlines()

new_step = '''
    fn step_mechanics(&mut self, arena: &ArenaConfig, dt: f32) {
        for player in self.players.values_mut() {
            let robot_config = effective_robot(&arena.robot, &player.mech);
            for (_id, roller) in &mut player.rollers {
                let channel_power = match roller.config.input_channel.as_str() {
                    "outtake" => -player.outtake_power,
                    "climb" => player.climb_power,
                    _ => player.intake_power,
                };
                let speed_scale = roller.config.target_surface_speed_mps;
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
'''

start = -1
end = -1
for i, line in enumerate(lines):
    if line.startswith('    fn step_mechanics(&mut self, arena: &ArenaConfig, dt: f32) {'):
        start = i
    if start != -1 and line.startswith('    pub fn tick(&mut self, dt: f64) {'):
        end = i
        break

if start != -1 and end != -1:
    lines = lines[:start] + [new_step, '\n'] + lines[end:]
    with open('server/src/game/sphere_runtime.rs', 'w') as f:
        f.writelines(lines)
    print('Replaced step_mechanics')
else:
    print('Could not find step_mechanics')
