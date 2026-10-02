//! Project Theseus — the Ship of Theseus machine for music.
//!
//! An interactive terminal instrument that makes LibGibson v0.4.0's HumanMusic
//! Cover Mode quotient *visible* and *audible*. The middle object — the
//! `CoverMap` — is the star: it is the only thing handed to the generator, and
//! every visual is derived from the quotient state and the *real* generated
//! score, never from the reference.
//!
//! Modes:
//!   * interactive (default) — fullscreen RGB reactor with looping audio
//!   * `--headless text|ansi|json|ppm` — deterministic single-frame capture
//!   * `--script "1f2w"` / `--replay file` — ordered input replay (deterministic)
//!   * `--matrix` — geometry × capability × glyph render matrix, no panic gate

use clap::Parser;
use gibson::audio::human_music::cover::{CoverFidelityPreset, CoverKnowledge};
use gibson::audio::human_music::reference_song::ReferenceSong;
use gibson::audio::human_music::rhythm::MetricPosition;
use gibson::audio::human_music::world::MusicWorld;
use gibson::input::{Event, KeyCode, KeyEvent};
use gibson::node::Node;
use gibson::particles::ParticleSystem;
use gibson::renderer::RenderMode;
use gibson::ui::{
    compile,
    element::*,
    skins,
    style::{Emphasis, Tone},
    App, AppEvent, BuildCx, Control, MotionPreference, UiEnvironment,
};
use gibson::{ColorDepth, Context};
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use project_theseus::model::{display_index, AudioMode, Model, DISPLAY_AXES};
use project_theseus::visual::{build_frame, identity_raster, GlyphMode, AXIS_DESC, AXIS_NAMES};

#[derive(Parser, Debug)]
#[command(
    name = "project-theseus",
    version,
    about = "The Ship of Theseus machine for music — a CoverMap quotient instrument for LibGibson v0.4.0"
)]
struct Args {
    /// Reference song fixture (TSV). Public-domain / synthetic only.
    #[arg(long, default_value = "fixtures/ode_to_joy.tsv")]
    fixture: String,
    /// Deterministic generator seed.
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Target world: VAPOR95 | BLACK_ICE | SWISS_SIGNAL.
    #[arg(long, default_value = "VAPOR95")]
    world: String,
    /// Fidelity regime: Loose | Interpretive | Faithful | Strict.
    #[arg(long, default_value = "Interpretive")]
    fidelity: String,
    /// Eight axis bits, e.g. 11111111 or 1,0,1,0,1,0,1,0.
    #[arg(long)]
    axes: Option<String>,
    /// Terminal geometry WxH, e.g. 120x40.
    #[arg(long, default_value = "120x40")]
    geometry: String,
    /// Colour capability: truecolor | ansi256 | ansi16 | mono.
    #[arg(long, default_value = "truecolor")]
    capability: String,
    /// Glyph family: auto | halfblock | braille | block | ascii.
    #[arg(long, default_value = "auto")]
    glyphs: String,
    /// Capture one deterministic frame: text | ansi | json | ppm | lines.
    #[arg(long)]
    headless: Option<String>,
    /// Render the geometry × capability × glyph matrix (no-panic gate).
    #[arg(long)]
    matrix: bool,
    /// Ordered key replay, e.g. "1f2w" or "1 2 3 4".
    #[arg(long)]
    script: Option<String>,
    /// Read an ordered key replay from a file.
    #[arg(long)]
    replay: Option<PathBuf>,
    /// Append every applied key to a file (input log).
    #[arg(long)]
    record: Option<PathBuf>,
    /// Directory for exported WAVs / evidence / PPM.
    #[arg(long, default_value = "exports")]
    export_dir: PathBuf,
    /// Never open an audio device (required for headless / CI).
    #[arg(long)]
    no_audio: bool,
    /// Derive a harmony chart from the reference (needed to reach pinned-harmony refusals).
    #[arg(long)]
    harmony: bool,
    /// Semantic skin override: VAPOR95 | BLACK_ICE | SWISS_SIGNAL.
    #[arg(long)]
    skin: Option<String>,
}

// ---------------------------------------------------------------------------
// construction
// ---------------------------------------------------------------------------

fn parse_world(name: &str) -> MusicWorld {
    match name.to_uppercase().as_str() {
        "BLACK_ICE" | "BLACKICE" => MusicWorld::black_ice(),
        "SWISS_SIGNAL" | "SWISS" => MusicWorld::swiss_signal(),
        _ => MusicWorld::vapor95(),
    }
}

