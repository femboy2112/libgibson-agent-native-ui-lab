use clap::Parser;
use gibson::audio::human_music::{
    contract::CompositionGrammar,
    cover::{
        cover, CoverAdmission, CoverError, CoverFidelityPreset, CoverFidelityProfile,
        CoverMap, CoverTarget, FormRelation, GrooveRelation, HarmonyRelation, LineRelation,
        OrchestrationRelation,
    },
    functor::Composition,
    policy::PerformanceProfile,
    reference_song::ReferenceSong,
    world::MusicWorld,
};
use gibson::cell::{Color, Style};
use gibson::field::{plasma, radial_pulse};
use gibson::input::{Event, KeyCode};
use gibson::node::{Node, WrapMode};
use gibson::ui::{
    element::*,
    skins,
    style::{Emphasis, Tone},
    App, AppEvent, BuildCx, Control,
};
use gibson::{BrailleCanvas, HalfBlockCanvas};
use std::io;
use std::time::Duration;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "fixtures/ode_to_joy.tsv")]
    fixture: String,
    #[arg(long, default_value = "42")]
    seed: u64,
    #[arg(long, default_value = "VAPOR95")]
    world: String,
    #[arg(long, default_value = "Interpretive")]
    fidelity: String,
    #[arg(long)]
    headless: Option<String>,
}

struct Model {
    source: ReferenceSong,
    world: MusicWorld,
    seed: u64,
    preset: CoverFidelityPreset,
    axes_on: [bool; 8], // 0: Motif, 1: Riff, 2: Groove, 3: HarmContour, 4: HarmLoop, 5: Form, 6: Orch, 7: Bass
    quotient: Option<CoverMap>,
    admission: Option<CoverAdmission>,
    cover_comp: Option<Composition>,
    error_msg: Option<String>,
    elapsed: Duration,
}

const AXIS_NAMES: [&str; 8] = [
    "MOTIF       (Melodic Contour)",
    "RIFF        (Secondary Hooks)",
    "GROOVE      (Rhythmic Pocket)",
    "H-CONTOUR   (Harmonic Path)  ",
    "H-LOOP      (Cadence Cycle)  ",
    "FORM        (AABA Structure) ",
    "ORCHESTRA   (Voice Seating)  ",
    "BASS        (Root Foundation)",
];

fn regenerate(model: &mut Model) {
    let mut profile = CoverFidelityProfile::preset(model.preset);
    if !model.axes_on[0] {
        profile.motif = LineRelation::Free;
    }
    if !model.axes_on[1] {
        profile.riff = LineRelation::Free;
    }
    if !model.axes_on[2] {
        profile.groove = GrooveRelation::Free;
    }
    if !model.axes_on[3] && !model.axes_on[4] {
        profile.harmony = HarmonyRelation::Free;
    }
    if !model.axes_on[5] {
        profile.form = FormRelation::Free;
    }
    if !model.axes_on[6] {
        profile.orchestration = OrchestrationRelation::Free;
    }
    if !model.axes_on[7] {
        profile.bass = LineRelation::Free;
    }

    match model.source.extract_fidelity(&profile, Some(model.preset), None) {
        Ok((map, _report)) => {
            model.quotient = Some(map.clone());
            let target = CoverTarget {
                world: &model.world,
                seed: model.seed,
                grammar: CompositionGrammar::DeflectedLift,
                options: Default::default(),
                profile: PerformanceProfile::BAND,
            };
            match cover(&map, target) {
                Ok(comp) => {
                    model.cover_comp = Some(comp);
                    model.admission = None;
                    model.error_msg = None;
                }
                Err(CoverError::Rejected(admission)) => {
                    model.cover_comp = None;
                    model.admission = Some(*admission);
                    model.error_msg = Some("Lawful Refusal: Invariant Violation In Target World".into());
                }
                Err(e) => {
                    model.cover_comp = None;
                    model.admission = None;
                    model.error_msg = Some(format!("{:?}", e));
                }
            }
        }
        Err(e) => {
            model.quotient = None;
            model.cover_comp = None;
            model.admission = None;
            model.error_msg = Some(format!("Extraction error: {:?}", e));
        }
    }
}

