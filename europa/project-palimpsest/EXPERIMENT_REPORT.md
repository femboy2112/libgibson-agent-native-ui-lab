# Project Palimpsest — LibGibson v0.2.0 external stress test

Campaign receipt for the git time-machine / code archaeology workstation
developed as an external holdout against the released LibGibson **v0.2.0**
tag. The application is preserved verbatim under `europa/project-palimpsest/`;
this document records what actually happened.

## Goal

Attempt the "impossible" visual idea: represent git history as a **navigable
temporal object** rather than a vertical log — a multiscale time-lens where
branch lineages are horizontal braids, merges visibly join histories, tags
anchor, files are persistent world-lines (strata), blame lines "unfold
backward" into provenance fibers, and a diff is a local section through the
topology. Combine that with repository navigation, search, temporal
scrubbing, metrics, and **finalized artifacts committed into native terminal
scrollback while the live app keeps running underneath** — the deliberate
stress of LibGibson's immutable-history/live-region architecture. Target
reaction: *"How the hell is this all one terminal interface?"* — on a library
whose lower-level `Node` path leaves list windowing and responsive policy to
applications. This project does not evaluate the experimental `gibson::ui` path.

## LibGibson surface exercised

Materially used (imported and load-bearing in the shipped application):

- **`gibson::context::Context`** — `inline()` / `fullscreen()` / `headless()`
  construction; `set_root` per frame; `render()`; `poll_event` loop with
  bounded waits; `take_output` for capture; `set_color_depth`;
  `ctx.session.is_tty` guard; `ctx.stats()` for `--profile`.
- **Native scrollback / live-region semantics** —
  `insert_text_before_live`, `insert_rich_text_before_live`,
  `insert_node_before_live`; `InsertStrategy` accounting observed via stats
  (fast path vs repaint fallback); artifacts inserted while the live region
  stays interactive (proven in the PTY smoke and the dump pipeline).
- **`gibson::node::Node`** — `col`/`row`/`stack` (base + dim veil + offset
  card overlay pattern), `panel`, `rule`, `text`, `rich_text_wrapped`
  (WordWrap and NoWrap), `surface(Arc)`/`raster`, `offset`, `percent_*`,
  `flex_grow`, width/height builders. (Not used: `viewport` — this project
  implements windowing itself; `spinner`, `text_input`, the experimental
  `gibson::ui` module.)
- **`gibson::surface::Surface`** — the core painting substrate: `print_str`,
  `set_cell` (wide-glyph overwrite invariants relied on), `get`,
  `fill_rect`, `draw_border`, `blit_transparent_clipped` (raster compositing).
- **`gibson::cell`** — `Cell`, `Glyph`, `Style`, `Color`, `RichText`, `Line`,
  `Span`; style `overlay` composition for syntax-lite emphasis.
- **`gibson::capability`** — `ColorDepth` quantization (Ansi256 default,
  Ansi16 for `--ansi16`/`--mono`); verified at the byte level in tests (no
  `38;5;`/`38;2;` escapes in the constrained modes).
- **`gibson::input`** — `poll_event`, `Event::{Key,Resize,Paste}`,
  `KeyCode`, `KeyModifiers`, `KeyEvent::char` for paste replay.
