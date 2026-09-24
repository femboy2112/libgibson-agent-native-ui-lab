# LibGibson agent-native UI lab

External, non-binding pressure tests of LibGibson's experimental public Rust API.
This is a research consumer, not a new engine, stable instrument schema or
claim of arbitrary generated UI. The dependency is pinned to a reviewed main
commit; all semantics are fictional local deterministic fixtures.

Protocol: [experiment gates](docs/EXPERIMENT_PROTOCOL.md),
[friction ledger](docs/FRICTION_LEDGER.md), [results](docs/RESULTS.md).

No network/model service is required at runtime. Cargo needs the public pinned
dependency at build time. Commands and evidence will be added with each consumer.