/// Renders a dynamic cybernetic plasma/energy field for the center quotient core.
fn render_reactor_canvas(width: u16, height: u16, time_secs: f32, world_name: &str, active_axes_count: usize) -> Element<()> {
    let mut canvas = HalfBlockCanvas::new(width, height);
    let pw = canvas.pixel_width() as f32;
    let ph = canvas.pixel_height() as f32;

    // Disassembly entropy increases as more axes are removed
    let entropy_factor = (8.0 - active_axes_count as f32) / 8.0;
    let turbulence = 1.0 + entropy_factor * 3.5;

    for y in 0..canvas.pixel_height() as i32 {
        for x in 0..canvas.pixel_width() as i32 {
            let u = x as f32 / pw.max(1.0);
            let v = y as f32 / ph.max(1.0);

            let p = plasma(u * 5.0 * turbulence, v * 5.0 * turbulence, time_secs * 1.5, 42.0);
            let pulse = radial_pulse(u * 4.0 - 2.0, v * 4.0 - 2.0, time_secs * 2.0);
            let val = ((p * 0.7 + pulse * 0.3) * (0.8 + 0.2 * entropy_factor)).clamp(0.0, 1.0);

            let rgb = match world_name {
                "VAPOR95" => {
                    // Cyberpunk sunset: deep purple -> hot magenta -> neon cyan -> warm amber
                    if val < 0.25 {
                        let t = val / 0.25;
                        (
                            (30.0 + t * 90.0) as u8,
                            (10.0 + t * 20.0) as u8,
                            (80.0 + t * 140.0) as u8,
                        )
                    } else if val < 0.6 {
                        let t = (val - 0.25) / 0.35;
                        (
                            (120.0 + t * 135.0) as u8,
                            (30.0 + t * 60.0) as u8,
                            (220.0 - t * 40.0) as u8,
                        )
                    } else {
                        let t = (val - 0.6) / 0.4;
                        (
                            255,
                            (90.0 + t * 140.0) as u8,
                            (180.0 - t * 150.0) as u8,
                        )
                    }
                }
                "BLACK_ICE" => {
                    // Cryogenic stealth: obsidian -> slate -> ice blue -> electric laser cyan
                    if val < 0.3 {
                        let t = val / 0.3;
                        (
                            (5.0 + t * 15.0) as u8,
                            (15.0 + t * 35.0) as u8,
                            (30.0 + t * 60.0) as u8,
                        )
                    } else if val < 0.7 {
                        let t = (val - 0.3) / 0.4;
                        (
                            (20.0 + t * 40.0) as u8,
                            (50.0 + t * 150.0) as u8,
                            (90.0 + t * 165.0) as u8,
                        )
                    } else {
                        let t = (val - 0.7) / 0.3;
                        (
                            (60.0 + t * 195.0) as u8,
                            (200.0 + t * 55.0) as u8,
                            255,
                        )
                    }
                }
                _ => {
                    // SWISS_SIGNAL: stark international red -> crisp chalk white -> deep carbon
                    if val < 0.5 {
                        let t = val / 0.5;
                        (
                            (20.0 + t * 200.0) as u8,
                            (10.0 + t * 15.0) as u8,
                            (15.0 + t * 25.0) as u8,
                        )
                    } else {
                        let t = (val - 0.5) / 0.5;
                        (
                            (220.0 + t * 35.0) as u8,
                            (25.0 + t * 230.0) as u8,
                            (40.0 + t * 215.0) as u8,
                        )
                    }
                }
            };

            canvas.set_pixel(x, y, rgb);
        }
    }

    let node = Node::rich_text_wrapped(canvas.to_rich_text(), WrapMode::NoWrap);
    raw(node)
}

