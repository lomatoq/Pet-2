# Synthetic contextual-learning experiment

Run on Apple Silicon, release, 2026-09-08. `main.rs` is a standalone probe, not an application component. To reproduce, create a temporary Cargo package with `lifecore = { path = "/absolute/path/to/Pet-2/crates/lifecore" }`, `serde_json = "1"`, `glam = "0.30"` and copy this file to its `src/main.rs`; run `cargo run --release --offline`.

Ten seeds, 2,400 confirmed training responses per seed, then 1,000 choices in balanced held-out contexts with no further learning. A fixed acknowledgement gets 50%; the learner gets 94.4–98.1%. The outcome rule is fixed independently of the learner's choice. Test contexts vary the social feature and do not occur during training.

This demonstrates learning the authored preference rule, not an improvement measured with a human or in the complete desktop runtime. Timing includes response selection/outcome plus one body-learning transition and random draws; it excludes sensing, rendering and the other brain modules. The maximum observed serialized learner was 19,916 bytes. The free-movement-only replay probe fills one regime and executes eight updates.
