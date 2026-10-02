# AGENTS.md — authoring routes for AI agents

This is a router, not documentation. Follow the route for your task.

## Building a LibGibson application (especially a visual/spatial one)

If you are an AI agent asked to build a LibGibson app, do this **in order** —
do not start by reading `src/`:

1. **Read [`docs/AI_VISUAL_AUTHORING.md`](docs/AI_VISUAL_AUTHORING.md).** It is
   ~400 lines and it is the method. Everything below is a summary of its
   discipline — the guide is the authority.
2. **Define the semantic world** (layer 1): what is the thing, as data?
3. **Define the visual metaphor / law** (layer 2): how does meaning become
   space? Write it in one sentence. *If you can't, you're about to build a panel
   farm.*
4. **Identify the dominant visual object**: what owns 60–90% of attention?
5. **Choose `Surface` / `RgbRaster` / `Scene` / `Camera`** to realize that
   object. `Node::canvas(|rect| -> Surface)` is the clean full-frame hero.
6. **Use `gibson::ui` for sparse chrome and interaction** — status, hints,
   contextual rail, overlays — *around* the world, not instead of it.
7. **Define size-class priority**: what is Essential / Secondary / Tertiary?
   Small screens drop Tertiary and keep the hero; they do not squeeze every box.
8. **Avoid the Panel Farm** unless the domain genuinely is a dashboard, form,
   inspector, or dense text workspace (then panels are correct — use them).
9. **Test 120×40 AND constrained sizes** (80×24, 42×15) **AND Mono.**
10. **Render the frame and look at it.** Compilation and passing tests are not
    visual acceptance. Every `examples/recipe_*.rs` has a
    `--capture WxH[:mono]` mode for exactly this.

Pick a matching starting point from the recipes:

| Your domain | Start from |
|---|---|
| A spatial/cinematic world owning the frame | [`examples/recipe_hero_with_hud.rs`](examples/recipe_hero_with_hud.rs) |
| One space navigated at many scales | [`examples/recipe_continuous_world.rs`](examples/recipe_continuous_world.rs) |
| Must work wide → tiny | [`examples/recipe_semantic_zoom.rs`](examples/recipe_semantic_zoom.rs) |
| Drill-in without losing place | [`examples/recipe_cinematic_overlay.rs`](examples/recipe_cinematic_overlay.rs) |
| Must survive TrueColor → Mono | [`examples/recipe_capability_safe_canvas.rs`](examples/recipe_capability_safe_canvas.rs) |
| A form / dashboard / inspector / editor | [`examples/ui_quickstart.rs`](examples/ui_quickstart.rs) + [`docs/UI_LAYER.md`](docs/UI_LAYER.md) |

## Other routes

- **The `gibson::ui` chrome layer** (components, skins, responsive, motion):
  [`docs/UI_LAYER.md`](docs/UI_LAYER.md).
- **Feature overview / stability tiers:** [`docs/FEATURES.md`](docs/FEATURES.md),
  [`docs/RELEASE_CONTRACT.md`](docs/RELEASE_CONTRACT.md).
- **Glyph/capability degradation:** [`docs/GLYPHS.md`](docs/GLYPHS.md).
- **Why this router exists** (the research behind it):
  [`docs/research/AI_VISUAL_AUTHORING_AUDIT.md`](docs/research/AI_VISUAL_AUTHORING_AUDIT.md).

Do not duplicate the documentation set into your context. Read the one route you
need.