fn parse_fidelity(name: &str) -> CoverFidelityPreset {
    match name.to_lowercase().as_str() {
        "loose" => CoverFidelityPreset::Loose,
        "faithful" => CoverFidelityPreset::Faithful,
        "strict" => CoverFidelityPreset::Strict,
        _ => CoverFidelityPreset::Interpretive,
    }
}

fn parse_capability(name: &str) -> ColorDepth {
    match name.to_lowercase().as_str() {
        "mono" | "none" => ColorDepth::Mono,
        "ansi16" | "16" => ColorDepth::Ansi16,
        "ansi256" | "256" => ColorDepth::Ansi256,
        _ => ColorDepth::TrueColor,
    }
}

fn parse_geometry(s: &str) -> Result<(u16, u16), String> {
    let (w, h) = s
        .split_once(|c| c == 'x' || c == 'X' || c == ',')
        .ok_or_else(|| format!("bad geometry {s:?} (want WxH)"))?;
    let w: u16 = w.trim().parse().map_err(|_| format!("bad width {w:?}"))?;
    let h: u16 = h.trim().parse().map_err(|_| format!("bad height {h:?}"))?;
    Ok((w.max(20), h.max(8)))
}

fn parse_axes(s: &str) -> [bool; 8] {
    let compact: Vec<bool> = if s.contains(',') {
        s.split(',').map(|p| p.trim() == "1").collect()
    } else {
        s.chars().map(|c| c == '1' || c == 't' || c == 'T').collect()
    };
    let mut axes = [true; 8];
    for (i, v) in compact.into_iter().take(8).enumerate() {
        axes[i] = v;
    }
    axes
}

fn skin_for(world: &MusicWorld, override_skin: Option<&str>) -> gibson::ui::Skin {
    match override_skin.map(|s| s.to_uppercase()) {
        Some(s) if s == "VAPOR95" => skins::VAPOR95,
        Some(s) if s == "BLACK_ICE" => skins::BLACK_ICE,
        Some(s) if s == "SWISS_SIGNAL" => skins::SWISS_SIGNAL,
        _ => match world.name {
            "BLACK_ICE" => skins::BLACK_ICE,
            "SWISS_SIGNAL" => skins::SWISS_SIGNAL,
            _ => skins::VAPOR95,
        },
    }
}

fn build_model(args: &Args) -> io::Result<Model> {
    let tsv = std::fs::read_to_string(&args.fixture)
        .map_err(|e| io::Error::new(e.kind(), format!("read {}: {e}", args.fixture)))?;
    let source = ReferenceSong::from_tsv(&tsv, "sop")
        .or_else(|_| ReferenceSong::from_tsv(&tsv, "lead"))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("parse fixture: {e:?}")))?;

    let derived = if args.harmony {
        match source.derive_harmony(MetricPosition::new(2, 1).expect("valid window")) {
            Ok(h) => Some(h),
            Err(e) => {
                eprintln!("harmony derivation refused: {e}");
                None
            }
        }
    } else {
        None
    };

    let glyph = if args.glyphs.eq_ignore_ascii_case("auto")
        && parse_capability(&args.capability) == ColorDepth::Mono
    {
        GlyphMode::Braille
    } else {
        GlyphMode::parse(&args.glyphs)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unknown --glyphs value"))?
    };

    let axes = args.axes.as_deref().map(parse_axes).unwrap_or([true; 8]);

    Ok(Model {
        source,
        source_label: args.fixture.clone(),
        derived,
        world: parse_world(&args.world),
        seed: args.seed,
        preset: parse_fidelity(&args.fidelity),
        glyph,
        axes_on: axes,
        quotient: None,
        relations: String::new(),
        knowledge: [CoverKnowledge::Unknown; 8],
        ceiling_known: [false; 8],
        conformance: [None; 8],
        admission_checks: Vec::new(),
        receipt_pass: None,
        cover_comp: None,
        refusal: None,
        envelope: Vec::new(),
        envelope_source: Vec::new(),
        evidence: Vec::new(),
        step: 0,
        elapsed: Duration::ZERO,
        particles: ParticleSystem::new(args.seed),
        audio: None,
        audio_mode: AudioMode::Stopped,
        status: String::new(),
        help: false,
        no_audio: args.no_audio,
        export_dir: args.export_dir.clone(),
        cover_wav_hash: None,
    })
}

