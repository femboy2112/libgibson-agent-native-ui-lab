# FRICTION — Project Theseus (LibGibson v0.4.0 consumer)

Observations are classified into the five buckets the experiment must keep separate:
**machine conformance**, **human recognition**, **app bugs**, **API friction**, and
**upstream defects**. Every item cites the public API or file it was observed at.
No LibGibson source was modified.

---

## API friction

### F1 — Three of eight axes are unreachable from a reference song
**Mechanism.** `ReferenceSong::extract_fidelity` constructs its `CoverMap` with
`form: None`, `groove: None`, `orchestration: None` unconditionally
(`reference_song.rs:232,242,244` and `:453,463,467`). Harmony is populated only from
a `DerivedHarmony`, and `BassFigure` only from a voice literally named `bass`.
**Impact.** An instrument that decomposes identity by axis can only ever *observe*
Motif, Riff, Harmony (contour/loop) and Bass from a reference — five of eight slots —
in every fidelity regime. The app prints `UNKNOWN` for the rest rather than faking a
removal. Filed upstream (see upstream defects).

### F2 — No fidelity-profile projection for a generated source
**Mechanism.** `CoverMap::extract` (`cover.rs:292`) and `CoverMap::extract_lane`
(`cover.rs:314`) take a `CoverSpec` (a binary axis subset), while
`ReferenceSong::extract_fidelity` (`reference_song.rs:393`) takes a named
`CoverFidelityProfile` preset. There is no public method that re-projects an existing
`CoverMap` / generated `Composition` under a changing named profile.
**Impact.** The four fidelity regimes can drive the reference path only; the richer
generated lane path (which can populate more axes) cannot be re-profiled. The app
therefore exercises the lane path only in tests (`adversarial_06`).

### F3 — One `HarmonyRelation`, two axis names
**Mechanism.** `CoverFidelityProfile` has a single `harmony: HarmonyRelation`; the
`harmony_axis: HarmonyAxis` label (`Contour` / `Loop` / `Both`) is what makes the
HarmonicContour and HarmonicLoop axes independently meaningful.
**Impact.** Toggling HarmonicLoop without setting `harmony_axis = Loop` appears to do
nothing. The app maps the two toggles onto the label, but the affordance is not
discoverable from the axis names.

### F4 — `Invalid` refusals carry no structured axis attribution
**Mechanism.** `CoverError::Invalid(&'static str)` carries only a human-readable
reason. `CoverError::Rejected(Box<CoverAdmission>)` carries per-axis checks, but the
`cover()` refusals in this experiment are all `Invalid`.
**Impact.** To highlight the involved axes the app keyword-matches the reason string
(e.g. "bass" → `BassFigure`, "harmony" → both harmony axes), with a fallback to the
pinned axes. That is app-side guesswork over an opaque error payload.

### F5 — Semantic-only text styling; RGB needs a full-body raster
**Mechanism.** `Element` exposes semantic `tone(Tone)` / `emphasis(Emphasis)`, not
direct `fg(Color)`. Rich RGB is only available by building a `Node::raster(Surface)`.
**Impact.** The reactor must replace the whole body rather than being an element in
the layout; this is a deliberate boundary (it keeps text consistent with the world
skin) but it does mean there is no way to tint a single styled `text` node with RGB.

### F6 — Package name vs crate name
**Mechanism.** The dependency is `libgibson`, but the Rust crate path is `gibson`
(`[lib] name = "gibson"` upstream). `use libgibson::…` does not resolve.
**Impact.** Initial import errors until the rename is discovered. Minor, but the
first thing a consumer hits.

### F7 — Fidelity module is private but re-exported
**Mechanism.** `cover_fidelity` is a private module inside `cover`, with its items
`pub use`-re-exported from `cover`. IDE suggestions may point at the private path.
**Impact.** A "module is private" error when importing the suggested path.

---

## App bugs (found and fixed during this experiment)

### A1 — Help overlay trapped the keyboard
The help overlay was a library `modal`. `UiRuntime` consumes *all* keyboard/paste
events while a modal is active, so `q` did not quit and only `Esc`/Ctrl-C escaped. A
lone `Esc` arriving fused to the next byte in a fast input burst never surfaced as
`Escape`, so the operator could be stranded inside help. **Fixed** by making help a
non-trapping `panel`; `Esc` now closes help when open and quits otherwise.

### A2 — Console reported `PRESERVED` for unreachable axes and never `PURGED`
Axis knowledge was read from the *active* (toggled) profile, so a disabled axis
became `Free`/`Unknown` instead of `PURGED`, and an enabled-but-unobserved axis read
`PRESERVED`. **Fixed** with a ceiling extraction that records what the regime *could*
know; `THESEUS INDEX` is now preserved-reachable / reachable.

### A3 — PTY drain died on `EINTR`
`resize` delivers `SIGWINCH`, which interrupts a blocked `read`; the harness treated
`ErrorKind::Interrupted` as fatal, stopped draining, filled the PTY buffer, and made
the app block on write (a false hang). **Fixed** by retrying `Interrupted`.

---

## Upstream defects

### U1 — Queued input stranded after resize under PTY backpressure
**Reproduction.** Over a real PTY, send the same ~90-key burst, then either resize
mid-backlog or not, then `q`:

| Variant | Runs | Result |
|---------|------|--------|
| burst without resize | 4/4 | clean quit (~7–8 s) |
| burst with resize mid-backlog | 4/4 | **hang** after 45 s |

A second lone `q` still fails; Ctrl-C is received and exits. So the queued byte is
stranded at the event-source layer, not lost to backpressure.

**Status.** `femboy2112/libgibson#15` (open) already documents and diagnoses this;
the owner isolated it to a key sharing one crossterm/Mio readiness batch with
`SIGWINCH` (upstream `crossterm-rs/crossterm#1126`, fix PR #1128 unmerged, no fixed
release yet). This experiment adds an independent consumer discriminator
(resize vs. no-resize) and keeps
`pty_resize_under_input_backpressure_regression` `#[ignore]`d as the acceptance test.
A comment was posted on #15; no new issue was filed for this.

---

## Machine conformance

* `CoverConformance::check` and `PerformanceReceipt::measure_under(BAND)` pass for
  every covered cell tested; refusals are typed `CoverError::Invalid`.
* Determinism holds: fixed fixture + seed + target + fidelity ⇒ byte-identical JSON
  and audio hash.
* Conformance is not a claim about "same song"; it only means the invariants a world
  can check hold.

## Human recognition

* The motif-only cover reads as the tune and the zero-axis cover reads as new music
  to the author, by listening and reading the generated rolls. This is an informal
  judgement, **not** a controlled study, and no recognition metric is claimed.
