# Visual Authoring for AI Agents

**You are a coding agent about to build a LibGibson application. Read this first.
It is ~400 lines and it will save you from reading 30,000 lines of source.**

LibGibson can render continuous, cinematic, spatial worlds in a terminal. It can
*also* render dashboards of bordered boxes. The substrate does not care which you
build — **you** decide, in the first five minutes, usually without noticing. This
guide exists because that decision is almost always made badly by default.

> **The failure this prevents.** Given an ambitious spatial prompt, a coding
> agent reaches for `panel()`, `row()`, `card()`, `status()`, draws a grid of
> bordered rectangles, fills them with text, picks a retro palette, and ships a
> 1980s-DOS dashboard — *even when the domain was a tunnel through time, a living
> dependency graph, or a planetary field.* This is **the Panel Farm**, and it is
> the nearest legible fallback for a model with no composition plan. Everything
> below is the plan.

This is not an aesthetic mandate. Panels are correct for some domains (see
[§7](#7-when-the-panel-farm-is-actually-right)). It is a method for making the
*right* choice on purpose instead of the default one by accident.

---

## 0. The cold-start route (read exactly this, in order)

You do **not** need to read the whole codebase. Read this, in order, and stop:

1. **This file** (`docs/AI_VISUAL_AUTHORING.md`) — the method. ~400 lines.
2. **One matching recipe** from `examples/recipe_*.rs` — pick by the decision
   tree in [§6](#6-decision-tree). ~100 lines.
3. **[`docs/UI_LAYER.md`](UI_LAYER.md), §"Local escape hatches and custom
   components"** (the `raw` / `surface` / `raster` / `Component` section) and
   §"Theme is a palette; Skin is a design grammar" — for the chrome layer. Skim,
   ~120 lines.
4. **Public API docs you reach from the recipe** — `Node::canvas`, `RgbRaster`,
   `Surface`, `Camera` — on demand.

Total: **~600 lines to a correct architecture.** If you find yourself reading
`src/` to figure out *what to build*, stop — that's an architecture question, and
it is answered here, not in the source.

---

## 1. The three-layer model

Build an application in three passes, **in this order**. Most panel farms are
built by skipping straight to layer 3.

### Layer 1 — DOMAIN / SEMANTIC WORLD

What is the thing? Model it as data, with **no pixels and no widgets yet**.

```
ExecutionHistory   DependencyGraph   SongIdentity
PlanetarySystem    RepositoryHistory   IncidentCascade
```

### Layer 2 — VISUAL METAPHOR / VISUAL LAW  ← the layer agents skip

Map *meaning* to *space*. Decide, in one sentence, how the domain's structure
becomes geometry. This is the layer that makes an app cinematic or leaves it a
dashboard. Examples of a visual law:

```
branch            -> a spatial fork that peels away
dependency        -> structural support / a bridge under load
time              -> depth (the camera travels along it)
identity          -> persistent geometry that deforms, never a new panel
uncertainty       -> diffusion / noise / desaturation
selection         -> camera attention (it moves to look)
causal importance -> scale and brightness
distance in state -> distance in the world
```

If you cannot state your visual law in one sentence ("*time is depth and forks
peel off the trunk*"), you do not have one yet, and you are about to build a
panel farm. **Stop and write the sentence.**

### Layer 3 — LIBGIBSON REALIZATION

*Now* choose primitives to realize the law:

| You need | Reach for |
|---|---|
| A pixel/cell world sized to the frame | `Node::canvas(\|rect\| -> Surface)` ([node.rs](../src/node.rs)) |
| An RGB field (half-block, 2px/row) | `RgbRaster::new(w, h*2)` → `.to_surface()` ([raster.rs](../src/raster.rs)) |
| Hand-placed cells / glyphs / borders | `Surface` (`print_str`, `set_cell`, `fill_rect`, `draw_border`) |
| A 3D/perspective world | `Camera` + `Rasterizer` ([raster3d.rs](../src/raster3d.rs)), or hand-roll a projector |
| Sub-cell line art | `BrailleCanvas` / `HalfBlockCanvas` ([canvas.rs](../src/canvas.rs)) |
| Finite, keyed visual transitions | `Scene` / `Story` + `SurfaceFx` |
| Semantic chrome & interaction | `gibson::ui` (`status`, `label`, `panel`, `modal`, …) |
| Design grammar (not just palette) | `Skin` ([UI_LAYER.md](UI_LAYER.md)) |

> **DO NOT BEGIN BY CHOOSING WIDGETS.** `panel()` / `row()` / `card()` are layer-3
> *chrome* primitives. If they are the first thing you write, you have skipped
> layers 1 and 2 and the Panel Farm is already forming.

---

## 2. The Dominant Visual Object rule

Before you write any UI code, answer one question:

> **What single object should occupy 60–90% of the user's perceptual attention?**

Examples:

```
time-travel debugger      -> a branching history tunnel
repository archaeology    -> the commit-history topology
music quotient            -> an identity transformation reactor
distributed incident      -> the dependency architecture under load
planetary system          -> the physical/spatial field
```

Then the second question:

> **What is the *minimal* text/chrome required to operate that object?**

That is your whole interface: **one dominant object, plus the least chrome that
lets a human drive it.** Not six co-equal panels. One subject, sparsely annotated.

### Primacy is not the same as a big rectangle

This is the trap that catches agents who *did* build a world. A large viewport
`Rect` is necessary but **not sufficient**. The world has primacy only when it is
simultaneously:

- **big** — it owns most of the frame, *and*
- **dense** — it has texture at the default size (not detail that only appears
  when you zoom in), *and*
- **the subject** — the information the user needs is *in* the world, not in a
  panel beside it.

A 76%-of-frame viewport containing a tiny strip of 1-column bars, with all the
readable numbers in a side panel, is a panel farm with a decorative picture. (It
has happened. See the audit.)

---

## 3. The Panel Farm (anti-pattern)

```
┌─────────┐ ┌─────────┐
│ METRIC  │ │ STATUS  │
├─────────┴─┴─────────┤
│ GRAPH               │
├─────────────────────┤
│ ANOTHER PANEL       │
└─────────────────────┘
```

**If your first sketch is a rectangle subdivided into many bordered rectangles,
STOP.** You may be building a dashboard where the domain wanted a world.

The Panel Farm is seductive to a coding agent because:

- the chrome vocabulary (`panel`, `card`, `row`, `table`) is the loudest, most
  documented, most example-covered part of the API;
- bordered boxes are an easy, "safe", always-compiles layout;
- it never requires you to invent a visual law (layer 2), so it feels faster.

It is still a panel farm **even if you draw the boxes by hand into a raster**, and
**even if you use zero `gibson::ui` widgets.** The farm is a *composition*, not a
widget set. Avoiding `panel()` does not save you; having a visual law does.

---

## 4. The pattern to build instead: Custom World + Semantic Chrome

```
  domain model (layer 1)
        │
        ▼
  custom visual world (layer 2 law, realized in layer 3)
        │   Node::canvas / RgbRaster / Surface / Camera / Scene
        │   — owns 60–90% of the frame, carries the information
        ▼
  sparse gibson::ui chrome
        status line · command hint · contextual rail · modal · toast
        — the least annotation that lets a human operate the world
```

Concretely, the skeleton every world-first app shares:

```rust
fn view(model: &Model, cx: &BuildCx) -> Element<Msg> {
    // The hero: a size-aware paint of the whole world.
    let hero = raw(Node::canvas({
        let snapshot = model.snapshot();           // pure state → geometry
        move |rect| paint_world(rect, &snapshot)   // you own every pixel
    })).grow(1.0);

    // screen() is NATURAL-HEIGHT. Without this pin, .grow() has nothing to
    // expand into and your world collapses to a single row. This line is
    // load-bearing.
    screen()
        .height(cx.environment.height)
        .child(status_line(model))     // 1 row of sparse chrome
        .child(hero)                   // the world, growing to fill
        .child(hint_line())            // 1 row of controls
    // .overlay(modal(..)) for inspectors — see §5
}
```

Put the **information in the world**: project labels onto the geometry, draw
relationships *as* geometry, encode state in color/position/scale. Reserve the
chrome for what the world genuinely cannot say (a title, a mode, a key hint).

See [`examples/recipe_hero_with_hud.rs`](../examples/recipe_hero_with_hud.rs) for
the minimal realization. Chronoscope (external) is a full realization: one
perspective camera over a persistent coordinate space, labels projected into the
world, chrome as one-line text and overlays. It is not a required aesthetic — the
*architecture* is the point.

---

## 5. Five composition laws (one recipe each)

Each recipe is tiny (~100 lines), teaches exactly one law, and has a
deterministic `--capture WxH[:mono|:ansi16|:ansi256]` mode so you can render and
**look at** the result (see [§8](#8-visual-acceptance-is-load-bearing)).

### HERO WITH HUD — the world owns the frame
```
tiny status ───────────────────────────────
         CUSTOM WORLD                 │ rail
         CUSTOM WORLD                 │
──────────────────────────── minimal controls
```
One dominant object; chrome is sparse, subordinate, and appears *because it fits*,
not because the frame was carved into boxes first.
→ [`recipe_hero_with_hud.rs`](../examples/recipe_hero_with_hud.rs)

### CONTINUOUS WORLD — one coordinate space, one camera
One coordinate space. One semantic object. The camera changes scale and position;
you never swap "pages". Navigating *is* moving the camera.
→ [`recipe_continuous_world.rs`](../examples/recipe_continuous_world.rs)

### SEMANTIC ZOOM — simplify meaning, don't squeeze boxes
```
wide   : hero + context rail + timeline + full controls
medium : hero + timeline + compact controls
tiny   : hero + essential controls
```
At small sizes, **drop secondary chrome and keep the hero** — do not cram the same
six boxes into 42 columns.
→ [`recipe_semantic_zoom.rs`](../examples/recipe_semantic_zoom.rs)

### CINEMATIC OVERLAY — context floats, the world never reflows
The world stays fixed. The inspector/modal floats above it via
`.overlay(modal(..))`. Toggling it does not move the world a single cell.
→ [`recipe_cinematic_overlay.rs`](../examples/recipe_cinematic_overlay.rs)

### CAPABILITY-SAFE CANVAS — survive TrueColor → Mono
The object stays recognizable in TrueColor, ANSI256, ANSI16, Mono, and glyph
fallback, because its meaning is carried in **shape/position/luminance** (a glyph
ramp), with color as enhancement — not in hue alone.
→ [`recipe_capability_safe_canvas.rs`](../examples/recipe_capability_safe_canvas.rs)

---

## 6. Decision tree

Useful, not dogmatic. Answer top-down; the first YES sets your architecture.

```
Is the domain spatial / continuous / cinematic / topological / transformational?
  YES → a custom Surface/Raster/Scene world should own 60–90% of the frame.
        gibson::ui supplies chrome + interaction only.    [§4, HERO WITH HUD]

Does the user navigate ONE coherent world at several scales?
  YES → one coordinate space + a camera/transform.
        Do NOT build a separate dashboard per scale.      [CONTINUOUS WORLD]

Does the semantic state transform continuously over time?
  YES → map state → geometry FIRST; motion interpolates the change.
        Animation deforms the one object; it is not decorative garnish.

Will the app run at many sizes?
  YES → plan which meaning is Essential / Secondary / Tertiary.
        Shrinking drops Tertiary, keeps the hero.          [SEMANTIC ZOOM]

Does the user drill into detail without losing place?
  YES → float it as an overlay; never reflow the world.    [CINEMATIC OVERLAY]

Is a visualization the REASON the application exists?
  YES → give it visual primacy. Chrome serves it, not the reverse.

Is the domain primarily editorial / CRUD / forms / text navigation / an inspector?
  YES → a semantic gibson::ui layout can legitimately own the frame.  [§7]

More than ~3 major bordered regions and you have NOT hit a YES above?
  → You are probably building a Panel Farm. Re-read §2.
```

---

## 7. When the Panel Farm is actually right

Panels are not banned. They are the **correct** primary structure when the domain
*is* a set of discrete, co-equal textual regions:

- forms and settings;
- inspectors and property sheets;
- business dashboards whose job is literally "many numbers at once";
- editors and dense textual workspaces;
- log / table / list views.

For these, reach for `gibson::ui` first and let it own the frame — that is what it
is *for*. The rule in this guide is scoped: **when the domain is spatial,
cinematic, topological, or transformational, panels should be chrome/annotation
around a custom world — not the main event.** State your domain assumption and
choose accordingly.

---

## 8. Visual acceptance is load-bearing

> **"It compiles. Tests pass. Therefore done."** — No. That is how AI-generated
> visual software ships a panel farm with a green checkmark.

Tests verify **structure**. A human (or you, looking) verifies **composition**.
Both are required; neither substitutes for the other.

**Tests should verify:** deterministic output; the expected semantic elements are
present; no overflow or corruption (no row exceeds its width); correct capability
fallback (Mono still legible); interaction where applicable.

**Human inspection answers** (tests cannot): Is there a dominant object? Is the
information *in* the world or stranded in chrome? Is the hierarchy clear? Is it
coherent? Does it look like one world or six boxes?

The recipes bake this in: every recipe has a `--capture` mode. Render at several
sizes and capabilities and **actually read the frames**:

```sh
cargo run --example recipe_hero_with_hud -- --capture 120x40          # wide
cargo run --example recipe_hero_with_hud -- --capture 80x24           # medium
cargo run --example recipe_hero_with_hud -- --capture 42x15           # constrained
cargo run --example recipe_capability_safe_canvas -- --capture 80x30:mono
```

A text capture shows exactly what a Mono terminal sees — if the object is
unrecognizable there, color was carrying meaning it should not have been. Do this
*before* you call the UI done.

---

## 9. The checklist (before you call it done)

- [ ] I wrote the visual law (layer 2) in one sentence before touching UI code.
- [ ] One dominant object owns 60–90% of the frame: big **and** dense **and** the
      subject.
- [ ] The information is **in** the world (projected labels, geometry,
      color/position), not stranded in side panels.
- [ ] One coordinate space; state changes **deform** the object, they don't swap
      views or refill panels.
- [ ] `screen().height(cx.environment.height)` is set (or the world collapses).
- [ ] Small sizes drop secondary meaning and keep the hero.
- [ ] Inspectors/detail are overlays; the world never reflows.
- [ ] The object survives Mono / glyph fallback.
- [ ] I **rendered captures at ≥3 sizes + Mono and looked at them.**
- [ ] If I have >3 bordered regions, I confirmed the domain is genuinely a
      dashboard ([§7](#7-when-the-panel-farm-is-actually-right)).

---

*Background and the evidence this guide is built on:
[`docs/research/AI_VISUAL_AUTHORING_AUDIT.md`](research/AI_VISUAL_AUTHORING_AUDIT.md).
The chrome/skin/responsive details: [`docs/UI_LAYER.md`](UI_LAYER.md).*