// ---------------------------------------------------------------------------
// semantic shell
// ---------------------------------------------------------------------------

fn theseus_tone(model: &Model) -> Tone {
    match model.active_count() {
        8 => Tone::Success,
        5..=7 => Tone::Info,
        2..=4 => Tone::Warning,
        _ => Tone::Danger,
    }
}

fn help_modal(model: &Model) -> Element<()> {
    let mut col = column()
        .child(heading("PROJECT THESEUS — operator manual"))
        .child(text("The centre object is the CoverMap quotient. `cover()` receives the quotient and the target world, never the reference performance.").tone(Tone::Neutral))
        .child(divider())
        .child(text("AXES (toggle 1-8; a removed axis leaves the quotient and the bridge)").tone(Tone::Accent).emphasis(Emphasis::Strong));
    for i in 0..8 {
        let on = model.axes_on[i];
        let known = model.knowledge[i] != CoverKnowledge::Unknown;
        let state = if on {
            "PRESERVED"
        } else if known {
            "PURGED"
        } else {
            "UNKNOWN"
        };
        col = col.child(
            text(format!(
                "[{}] {:<14} {:<11} {}",
                i + 1,
                AXIS_NAMES[i],
                state,
                AXIS_DESC[i]
            ))
            .tone(if on { Tone::Success } else { Tone::Neutral }),
        );
    }
    col = col
        .child(divider())
        .child(text("KEYS  d = WTF snap (strip identity → new music; restore one axis → recognisability)".to_string()).tone(Tone::Info))
        .child(text("      f = fidelity regime   w = target world   s = new seed   r = reset".to_string()).tone(Tone::Info))
        .child(text("      p/space = play cover   o = audition reference   x = export cover+source+evidence   q = quit".to_string()).tone(Tone::Info))
        .child(divider())
        .child(text("LAWFUL REFUSAL is a first-class outcome: a target world may reject the bridge, and the involved axes light up red.").tone(Tone::Warning))
        .child(text("`?` toggles this manual; Esc closes it when open (otherwise Esc quits).").tone(Tone::Neutral).emphasis(Emphasis::Muted));
    // Deliberately a non-trapping `panel`, not the library `modal`: a modal
    // consumes every key while open (including `q`), so a lone Esc that arrives
    // fused to the next byte in a fast input burst can strand the user inside
    // help. This overlay teaches the same material without owning the keyboard.
    panel("HELP").child(col)
}

fn view(model: &Model, cx: &BuildCx) -> Element<()> {
    let w = cx.environment.width.max(24);
    let h = cx.environment.height.max(8);
    let params = model
        .visual(model.elapsed.as_secs_f32(), cx.environment.color_depth);

    let compact = w < 70;
    let header = if compact {
        row()
            .child(text("◆ THESEUS").tone(Tone::Accent).emphasis(Emphasis::Strong))
            .child(spacer())
            .child(text(format!("[{}] ", model.world.name)).tone(Tone::Info))
            .child(
                text(format!("{:.0}%", model.theseus_pct()))
                    .tone(theseus_tone(model))
                    .emphasis(Emphasis::Strong),
            )
    } else {
        row()
            .child(
                text("◆ PROJECT THESEUS")
                    .tone(Tone::Accent)
                    .emphasis(Emphasis::Strong),
            )
            .child(spacer())
            .child(
                text(format!("[{}] ", model.world.name))
                    .tone(Tone::Accent)
                    .emphasis(Emphasis::Strong),
            )
            .child(
                text(format!("[{}] ", model.preset.label()))
                    .tone(Tone::Info)
                    .emphasis(Emphasis::Strong),
            )
            .child(
                text(format!("[seed {}] ", model.seed))
                    .tone(Tone::Neutral)
                    .emphasis(Emphasis::Muted),
            )
            .child(
                text(format!("{:.0}%", model.theseus_pct()))
                    .tone(theseus_tone(model))
                    .emphasis(Emphasis::Strong),
            )
    };

    let body_h = h.saturating_sub(2).max(4);
    let body = raw(Node::raster(build_frame(w, body_h, &params)));

    let footer = if compact {
        row().child(
            text("[1-8][f][w][s][d][p][o][x][?][q]")
                .tone(Tone::Neutral)
                .emphasis(Emphasis::Muted),
        )
    } else {
        row().child(
            text("[1-8] axes [f] fidelity [w] world [s] seed [d] WTF [p] play [o] source [x] export [r] reset [?] help [q] quit")
                .tone(Tone::Neutral)
                .emphasis(Emphasis::Muted),
        )
    };

    let base = column().child(header).child(body).child(footer);
    if model.help {
        screen().child(base).overlay(help_modal(model))
    } else {
        screen().child(base)
    }
}

