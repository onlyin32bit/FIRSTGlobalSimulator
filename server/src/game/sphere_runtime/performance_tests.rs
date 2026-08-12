use super::*;
use std::time::Instant;

fn arena() -> ArenaConfig {
    crate::game::pack_loader::PackLoader::new("0.1.0")
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap()
        .arena
}

#[test]
#[ignore = "manual release-mode 1,000-ball performance benchmark"]
fn benchmark_1000_ball_robot_interaction() {
    let mut arena = arena();
    arena.object_count = 1000;
    let mut runtime = SphereRuntime::new("benchmark".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    runtime.add_player("p".into(), "Player".into(), "Team".into(), None, &arena);
    runtime.set_player_input("p", 0.28, 1.0, 1.0, 0.0, 1);
    for _ in 0..120 {
        runtime.apply_player_drive(&arena, 1.0 / 60.0);
        runtime.tick(1.0 / 60.0);
    }
    let started = Instant::now();
    let mut samples = Vec::with_capacity(600);
    let mut maximum_candidates = 0;
    let mut maximum_contacts = 0;
    let mut minimum_active = arena.object_count;
    for tick in 0..600 {
        // Re-energize the field once per second. This prevents sleeping
        // from turning a sustained-contact benchmark into an idle test.
        if tick % 60 == 0 {
            for (index, ball) in runtime.balls.iter_mut().enumerate() {
                let angle = index as f32 * 0.618_034;
                ball.velocity[0] += angle.cos() * 1.5;
                ball.velocity[2] += angle.sin() * 1.5;
                ball.sleeping = false;
                ball.quiet_ticks = 0;
            }
        }
        let tick_started = Instant::now();
        runtime.apply_player_drive(&arena, 1.0 / 60.0);
        runtime.tick(1.0 / 60.0);
        samples.push(tick_started.elapsed().as_secs_f64() * 1_000.0);
        let tick_metrics = runtime.step_metrics();
        maximum_candidates = maximum_candidates.max(tick_metrics.candidate_pairs);
        maximum_contacts = maximum_contacts.max(tick_metrics.contacts);
        minimum_active = minimum_active.min(tick_metrics.active_balls);
    }
    samples.sort_by(f64::total_cmp);
    let p95 = samples[(samples.len() as f32 * 0.95) as usize];
    let p99 = samples[(samples.len() as f32 * 0.99) as usize];
    let average = started.elapsed().as_secs_f64() * 1_000.0 / samples.len() as f64;
    let metrics = runtime.step_metrics();
    eprintln!(
        "sphere_xpbd balls={} avg={average:.3}ms p95={p95:.3}ms p99={p99:.3}ms candidates(max)={} contacts(max)={} active(min)={} sleeping(final)={}",
        arena.object_count,
        maximum_candidates,
        maximum_contacts,
        minimum_active,
        metrics.sleeping_balls,
    );
    assert_eq!(runtime.field_object_positions().count, 1000);
    assert!(p95 <= 12.0, "p95 tick time was {p95:.3}ms");
    assert!(p99 <= 16.67, "p99 tick time was {p99:.3}ms");
}
