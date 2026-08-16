use super::*;

impl SphereRuntime {
    pub(super) fn step_mechanics(&mut self, arena: &ArenaConfig, dt: f32) {
        let radius = arena.ball.radius_m();
        for (player_id, player) in self.players.iter_mut() {
            let robot = effective_robot(&arena.robot, &player.mech);
            let ground_offset_y = -arena.robot.height_m * 0.5;
            let intake_zone = self.robot_definition.as_ref().and_then(|definition| {
                definition
                    .zones
                    .iter()
                    .find(|zone| zone.kind == RobotSemanticKind::Intake)
                    .map(|zone| {
                        robot_local_collider_pose(
                            &zone.collider,
                            player.position,
                            player.rotation,
                            ground_offset_y,
                        )
                    })
            });
            let outtake_zone = self.robot_definition.as_ref().and_then(|definition| {
                definition
                    .zones
                    .iter()
                    .find(|zone| zone.kind == RobotSemanticKind::Outtake)
                    .map(|zone| {
                        (
                            robot_local_collider_pose(
                                &zone.collider,
                                player.position,
                                player.rotation,
                                ground_offset_y,
                            ),
                            rotate_robot_local_pose(zone.direction, player.rotation),
                        )
                    })
            });

            // Intake capture.
            if player.intake_power > 0.0
                && robot.storage_capacity > 0
                && robot.intake_rate_bps > 0.0
            {
                let forward = rotate_robot_local_pose([0.0, 0.0, 1.0], player.rotation);
                let right = rotate_robot_local_pose([-1.0, 0.0, 0.0], player.rotation);
                player.intake_accumulator = (player.intake_accumulator
                    + robot.intake_rate_bps * player.intake_power * dt)
                    .min(120.0);
                self.intake_candidates.clear();
                let intake_world_y =
                    (player.position[1] - robot.height_m * 0.5) + robot.intake_center_height_m;
                for (index, ball) in self.balls.iter().enumerate() {
                    if !ball.active {
                        continue;
                    }
                    if let Some(zone) = &intake_zone {
                        if sphere_authored_obb_contact(ball.position, radius, zone).is_none() {
                            continue;
                        }
                        self.intake_candidates
                            .push((length_sq(sub(ball.position, zone.center)), index));
                        continue;
                    }
                    let delta = sub(ball.position, player.position);
                    let forward_dist = dot(delta, forward);
                    if forward_dist < -0.10
                        || forward_dist > robot.intake_forward_offset_m + radius + 0.10
                    {
                        continue;
                    }
                    let lateral_dist = dot(delta, right);
                    if lateral_dist.abs() > robot.intake_width_m * 0.5 + 0.08 {
                        continue;
                    }
                    let vertical_dist = (ball.position[1] - intake_world_y).abs();
                    if vertical_dist > radius + robot.intake_radius_m + 0.10 {
                        continue;
                    }
                    self.intake_candidates.push((forward_dist.abs(), index));
                }
                self.intake_candidates.sort_by(|left, right| {
                    left.0
                        .partial_cmp(&right.0)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                for i in 0..self.intake_candidates.len() {
                    let (_, index) = self.intake_candidates[i];
                    if player.intake_accumulator < 1.0 {
                        break;
                    }
                    if player.stored.len() >= robot.storage_capacity {
                        break;
                    }
                    if !self.balls[index].active {
                        continue;
                    }
                    self.balls[index].active = false;
                    player.stored.push_back(index);
                    player.intake_accumulator -= 1.0;
                    self.semantic_events.push(SemanticEvent {
                        kind: "intake",
                        target_id: player_id.clone(),
                        entity_id: format!("ball:{index}"),
                    });
                }
            }

            // Wide flywheel outtake.
            if player.outtake_power > 0.0
                && !player.stored.is_empty()
                && robot.outtake_rate_bps > 0.0
                && robot.outtake_velocity_mps > 0.0
            {
                let forward = rotate_robot_local_pose([0.0, 0.0, 1.0], player.rotation);
                let right = rotate_robot_local_pose([-1.0, 0.0, 0.0], player.rotation);
                player.outtake_accumulator += robot.outtake_rate_bps * player.outtake_power * dt;
                let pitch = robot.outtake_angle_deg.to_radians();
                let horizontal = robot.outtake_velocity_mps * pitch.cos();
                let vertical = robot.outtake_velocity_mps * pitch.sin();
                while player.outtake_accumulator >= 1.0 && !player.stored.is_empty() {
                    player.outtake_accumulator -= 1.0;
                    let index = player.stored.pop_front().unwrap();
                    // Deterministic hash spread so the wide mouth actually
                    // spits across its width without breaking replayability.
                    let jitter =
                        (((index as u32).wrapping_mul(2654435761u32)) as f32 / 4294967296.0) - 0.5;
                    let (exit, launch_forward) = if let Some((zone, direction)) = &outtake_zone {
                        (add(zone.center, mul(*direction, radius + 0.01)), *direction)
                    } else {
                        (
                            add(
                                add(
                                    player.position,
                                    mul(forward, robot.outtake_forward_offset_m),
                                ),
                                mul(right, jitter * robot.flywheel_width_m),
                            ),
                            forward,
                        )
                    };
                    let ball = &mut self.balls[index];
                    ball.position = [
                        exit[0],
                        exit[1] + jitter * robot.flywheel_width_m * 0.15,
                        exit[2],
                    ];
                    ball.velocity = [
                        launch_forward[0] * horizontal + player.velocity[0],
                        vertical + player.velocity[1],
                        launch_forward[2] * horizontal + player.velocity[2],
                    ];
                    ball.pre_solve_velocity = ball.velocity;
                    // Realistic flywheel backspin (spin axis along right vector)
                    let spin_rate = horizontal / arena.ball.radius_m().max(0.01);
                    ball.angular_velocity = mul(right, -spin_rate);
                    ball.quiet_ticks = 0;
                    ball.sleeping = false;
                    ball.grounded = false;
                    ball.active = true;
                    ball.last_outtake_alliance = Some(player.team_name.clone());
                    self.semantic_events.push(SemanticEvent {
                        kind: "outtake",
                        target_id: player_id.clone(),
                        entity_id: format!("ball:{index}"),
                    });
                }
            }
        }
    }
}
