# Agent-native UI expressivity: final research report

**Verdict: CORROBORATED within the tested domain, not universally proved.**
Three planned consumers and three post-freeze holdouts were built outside
LibGibson using its public API. No experiment required a LibGibson source patch,
private renderer access, raw ANSI, or a new core widget. No generic primitive
was promoted. The provisional instrument IR remained external and unchanged
through the holdouts. Project maturity remains **ENGINEERING ALPHA**.

This result is evidence for a compositional visual substrate. It does not prove
that every professional interface is easy, that every future interface is
representable, or that a model may safely execute arbitrary generated code.

## Provenance and gates

| Milestone | Exact record |
|---|---|
| Approved flagship tip | `11d2cca12430f68eb8fa3de3231827e1176e6c27` |
| Flagship PR #14 ordinary merge | `e5ede0a4ab8ff52caa567ee10d45824e42c1ebc6` |
| First green exact-main run | [35960000454](https://github.com/femboy2112/libgibson/actions/runs/35960000454): Rust, bindings, sanitizers, Go, PTY executed and passed |
| Refreshed research dossier commit | `8fe58aec3320e8f6b413718c4f352ab9df96fad9` |
| Research PR #16 ordinary merge | `3d9117d4bfa45f7d890bff12d40879a8d2bb6a47`; exact-main run [35961022598](https://github.com/femboy2112/libgibson/actions/runs/35961022598) passed |
| External preregistration | `4633096`, before implementation |
| Planned consumers / frozen helper and IR | `1d1d27286b50c752527021697c14fd8a32ccd0ea`; [CI 35962374210](https://github.com/femboy2112/libgibson-agent-native-ui-lab/actions/runs/35962374210) passed |
| Freeze receipt | `af812d172b96298d9b711b5be2145bd08ce38747`; [CI 35962407150](https://github.com/femboy2112/libgibson-agent-native-ui-lab/actions/runs/35962407150) passed |
| Holdout implementation | `15a67ebfbbf8365d24f33107d98196cb9eed761b` |

**FROZEN_LIBGIBSON_SHA:** `e5ede0a4ab8ff52caa567ee10d45824e42c1ebc6`.
**FROZEN_LAB_HELPER_IR_SHA:** `1d1d27286b50c752527021697c14fd8a32ccd0ea`.
The Cargo Git revision never moved. The later LibGibson dossier merge changes
only docs. This is a campaign freeze, not a stable-API or SemVer promise.

[FREEZE.json](FREEZE.json) hashes 21 existing source/helper/test/dependency files.
The verifier checks both working bytes and bytes at the frozen commit. It passed
on the completed implementation and rejected a deliberate helper edit in a
throwaway clone. Only new holdout files and additive module exports were used.
Neither the renderer nor the provisional schema studied for the holdout exam.

PR #14 preserved the flagship and D1/D2/D5 fixes. Issues #7, #8 and #9 closed;
#10, #11 and #15 remain open. The docs-only research branch was recreated from
the merged main without rewriting the old research ref. Five forbidden control
bytes and tab-corrupted notation were repaired; a scan of the original commit
was the negative control. The refreshed dossier retains non-binding status.

## Planned experiments on first external contact

All consume the same finite 18-second, four-agent fixture: one goal, concurrent
workers, failed tool, local recovery, permission, artifact and synthesis. Fifteen
semantic event families are present. Explicit actions are separate ordered
records. Presentation cannot reorder source events or silently decide permission.
The prerecorded default approval is fictional fixture policy, not real authority.

| Consumer | Observed result | Boundary |
|---|---|---|
| Manga | Concurrent panels/gutters, failure-crossing repair seam, permission, synthesis, reflow/focus | App owns comic grammar; unforeseen repair stitching is spent within-experiment evidence |
| Reactions | NONE/RESTRAINED/HEAVY × constellation/paperwork retain full semantic equality; useful explicit scope and permission actions | Original terminal-native jokes; no copyrighted asset library or semantic-understanding claim |
| Instruments | Validated runtime specs dynamically choose dependency/timeline/source/decision views, mount/patch/unmount, retain focus and emit actions | Six-form static IR; effects/nonzero animation programs deliberately rejected; no generated native code/model service |

**OBSERVED:** all three succeeded without core patches on first contact with the
pinned library. Their initial implementations were not bug-free. Actions-only
replay missed focus, one action quota silently dropped decisions, a layout bound
underflowed, and a custom mounted instrument could be focused without appearing.
Those consumer defects were corrected before freeze and their counterexamples
are retained. “First contact” is not a claim of first-draft success.

Full-Snapshot comparison across all three checks both default behavior and
explicit denial. A no-action control differs. Denial remains effective after
the scripted default and withholds the artifact. Resize/paint cannot change the
semantic snapshot. Reaction mode/metaphor form a complete 3×2 configuration
comparison, not six independent sources. Replay configuration is explicit;
recorded application keys include focus-only input, not just emitted actions.

The instrument port owns authority and limits. It admits no Context/renderer,
file/network tool, native code, or terminal escape access. Byte bounds precede
JSON parsing; unknown fields, invalid IDs/types/targets/subscriptions and
node/depth/raster/history/effect/rate violations reject atomically. Seed/fallback
requirements and lifetime focus-ID bounds are exercised. This is an experimental
consumer policy, not a security proof or general generated-UI schema.

## Reconciliation and promotion

[The detailed synthesis](CROSS_EXPERIMENT_FRICTION.md) and
[friction ledger](FRICTION_LEDGER.md) preserve each attempted public route.

- **ERGONOMIC INCONVENIENCE:** ID-to-domain-action maps, availability checks,
  responsive policy, and C's dynamic focus registry/restoration.
- **HARNESS-SPECIFIC:** trace completeness, comic/reaction geometry, chosen
  lifecycle policy and consumer composition corrections.
- **DELIBERATE SAFETY BOUNDARY:** bounded declarative admission, no generated
  effects/native code/terminal authority, finite sessions.
- **GENERIC PRIMITIVE GAP / EXPRESSIVE WALL:** none demonstrated by these probes.

G1 received repeated pressure, but existing FocusId/FocusRing plus explicit
application dispatch worked. A/B have fixed targets; C owns a dynamic namespace
and stronger authority/lifecycle policy. Repetition makes a promotion eligible
for consideration, not mandatory. No common missing contract was shown to
remove substantial duplication without importing domain policy. **No core PR.**

## Fresh selection and holdout verdicts

Selection happened after the committed freeze. A fresh context received the
neutral public API guide/source, not the dossier or its example list. Three of
its initial five proposals were excluded for actual conceptual overlap with
prior examples; replacements were requested without supplying that old list.
The complete pool, exclusions and timing are in [SELECTION](holdouts/SELECTION.md).
No implementation result determined eligibility.

Foldroom and Cuebox had fresh implementer contexts. A four-thread tool limit
prevented a third, so Weavebench reused its fresh proposer context. This weaker
design/implementation separation was recorded **before** its implementation.
Contexts share a model family and filesystem; isolation was instructional, not
OS-enforced blindness. There is no claim of independent model-family evidence.

| Holdout | Main discriminating evidence | Verdict |
|---|---|---|
| [Foldroom](holdouts/FOLDROOM.md) | Six rigid faces/four tabs; explicit flat/closed coordinate oracle, sampled hinge/edge invariants, true depth occlusion with persistent selection, two fold orders and replay | **PUBLIC API SUFFICIENT** for the finite folding workbench |
| [Cuebox](holdouts/CUEBOX.md) | Eight actors/four lights/twelve cues, explicit delayed-entrance branch, actual branch-dependent actor cells, modal capture/restoration and replay | **PUBLIC API SUFFICIENT** for the finite rehearsal |
| [Weavebench](holdouts/WEAVEBENCH.md) | Editable four-shaft draft, Boolean lift and cyclic-interval oracles, exact dependent crossings after edit, visibly different Mono over/under topology, replay | **PUBLIC API SUFFICIENT** for the finite draft editor |

Foldroom uses public Rasterizer/Camera/depth/RgbRaster/Surface. Cuebox composes
public Story/Scene/FocusRing with a Surface stage and ordinary Node overlays.
Weavebench uses public BrailleCanvas/Surface with consumer-owned draft semantics.
Thus all three exercised lower layers in addition to UI text; none required
extending the instrument IR. **This does not establish that the six-form IR can
express the holdouts.** The native public API is the tested escape route.

Two especially useful falsifiers landed in consumer code: Cuebox's initial
opaque actor mount erased its floor, and Weavebench's first tiny crossing geometry
made both strand orders identical in Mono. The first was corrected by composing
the floor inside the same Scene; the second by preserving larger crossing cells
and following selection/shuttle with a viewport. Actual-cell regressions detect
both. Neither correction changed the frozen API, helper or schema, or weakened
the prerecorded acceptance contract. No genuine API wall was hidden or rescued.

A post-implementation Cuebox review also tested the exact 14,999/15,000 ms branch
boundary and a modal spanning it through executable cell JSON: forbidden modal
input stayed blocked, focus returned, and repeated recordings matched. This is
an additional instrument, still from the same model family.

## Validation

LibGibson baseline: **574 tests = 226 unit + 348 integration**, plus 3 FX Lab
example tests. All requested new-main commands passed; exact command/exit/time
records are in [BASELINE_VALIDATION.json](BASELINE_VALIDATION.json):

```sh
cargo +1.98.1 fmt --check
cargo +1.98.1 clippy --all-targets --all-features -- -D warnings
cargo +1.98.1 test
cargo +1.98.1 build --release
cargo +1.98.1 build --examples
cargo +1.98.1 build --release --examples
RUSTDOCFLAGS="-D warnings" cargo +1.98.1 doc --no-deps
cargo +1.98.1 test --example fx_lab
RUSTUP_TOOLCHAIN=1.98.1 bash scripts/dev/bindings_smoke.sh --asan
git diff --check
```

Lab final local gates: **61 integration tests; 0 library unit tests**. Exact
commands, all exit 0 on Rust 1.98.1:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo build --release --bins
python3 scripts/verify_freeze.py
git diff --check
```

The 61 consist of fixture 6, input 1, cross-consumer 1, manga 5, reactions 7,
instruments 11, Foldroom 6, Cuebox 10, Weavebench 12, planned PTY reconstruction
1, and holdout PTY reconstruction 1. Count is an inventory, not a coverage score.

All four requested sizes are exercised. Planned consumers cover TrueColor,
ANSI16 and Mono; holdouts cover TrueColor and Mono plus release ANSI16 smoke.
Every consumer has deterministic frame/state replay and frozen **0 exact /
0 affected / 0 wire** checks. Geometry-only resize leaves semantic state intact.

Eighteen release PTY matrix runs (six consumers × three initial sizes/capabilities)
passed input/resize/exit/termios restoration. The two PTY integration tests
reconstruct terminal cells and verify actual selected objects/denial. Full
fixture auto runs for all three planned consumers also exited cleanly. Foldroom
recorded a live six-key edit sequence; Cuebox reached its 45-second curtain;
Weavebench had a live Mono smoke. These are short, bounded Linux sessions, not
long-session, throughput, descendant-process or terminal-platform certification.

Root inspected reconstructed TrueColor and small Mono holdout frames. That
inspection found the floor and crossing errors above. Scratch images/cell JSON
remain outside the repository. The scratch font needed explicit Braille-dot
rendering, and the image viewer does not faithfully represent every font/dim
attribute. No independent human usability or aesthetic acceptance is claimed.

Public holdout implementation CI: [run 35964133018](https://github.com/femboy2112/libgibson-agent-native-ui-lab/actions/runs/35964133018)
passed at exact implementation commit `15a67ebfbbf8365d24f33107d98196cb9eed761b`.
Formatting, strict Clippy, all tests, release binaries and the freeze verifier
actually executed successfully.

## Answers and claim ledger

| Question / claim | Status and answer |
|---|---|
| Planned consumers fit the unmodified public API | **OBSERVED**, after recorded consumer corrections |
| Main interaction frontier is G1 | **CORROBORATED** as repeated ergonomics here; not a demonstrated blocking contract gap |
| Provisional generated IR can remain external | **OBSERVED** for this bounded static schema and deterministic planner |
| Three post-freeze novel interfaces are buildable | **CORROBORATED** relative to the preregistered dossier and recorded selection limits |
| Safe lower-layer escape routes worked | **OBSERVED** in raster depth, cell composition and subcell crossings; no raw ANSI/private renderer |
| Mono, resize, input replay and frozen frames survive | **OBSERVED** for the exact fixtures and stated matrix |
| Safety policy made the accepted tasks impossible | **Not observed**; static IR rejects out-of-scope programs, while native consumer code retains public lower layers |
| Every ambitious instrument fits this IR | **UNVERIFIED**, not tested or claimed |
| First consumer drafts were correct | **REFUTED** by retained counterexamples |
| Ordinary professional UI is easy for external developers | **UNVERIFIED**; no independent human or comparative-effort study |
| Arbitrary UI is universally representable | **UNVERIFIED**, not implied by three holdouts |
| New core widgets were necessary for these domains | **REFUTED for these implementations**; no source change was required |

The smallest accurate identity remains a **cell-framebuffer terminal UI engine
with native scrollback/live-region semantics and experimental compositional
animation and software graphics**. The experiments corroborate its use as a
compositional visual substrate; they do not make it a universal runtime or a
stable platform. Domain semantics, trust policy and orchestration still belong
above the renderer.

We can truthfully say: **public API expressivity is corroborated across the
tested dynamic terminal interaction domain, including three post-freeze
interfaces absent from the research dossier.** In that bounded sense, "unknown
dope shit remains possible." The broader ease/universality thesis remains open.

## Next round

Prioritize issue #15 with a discriminating resize/input/backpressure experiment:
separate transport congestion, event draining and model updates, then prove
bounded input delivery under sustained rendering. Keep D6 API/MSRV/distribution,
D7 general retention/resources (#10), and D8 embedded ownership/restoration (#11)
visible and separate. Linux-only evidence does not resolve platform portability.

For further expressivity research, an independent external developer cohort
using larger changing target sets would test ergonomics better than another
same-model effect showcase. Do not add a general router or expand the IR merely
because this campaign predicted one. The current evidence earns restraint.
