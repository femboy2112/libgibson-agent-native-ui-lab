# Build task — Project Estuary

Build a standalone LibGibson v0.4.0 consumer application.

Create a deterministic synthetic **maritime / navigation environment**
containing things such as:

- an archipelago;
- tidal and current fields;
- temperature / salinity or depth structure;
- weather / storm systems;
- drifting objects or vessels;
- route planning;
- changing hazards;
- forecast versus observed state.

The user must be able to:

- inspect the environment;
- advance or scrub simulated time;
- plan or alter a route;
- inspect local conditions;
- follow a storm or current event as it changes the route;
- compare predicted and realized outcomes;
- resize the terminal while preserving a useful representation.

The result should be an unusually ambitious, visually striking terminal
application, not merely a static report.

Constraints:

- No external network. No external map or image assets. Synthetic, deterministic
  data only, from a fixed seed.
- Consume the released LibGibson v0.4.0 tag exactly. Your `Cargo.toml` must
  depend on it as:

  ```toml
  libgibson = { git = "https://github.com/femboy2112/libgibson", tag = "v0.4.0" }
  ```

  No path override, no patch.
- A read-only checkout of the released library and its documentation is available
  at `libgibson-reference/`. Use it if you need API guidance. Do not modify that
  reference tree. Do not modify upstream LibGibson.

Deliver:

- a standalone Cargo package;
- a committed `Cargo.lock`;
- a deterministic seed;
- a deterministic, bounded demo;
- a headless / capture path;
- captures at 120x40, 80x24, 42x15, and Mono;
- a real PTY smoke test for resize / input / exit restoration;
- tests for semantic correctness;
- a README with the exact commands to build, run, capture, and test;
- a brief report of any LibGibson friction you encountered.

Do not file upstream issues unless you isolate a genuinely generic mechanism with
a minimal reproducer; record any such friction in your report instead.

The application itself is the priority.