// ---------------------------------------------------------------------------
// update / input
// ---------------------------------------------------------------------------

fn regen(model: &mut Model) {
    let was_playing = model.audio_mode == AudioMode::Cover;
    model.regenerate();
    if was_playing && !model.no_audio {
        model.play_cover();
    }
}

fn toggle_axis(model: &mut Model, i: usize) {
    if i >= 8 {
        return;
    }
    if model.axes_on[i] {
        model.axes_on[i] = false;
        model.spawn_sever(i);
    } else {
        model.axes_on[i] = true;
    }
    regen(model);
}

fn wtf_snap(model: &mut Model) {
    let active = model.active_count();
    if active > 1 {
        // strip identity: leave a single surviving axis
        model.axes_on = [false; 8];
        model.axes_on[0] = true;
        for i in 1..8 {
            model.spawn_sever(i);
        }
    } else if model.axes_on[0] {
        // nothing left — "new music"
        model.axes_on = [false; 8];
        model.spawn_sever(0);
    } else {
        // restore the motif — recognisability snaps back
        model.axes_on[0] = true;
    }
    regen(model);
}

/// Apply one key. Returns `true` when the application should quit.
fn handle_key(model: &mut Model, k: KeyEvent) -> bool {
    match k.code {
        KeyCode::Char('q') => {
            model.stop_audio();
            model.help = false;
            return true;
        }
        KeyCode::Esc => {
            // Esc closes the manual when it is open, otherwise it quits.
            if model.help {
                model.help = false;
            } else {
                model.stop_audio();
                return true;
            }
        }
        KeyCode::Char('?') | KeyCode::Char('h') => {
            model.help = !model.help;
        }
        KeyCode::Char(c @ '1'..='8') => {
            toggle_axis(model, (c as u8 - b'1') as usize);
        }
        KeyCode::Char('f') => {
            model.preset = match model.preset {
                CoverFidelityPreset::Loose => CoverFidelityPreset::Interpretive,
                CoverFidelityPreset::Interpretive => CoverFidelityPreset::Faithful,
                CoverFidelityPreset::Faithful => CoverFidelityPreset::Strict,
                CoverFidelityPreset::Strict => CoverFidelityPreset::Loose,
            };
            regen(model);
        }
        KeyCode::Char('w') => {
            model.world = match model.world.name {
                "VAPOR95" => MusicWorld::black_ice(),
                "BLACK_ICE" => MusicWorld::swiss_signal(),
                _ => MusicWorld::vapor95(),
            };
            regen(model);
        }
        KeyCode::Char('s') => {
            model.seed = model.seed.wrapping_add(314_159);
            regen(model);
        }
        KeyCode::Char('r') => {
            model.axes_on = [true; 8];
            model.preset = CoverFidelityPreset::Interpretive;
            regen(model);
        }
        KeyCode::Char('d') => wtf_snap(model),
        KeyCode::Char('p') | KeyCode::Char(' ') => {
            if model.audio_mode == AudioMode::Cover {
                model.stop_audio();
            } else {
                model.play_cover();
            }
        }
        KeyCode::Char('o') => {
            if model.audio_mode == AudioMode::Source {
                model.stop_audio();
            } else {
                model.play_source();
            }
        }
        KeyCode::Char('x') => match model.export() {
            Ok(files) => println!("{}", files.join("\n")),
            Err(e) => model.status = format!("export failed: {e}"),
        },
        _ => {}
    }
    false
}

fn update(model: &mut Model, event: AppEvent<()>) -> Control {
    match event {
        AppEvent::Tick(elapsed) => {
            model.elapsed = elapsed;
            model.tick(1.0 / 30.0);
        }
        AppEvent::Input(Event::Key(k)) => {
            if handle_key(model, k) {
                return Control::Quit;
            }
        }
        AppEvent::Action(_) => {
            // Dispatched by the help modal's dismiss binding.
            model.help = false;
        }
        _ => {}
    }
    Control::Continue
}

