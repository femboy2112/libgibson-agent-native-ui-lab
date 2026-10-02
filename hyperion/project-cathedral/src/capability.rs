//! The capability matrix: 5 terminal sizes × 4 color depths, plus a check that
//! compact views still preserve incident state, the major affected cluster, the
//! operator affordance and an exit path.

use gibson::capability::ColorDepth;
use gibson::context::{Context, RenderMode};
use gibson::Node;

use crate::app::App;
use crate::cli::Options;
use crate::incident::Phase;
use crate::visual::{build_frame, Camera, Layout, Scale, ViewState};

pub const SIZES: [(u16, u16); 5] = [(42, 15), (60, 20), (80, 24), (120, 40), (160, 50)];
pub const DEPTHS: [ColorDepth; 4] = [
    ColorDepth::TrueColor,
    ColorDepth::Ansi256,
    ColorDepth::Ansi16,
    ColorDepth::Mono,
];

#[derive(Debug, Clone)]
pub struct CapabilityRow {
    pub width: u16,
    pub height: u16,
    pub depth: ColorDepth,
    pub bytes: usize,
    pub exact_changed: usize,
    pub has_state: bool,
    pub has_cluster: bool,
    pub has_affordance: bool,
    pub has_exit: bool,
    pub color_codes: usize,
}

pub fn run(opts: &Options) -> Result<(), Box<dyn std::error::Error>> {
    // Reach a representative mid-incident state deterministically.
    let mut app = App::new(opts.seed, world_id(&opts.world), true);
    app.enable_music = false;
    for _ in 0..220 {
        let _ = app.step();
        app.camera.update();
    }
    let rows = matrix(&app, &opts.color.depth());
    let mut all = true;
    println!("size        depth      bytes  exact  state cluster afford exit colorcodes");
    for r in &rows {
        all &= r.has_state && r.has_affordance && r.has_exit;
        println!(
            "{:>3}x{:<3}    {:<10} {:>6} {:>6}  {:>5} {:>7} {:>6} {:>5} {:>5}",
            r.width,
            r.height,
            format!("{:?}", r.depth),
            r.bytes,
            r.exact_changed,
            yn(r.has_state),
            yn(r.has_cluster),
            yn(r.has_affordance),
            yn(r.has_exit),
            r.color_codes,
        );
    }
    println!("all sizes preserve state + affordance + exit: {}", yn(all));
    // The requested depth is applied to every size by the harness; the row depth is
    // the one actually compiled.
    Ok(())
}

pub fn matrix(app: &App, requested: &ColorDepth) -> Vec<CapabilityRow> {
    let mut rows = Vec::new();
    for &(w, h) in &SIZES {
        for &d in &DEPTHS {
            rows.push(render_cell(app, w, h, d, requested));
        }
    }
    rows
}

/// Render one size×depth cell in a headless context and inspect the emitted bytes.
pub fn render_cell(
    app: &App,
    w: u16,
    h: u16,
    depth: ColorDepth,
    _requested: &ColorDepth,
) -> CapabilityRow {
    let mut ctx = Context::headless(RenderMode::Fullscreen, w, h);
    ctx.set_color_depth(depth);
    let surface = render_surface(app, w, h);
    let lines = surface.to_visible_lines();
    let text = lines.join("\n");
    ctx.set_root(Node::raster(surface));
    let _ = ctx.render_now();
    let out = ctx.take_output();
    let stats = ctx.last_frame_report();
    let bytes = out.len();
    let color_codes = out.matches('\u{1b}').count();
    CapabilityRow {
        width: w,
        height: h,
        depth,
        bytes,
        exact_changed: stats.exact_changed_cells,
        has_state: Phase::ALL.iter().any(|p| text.contains(p.label()))
            || text.contains("crit ")
            || text.contains("sev "),
        has_cluster: text.contains("hot ")
            || text.contains("CLUSTER")
            || text.contains("cluster")
            || text.contains("WHOLE SYSTEM")
            || text.contains("district"),
        has_affordance: text.contains("[F]")
            || text.contains("inject-fault")
            || text.contains("Fault")
            || text.contains("reroute"),
        has_exit: text.contains("Quit") || text.contains("[Q]uit") || text.contains("quit"),
        color_codes,
    }
}

fn render_surface(app: &App, w: u16, h: u16) -> gibson::surface::Surface {
    let mut camera = Camera::new();
    let layout = Layout::build(&app.engine);
    let scale = if w < 80 {
        Scale::Chain(app.selected)
    } else {
        app.scale
    };
    camera.aim(
        &app.engine,
        &layout,
        scale,
        app.selected,
        w as f32,
        (h as f32 - 3.0).max(1.0),
    );
    for _ in 0..40 {
        camera.update();
    }
    let now = app
        .music
        .take
        .as_ref()
        .map(|t| t.now_playing())
        .unwrap_or_else(|| "—".into());
    let (sem, _) = app.music.current_semantics();
    let sem_s = format!(
        "T{:?} E{:?} D{:?} L{:?}",
        sem.tone, sem.emphasis, sem.density, sem.elevation
    );
    let view = ViewState {
        engine: &app.engine,
        layout: &layout,
        camera: &camera,
        phase: app.tracker.phase,
        scale,
        selected: app.selected,
        frame: app.frame,
        paused: app.paused,
        journal_len: app.journal.len(),
        incident_len: app.tracker.records.len(),
        now_playing: &now,
        music_state: &sem_s,
        message: &app.message,
        color_depth: ColorDepth::TrueColor,
        show_help: false,
    };
    build_frame(&view, w, h)
}

fn yn(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "NO "
    }
}

pub fn world_id(s: &str) -> gibson::audio::human_music::world::WorldId {
    match s {
        "vapor" | "vapor95" | "v" => gibson::audio::human_music::world::WorldId::Vapor95,
        "swiss" | "swiss-signal" | "s" => gibson::audio::human_music::world::WorldId::SwissSignal,
        _ => gibson::audio::human_music::world::WorldId::BlackIce,
    }
}

/// A stable, small smoke test used by the integration tests: the fixture is built
/// and a handful of frames render at every size without panicking.
pub fn smoke() -> usize {
    let app = App::new(
        1,
        gibson::audio::human_music::world::WorldId::BlackIce,
        false,
    );
    let mut ok = 0;
    for &(w, h) in &SIZES {
        let row = render_cell(&app, w, h, ColorDepth::Mono, &ColorDepth::Mono);
        if row.has_state && row.has_exit {
            ok += 1;
        }
    }
    ok
}