- **`gibson::focus::FocusRing`** — used for modal capture/restore only;
  membership is static (see issue #40), so pane focus is app-owned.
- **`gibson::clock::ease_out`** — camera pan/zoom animation.
- **`gibson::scheduler::RenderStats`** — frame/dirty/byte accounting surfaced
  through `--profile`.
- **Headless capture** — `Context::headless` is the backbone of the entire
  test suite (rendered-output tests, wire probes).

Not exercised: `gibson::ui` (BuildCx/PresentationCx/skins/motion),
`canvas`/`raster`/`raster3d`/`raster_fx`, `scene`, `story`, `field`,
`particles`, `viewport::ViewportState`, truecolor depth.

## What worked surprisingly well

Concrete, measured:

- **Headless capture is a first-class test harness.** `Context::headless` +
  `take_output` let the test suite assert on *actual wire bytes*, and a small
  terminal model (cursor addressing, SGR, `CSI K`/`CSI L`) turns those bytes
  into the visible screen. The 89-test suite includes rendered-output checks.
  This is the single best API decision in v0.2.0 for consumers.
- **The differential renderer is byte-honest.** A scripted-session probe
  replays multi-frame sessions (atlas → lens → atlas → health → report →
  atlas) and snapshots the screen after every frame: headers compose cleanly,
  no stale-cell chimera, and the selection marker never smears
  (`tests/wire_probe.rs`). A *suspected* corruption during development
  turned out to be this project's own terminal model ignoring `CSI G` — the
  library was right; the test was wrong.
- **Insertion above the live region genuinely keeps the app interactive.**
  The PTY smoke commits a health report into native scrollback and the live
  region continues rendering and accepting keys without a flicker.
- **Atlas render cost is bounded by the visible window.** The measured
  fixture runs cover 24, 152, and 2,890 indexed commits while initial frames
  remain sub-millisecond (see performance table). The engine's diff accounting
  (`RenderStats`) made this measurable without instrumentation of our own.
- **The cell model's wide-glyph invariants held everywhere.** Box-drawing
  braids across five views never split a glyph at a viewport edge; the
  glyph-vocabulary unit test pins single-width expectations.
- **Health row columns are measured in display cells.** A real-repository
  capture exposed a long commit summary running into its right-aligned path,
  author, and age metadata. The summary now truncates before a two-cell gap;
  narrow layouts shorten the metadata or use a single column. A rendered
  regression covers long Unicode summaries at 120, 80, 42, and 20 columns.
- **Color-depth quantization is centralized.** Palette code expresses intent
  once (`Color::Ansi256`/16/Reset); `set_color_depth` handles the ladder, and
  the mono mode emits zero color escapes — verified by byte-class tests.

## Friction encountered

### Application-specific friction

- **git2 0.19 API shape** (this project reads git via git2, by mission
  choice): `Patch::hunk` returns `Result<(DiffHunk, usize)>` with the hunk
  *first* and line count second; rename detection is a `find_similar`
  post-pass, not a diff option (a pathspec-filtered diff cannot pair both
  sides of a rename — the strata view must diff whole commits and match
  deltas); blame-at-a-commit requires `BlameOptions::newest_commit`. All
  resolved; documented in code.
- **Deterministic fixture cost.** Building a ~2,900-commit fixture in-process
  writes ~90k loose objects: minutes of syscall time on a slow `/tmp`. The
  fixtures are deterministic, so a marker-file cache reuses them; the
  one-time cost is stated in the README.
- **Terminal-model fidelity is the hard part of testing TUIs.** Two
  independent bugs in this project's own flattener (missing `CSI G`; OSC
  parsing) produced phantom "corruption" that cost real investigation time.
  The lesson is encoded in `tests/wire_probe.rs` so the harness itself is
  now under test discipline.

### Generic LibGibson ergonomic friction