/// Renders a dynamic piano roll on BrailleCanvas for the notes in a track.
fn render_piano_roll_braille(
    notes: &[(f32, f32, i32)], // (beat_start, dur, midi_pitch)
    total_beats: f32,
    width: u16,
    height: u16,
    style: Style,
) -> Element<()> {
    let mut canvas = BrailleCanvas::new(width, height);
    if notes.is_empty() || total_beats <= 0.0 {
        return raw(Node::rich_text_wrapped(canvas.to_rich_text(style), WrapMode::NoWrap));
    }

    let pw = canvas.pixel_width() as i32;
    let ph = canvas.pixel_height() as i32;

    // Pitch bounds
    let min_pitch = notes.iter().map(|n| n.2).min().unwrap_or(60).max(36);
    let max_pitch = notes.iter().map(|n| n.2).max().unwrap_or(72).max(min_pitch + 12);
    let pitch_range = (max_pitch - min_pitch).max(1) as f32;

    for note in notes {
        let x0 = ((note.0 / total_beats) * pw as f32).round() as i32;
        let x1 = (((note.0 + note.1) / total_beats) * pw as f32).round() as i32;
        let x1 = x1.max(x0 + 1);

        let y_norm = (note.2 - min_pitch) as f32 / pitch_range;
        let y = ph - 1 - (y_norm * (ph - 1) as f32).round() as i32;

        for x in x0..x1.min(pw) {
            canvas.set(x, y);
            if y > 0 {
                canvas.set(x, y - 1); // Thick note bars
            }
        }
    }

    raw(Node::rich_text_wrapped(canvas.to_rich_text(style), WrapMode::NoWrap))
}

