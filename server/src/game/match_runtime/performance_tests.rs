use super::*;
use std::time::Instant;

fn arena() -> ArenaConfig {
    crate::game::pack_loader::PackLoader::new("0.1.0")
        .load_pack("../pkgs/games/fgc-2026/manifest.json")
        .unwrap()
        .arena
}

#[test]
#[ignore = "manual Rapier performance comparison"]
fn benchmark_rapier_ball_interaction() {
    let arena = arena();
    let mut runtime = MatchRuntime::new("perf".into(), "fgc-2026".into(), 0);
    runtime.create_test_arena(&arena);
    runtime.add_player("player".into(), "Player".into(), "Team".into(), &arena);
    runtime.set_player_input("player", 0.35, 1.0, 1);

    let started = Instant::now();
    for _ in 0..300 {
        runtime.apply_player_drive(&arena);
        runtime.tick(1.0 / 60.0);
    }
    let elapsed = started.elapsed();
    let milliseconds_per_tick = elapsed.as_secs_f64() * 1_000.0 / 300.0;

    assert_eq!(
        runtime.field_object_positions().count as usize,
        arena.object_count
    );
    eprintln!(
        "Rapier {}-ball interaction: {:.2} ms/tick ({:.1} simulated FPS)",
        arena.object_count,
        milliseconds_per_tick,
        1_000.0 / milliseconds_per_tick
    );
}
