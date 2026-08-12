use super::*;

impl SphereRuntime {
    pub(super) fn reconcile_scoring(&mut self) {
        for ball_index in 0..self.balls.len() {
            let ball = &self.balls[ball_index];
            let previous = self.scored_target_by_ball[ball_index];
            let next =
                self.scoring_targets
                    .iter()
                    .enumerate()
                    .position(|(target_index, target)| {
                        let remains_in_hopper = previous == Some(target_index)
                            && target.retention.as_ref().is_some_and(|retention| {
                                point_inside_aabb(ball.position, retention.min, retention.max)
                            });
                        ball.active
                    && target.enabled
                    // The game pack authors a scoring volume around the ball
                    // center path through the lower SU hopper. Requiring the
                    // whole sphere to fit shrinks that volume enough to reject
                    // visibly valid scores along the hopper walls.
                    && (point_inside_aabb(ball.position, target.min, target.max)
                        || remains_in_hopper)
                    && (!target.requires_robot_outtake
                        || target.alliance.as_deref() == ball.last_outtake_alliance.as_deref())
                    });
            if previous == next {
                continue;
            }
            if let Some(target_index) = previous {
                let target = self.scoring_targets[target_index].clone();
                self.apply_score_delta(&target, -1);
                self.semantic_events.push(SemanticEvent {
                    kind: "score_removed",
                    target_id: target.id.clone(),
                    entity_id: format!("ball:{ball_index}"),
                });
            }
            if let Some(target_index) = next {
                let target = self.scoring_targets[target_index].clone();
                self.apply_score_delta(&target, 1);
                self.semantic_events.push(SemanticEvent {
                    kind: "score_added",
                    target_id: target.id.clone(),
                    entity_id: format!("ball:{ball_index}"),
                });
            }
            self.scored_target_by_ball[ball_index] = next;
        }
    }

    /// Keep scored pieces in the pack-authored hopper pocket. The top stays
    /// open so a real exit can still remove a score; only the floor and four
    /// containment faces are enforced here.
    pub(super) fn retain_scored_balls(&mut self, radius: f32) -> usize {
        let mut contacts = 0;
        for (ball_index, ball) in self.balls.iter_mut().enumerate() {
            let Some(target_index) = self.scored_target_by_ball[ball_index] else {
                continue;
            };
            let Some(retention) = self.scoring_targets[target_index].retention.as_ref() else {
                continue;
            };
            if !ball.active {
                continue;
            }
            for axis in [0, 2] {
                let min = retention.min[axis] + radius;
                let max = retention.max[axis] - radius;
                let constrained = ball.position[axis].clamp(min, max);
                if constrained != ball.position[axis] {
                    ball.position[axis] = constrained;
                    contacts += 1;
                }
            }
            let floor = retention.min[1] + radius;
            if ball.position[1] < floor {
                ball.position[1] = floor;
                contacts += 1;
            }
            if !retention.open_top {
                let ceiling = retention.max[1] - radius;
                if ball.position[1] > ceiling {
                    ball.position[1] = ceiling;
                    contacts += 1;
                }
            }
        }
        contacts
    }

    pub(super) fn settle_retained_balls(&mut self, dt: f32) {
        let damping = (-8.0 * dt).exp();
        for (ball_index, ball) in self.balls.iter_mut().enumerate() {
            let retained = self.scored_target_by_ball[ball_index]
                .and_then(|index| self.scoring_targets.get(index))
                .is_some_and(|target| target.retention.is_some());
            if retained && ball.active {
                ball.velocity = mul(ball.velocity, damping);
                ball.angular_velocity = mul(ball.angular_velocity, damping * damping);
            }
        }
    }

    fn apply_score_delta(&mut self, target: &FieldScoringTarget, direction: i32) {
        let points = target.points * direction;
        match target.alliance.as_deref() {
            Some("blue") => self.score_state.blue_score += points,
            Some("red") => self.score_state.red_score += points,
            _ => self.score_state.global_score += points,
        }
        let entry = self
            .score_state
            .breakdown
            .entry(target.id.clone())
            .or_insert(0);
        *entry += points;
        if *entry == 0 {
            self.score_state.breakdown.remove(&target.id);
        }
    }
}
