use clap::Parser;
use gibson::audio::buffer::StereoBlock;
use gibson::audio::device::AudioDevice;
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
    score::Score,
    synth::HumanMusicSynth,
    world::MusicWorld,
};
use gibson::audio::render::{AudioSource, OfflineRenderer, RenderCtx};
use gibson::audio::time::{SampleRate, SampleTime};
use gibson::audio::wav::write_wav_i16;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioMode {
    Stopped,
    PlayingCover,
    PlayingSource,
}

/// Seamlessly loops an audio source indefinitely so the performance never abruptly stops.
struct LoopingSynth {
    score: Score,
    world: MusicWorld,
    sr: SampleRate,
    synth: HumanMusicSynth,
    loop_offset: u64,
}

impl LoopingSynth {
    fn new(score: Score, world: MusicWorld, sr: SampleRate) -> Self {
        let synth = HumanMusicSynth::new(&score, &world, sr);
        Self {
            score,
            world,
            sr,
            synth,
            loop_offset: 0,
        }
    }
}

impl AudioSource for LoopingSynth {
    fn render(&mut self, out: &mut StereoBlock, ctx: &RenderCtx) {
        let relative_start = ctx.start.0.saturating_sub(self.loop_offset);
        let rel_ctx = RenderCtx {
            sr: ctx.sr,
            start: SampleTime(relative_start),
        };
        self.synth.render(out, &rel_ctx);
        if self.synth.is_finished(SampleTime(relative_start + out.frames() as u64)) {
            self.loop_offset = ctx.start.0 + out.frames() as u64;
            self.synth = HumanMusicSynth::new(&self.score, &self.world, self.sr);
        }
    }

    fn is_finished(&self, _at: SampleTime) -> bool {
        false
    }
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
    active_audio: Option<AudioDevice>,
    audio_mode: AudioMode,
    audio_status_msg: Option<String>,
}

const AXIS_NAMES: [&str; 8] = [
    "MOTIF     [Lead Contour] ",
    "RIFF      [Secondary Hook]",
    "GROOVE    [Drum Pocket]  ",
    "H-CONTOUR [Harmonic Path]",
    "H-LOOP    [Cadence Cycle]",
    "FORM      [AABA Phrasing]",
    "ORCHESTRA [Timbre Map]   ",
    "BASS      [Sub-Root Line]",
];

fn stop_audio(model: &mut Model) {
    model.active_audio = None;
    model.audio_mode = AudioMode::Stopped;
    model.audio_status_msg = Some("Audio Muted".into());
}

fn play_cover(model: &mut Model) {
    model.active_audio = None;
    if let Some(comp) = &model.cover_comp {
        let looper = LoopingSynth::new(comp.score.clone(), model.world.clone(), SampleRate::STUDIO);
        let source: Box<dyn AudioSource + Send> = Box::new(looper);
        match AudioDevice::play(source, SampleRate::STUDIO) {
            Ok(device) => {
                model.active_audio = Some(device);
                model.audio_mode = AudioMode::PlayingCover;
                model.audio_status_msg = Some(format!("Streaming Cover [{}] (Looping @ 48kHz)", model.world.name));
            }
            Err(e) => {
                model.audio_mode = AudioMode::Stopped;
                model.audio_status_msg = Some(format!("Audio Device Error: {:?}", e));
            }
        }
    } else {
        model.audio_mode = AudioMode::Stopped;
        model.audio_status_msg = Some("Cover refused by target world".into());
    }
}

fn play_source(model: &mut Model) {
    model.active_audio = None;
    if let Ok(score) = model.source.melody_score() {
        let looper = LoopingSynth::new(score, model.world.clone(), SampleRate::STUDIO);
        let source: Box<dyn AudioSource + Send> = Box::new(looper);
        match AudioDevice::play(source, SampleRate::STUDIO) {
            Ok(device) => {
                model.active_audio = Some(device);
                model.audio_mode = AudioMode::PlayingSource;
                model.audio_status_msg = Some("Auditioning Source Melody (A/B Test)".into());
            }
            Err(e) => {
                model.audio_mode = AudioMode::Stopped;
                model.audio_status_msg = Some(format!("Audio Device Error: {:?}", e));
            }
        }
    }
}

