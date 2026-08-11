import sys
code = r"""
    #[test]
    fn test_roller_friction_acceleration() {
        let mut arena = arena();
        arena.gravity_scale = 0.0;
        let roller_collider = FieldCollider {
            id: "Roller1".into(),
            min: [-0.5, -0.1, -0.1],
            max: [0.5, 0.1, 0.1],
            center: [0.0, 0.0, 0.0],
            half_extents: [0.5, 0.1, 0.1],
            axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            actuator: Some(ActuatorConfig {
                input_channel: "intake".into(),
                spin_axis: [1.0, 0.0, 0.0],
                max_torque: 100.0,
                mass_kg: 1.0,
                target_surface_speed_mps: 10.0,
            }),
        };
        let field = FieldDefinition {
            colliders: vec![],
            anchors: std::collections::BTreeMap::new(),
            triggers: Vec::new(),
            floor_height_m: 0.0,
            boundary: FieldBoundary {
                min: [-8.0, 0.0, -8.0],
                max: [8.0, 1.0, 8.0],
            },
        };
        let mut runtime = SphereRuntime::new("test".into(), "test".into(), 0);
        runtime.create_field_arena(&arena, &field);
        runtime.set_robot_colliders(&[roller_collider]);
        runtime.add_player("p1".into(), "Player 1".into(), "Red1".into(), None, &arena);
        if let Some(p) = runtime.players.get_mut("p1") {
            p.position = [0.0, 0.0, 0.0];
        }
        runtime.set_player_input("p1", 0.0, 0.0, 1.0, 0.0, 0.0, 0);
        
        let ball = Ball {
            position: [0.0, 0.1 + arena.ball.radius_m(), 0.0],
            velocity: [0.0, 0.0, 0.0],
            pre_solve_velocity: [0.0, 0.0, 0.0],
            angular_velocity: [0.0, 0.0, 0.0],
            quiet_ticks: 0,
            sleeping: false,
            grounded: false,
            on_ramp: false,
            active: true,
            release_at_seconds: 0.0,
            released: true,
        };
        runtime.balls.push(ball);

        for _ in 0..10 {
            runtime.tick(1.0 / 60.0);
            let b = &runtime.balls[0];
            let roller = &runtime.players["p1"].rollers["Roller1"];
            println!("t: roller_w={:.2} ball_v={:.3},{:.3},{:.3}", 
                     roller.angular_velocity, b.velocity[0], b.velocity[1], b.velocity[2]);
        }
        
        let final_ball = &runtime.balls[0];
        assert!(final_ball.velocity[2] > 0.0, "Ball should accelerate along Z due to roller spinning on X");
    }
}
"""
import re
with open('server/src/game/sphere_runtime.rs', 'r') as f:
    text = f.read()
if 'fn test_roller_friction_acceleration' in text:
    text = re.sub(r'#\[test\]\s*fn test_roller_friction_acceleration\(\) \{.*?\n\}\n\}', '', text, flags=re.DOTALL)
if 'fn test_roller_friction_acceleration' not in text:
    text = text.rsplit('}', 1)[0] + code
    with open('server/src/game/sphere_runtime.rs', 'w') as f:
        f.write(text)
