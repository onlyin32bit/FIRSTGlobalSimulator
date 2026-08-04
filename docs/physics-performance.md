# Physics performance notes

The previous 1,000-ball capture had a healthy client frame rate and local
network, but server physics consumed more than twice its 16.67 ms tick budget.
General rigid-body island/constraint work, predictive contact expansion, and
JSON state construction all happened on the match thread. Once that thread
fell behind, TPS, snapshot rate, and simulation time declined together.

The optimized path applies these techniques:

- a contiguous sphere-specific solver instead of general rigid-body graphs;
- a preallocated spatial hash, limiting ball tests to neighboring cells;
- analytic sphere, carpet, wall, and oriented robot-box contacts;
- fixed-iteration XPBD-style penetration correction and velocity restitution;
- sleeping for settled balls and contact-driven waking;
- no allocation in integration, broad phase, or contact solving after startup;
- a separate 20 Hz binary publisher with latest-state semantics;
- one typed position buffer and one persistent instanced mesh in the browser.

Performance is measured in an optimized build:

```bash
cd server
cargo test --release benchmark_1000_ball_robot_interaction -- --ignored --nocapture
```

The benchmark warms the scene, drives a robot through 1,000 colliding balls for
600 measured ticks, and enforces p95 <= 12 ms and p99 <= 16.67 ms.