fn view(model: &Model, cx: &BuildCx) -> Element<()> {
    let term_w = cx.environment.width.max(60);
    let time_s = model.elapsed.as_secs_f32();
    let active_count = model.axes_on.iter().filter(|&&x| x).count();
    let pct_preserved = (active_count as f32 / 8.0) * 100.0;

    // 1. TOP TELEMETRY BANNER
    let world_badge = format!(" [WORLD: {}] ", model.world.name);
    let fidelity_badge = format!(" [FIDELITY: {}] ", model.preset.label());
    let seed_badge = format!(" [SEED: {}] ", model.seed);

    let header_card = card("◆ PROJECT THESEUS // RECOMBINANT QUOTIENT MACHINE ◆")
        .child(
            row()
                .child(text(world_badge).tone(Tone::Accent).emphasis(Emphasis::Strong))
                .child(text(fidelity_badge).tone(Tone::Info).emphasis(Emphasis::Strong))
                .child(text(seed_badge).tone(Tone::Neutral).emphasis(Emphasis::Muted))
                .child(spacer())
                .child(
                    text(format!("TIME: {:05.2}s ", time_s))
                        .tone(Tone::Neutral)
                        .emphasis(Emphasis::Faint),
                ),
        );

    // 2. THE THREE TIERS: (Reference) -> (CoverMap Quotient) -> (Fresh Cover)

    // --- TIER 1: REFERENCE PERFORMANCE ---
    let ref_notes: Vec<(f32, f32, i32)> = model
        .source
        .voices
        .first()
        .map(|v| {
            v.notes
                .iter()
                .map(|n| (n.at.beats() as f32, n.duration.beats() as f32, n.pitch))
                .collect()
        })
        .unwrap_or_default();

    let ref_beats = model.source.length.beats() as f32;
    let ref_canvas_w = (term_w / 3).max(18).min(32);
    let ref_canvas = render_piano_roll_braille(
        &ref_notes,
        ref_beats,
        ref_canvas_w,
        3,
        Style::new().fg(Color::Rgb(0, 240, 255)),
    );

    let ref_panel = card("1. REFERENCE PERFORMANCE (SOURCE DNA)")
        .child(text("Ode to Joy (Beethoven) // Monophonic Lead").tone(Tone::Accent))
        .child(ref_canvas)
        .child(
            text(format!(
                "Meter: {}/{} | Length: {:.0}b | Notes: {}",
                model.source.meter.0,
                model.source.meter.1,
                ref_beats,
                ref_notes.len()
            ))
            .tone(Tone::Neutral)
            .emphasis(Emphasis::Muted),
        );

    // --- TIER 3: FRESH COVER PERFORMANCE ---
    let cover_canvas_w = ref_canvas_w;
    let (_cover_status_title, cover_panel) = if let Some(comp) = &model.cover_comp {
        let cov_notes: Vec<(f32, f32, i32)> = comp
            .score
            .notes
            .iter()
            .map(|n| (n.start_beat as f32, n.dur_beats, n.pitch))
            .collect();
        let cov_beats = comp.score.total_beats as f32;
        let cov_canvas = render_piano_roll_braille(
            &cov_notes,
            cov_beats,
            cover_canvas_w,
            3,
            Style::new().fg(Color::Rgb(50, 255, 120)),
        );

        let p = card("3. FRESH COVER (GENERATED FROM QUOTIENT ONLY)")
            .child(
                text(format!(
                    "Synthesized in {} @ {:.0} BPM",
                    model.world.name, model.world.tempo_bpm
                ))
                .tone(Tone::Success),
            )
            .child(cov_canvas)
            .child(
                text(format!(
                    "Total Notes: {} | Beats: {:.0} | Anchor: PASS",
                    cov_notes.len(),
                    cov_beats
                ))
                .tone(Tone::Success)
                .emphasis(Emphasis::Strong),
            );
        ("STATUS: SYNTHESIZED", p)
    } else {
        let refusal_alert = if let Some(adm) = &model.admission {
            let failed_checks: Vec<String> = adm
                .conformance
                .checks
                .iter()
                .filter(|c| !c.passed)
                .map(|c| format!("AXIS REJECTED: {:?} [{}]", c.axis, c.detail))
                .collect();

            let err_summary = failed_checks.join(" | ");
            card("3. ⚡ LAWFUL REFUSAL // BRIDGE REJECTED ⚡")
                .child(
                    text("TARGET WORLD REJECTED IDENTITY CONSTRAINTS")
                        .tone(Tone::Danger)
                        .emphasis(Emphasis::Strong),
                )
                .child(
                    text("Invariant collision: Cannot realize requested pins without compromise.")
                        .tone(Tone::Danger),
                )
                .child(text(err_summary).tone(Tone::Warning).emphasis(Emphasis::Strong))
        } else {
            card("3. SYNTHESIS FAILURE")
                .child(
                    text(model.error_msg.clone().unwrap_or_else(|| "Unknown error".into()))
                        .tone(Tone::Danger),
                )
        };
        ("STATUS: REFUSED", refusal_alert)
    };

    // --- TIER 2: IDENTITY QUOTIENT (THE MIDDLE STAR) ---
    // Ship of Theseus Gauge
    let gauge_blocks = (pct_preserved / 5.0).round() as usize;
    let filled_bar = "█".repeat(gauge_blocks);
    let empty_bar = "░".repeat(20 - gauge_blocks);
    let gauge_str = format!("[{}{}] {:.1}%", filled_bar, empty_bar, pct_preserved);

    let identity_state_desc = if active_count == 8 {
        "CLASSICAL IDENTITY CONSERVED (ALL AXES PINNED)"
    } else if active_count >= 5 {
        "SURFACE TRANSLATION // CORE IDENTIFIABLE"
    } else if active_count >= 2 {
        "CRITICAL SHIP DRIFT // BORDERLINE FAMILIAR"
    } else if active_count == 1 {
        "WTF THRESHOLD // SINGLE AXIS ISOLATION"
    } else {
        "TOTAL SHIP OF THESEUS EVAPORATION (NEW MUSIC)"
    };

    let gauge_tone = if active_count >= 6 {
        Tone::Success
    } else if active_count >= 3 {
        Tone::Warning
    } else {
        Tone::Danger
    };

    // Animated Reactor Canvas (center visualizer)
    let reactor_w = (term_w.saturating_sub(6)).min(74);
    let reactor_element = render_reactor_canvas(reactor_w, 4, time_s, model.world.name, active_count);

    // Conduits / Channels
    let mut conduit_col = column();
    for i in 0..8 {
        let is_on = model.axes_on[i];
        let axis_label = AXIS_NAMES[i];
        let key_num = i + 1;

        let conduit_line = if is_on {
            // Pulse wave animation along conduit
            let anim_offset = ((time_s * 6.0) as usize + i * 3) % 20;
            let mut wire = "════════════════════".chars().collect::<Vec<_>>();
            if anim_offset < wire.len() {
                wire[anim_offset] = '◈';
            }
            let wire_str: String = wire.into_iter().collect();

            row()
                .child(
                    text(format!("[{}] {}", key_num, axis_label))
                        .tone(Tone::Accent)
                        .emphasis(Emphasis::Strong),
                )
                .child(
                    text(format!(" ──»» {} »»── ", wire_str))
                        .tone(Tone::Success)
                        .emphasis(Emphasis::Strong),
                )
                .child(text(" [LOCKED / PRESERVED] ").tone(Tone::Success))
        } else {
            row()
                .child(
                    text(format!("[{}] {}", key_num, axis_label))
                        .tone(Tone::Neutral)
                        .emphasis(Emphasis::Faint),
                )
                .child(
                    text(" ──⚡  ░░░░  [SEVERED // AXIS PURGED]  ░░░░  ⚡── ")
                        .tone(Tone::Danger)
                        .emphasis(Emphasis::Faint),
                )
                .child(text(" [DISASSEMBLED] ").tone(Tone::Neutral).emphasis(Emphasis::Faint))
        };

        conduit_col = conduit_col.child(conduit_line);
    }

    let quotient_panel = panel("2. IDENTITY QUOTIENT // THE COVERMAP (THE BRIDGE)")
        .child(
            row()
                .child(text("THESEUS INDEX: ").tone(Tone::Neutral).emphasis(Emphasis::Strong))
                .child(text(gauge_str).tone(gauge_tone).emphasis(Emphasis::Strong))
                .child(spacer())
                .child(text(identity_state_desc).tone(gauge_tone).emphasis(Emphasis::Strong)),
        )
        .child(reactor_element)
        .child(conduit_col);

    // 3. INTERACTIVE CONTROL COCKPIT
    let control_hud = panel("◆ RECOMBINANT CONTROL COCKPIT ◆").child(
        row()
            .child(
                text("[1-8] Toggle Axes ")
                    .tone(Tone::Accent)
                    .emphasis(Emphasis::Strong),
            )
            .child(text("| [F] Fidelity Preset ").tone(Tone::Info))
            .child(text("| [W] Cycle Music World ").tone(Tone::Success))
            .child(text("| [S] Mutate Seed ").tone(Tone::Neutral))
            .child(text("| [D] WTF Snap/Drop ").tone(Tone::Warning).emphasis(Emphasis::Strong))
            .child(text("| [R] Full Reset ").tone(Tone::Accent))
            .child(text("| [Q] Quit").tone(Tone::Danger)),
    );

    // ASSEMBLE COMPLETE SCREEN
    screen().child(
        column()
            .child(header_card)
            .child(
                row()
                    .child(ref_panel)
                    .child(spacer())
                    .child(cover_panel),
            )
            .child(quotient_panel)
            .child(control_hud),
    )
}