fn export_wavs(model: &mut Model) {
    let mut msgs = Vec::new();
    if let Some(comp) = &model.cover_comp {
        let mut synth = HumanMusicSynth::new(&comp.score, &model.world, SampleRate::STUDIO);
        let renderer = OfflineRenderer::new(SampleRate::STUDIO, 1024);
        let result = renderer.render_seconds(&mut synth, comp.score.total_beats as f64 * (60.0 / model.world.tempo_bpm as f64));
        let path = "/tmp/theseus_cover.wav";
        if write_wav_i16(path, &result.audio, SampleRate::STUDIO).is_ok() {
            msgs.push(format!("Saved {}", path));
        }
    }
    if let Ok(score) = model.source.melody_score() {
        let mut synth = HumanMusicSynth::new(&score, &model.world, SampleRate::STUDIO);
        let renderer = OfflineRenderer::new(SampleRate::STUDIO, 1024);
        let result = renderer.render_seconds(&mut synth, score.total_beats * (60.0 / 100.0));
        let path = "/tmp/theseus_source.wav";
        if write_wav_i16(path, &result.audio, SampleRate::STUDIO).is_ok() {
            msgs.push(format!("Saved {}", path));
        }
    }
    model.audio_status_msg = Some(msgs.join(" | "));
}

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

    let was_playing_cover = model.audio_mode == AudioMode::PlayingCover;

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
                    if was_playing_cover {
                        play_cover(model);
                    }
                }
                Err(CoverError::Rejected(admission)) => {
                    model.cover_comp = None;
                    model.admission = Some(*admission);
                    model.error_msg = Some("Lawful Refusal: Invariant Violation In Target World".into());
                    if was_playing_cover {
                        stop_audio(model);
                    }
                }
                Err(e) => {
                    model.cover_comp = None;
                    model.admission = None;
                    model.error_msg = Some(format!("{:?}", e));
                    if was_playing_cover {
                        stop_audio(model);
                    }
                }
            }
        }
        Err(e) => {
            model.quotient = None;
            model.cover_comp = None;
            model.admission = None;
            model.error_msg = Some(format!("Extraction error: {:?}", e));
            if was_playing_cover {
                stop_audio(model);
            }
        }
    }
}

/// Renders a full-width real-time audio spectrum & waveform visualizer on HalfBlockCanvas.
fn render_spectrum_banner(width: u16, time_secs: f32, tempo: f32, world_name: &str, _active_axes_count: usize) -> Element<()> {
    let height = 3;
    let mut canvas = HalfBlockCanvas::new(width, height);
    let pw = canvas.pixel_width() as f32;
    let ph = canvas.pixel_height() as f32;

    let beat_phase = (time_secs * (tempo / 60.0) * std::f32::consts::PI * 2.0).sin().abs();

    for x in 0..canvas.pixel_width() as i32 {
        let x_norm = x as f32 / pw.max(1.0);
        // Multi-harmonic spectrum bars
        let f1 = (x_norm * 14.0 + time_secs * 3.0).sin();
        let f2 = (x_norm * 28.0 - time_secs * 5.0).cos();
        let f3 = (x_norm * 7.0 + time_secs * 1.5).sin();
        let amp = ((f1 * 0.4 + f2 * 0.3 + f3 * 0.3).abs() * (0.6 + 0.4 * beat_phase)).clamp(0.05, 1.0);

        let bar_h = (amp * ph).round() as i32;

        for y in (canvas.pixel_height() as i32 - bar_h)..canvas.pixel_height() as i32 {
            let y_norm = y as f32 / ph.max(1.0);
            let rgb = match world_name {
                "VAPOR95" => {
                    // Sunset gradient: violet -> hot neon pink -> electric cyan
                    let r = ((0.8 - y_norm * 0.5) * 255.0) as u8;
                    let g = ((y_norm * 0.8) * 240.0) as u8;
                    let b = ((0.5 + y_norm * 0.5) * 255.0) as u8;
                    (r, g, b)
                }
                "BLACK_ICE" => {
                    // Cryogenic stealth: slate -> electric laser blue -> ice white
                    let r = ((y_norm * y_norm) * 160.0) as u8;
                    let g = ((y_norm * 0.9) * 255.0) as u8;
                    let b = 255;
                    (r, g, b)
                }
                _ => {
                    // SWISS_SIGNAL: pure international typographic red and crisp white
                    if y_norm > 0.6 {
                        (255, 255, 255)
                    } else {
                        (240, 20, 45)
                    }
                }
            };
            canvas.set_pixel(x, y, rgb);
        }
    }

    raw(Node::rich_text_wrapped(canvas.to_rich_text(), WrapMode::NoWrap))
}

