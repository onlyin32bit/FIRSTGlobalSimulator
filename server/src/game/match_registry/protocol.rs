use super::*;

// FGS1 is a little-endian, sectioned WebSocket protocol. Every section is
// [tag:u16, flags:u16, byte_length:u32, payload]. Readers must skip unknown
// tags, which makes compatible additions possible without changing old fields.
pub(super) fn encode_state(
    state: &MatchStateSync,
    process: ProcessMetrics,
    include_physics: bool,
) -> Vec<u8> {
    const METADATA: u16 = 1;
    const CLOCKS: u16 = 2;
    const METRICS: u16 = 3;
    const PLAYERS: u16 = 4;
    const OBJECTS: u16 = 5;
    const PHYSICS: u16 = 6;
    const SEMANTIC_EVENTS: u16 = 7;
    const DRIVE: u16 = 8;
    const SCORE: u16 = 9;
    const PLAYER_PHYSICS: u16 = 10;
    const TRANSFER_DEBUG: u16 = 11;
    const BALL_DEBUG: u16 = 12;
    let mut output = Vec::with_capacity(1024 + state.object_positions.count as usize * 12);
    output.extend_from_slice(b"FGS1");
    put_u16(&mut output, 1);
    put_u16(&mut output, 5);
    put_u16(&mut output, 1); // StateSnapshot
    put_u16(&mut output, 0);
    put_u32(&mut output, 0);

    section(&mut output, METADATA, |bytes| {
        put_string(bytes, &state.game_pack_id);
        put_string(bytes, &state.game_pack_version);
        put_string(bytes, &state.object_id);
        put_string(bytes, &state.object_color);
        put_f32(bytes, state.object_radius);
    });
    section(&mut output, CLOCKS, |bytes| {
        put_u64(bytes, state.tick);
        put_f64(bytes, state.match_clock);
        put_f64(bytes, state.simulation_clock);
        put_f64(bytes, state.clock_drift_ms);
        put_f64(bytes, state.match_duration_seconds);
        put_f64(bytes, state.pre_match_remaining_seconds);
        put_u8(bytes, u8::from(state.match_running));
        put_u8(bytes, u8::from(state.practice_running));
    });
    section(&mut output, METRICS, |bytes| {
        put_f64(bytes, state.physics_tick_ms);
        put_f64(bytes, state.physics_load_percent);
        put_f64(bytes, state.ticks_per_second);
        put_f64(bytes, state.target_ticks_per_second);
        put_u32(bytes, state.contacts as u32);
        put_f64(bytes, state.step_metrics.integrate_ms);
        put_f64(bytes, state.step_metrics.broad_phase_ms);
        put_f64(bytes, state.step_metrics.solve_ms);
        put_u32(bytes, state.step_metrics.candidate_pairs as u32);
        put_u32(bytes, state.step_metrics.active_balls as u32);
        put_u32(bytes, state.step_metrics.sleeping_balls as u32);
        put_f64(bytes, process.cpu_percent);
        put_f64(bytes, process.rss_mib);
    });
    section(&mut output, PLAYERS, |bytes| {
        put_u32(bytes, state.players.len() as u32);
        for player in &state.players {
            put_string(bytes, &player.id);
            put_string(bytes, &player.name);
            put_string(bytes, &player.team_name);
            put_string(bytes, &player.color);
            for value in [
                player.x,
                player.y,
                player.z,
                player.yaw,
                player.heading_deg,
                player.velocity_x,
                player.velocity_y,
                player.velocity_z,
                player.angular_velocity_y,
            ] {
                put_f32(bytes, value);
            }
            put_u32(bytes, player.stored_balls as u32);
            put_u32(bytes, player.capacity as u32);
            put_u8(bytes, player.brace_zone.unwrap_or_default());
            put_f32(bytes, player.brace_multiplier);
        }
    });
    section(&mut output, PLAYER_PHYSICS, |bytes| {
        put_u32(bytes, state.players.len() as u32);
        for player in &state.players {
            put_string(bytes, &player.id);
            for value in [
                player.rotation_x,
                player.rotation_y,
                player.rotation_z,
                player.rotation_w,
                player.angular_velocity_x,
                player.angular_velocity_y,
                player.angular_velocity_z,
                player.brace_support_impulse,
                player.climb_wheel_angle,
                player.climb_wheel_radps,
            ] {
                put_f32(bytes, value);
            }
            put_u8(bytes, u8::from(player.floor_supported));
            put_u8(bytes, u8::from(player.brace_contact));
        }
    });
    section(&mut output, OBJECTS, |bytes| {
        put_u32(bytes, state.object_positions.count);
        bytes.extend_from_slice(&state.object_positions.active_mask);
        bytes.extend_from_slice(&state.object_positions.moving_mask);
        for value in &state.object_positions.quantized_positions {
            put_u16(bytes, *value);
        }
    });
    if include_physics {
        section(&mut output, PHYSICS, |bytes| {
            put_string(bytes, &state.physics.ball_material);
            put_string(bytes, &state.physics.floor_material);
            for value in [
                state.physics.ball_diameter_m,
                state.physics.ball_diameter_tolerance_m,
                state.physics.ball_mass_kg,
                state.physics.ball_friction,
                state.physics.ball_restitution,
                state.physics.ball_rolling_resistance_mps2,
                state.physics.floor_friction,
                state.physics.robot_mass_kg,
                state.physics.robot_width_m,
                state.physics.robot_height_m,
                state.physics.robot_length_m,
                state.physics.robot_max_speed_mps,
            ] {
                put_f32(bytes, value);
            }
            put_u8(bytes, u8::from(state.physics.intake_enabled));
            put_u8(bytes, u8::from(state.physics.ramp_enabled));
            for value in [
                state.physics.ball_inertia_factor,
                state.physics.ball_drag_coefficient,
                state.physics.air_density_kg_m3,
                state.physics.ball_ball_friction,
                state.physics.floor_static_friction,
                state.physics.floor_dynamic_friction,
                state.physics.floor_rolling_resistance_mps2,
                state.physics.intake_width_m,
                state.physics.intake_radius_m,
                state.physics.intake_forward_offset_m,
                state.physics.intake_center_height_m,
                state.physics.intake_surface_speed_mps,
                state.physics.ramp_center_x,
                state.physics.ramp_start_z,
                state.physics.ramp_width_m,
                state.physics.ramp_length_m,
                state.physics.ramp_angle_deg,
                state.physics.solver_position_iterations,
                state.physics.solver_velocity_iterations,
                state.physics.max_depenetration_speed_mps,
                state.physics.max_ball_speed_mps,
                state.physics.max_ball_angular_speed_radps,
                state.physics.max_drive_force_n,
                state.physics.max_drive_power_w,
                state.physics.max_brake_force_n,
            ] {
                put_f32(bytes, value);
            }
            for value in [
                state.physics.storage_capacity,
                state.physics.intake_rate_bps,
                state.physics.outtake_rate_bps,
                state.physics.outtake_velocity_mps,
                state.physics.outtake_angle_deg,
                state.physics.flywheel_width_m,
                state.physics.outtake_forward_offset_m,
                state.physics.outtake_height_m,
            ] {
                put_f32(bytes, value);
            }
        });
    }
    section(&mut output, SCORE, |bytes| {
        put_i32(bytes, state.score.blue_score);
        put_i32(bytes, state.score.red_score);
        put_i32(bytes, state.score.global_score);
        put_u32(bytes, state.score.breakdown.len() as u32);
        for (category, points) in &state.score.breakdown {
            put_string(bytes, category);
            put_i32(bytes, *points);
        }
    });
    section(&mut output, SEMANTIC_EVENTS, |bytes| {
        put_u16(bytes, state.semantic_events.len() as u16);
        for event in &state.semantic_events {
            put_string(bytes, event);
        }
    });
    if include_physics {
        section(&mut output, DRIVE, |bytes| {
            for value in [
                state.drive.max_acceleration_mps2,
                state.drive.max_deceleration_mps2,
                state.drive.max_turn_rate_radps,
                state.drive.max_angular_acceleration_radps2,
                state.drive.lateral_grip_mps2,
                state.drive.traction_friction,
                state.drive.track_width_m,
            ] {
                put_f32(bytes, value);
            }
        });
    }
    // Transfer debug section — always emitted so the client can diagnose issues.
    // Format: u8 count, then per-player: string name, u8 flags, f32 intake, f32 outtake,
    // f32 outtake force, f32 outtake target speed, u16 contact count, f32 contact speed.
    // Flags bitmask: bit0=has_ball, bit1=transfer_power_ok, bit2=inside_robot,
    //                bit3=touches_outtake, bit4=has_transfer_zone, bit5=has_robot_definition
    section(&mut output, TRANSFER_DEBUG, |bytes| {
        put_u8(bytes, state.transfer_debug.len() as u8);
        for dbg in &state.transfer_debug {
            put_string(bytes, &dbg.player_name);
            let flags: u8 = (dbg.has_ball as u8)
                | ((dbg.transfer_power_ok as u8) << 1)
                | ((dbg.inside_robot as u8) << 2)
                | ((dbg.touches_outtake as u8) << 3)
                | ((dbg.has_transfer_zone as u8) << 4)
                | ((dbg.has_robot_definition as u8) << 5);
            put_u8(bytes, flags);
            put_f32(bytes, dbg.intake_power);
            put_f32(bytes, dbg.outtake_power);
            put_f32(bytes, dbg.outtake_force_n);
            put_f32(bytes, dbg.outtake_target_speed_mps);
            put_u16(bytes, dbg.outtake_contact_balls);
            put_f32(bytes, dbg.max_outtake_contact_speed_mps);
        }
    });
    // Ball debug section — lightweight per-ball flags for rendering transfer vectors/collisions.
    // Per active ball: u8 flags, u8 contact_count, then contact_count × string collider id
    // (the authored robot collision box the ball is touching).
    section(&mut output, BALL_DEBUG, |bytes| {
        let count = state.object_positions.quantized_positions.len() / 3;
        put_u16(bytes, count as u16);
        for (i, active) in state.object_positions.active_mask.iter().enumerate() {
            if *active != 0 {
                if i < state.ball_debug.len() {
                    put_u8(bytes, state.ball_debug[i]);
                } else {
                    put_u8(bytes, 0);
                }
                let contacts = state
                    .ball_contact_colliders
                    .get(i)
                    .map(|ids| ids.as_slice())
                    .unwrap_or(&[]);
                let contact_count = contacts.len().min(8);
                put_u8(bytes, contact_count as u8);
                for id in contacts.iter().take(contact_count) {
                    put_string(bytes, id);
                }
            }
        }
    });
    let payload_len = (output.len() - 16) as u32;
    output[12..16].copy_from_slice(&payload_len.to_le_bytes());
    output
}
