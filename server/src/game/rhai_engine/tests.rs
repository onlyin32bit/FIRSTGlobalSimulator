use super::RhaiEngine;

#[test]
fn inspects_functions_and_engine_calls() {
    let engine = RhaiEngine::new();
    let metadata = engine
        .inspect_source(
            "rules/example.rhai",
            "fn on_tick(state) { add_score(\"blue\", \"SU\", 1); }",
        )
        .unwrap();
    assert_eq!(metadata.functions[0].name, "on_tick");
    assert_eq!(metadata.functions[0].parameters, vec!["state"]);
    assert_eq!(metadata.engine_calls, vec!["add_score"]);
}

#[test]
fn rejects_invalid_rhai() {
    let engine = RhaiEngine::new();
    assert!(
        engine
            .inspect_source("rules/broken.rhai", "fn broken( {")
            .is_err()
    );
}

#[test]
fn executes_authored_trigger_hook_and_captures_score() {
    let mut engine = RhaiEngine::new();
    assert!(engine.load_source(
        "rules/scoring.rhai",
        r#"fn on_trigger_enter(trigger_id, entity_id) { add_score("blue", "SU", 1); }"#
    ));
    let outcomes = engine.on_trigger_enter("blueSUscore", "ball:42");
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].team, "blue");
    assert_eq!(outcomes[0].category, "SU");
    assert_eq!(outcomes[0].points, 1);
}