- The absence of built-in virtualization (#38), list/table widgets (#41),
  scroll-to-item viewports (#39), and responsive breakpoints (#42) forced
  ~five reimplementations of windowing/selection/scroll-keep-visible across
  the five views in the **lower-level Node path this application uses**. This
  is the measured pain those issues predict for that path; the experimental
  `gibson::ui` runtime is outside this experiment.
- `FocusRing` cannot shrink or reshape (#40), so per-view pane focus cannot
  live in the ring; this app tracks focus itself and uses the ring only for
  modal capture/restore.
- `Node::rule(None, …)` does not compile without a turbofish (filed as #44).
- No safe preformatted insertion path for scrollback text (filed as #45) —
  this application's *defining* feature class (committing diffs/ledgers)
  had to be width-bounded by policy.
- Interactive `Context` on non-TTY stdout silently renders nothing (filed as
  #46). This is distinct from the rootless-render observation in #48. The
  application checks `ctx.session.is_tty` and exits with a useful error.

### Probable LibGibson defects

- None currently suspected. The one candidate raised during development
  (apparent differential-frame corruption) was disproven and attributed to
  this project's test harness.

### Proven LibGibson defects

- None found. No LibGibson behavior was contradicted by its documentation or
  reasonable contract during this work.

## GitHub issues filed

Consolidated ergonomics & architecture analysis (silent failure modes, the
declarative/imperative seam, rendered-output testing):
[#48](https://github.com/femboy2112/libgibson/issues/48). Its initial text
overstated missing onboarding guidance: the released v0.2.0 README already
documents the `libgibson` package / `use gibson` import and a `run_once` loop.
A clarifying comment on the issue corrects those points and separates
non-TTY behavior from rootless rendering; this report does not adopt the
original overclaims.

Filed by this attempt against `femboy2112/libgibson`:

| # | Title | Link | Mechanism / impact | Workaround used |
|---|---|---|---|---|
| 44 | `Node::rule` requires a turbofish for the common untitled case | [#44](https://github.com/femboy2112/libgibson/issues/44) | `Option<impl Into<String>` in argument position leaves `None` uninferable (E0283); every plain separator needs `None::<String>` | Turbofish at each untitled-rule site |
| 45 | No safe preformatted pathway for `commit_text` / `insert_text_before_live` | [#45](https://github.com/femboy2112/libgibson/issues/45) | WordWrap is hard-coded; aligned tables wider than the terminal are re-flowed (columns destroyed) while the NoWrap node path silently truncates (data loss) | Artifacts bounded to terminal width; multi-line RichText for structure |
| 46 | Interactive `Context` succeeds on non-TTY stdout and silently renders nothing | [#46](https://github.com/femboy2112/libgibson/issues/46) | `Context::new` succeeds off-TTY; `render()` suppresses all frames and `poll_event` returns `Ok(None)` forever — consumers appear hung on a blank screen | Manual `ctx.session.is_tty` guard with actionable error (exit 2) |

Pre-existing campaign issues whose lower-level API cost this implementation
encountered: #38, #39, #40, #41, #42 (see "Friction encountered"). A
candidate FocusRing-membership issue was deliberately *not* filed — #40
already covers the mechanism.

## Performance / scale observations

Measured with `--profile` on the `--release` binary (Linux, this development
machine), one `--dump --frame-only --width=120 --height=40` render per cached
fixture after index load (single-run timings; build/setup excluded):

| Fixture | Commits | Lanes | Index load | Atlas frame | First-frame bytes |
|---|---:|---:|---:|---:|---:|
| tiny | 24 | 3 | 0 ms | 391 µs | 9,096 B |
| medium | 152 | 5 | 2 ms | 397 µs | 9,593 B |
| large | 2,890 | 9 | 36 ms | 346 µs | 8,364 B |
| local checkout | 14 | 1 | 0 ms | 342 µs | 6,817 B |

- Index→atlas decoupling: window extraction uses a binary search over a
  time-sorted key plus O(visible commits) painting. Frame time may vary with
  terminal dimensions, scene complexity, and repository shape.
- Tier-A (changed-path counts) resolves lazily within a per-frame budget of
  24 delta enumerations. The health report separately samples at most 400
  commits to compute churn and actual changed-path counts for the largest
  sampled commits; it labels partial samples. Tier-B (line counts) is
  per-selected-commit with a 512-entry LRU. `--limit` caps the index
  (default 5,000) and the truncation is flagged in the UI.
- First fixture builds: tiny ~0.1–1 s, medium ~0.7 s, large ~3–4 min
  (syscall-bound loose-object writes; one-time, cached deterministically).
- The `--limit=200` truncation path was exercised on the large fixture: 200
  rows loaded, honest truncation flag, UI notice rendered.
- A non-fixture smoke opened the local Palimpsest checkout read-only; its
  14 indexed commits rendered under the same 120×40 capture path.
- Provenance's blame origin identifies the actual author of a line. Its
  optional unfolded stations display later changed rows within one numbered
  line as **positional context**, never as proof that those commits modified
  the selected text. An append-only regression guards against hunk context
  falsely attributing a first line to a later commit.
- History indexes HEAD ancestry; health resolves branch-tip ages outside
  that walk separately, marks those tips `*`, and bases relative ages on the
  newest known tip. Unreachable branch commits do not appear in the braid.

## Capability & fallback observations

- **ANSI-256** (default) and **ANSI16** (`--ansi16`) both verified at the
  byte level: constrained modes emit no `38;5;`/`38;2;` sequences (tests
  assert the escape classes).
- **Mono** (`--mono`) is a first-class mode: the entire braid grammar
  (mass/merge/tag/selection/search-hit/lane identity) is carried by shape
  alone; a unit test pins the glyph vocabulary to single-width codepoints so
  ambiguous-width terminals stay aligned.
- **TrueColor was not exercised** — honest boundary; the app never emits
  `Color::Rgb`.
- **Non-TTY degradation** is refused loudly (issue #46 workaround) rather
  than silently; `--dump` is the supported headless path.
- Resize mid-session is exercised on a real PTY (shrink to 56×22 → compact
  identity code; grow back → full title restored), in addition to the
  six-geometry responsive matrix in the render tests.

## What this experiment demonstrates

Bounded claims only:

- A consumer can build a genuinely unusual, information-dense instrument —
  braided temporal topology, strata, fibers, an editorial metrics document —
  on the released v0.2.0 substrate, with **no** engine modifications, and
  keep every view legible from 160 columns down to 42.
- The scrollback/live-region contract holds under the intended flagship
  load: artifacts flow above a still-interactive live region, and the
  differential renderer's multi-frame output composes faithfully (proven by
  wire-level replay tests, not assertion of intent).
- Headless capture plus a terminal model is a sufficient methodology for
  testing rendered TUI output — this suite never merges to "trust me"
  assertions.
- What one application demonstrates about *general* UI expressivity is
  exactly what this one application demonstrates; no broader claim is made.

## What remains unproven

- Repositories beyond ~2,890 commits and ~1300 files/tree (the mission's
  "thousands of commits" bar is met, but 100k-commit monorepos, packed
  object stores, and pathological merge topologies are not covered).
- TrueColor terminals; Windows/macOS terminals; terminals without `CSI L`
  (the repaint fallback path is exercised headlessly but not on exotic
  hardware).
- Hours-long interactive sessions: memory retention under sustained use was
  not measured (the prior corpus tracked this; this project did not).
- Mouse input (out of scope for v0.2.0's input surface).
- The experimental `gibson::ui` runtime (BuildCx/skins/motion) — untouched by
  this consumer, so this experiment says nothing about it.
