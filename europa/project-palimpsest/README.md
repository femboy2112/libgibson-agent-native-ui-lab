# Project Palimpsest

**An interactive git repository time machine / code archaeology workstation** —
git history rendered as a navigable temporal object, built as an *external,
read-only consumer* of **LibGibson v0.2.0** (released tag; no path
dependencies, no private APIs, no library modifications).

> "Git history became a place."

```
 PALIMPSEST ─ palimpsest-fixtur…            ◈ 877c6dd  refactor: rename src/core.rs…    152c · 6b · 3t │ HISTORY ATLAS
┬2023-12-28 ┬2024-01-04  ┬2024-01-11 ┬2024-01-18 ┬2024-01-25 ┬2024-02-01 ┬2024-02-08 ┬2024-02-15 ┬2024-02-22 ┬2
       ○─○○○─○○○○○○─○──○●──●─●─●──●─●─●─●─●──●●─●◉◉◉◉●●●●─●●─●●●●─●●●●●●─●●●●─●●●●●●●●●●●●●●●●●●●●●●●●●●●●◈┤ main
             │  ││─○──○─●─●─●──●●──●─●─●──●─●─●─●│─feature/strand-4
             │  │──○─○──●●──●─●─●──●─●─●─●─●─●─●│─feature/strand-3
             │  ───○─○─●─●──●─●─●─●──●─●─●─●─●─●│ feature/strand-2
             ─────○──○─○─●─●─●─●──●─●─●─●──●─●─●─ feature/strand-1
```

## Mission

Open an arbitrary local git repository and let a developer *understand how it
evolved*: a multiscale **time-lens** over the commit DAG where branches are
braided trajectories, merges visibly join histories, tags anchor, files are
persistent world-lines, blame is provenance fiber, and a diff is a local
section through history. Finalized artifacts (dossiers, diffs, reports) are
committed into **native terminal scrollback** while the live application
keeps running beneath — deliberately stressing LibGibson's
immutable-history/live-region architecture.

## Why this is an Europa-shot

