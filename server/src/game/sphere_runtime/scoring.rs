use super::*;

impl SphereRuntime {
    pub(super) fn reconcile_scoring(&mut self) {
        for ball_index in 0..self.balls.len() {
            let ball = &self.balls[ball_index];
            let previous = self.scored_target_by_ball[ball_index];
            let next = self.scoring_targets.iter().position(|target| {
                ball.active
                    && target.enabled
                    && point_inside_aabb(ball.position, target.min, target.max)
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

    fn apply_score_delta(&mut self, target: &FieldScoringTarget, direction: i32) {
        let points = target.points * direction;
        match target.alliance.as_deref() {
            Some("blue") => {
                self.score_state.blue_score += points;
                if target.kind == "suppression-unit" {
                    self.score_state.blue_su_score += points;
                }
            }
            Some("red") => {
                self.score_state.red_score += points;
                if target.kind == "suppression-unit" {
                    self.score_state.red_su_score += points;
                }
            }
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