// ---------------------------------------------------------------------------
// capture helpers
// ---------------------------------------------------------------------------

fn build_node(model: &Model, args: &Args, w: u16, h: u16, depth: ColorDepth) -> Node {
    let skin = skin_for(&model.world, args.skin.as_deref());
    let env = UiEnvironment {
        width: w,
        height: h,
        color_depth: depth,
        glyph_mode: model.glyph.subcell(),
        motion: MotionPreference::None,
    };
    let cx = BuildCx::new(skin, env);
    match compile(&view(model, &cx), &cx) {
        Ok(c) => c.node,
        Err(e) => Node::text(format!("compile error: {e}"), gibson::cell::Style::new()),
    }
}

fn capture(model: &Model, args: &Args, w: u16, h: u16, depth: ColorDepth, ansi: bool) -> io::Result<String> {
    let node = build_node(model, args, w, h, depth);
    let mut ctx = Context::headless(RenderMode::Fullscreen, w, h);
    ctx.set_color_depth(depth);
    ctx.set_root(node);
    ctx.render()?;
    if ansi {
        Ok(ctx.take_output())
    } else {
        Ok(ctx.last_frame_lines().join("\n"))
    }
}

fn run_headless(model: &mut Model, args: &Args, format: &str) -> io::Result<()> {
    let (w, h) = parse_geometry(&args.geometry).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let depth = parse_capability(&args.capability);
    match format.to_lowercase().as_str() {
        "json" => {
            print!("{}", headless_json(model, args, w, h, depth));
        }
        "ppm" => {
            let params = model.visual(model.elapsed.as_secs_f32(), depth);
            let raster = identity_raster(w, h.saturating_sub(2).max(2), &params);
            std::fs::create_dir_all(&model.export_dir)?;
            let path = model
                .export_dir
                .join(format!("theseus_identity_{}x{}.ppm", w, h));
            let mut file = std::fs::File::create(&path)?;
            raster.write_ppm(&mut file)?;
            println!("wrote {}", path.display());
        }
        "ansi" | "raw" => {
            io::stdout().write_all(capture(model, args, w, h, depth, true)?.as_bytes())?;
        }
        _ => {
            println!("{}", capture(model, args, w, h, depth, false)?);
        }
    }
    Ok(())
}

