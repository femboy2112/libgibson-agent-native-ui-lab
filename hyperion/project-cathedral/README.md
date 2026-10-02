# Project Cathedral

**Project Cathedral** is a deterministic simulation of a distributed-system incident whose
state is rendered twice at once: as a **living architectural structure** (a nine-district
cathedral/megacity in the terminal) and as a **HumanMusic score**. It is a standalone Cargo
package and an external consumer, not a new engine. Its only terminal UI engine is the
released **LibGibson `v0.4.0`** Git tag.

This is an **external pressure test**, not a model of production systems. The service graph,
capacities and failure couplings are a deliberately fictional stress domain chosen so a single
minor fault can propagate through ~269 services and 482 dependency edges. The score is a
documented musical *response* to the incident's semantic trajectory; it does **not understand
distributed systems**. See [EXPERIMENT_REPORT.md](EXPERIMENT_REPORT.md) for the exact mapping,
substrate provenance and measured evidence, and [FRICTION.md](FRICTION.md) for the friction
ledger.

## What you are looking at

The whole system is one fixed scene. The camera moves over it; it is never swapped out.

| semantic | spatial |
|---|---|
| service | tower / chamber, height = stress, color = integrity |
| dependency edge | bridge between tower crowns |
| request flow | an energy glyph travelling the bridge |
| queue pressure | a compressed amber band climbing from the tower base |
| latency | vertical stretch of the tower |
| retry storm | recursive echo copies of the travelling glyph |
| cascading failure | bridges fade, crowns collapse to rubble |
| availability | structural integrity (color + solidity) |
| isolated service | a severed wing, displaced across a gap |
| recovery | the same geometry easing back into coherent form |

Four continuous scale transitions — **whole system → cluster → causal chain → service** — are
one camera over the same scene, so a transition reads as travelling through one structure.

The incident phase machine (`NORMAL`, `RISING UNCERTAINTY`, `OVERLOAD`, `LOCAL FAULT`,
`CASCADE`, `DIAGNOSIS`, `INTERVENTION`, `PARTIAL RECOVERY`, `RESTORED`) drives both the
architecture and the score. A minor deployment fault on the single most-depended-upon service
is injected, cascades unassisted, and is then rolled back and rerouted to full recovery.

Milestones, operator actions, analyses and the compact incident report are committed to the
terminal's **native scrollback** above the **live** region while the live system keeps animating.

## Start here