/// Renders a dynamic cybernetic plasma/energy field for the center quotient core.
fn render_reactor_canvas(width: u16, height: u16, time_secs: f32, world_name: &str, active_axes_count: usize) -> Element<()> {
    let mut canvas = HalfBlockCanvas::new(width, height);
    let pw = canvas.pixel_width() as f32;
    let ph = canvas.pixel_height() as f32;

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

    raw(Node::rich_text_wrapped(canvas.to_rich_text(), WrapMode::NoWrap))
}

/// Renders a dynamic piano roll on BrailleCanvas for the notes in a track.
fn render_piano_roll_braille(
    notes: &[(f32, f32, i32)],
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

    let min_pitch = notes.iter().map(|n| n.2).min().unwrap_or(60).max(36);
    let max_pitch = notes.iter().map(|n| n.2).max().unwrap_or(74).max(min_pitch + 12);
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
                canvas.set(x, y - 1);
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

    // 1. SLEEK TOP METRIC TELEMETRY (Cyberpunk Console Header)
    let audio_badge = match model.audio_mode {
        AudioMode::PlayingCover => format!("▶ LIVE COVER [{}]", model.world.name),
        AudioMode::PlayingSource => "▶ AUDITIONING REF".into(),
        AudioMode::Stopped => "■ AUDIO MUTED".into(),
    };
    let audio_tone = match model.audio_mode {
        AudioMode::PlayingCover => Tone::Success,
        AudioMode::PlayingSource => Tone::Accent,
        AudioMode::Stopped => Tone::Neutral,
    };

    let header_line = row()
        .child(text("◆ PROJECT THESEUS").tone(Tone::Accent).emphasis(Emphasis::Strong))
        .child(text(" // HUMAN_MUSIC RECOMBINANT QUOTIENT ENGINE ").tone(Tone::Neutral).emphasis(Emphasis::Muted))
        .child(spacer())
        .child(text(format!("[{}] ", model.world.name)).tone(Tone::Accent).emphasis(Emphasis::Strong))
        .child(text(format!("[FIDELITY: {}] ", model.preset.label())).tone(Tone::Info).emphasis(Emphasis::Strong))
        .child(text(format!("[SEED: {}] ", model.seed)).tone(Tone::Neutral).emphasis(Emphasis::Muted))
        .child(text(format!("[{}] ", audio_badge)).tone(audio_tone).emphasis(Emphasis::Strong));

    // Full-width real-time spectrum banner
    let spectrum_w = term_w.saturating_sub(2).min(100);
    let spectrum_element = render_spectrum_banner(spectrum_w, time_s, model.world.tempo_bpm, model.world.name, active_count);

    // 2. THE THREE TIERS: (Reference DNA) -> (CoverMap Quotient) -> (Fresh Cover)

    // --- TIER 1: REFERENCE PERFORMANCE (Beethoven 64-beat Soprano) ---
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
    let col_w = (term_w / 3).max(18).min(32);
    let ref_canvas = render_piano_roll_braille(
        &ref_notes,
        ref_beats,
        col_w,
        3,
        Style::new().fg(Color::Rgb(0, 240, 255)),
    );

    let ref_col = column()
        .child(text("1. REFERENCE SOURCE DNA").tone(Tone::Accent).emphasis(Emphasis::Strong))
        .child(text("Ode to Joy (Beethoven) · 64b").tone(Tone::Neutral).emphasis(Emphasis::Muted))
        .child(ref_canvas)
        .child(text(format!("Meter 4/4 · D-Maj · {} Notes", ref_notes.len())).tone(Tone::Neutral).emphasis(Emphasis::Faint));

    // --- TIER 3: FRESH COVER PERFORMANCE ---
    let cover_canvas = if let Some(comp) = &model.cover_comp {
        let cov_notes: Vec<(f32, f32, i32)> = comp
            .score
            .notes
            .iter()
            .map(|n| (n.start_beat as f32, n.dur_beats, n.pitch))
            .collect();
        let cov_beats = comp.score.total_beats as f32;
        render_piano_roll_braille(
            &cov_notes,
            cov_beats,
            col_w,
            3,
            Style::new().fg(Color::Rgb(50, 255, 120)),
        )
    } else {
        render_piano_roll_braille(&[], 1.0, col_w, 3, Style::new().fg(Color::Rgb(255, 50, 50)))
    };

    let cover_col = if let Some(comp) = &model.cover_comp {
        column()
            .child(text(format!("3. FRESH COVER [{}]", model.world.name)).tone(Tone::Success).emphasis(Emphasis::Strong))
            .child(text(format!("Synthesized @ {:.0} BPM · Looping", model.world.tempo_bpm)).tone(Tone::Success))
            .child(cover_canvas)
            .child(text(format!("Score: {} notes · Receipts: PASS", comp.score.notes.len())).tone(Tone::Neutral).emphasis(Emphasis::Faint))
    } else {
        let err_desc = model.error_msg.clone().unwrap_or_else(|| "World Refusal".into());
        column()
            .child(text("3. ⚡ LAWFUL REFUSAL ⚡").tone(Tone::Danger).emphasis(Emphasis::Strong))
            .child(text("Target world rejected pins").tone(Tone::Danger))
            .child(cover_canvas)
            .child(text(err_desc).tone(Tone::Warning).emphasis(Emphasis::Strong))
    };

    // --- TIER 2: IDENTITY QUOTIENT (THE MIDDLE STAR) ---
    let gauge_blocks = (pct_preserved / 5.0).round() as usize;
    let filled_bar = "█".repeat(gauge_blocks);
    let empty_bar = "░".repeat(20 - gauge_blocks);
    let gauge_str = format!("[{}{}] {:.1}%", filled_bar, empty_bar, pct_preserved);

    let identity_state_desc = if active_count == 8 {
        "CLASSICAL IDENTITY INTACT"
    } else if active_count >= 5 {
        "SURFACE TRANSLATION SHIFT"
    } else if active_count >= 2 {
        "CRITICAL IDENTITY DRIFT"
    } else if active_count == 1 {
        "WTF THRESHOLD (NEW MUSIC)"
    } else {
        "TOTAL SHIP OF THESEUS DISSOLUTION"
    };

    let gauge_tone = if active_count >= 6 {
        Tone::Success
    } else if active_count >= 3 {
        Tone::Warning
    } else {
        Tone::Danger
    };

    let reactor_w = (term_w.saturating_sub(4)).min(74);
    let reactor_element = render_reactor_canvas(reactor_w, 3, time_s, model.world.name, active_count);

    let mut conduit_col = column();
    for i in 0..8 {
        let is_on = model.axes_on[i];
        let axis_label = AXIS_NAMES[i];
        let key_num = i + 1;

        let conduit_line = if is_on {
            let anim_offset = ((time_s * 7.0) as usize + i * 2) % 24;
            let mut wire = "════════════════════════".chars().collect::<Vec<_>>();
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
                    text(format!(" ──▶▶ {} ▶▶── ", wire_str))
                        .tone(Tone::Success)
                        .emphasis(Emphasis::Strong),
                )
                .child(text("PRESERVED").tone(Tone::Success).emphasis(Emphasis::Strong))
        } else {
            row()
                .child(
                    text(format!("[{}] {}", key_num, axis_label))
                        .tone(Tone::Neutral)
                        .emphasis(Emphasis::Faint),
                )
                .child(
                    text(" ──⚡ ░░░░░░ [SEVERED: PURGED] ░░░░░░ ⚡── ")
                        .tone(Tone::Danger)
                        .emphasis(Emphasis::Faint),
                )
                .child(text("STRIPPED").tone(Tone::Neutral).emphasis(Emphasis::Faint))
        };

        conduit_col = conduit_col.child(conduit_line);
    }

    let middle_tier = column()
        .child(
            row()
                .child(text("THESEUS GAUGE: ").tone(Tone::Neutral).emphasis(Emphasis::Strong))
                .child(text(gauge_str).tone(gauge_tone).emphasis(Emphasis::Strong))
                .child(spacer())
                .child(text(identity_state_desc).tone(gauge_tone).emphasis(Emphasis::Strong)),
        )
        .child(reactor_element)
        .child(conduit_col);

    // 3. SLEEK CONTROL COCKPIT (Clean Keycaps, No 90s Boxes)
    let status_text = model
        .audio_status_msg
        .as_ref()
        .map(|s| text(format!(">> {}", s)).tone(Tone::Accent))
        .unwrap_or_else(|| text(">> Ready. Press P to Stream Cover, O to Audition Reference.").tone(Tone::Neutral));

    let control_hud = column()
        .child(status_text)
        .child(
            row()
                .child(text("[P/Space] Play Cover ").tone(Tone::Success).emphasis(Emphasis::Strong))
                .child(text("| [O] Audition Ref ").tone(Tone::Accent).emphasis(Emphasis::Strong))
                .child(text("| [1-8] Axes ").tone(Tone::Info))
                .child(text("| [F] Fidelity ").tone(Tone::Info))
                .child(text("| [W] World ").tone(Tone::Success))
                .child(text("| [D] WTF Snap ").tone(Tone::Warning).emphasis(Emphasis::Strong))
                .child(text("| [E] Export WAV ").tone(Tone::Neutral))
                .child(text("| [Q] Quit").tone(Tone::Danger)),
        );

    // ASSEMBLE CLEAN HIGH-TECH INTERFACE
    screen().child(
        column()
            .child(header_line)
            .child(spectrum_element)
            .child(
                row()
                    .child(ref_col)
                    .child(spacer())
                    .child(cover_col),
            )
            .child(divider())
            .child(middle_tier)
            .child(divider())
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
                KeyCode::Char('q') | KeyCode::Esc => {
                    stop_audio(model);
                    return Control::Quit;
                }
                KeyCode::Char('p') | KeyCode::Char(' ') => {
                    if model.audio_mode == AudioMode::PlayingCover {
                        stop_audio(model);
                    } else {
                        play_cover(model);
                    }
                }
                KeyCode::Char('o') => {
                    if model.audio_mode == AudioMode::PlayingSource {
                        stop_audio(model);
                    } else {
                        play_source(model);
                    }
                }
                KeyCode::Char('e') => {
                    export_wavs(model);
                }
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
                    if model.axes_on.iter().filter(|&&x| x).count() > 1 {
                        model.axes_on = [false; 8];
                        model.axes_on[0] = true;
                    } else if model.axes_on[0] {
                        model.axes_on[0] = false;
                    } else {
                        model.axes_on[0] = true;
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
    let ref_song = ReferenceSong::from_tsv(&tsv, "sop")
        .or_else(|_| ReferenceSong::from_tsv(&tsv, "lead"))
        .expect("Failed to load reference TSV fixture");

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
        active_audio: None,
        audio_mode: AudioMode::Stopped,
        audio_status_msg: None,
    };

    regenerate(&mut model);

    // Auto-start live looping audio stream of the freshly synthesized cover
    play_cover(&mut model);

    App::fullscreen().skin(skins::VAPOR95).run(model, update, view)?;
    Ok(())
}