fn headless_json(model: &Model, args: &Args, w: u16, h: u16, _depth: ColorDepth) -> String {
    let outcome = match &model.refusal {
        Some(r) => format!(
            "{{\"outcome\":\"refused\",\"kind\":\"{}\",\"reason\":\"{}\"}}",
            esc(r.kind),
            esc(&r.reason)
        ),
        None => match (&model.cover_comp, model.receipt_pass) {
            (Some(c), receipt) => format!(
                "{{\"outcome\":\"covered\",\"notes\":{},\"receipt\":{}}}",
                c.score.notes.len(),
                receipt.unwrap_or(false)
            ),
            _ => "{\"outcome\":\"none\"}".to_string(),
        },
    };
    let state = format!(
        "{{\"world\":\"{}\",\"fidelity\":\"{}\",\"seed\":{},\"axes\":\"{}\",\"theseus_pct\":{:.1},\"harmony_derived\":{},\"geometry\":\"{}x{}\",\"capability\":\"{}\",\"glyphs\":\"{}\"}}",
        model.world.name,
        model.preset.label(),
        model.seed,
        model.axes_string(),
        model.theseus_pct(),
        model.derived.is_some(),
        w,
        h,
        args.capability,
        model.glyph.label(),
    );
    format!(
        "{{\"application\":\"project-theseus\",\"libgibson\":\"v0.4.0\",\"state\":{state},\"current\":{outcome},\"evidence\":\n{}\n}}\n",
        model.evidence_json()
    )
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

// ---------------------------------------------------------------------------
// script replay / input log
// ---------------------------------------------------------------------------

fn run_script(model: &mut Model, args: &Args) -> io::Result<()> {
    let mut keys: Vec<char> = Vec::new();
    if let Some(s) = &args.script {
        keys.extend(s.chars().filter(|c| !c.is_whitespace()));
    }
    if let Some(path) = &args.replay {
        let text = std::fs::read_to_string(path)?;
        keys.extend(text.chars().filter(|c| !c.is_whitespace() && *c != '\n'));
    }

    let (w, h) = parse_geometry(&args.geometry).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let depth = parse_capability(&args.capability);

    let mut log = Vec::new();
    println!("== script start: {} keys, geometry {}x{}, {} ==", keys.len(), w, h, args.capability);
    for (step, &ch) in keys.iter().enumerate() {
        model.elapsed = Duration::from_secs_f32(step as f32 * 0.1);
        model.tick(0.1);
        let quit = handle_key(model, KeyEvent::char(ch));
        log.push(ch);
        println!(
            "-- step {} key={:?} world={} fidelity={} axes={} step#={} outcome={} --",
            step + 1,
            ch,
            model.world.name,
            model.preset.label(),
            model.axes_string(),
            model.step,
            model.outcome_tag(),
        );
        println!("{}", capture(model, args, w, h, depth, false)?);
        if quit {
            println!("-- script quit at step {} --", step + 1);
            break;
        }
    }
    if let Some(path) = &args.record {
        let text: String = log.iter().collect();
        std::fs::write(path, text)?;
        println!("-- recorded {} keys to {} --", log.len(), path.display());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// geometry × capability × glyph matrix
// ---------------------------------------------------------------------------

fn run_matrix(model: &mut Model, args: &Args) -> io::Result<()> {
    let geometries = [(42u16, 15u16), (60, 20), (80, 24), (120, 40), (160, 50)];
    let caps = [
        ("truecolor", ColorDepth::TrueColor),
        ("ansi256", ColorDepth::Ansi256),
        ("ansi16", ColorDepth::Ansi16),
        ("mono", ColorDepth::Mono),
    ];
    let glyphs = [
        ("halfblock", GlyphMode::HalfBlock),
        ("block", GlyphMode::Block),
        ("braille", GlyphMode::Braille),
        ("ascii", GlyphMode::Ascii),
    ];

    println!("geometry\tcapability\tglyphs\tlines\tnonblank\tquotient\taxes\trefusal\tstatus");
    let mut failures = 0usize;
    for (gw, gh) in geometries {
        for (cap_name, cap) in caps {
            for (glyph_name, glyph) in glyphs {
                let saved = model.glyph;
                model.glyph = glyph;
                let frame = capture(model, args, gw, gh, cap, false)?;
                model.glyph = saved;
                let lines = frame.lines().count();
                let nonblank = frame
                    .lines()
                    .filter(|l| l.chars().any(|c| !c.is_whitespace()))
                    .count();
                let has_quotient = frame.contains("IDENTITY QUOTIENT") || frame.contains("QUOTIENT");
                let has_axes = frame.contains("PRESERVATION DIMENSIONS");
                let has_refusal = frame.contains("REFUSAL");
                let ok = lines >= (gh as usize).saturating_sub(2) && nonblank > 0 && has_quotient;
                if !ok {
                    failures += 1;
                }
                println!(
                    "{}x{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    gw,
                    gh,
                    cap_name,
                    glyph_name,
                    lines,
                    nonblank,
                    has_quotient,
                    has_axes,
                    has_refusal,
                    if ok { "OK" } else { "FAIL" }
                );
            }
        }
    }
    println!("-- matrix failures: {failures} --");
    if failures > 0 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("{failures} matrix cell(s) failed the visibility gate"),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// entry point
// ---------------------------------------------------------------------------

fn main() -> io::Result<()> {
    let args = Args::parse();
    let mut model = build_model(&args)?;
    model.regenerate();

    if args.matrix {
        return run_matrix(&mut model, &args);
    }
    if let Some(format) = args.headless.clone() {
        return run_headless(&mut model, &args, &format);
    }
    if args.script.is_some() || args.replay.is_some() || args.record.is_some() {
        return run_script(&mut model, &args);
    }

    if !args.no_audio {
        model.play_cover();
    }
    let skin = skin_for(&model.world, args.skin.as_deref());
    App::fullscreen().skin(skin).run(model, update, view)?;
    Ok(())
}

// Silence "unused" for the public axis index helper shared with tests.
#[allow(dead_code)]
fn axis_display_index(axis: gibson::audio::human_music::cover::CoverAxis) -> Option<usize> {
    display_index(axis)
}

#[allow(dead_code)]
const _DISPLAY_AXES: [gibson::audio::human_music::cover::CoverAxis; 8] = DISPLAY_AXES;
