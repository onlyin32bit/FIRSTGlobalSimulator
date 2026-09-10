use super::*;

impl SphereRuntime {
    pub(super) fn step_mechanics(&mut self, arena: &ArenaConfig, dt: f32) {
        let radius = arena.ball.radius_m();
        for (player_id, player) in self.players.iter_mut() {
            let robot = &arena.robot;
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
            // Keep the logical queue synchronized with the authored robot volume.
            if self.robot_definition.is_some() {
                let envelope = robot_local_collider_pose(
                    &self
                        .robot_definition
                        .as_ref()
                        .expect("definition checked")
                        .bounds,
                    player.position,
                    player.rotation,
                    ground_offset_y,
                );
                let queued: Vec<_> = player.stored.drain(..).collect();
                for index in queued {
                    let owned = self.balls[index].owner.as_deref() == Some(player_id);
                    let inside = self.balls[index].active
                        && sphere_authored_obb_contact(
                            self.balls[index].position,
                            radius,
                            &envelope,
                        )
                        .is_some();
                    if owned && inside {
                        player.stored.push_back(index);
                    } else if owned {
                        self.balls[index].owner = None;
                    }
                }
            }
            let _transfer_zone = self.robot_definition.as_ref().and_then(|definition| {
                definition
                    .zones
                    .iter()
                    .find(|zone| zone.kind == RobotSemanticKind::Transfer)
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
            if player.intake_power > 0.0 && robot.intake_rate_bps > 0.0 {
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
                    if !self.balls[index].active
                        || self.balls[index].owner.is_some()
                        || player.stored.len() >= robot.storage_capacity
                    {
                        continue;
                    }
                    player.stored.push_back(index);
                    self.balls[index].owner = Some(player_id.clone());
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
                && robot.outtake_rate_bps > 0.0
                && robot.outtake_velocity_mps > 0.0
            {
                let forward = rotate_robot_local_pose([0.0, 0.0, 1.0], player.rotation);
                let right = rotate_robot_local_pose([-1.0, 0.0, 0.0], player.rotation);
                player.outtake_accumulator = (player.outtake_accumulator
                    + robot.outtake_rate_bps * player.outtake_power * dt)
                    .min(1.0);
                if player.outtake_accumulator >= 1.0 {
                    let candidate = if outtake_zone.is_some() {
                        player.stored.iter().position(|index| {
                            let ball = &self.balls[*index];
                            ball.active
                                && ball.owner.as_deref() == Some(player_id)
                                && sphere_authored_obb_contact(
                                    ball.position,
                                    radius,
                                    &outtake_zone.as_ref().expect("zone checked").0,
                                )
                                .is_some()
                        })
                    } else {
                        player.stored.iter().position(|index| {
                            self.balls[*index].owner.as_deref() == Some(player_id)
                        })
                    };
                    let Some(position) = candidate else {
                        continue;
                    };
                    let Some(index) = player.stored.remove(position) else {
                        continue;
                    };
                    player.outtake_accumulator -= 1.0;
                    self.balls[index].owner = None;

                    let (exit_offset, launch_dir) = if let Some((zone, direction)) = &outtake_zone {
                        (zone.center, *direction)
                    } else {
                        let pitch = robot.outtake_angle_deg.to_radians();
                        let horiz = pitch.cos();
                        let vert = pitch.sin();
                        (
                            add(
                                player.position,
                                mul(forward, robot.outtake_forward_offset_m),
                            ),
                            [forward[0] * horiz, vert, forward[2] * horiz],
                        )
                    };
                    let ball = &mut self.balls[index];
                    if self.robot_definition.is_none() {
                        if ball.position[1] < exit_offset[1] {
                            ball.position[1] = exit_offset[1];
                        }
                        if !ball.active {
                            ball.position = [exit_offset[0], exit_offset[1], exit_offset[2]];
                            ball.active = true;
                        }
                        let launch_speed = robot.outtake_velocity_mps * player.outtake_power;
                        ball.velocity = [
                            launch_dir[0] * launch_speed + player.velocity[0],
                            launch_dir[1] * launch_speed + player.velocity[1],
                            launch_dir[2] * launch_speed + player.velocity[2],
                        ];
                        ball.pre_solve_velocity = ball.velocity;
                        ball.previous_position = sub(ball.position, mul(ball.velocity, dt));
                        let spin_rate = launch_speed / arena.ball.radius_m().max(0.01);
                        ball.angular_velocity = mul(right, -spin_rate);
                    }
                    ball.quiet_ticks = 0;
                    ball.sleeping = false;
                    ball.grounded = false;
                    ball.physics_dirty = true;
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
