pub mod error;
pub mod match_registry;
pub mod match_runtime;
pub mod pack_loader;
pub mod rhai_engine;
pub mod sphere_runtime;

// Stable game-facing types stay available from this module while their
// implementations live in smaller files.
pub use match_runtime::{MatchContext, MatchPhase, PlayerSnapshot, ScoreState};
pub use sphere_runtime::{MechSpec, SemanticEvent, SphereRuntime, StepMetrics, TransferDebug, BallDebugFlag};