fn update(model: &mut Model, event: AppEvent<()>) -> Control {
    match event {
        AppEvent::Tick(elapsed) => {
            model.elapsed = elapsed;
            Control::Continue
        }
        AppEvent::Input(Event::Key(k)) => {
            match k.code {
                KeyCode::Char('q') | KeyCode::Esc => return Control::Quit,
                KeyCode::Char('f') => {
                    model.preset = match model.preset {
                        CoverFidelityPreset::Loose => CoverFidelityPreset::Interpretive,
                        CoverFidelityPreset::Interpretive => CoverFidelityPreset::Faithful,
                        CoverFidelityPreset::Faithful => CoverFidelityPreset::Strict,
                        CoverFidelityPreset::Strict => CoverFidelityPreset::Loose,
                    };
                    regenerate(model);
                }
                KeyCode::Char('1') => {
                    model.axes_on[0] = !model.axes_on[0];
                    regenerate(model);
                }
                KeyCode::Char('2') => {
                    model.axes_on[1] = !model.axes_on[1];
                    regenerate(model);
                }
                KeyCode::Char('3') => {
                    model.axes_on[2] = !model.axes_on[2];
                    regenerate(model);
                }
                KeyCode::Char('4') => {
                    model.axes_on[3] = !model.axes_on[3];
                    regenerate(model);
                }
                KeyCode::Char('5') => {
                    model.axes_on[4] = !model.axes_on[4];
                    regenerate(model);
                }
                KeyCode::Char('6') => {
                    model.axes_on[5] = !model.axes_on[5];
                    regenerate(model);
                }
                KeyCode::Char('7') => {
                    model.axes_on[6] = !model.axes_on[6];
                    regenerate(model);
                }
                KeyCode::Char('8') => {
                    model.axes_on[7] = !model.axes_on[7];
                    regenerate(model);
                }
                KeyCode::Char('w') => {
                    if model.world.name == "VAPOR95" {
                        model.world = MusicWorld::black_ice();
                    } else if model.world.name == "BLACK_ICE" {
                        model.world = MusicWorld::swiss_signal();
                    } else {
                        model.world = MusicWorld::vapor95();
                    }
                    regenerate(model);
                }
                KeyCode::Char('s') => {
                    model.seed = model.seed.wrapping_add(314159);
                    regenerate(model);
                }
                KeyCode::Char('r') => {
                    model.axes_on = [true; 8];
                    model.preset = CoverFidelityPreset::Interpretive;
                    regenerate(model);
                }
                KeyCode::Char('d') => {
                    // WTF Moment: Drop to only Motif (or snap back if already dropped)
                    if model.axes_on.iter().filter(|&&x| x).count() > 1 {
                        model.axes_on = [false; 8];
                        model.axes_on[0] = true; // only motif remains
                    } else if model.axes_on[0] {
                        model.axes_on[0] = false; // drop even motif -> completely alien
                    } else {
                        model.axes_on[0] = true; // snap motif back -> recognizability snaps!
                    }
                    regenerate(model);
                }
                _ => {}
            }
            Control::Continue
        }
        _ => Control::Continue,
    }
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    let tsv = std::fs::read_to_string(&args.fixture).unwrap_or_else(|_| "".into());
    let ref_song = ReferenceSong::from_tsv(&tsv, "lead").expect("Failed to load reference TSV fixture");

    let initial_world = match args.world.to_uppercase().as_str() {
        "BLACK_ICE" => MusicWorld::black_ice(),
        "SWISS_SIGNAL" => MusicWorld::swiss_signal(),
        _ => MusicWorld::vapor95(),
    };

    let initial_fidelity = match args.fidelity.to_lowercase().as_str() {
        "loose" => CoverFidelityPreset::Loose,
        "faithful" => CoverFidelityPreset::Faithful,
        "strict" => CoverFidelityPreset::Strict,
        _ => CoverFidelityPreset::Interpretive,
    };

    let mut model = Model {
        source: ref_song,
        world: initial_world,
        seed: args.seed,
        preset: initial_fidelity,
        axes_on: [true; 8],
        quotient: None,
        admission: None,
        cover_comp: None,
        error_msg: None,
        elapsed: Duration::ZERO,
    };

    regenerate(&mut model);

    App::fullscreen().skin(skins::VAPOR95).run(model, update, view)?;
    Ok(())
}
