# LibGibson agent-native UI lab

External, non-binding pressure tests of LibGibson's experimental public Rust API.
This is a research consumer, not a new engine, stable instrument schema or
claim of arbitrary generated UI. The dependency is pinned to a reviewed main
commit; all semantics are fictional local deterministic fixtures.

Protocol: [experiment gates](docs/EXPERIMENT_PROTOCOL.md),
[friction ledger](docs/FRICTION_LEDGER.md), [results](docs/RESULTS.md).

No network/model service is required at runtime. Cargo needs the public pinned
dependency at build time. The three planned consumers share one semantic fixture.

Run the first consumer: `cargo run --release --bin manga -- --auto`.
Tab/arrows select, Enter emits selection, Y/N resolves the permission request,
Space pauses; Esc/Ctrl-C restore the terminal. `--at=10000 --color=mono --dump`
prints a bounded text frame; `--cells` emits cell/color JSON for development.

`cargo test` checks fixture replay, explicit actions and the responsive matrix.
`python3 scripts/pty_smoke.py target/debug/manga` exercises input, resize,
exit and terminal restoration on Linux. No screenshots or raw captures are retained.

Other consumers:

```sh
cargo run --release --bin reactions -- --mode=heavy
cargo run --release --bin reactions -- --mode=none
cargo run --release --bin reactions -- --metaphor=paperwork
cargo run --release --bin instruments
```

All use the same bounded host runner. `--record=keys.json` records application
keys, including focus; `--replay=keys.json` reconstructs them. Space/exit are
host transport controls. Reaction L/E selects explicit follow-up scope, while
Y/N in every consumer resolves the same permission request. Instruments compile
validated declarative specifications; no model or generated code execution is
required. [First-contact A](docs/MANGA_FIRST_CONTACT.md),
[B](docs/REACTION_FIRST_CONTACT.md), [C](docs/INSTRUMENT_FIRST_CONTACT.md),
and [reconciliation](docs/CROSS_EXPERIMENT_FRICTION.md) distinguish consumer bugs,
API ergonomics and deliberate safety bounds.