It demands, in one terminal interface: a zoomable temporal topology canvas
(not `git log --graph`), continuous camera animation, per-line blame fibers
that "unfold backward" into history, rename-following file strata, an
editorial metrics report, a modal command palette, filtered lists with stable
keyed selection, and a responsive ladder down to ~40 columns — all against a
library that ships **no virtualization, no list/table widget, no responsive
breakpoints** (see issues [#38](https://github.com/femboy2112/libgibson/issues/38),
[#41](https://github.com/femboy2112/libgibson/issues/41),
[#42](https://github.com/femboy2112/libgibson/issues/42)).
Every one of those gaps had to be closed by bounded application policy — the
pain was the measurement.

## LibGibson version

**v0.2.0**, exact external dependency (no path override, no fork):

```toml
[dependencies]
libgibson = { git = "https://github.com/femboy2112/libgibson", tag = "v0.2.0" }
```

The crate's `[lib] name` is `gibson`, so code imports read `use gibson::…`.

## Architecture

```
src/
├── main.rs        entry: interactive loop (inline live-region default), dump capture
├── cli.rs         --key=value and --key value parsing, help
├── lib.rs         library surface (the binary is a thin shell; tests drive the lib)
├── theme.rs       SWISS_SIGNAL editorial palette (color/ansi16/mono) + glyph vocabulary
├── fixture.rs     deterministic in-process git fixtures (git2, seeded LCG, fixed clocks)
├── git/           read-only git intelligence (git2)
│   ├── repo.rs        open/discover/refs/tags/revparse, detached-HEAD & empty-repo safe
│   ├── history.rs     bounded commit index: braid lane scheduler, day-window key,
│   │                  density minimap, tier-A (delta count) lazy stats, tier-B LRU
│   ├── diff.rs        commit diffs with hunk model + intraline emphasis
│   ├── blame.rs       blame at a commit (BlameOptions::newest_commit)
│   ├── filelog.rs     file world-lines following renames (find_similar post-pass)
│   └── metrics.rs     sampled churn/author/branch-age/merge/concentration report
├── app/           state machine
│   ├── mod.rs         views, camera (px/day + t_center), pan animation, scrollback queue
│   ├── keys.rs        full key routing per input mode (5 modes)
│   ├── lens.rs        diff flattening + in-diff search
│   └── search.rs      keyed search (selection survives filtering)
└── views/         the visual grammar (all painted into Surfaces)
    ├── atlas.rs      HISTORY ATLAS: ruler, braids, jogs, labels, pin, minimap scrub
    ├── strata.rs     FILE STRATA: one file's world-line, event ledger
    ├── lens.rs       DIFF LENS: file ledger + hunk stream, syntax-lite emphasis
    ├── prov.rs        PROVENANCE: blame fibers; `u` unfolds a line into its history
    ├── health.rs      editorial metrics document (scrollable)
    ├── chrome.rs      masthead, command line, help/file overlays (stack + dim + panel)
    ├── widgets.rs     if-empty printing, bars, sparklines, kv rows, scrollbars
    └── mod.rs         responsive layout policy (160→42 columns)
```

Bounded by policy everywhere: the commit index is capped (`--limit`, default
5000), visible-window extraction is O(window), tier-A stats resolve ≤24
commits/frame, tier-B line stats are computed on selection and LRU-cached
(512), churn is sampled and marked `≈` when extrapolated.

## Modes and views

| Key | View | What it is |
|---|---|---|
| 1 | HISTORY ATLAS | braided topology, time ruler, selection pin, density minimap scrub |
| 2 | FILE STRATA | one file's world-line with renames, hot periods, event ledger |
| 3 | DIFF LENS | file ledger + hunk stream, line numbers, intraline emphasis |
| 4 | PROVENANCE | blame fibers; `u` unfolds the selected line into its origin history |
| 5 | REPOSITORY HEALTH | editorial metrics: authors, churn, largest commits, branch ages |

Overlays: `?` help · `f` file browser (filterable, keyed focus restoration) ·
`:` command palette (`goto <rev>`, `file <p>`, `author <n>`, `zoom <n>`,
`export`, `report`, `help`, `quit`) · `/` incremental search with hit halos.

## Controls

```
1..5        switch views            j/k or ←→  step through commits (camera follows)
J / K       jump one lane           , / .       pan the camera through time
+ / -       zoom the time-lens      z0..z5      lens presets: eon → day
Enter       zoom in: commit → lens  Esc / q     back out one level (q quits in atlas)
f           browse files            /           search history (n/N cycles hits)
c           commit dossier to scrollback         E           export diff to scrollback
R           commit health report to scrollback   u           unfold provenance fiber
Tab         cycle lens pane focus   g / G       newest / oldest commit
r           recenter camera         Ctrl-C      quit
```

## Build and run

```sh
cargo build --release
# open a repository (discovers upward from cwd)
./target/release/palimpsest --repo /path/to/repo
# or a deterministic synthetic fixture (built in-process with git2, then cached)
./target/release/palimpsest --demo            # medium (~150 commits, 5 lanes)
./target/release/palimpsest --demo=tiny       # ~24 commits
./target/release/palimpsest --demo=large     # ~2,900 commits (first build takes minutes)
```

Default embedding is **inline**: the live region sits at the bottom of the
terminal and finalized artifacts are inserted into native scrollback above it
while the app keeps running. `--fullscreen` opts into the alternate screen.

Non-TTY stdout is refused with guidance (exit code 2) — an interactive LibGibson
app on piped output otherwise renders nothing (see
[issue #46](https://github.com/femboy2112/libgibson/issues/46)).

## Deterministic / headless commands

```sh
# one-frame capture; pipe through the flattener for the visible screen
./target/release/palimpsest --demo --dump --width=120 --height=40 \
    | python3 scripts/flatten_dump.py 120 40

--view=atlas|strata|lens|provenance|health   initial view
--commit=<rev>            jump to a commit (rev or abbreviated oid)
--file=<path>              open strata/provenance for this path
--mono / --ansi16          shape-only / 16-color modes (palette AND ColorDepth)
--limit=N                  commit index bound
--profile                  load/render statistics to stderr
```

`--dump` also demonstrates the scrollback contract end-to-end: frame →
`───── palimpsest artifact ─────` insertion → live-region repaint.

## Tests

```sh
cargo test                 # 80 tests: 31 lib unit, 3 bin unit, 46 integration, 3 doc-tests
cargo test --release
python3 scripts/pty_smoke.py                    # real PTY: frames, keys, resize, scrollback, clean quit
python3 scripts/pty_smoke.py --profile=medium
```

Integration suites assert **actual rendered output** — the wire bytes are
replayed through a terminal model and the visible grid is checked:

- `tests/render_tests.rs` — all six responsive targets, all five views,
  mono/ansi16 escape-class checks, scrollback artifact reaching output,
  truncation notices, empty/detached repositories
- `tests/interaction_tests.rs` — key-driven behavior: keyed selection
  stability under filtering, browser focus restoration, lane jumps, palette
  commands, fiber unfold + origin jump
- `tests/model_tests.rs` — fixture determinism (byte-identical rebuilds),
  rename chains, blame mapping, window bounds, camera round-trips
- `tests/wire_probe.rs` — differential-frame composition: scripted sessions
  snapshot the screen after every frame; asserts no stale-cell chimera

## Capability fallbacks

- `--mono` — every semantic (mass, merge, tag, selection, search-hit, lanes)
  survives through **shape alone** (○ ● ◆ ◉ ◈ ◎, rail/jog/pin glyphs); no
  color codes are emitted at all
- `--ansi16` — palette and `ColorDepth::Ansi16` quantization together
- color (default) — ANSI-256 palette; terminal capability detection via the
  session, depth set explicitly in dump modes
- Box-drawing vocabulary is single-width everywhere (unit-tested), so
  ambiguous-width terminals keep the braids aligned

## Known limitations (accepted, by design)

- Search covers summary/author/oid; file paths are reached through the
  browser/`--file` instead
- Hottest-files churn is **sampled** across the loaded window and marked `≈`
  when extrapolated — no manufactured certainty
- `--demo=large` builds ~90k loose objects in-process on first run (minutes,
  syscall-bound on slow filesystems); deterministic, so it is cached and
  reused thereafter
- `t_of` (column → time) is exact at integer px/day scales only; the ruler
  never reads through this inverse
- The interactive path targets Linux terminals (raw mode, CSI); no
  Windows/macOS validation

## Known project-local bugs

- Tag labels share the inter-lane row with merge jogs; occupied cells are
  skipped (label text can interleave with rail glyphs — visible as e.g.
  `◆v0.3.0` pierced by a jog)
- Lens hunk auto-scroll only recenters when `hunk_cursor > 0`; stepping into a
  file from its top can land mid-hunk without recentering
- `-h` is bound to `--help`, so `--height` has no short flag
- The dump artifact demo always emits one dossier after the frame (capture
  evidence by design, but it makes `--dump` non-minimal)

## LibGibson issues discovered from this project

Filed by this attempt (see `EXPERIMENT_REPORT.md` for full evidence):

- [#44](https://github.com/femboy2112/libgibson/issues/44) — `Node::rule`
  requires a turbofish for the common untitled case
- [#45](https://github.com/femboy2112/libgibson/issues/45) — no safe
  preformatted pathway for scrollback text insertion (WordWrap re-flows
  aligned tables; NoWrap node path silently truncates)
- [#46](https://github.com/femboy2112/libgibson/issues/46) — interactive
  `Context` succeeds on non-TTY stdout and then silently renders nothing

Pre-existing campaign issues this implementation validated by paying the cost:
[#38](https://github.com/femboy2112/libgibson/issues/38) (windowing),
[#39](https://github.com/femboy2112/libgibson/issues/39) (scroll-to-item),
[#40](https://github.com/femboy2112/libgibson/issues/40) (focus membership;
this app keeps focus app-owned and uses `FocusRing` only for modal
capture/restore), [#41](https://github.com/femboy2112/libgibson/issues/41)
(list widgets), [#42](https://github.com/femboy2112/libgibson/issues/42)
(responsive layout).

No LibGibson *defect* was proven during this work — a suspected differential
renderer corruption was traced to this project's own test terminal model
(missing `CSI G` handling) and is documented honestly in the report.

## Performance (measured, `--release`, Linux)

| Fixture | Commits | Lanes | Index load | Frame render | First-frame bytes |
|---|---|---|---|---|---|
| tiny | 24 | 3 | 0–1 ms | ~525 µs | ~7.2 KB |
| medium | 152 | 5 | 3 ms | ~506 µs | ~7.4 KB |
| large | 2,890 | 9 | 49 ms | ~453 µs | ~7.5 KB |

Render cost is O(visible window), not O(history): the large fixture renders
its atlas as fast as the tiny one. Artifacts insert above the live region for
~7.7–7.9 KB per commit. Full numbers and methodology: `EXPERIMENT_REPORT.md`.

## Captures

`assets/captures/` holds flattened visible-screen captures (ANSI replayed
through a terminal model), all byte-reproducible:

```sh
./target/release/palimpsest --demo --dump --width=160 --height=50 \
    | python3 scripts/flatten_dump.py 160 50
```

Regenerate any capture with the flags named in each file's header line.