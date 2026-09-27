# LibGibson agent-native UI lab

External, non-binding pressure tests of LibGibson's experimental public Rust API.
The original root campaign is a research consumer, not a new engine, stable
instrument schema or claim of arbitrary generated UI. Its dependency is pinned
to a reviewed pre-v0.2.0 commit; its semantics are fictional local deterministic
fixtures. The separate Europa applications below consume the released v0.2.0 tag.

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


Post-freeze holdouts:

```sh
cargo run --release --bin foldroom
cargo run --release --bin cuebox
cargo run --release --bin weavebench
```

Each footer describes its controls. Foldroom scrubs prescribed hinges and orbits
selected faces; Cuebox branches a rehearsal and captures/restores modal focus;
Weavebench edits the draft behind visible over/under crossings. These are bounded
research consumers, not production domain tools. See the
[final report](docs/AGENT_NATIVE_UI_FINAL_REPORT.md),
[freeze receipt](docs/API_FREEZE.md), and
[selection record](docs/holdouts/SELECTION.md). No core promotion was justified.

Final local suite: 61 integration tests. Frozen dependency/helper/IR integrity is
checked with `python3 scripts/verify_freeze.py` locally and in public CI.

## LibGibson v0.2.0 Europa-shot campaign

Three independently developed application branches now coexist as standalone
Cargo packages. Each consumes the **released v0.2.0 tag**, with its lockfile
resolving LibGibson to the same commit, without a local path dependency or
LibGibson source changes. Their separate Git histories and
experiment reports preserve which application produced each observation.

| Application | Directory | Distinct pressure test |
|---|---|---|
| [Project Galileo](europa/project-galileo/README.md) | `europa/project-galileo/` | Jovian procedural visualization, semantic UI, scale transitions and radar graphics |
| [Project Palimpsest](europa/project-palimpsest/README.md) | `europa/project-palimpsest/` | Large irregular Git histories, provenance, bounded viewports and native scrollback |
| [Project Synesthesia](europa/project-synesthesia/README.md) | `europa/project-synesthesia/` | Sustained state-driven music graphics, editing, audio separation and input under animation |

Read the [cross-Europa report](docs/EUROPA_V0_2_0_REPORT.md) and the individual
`EXPERIMENT_REPORT.md` files for validation, limitations and upstream LibGibson
issues. These applications are external research holdouts; the original frozen
campaign above remains historical evidence against its earlier pinned commit.

Each application builds and tests from its own directory; `cargo test` at the
repository root checks only the original lab. To check every consumer, run the
root suite and then `cargo test --locked` separately in each `europa/project-*`
directory. The original freeze receipt remains checked with
`python3 scripts/verify_freeze.py`.