From this directory (Rust 1.98.1 is pinned by the repository's `rust-toolchain.toml`):

```sh
cargo run --locked --release
```

Interactive controls:

| Key | Action |
|---|---|
| `1` / `2` / `3` / `4`, `Tab` | whole system / cluster / causal chain / service; `Tab` cycles |
| Left / Right, `D` | move focus; `D` jumps to the most distressed service — at rest, the largest blast radius (the keystone `#138`) |
| `F` | inject a minor fault on the focused service |
| `R` | roll back the (bad) deployment |
| `I` | isolate / sever the focused service |
| `X` | reroute traffic to same-cluster peers |
| `S` | shed 50% of the focused service's load |
| `A` | acknowledge (opens the diagnosis phase) |
| `N` | annotate (cycles canned operator notes) |
| `Space` | pause / resume |
| `W` | export the current checked performance to `cathedral.wav` |
| `H` / `?` | toggle help |
| `Q` / `Esc` / `Ctrl-C` | quit and restore the terminal |

**There is no live audio.** The score is built and checked in memory; nothing is sent to a sound
device. Press `W` (or run with `--music-out=DIR`) to write `cathedral.wav`, then play it with
`ffplay cathedral.wav`, `aplay cathedral.wav`, or any audio player. The footer's
`Tone/Emphasis/Density/Elevation` axis shows the score's current semantic state, not sound.

**A fault only cascades if it has a blast radius.** The footer shows the focused service's
downstream dependents (`focus #N·D↓`). The default focus `#0` is a leaf with `0↓`, so a fault
there degrades exactly that one service and then sits still — that is the model working, not a
hang. Press `D` to jump to the keystone (`#138`, `62↓`) before pressing `F`, or run `--wtf` to
watch the scripted incident on the keystone.

Every operator action enters one ordered journal. Replay is
`fixture + seed + action journal → equivalent semantic state`:

```sh
cargo run --release -- --wtf --frames=10000 --music --record=run.json
cargo run --release -- --replay=run.json
```

### Headless, capture and evidence

```sh
# One byte-stable readable frame at a deterministic frame index.
cargo run --release -- --wtf --at=220 --text --no-music --width=100 --height=30

# Bounded deterministic run with metrics + receipt.
cargo run --release -- --wtf --frames=10000 --music --record=run.json

# Capability matrix: 5 sizes × 4 depths, checking state/cluster/affordance/exit.
cargo run --release -- --capability

# Export the final checked BAND performance (WAV + PCM hash + receipts).
cargo run --release -- --wtf --frames=650 --music --music-out=out/
```

`--at=N` runs N frames headlessly and prints the final frame without wall-clock pacing;
`--text` prints the frame as plain readable text (no ANSI). `--dump` prints the raw ANSI frame.
`--color=auto|truecolor|256|16|mono` degrades the palette. `--at`/`--frames` are explicit capture
inputs. `--seed` seeds the operator journal and the music take; the simulation fixture itself is
fixed, so the topology is identical across seeds. `CATHEDRAL_TRACE=1` copies committed scrollback
lines to stderr.

### Validate

```sh
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked          # 15 unit + 8 black-box integration tests
cargo build --locked --release
python3 scripts/pty_stress.py --binary target/release/cathedral --live-seconds=3
python3 scripts/pty_stress.py --binary target/release/cathedral --observe-only --fps=300 --live-seconds=40
```

The PTY probe is Linux-specific. It drives the *real* interactive binary in a pseudo-terminal:
operator-command burst, resize during the cascade, output backpressure, sustained interaction,
and terminal restoration. It reports observable action side-effects and explicitly records the
boundary that the app emits no per-key input receipt.

## Architecture and bounds

- `src/sim/fixture.rs` deterministically generates the named `cathedral` fixture from a fixed
  seed (`0xCA7ED_BA5E_2026_04`): 9 districts, 269 services, 482 acyclic dependency edges, then
  provisions per-replica capacity so the **resting** system sits near 60% utilization and is
  stable. A minor fault — not a permanent baseline overload — is what creates the incident.
- `src/sim/engine.rs` is a closed tick-based model: demand, admission, queues, retry debt,
  circuit breakers with a real half-open trial window, recoverable health relaxation, latency
  stretch, and realized link flows. No sockets, no clocks, no OS state.
- `src/incident.rs` classifies the raw tick metrics into the nine-phase vocabulary with a 6-tick
  dwell and a Schmitt trigger on cascade release.
- `src/scenario.rs` drives the scripted "WTF" incident through LibGibson's public `story`
  director. Each beat's directive becomes an ordinary journaled `Action`; the story names the
  kind of move, the application chooses the live target and performs it through the real
  simulation.
- `src/visual.rs` builds one custom `Surface` per frame from the visual grammar and composes it
  with public `Node` text/raster nodes. `src/app.rs` owns the deterministic transition and the
  journal; `src/music.rs` owns the HumanMusic mapping; `src/replay.rs` owns the record/replay
  receipt; `src/capability.rs` runs the size × depth matrix.
- The package uses the public LibGibson `Context` (headless/fullscreen), `Node`, `Surface`,
  capability quantization, input events and the released `story`, `audio::human_music` and
  `audio::render`/`audio::wav` modules. It does not use `gibson::ui`, private modules, raw ANSI
  or a path/patch override.

## Results and limits

On this Ubuntu host, a **10,000-frame** deterministic run with live checked-BAND music and a
final WAV export completed in **73.0 s**. Mean frame **5.30 ms**, p95 **1.30 ms**, max **1.14 s**
(a music rebuild), mean **8,369 emitted bytes/frame** and **419 exact changed cells/frame**;
bounded history stayed at **11 scrollback insertions / max 2 per frame**; the frame loop's RSS
was 7.1 MiB → 23.2 MiB, and the whole run (including the materialized 100 MB PCM for export)
peaked at **~612 MiB**. A `fixture + seed + journal` replay reproduced the 10,000-frame semantic
digest `b22330570bb7875f` with all **50 checkpoints** matching. The 650-frame checked performance
exported a 35.8 MB PCM-hashed WAV (`3fd56c…`); the 10,000-frame final take exported a 100.5 MB WAV
(`5496b5…`). All **20** capability cells (5 sizes × 4 depths) preserved incident state, the hot
cluster, the action affordance and the exit path. Three real-PTY probes exited 0 with the
alternate screen restored; an observe-only probe watched the compact incident report commit to
native scrollback while the live region animated. (Frame times vary a little run to run; the
semantic digest and audio hashes do not.)

These measurements are host-, size- and input-specific, and the headless runs are unpaced. The
simulation is fictional; nothing here is a claim about any production distributed system, and
the score is a response to a hand-written semantic map, not a comprehension of distributed
systems. `Cargo.lock` resolves `tag = "v0.4.0"` to
`c2f6483d92fe2b351e6cd50936a97d8cdf73cb79`.
