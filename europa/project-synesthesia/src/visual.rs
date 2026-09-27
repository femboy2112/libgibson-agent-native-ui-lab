//! Signal-first rendering. The sequencer, routing path, spectrogram, stereo
//! waveform, and mixer share one clipped sub-cell surface.

use std::time::Instant;

use gibson::cell::{Cell, Color, Style};
use gibson::node::Node;
use gibson::surface::{BorderType, Rect, Surface};
use gibson::{BrailleCanvas, SubcellGlyphMode};

use crate::engine::{Engine, Frame, FFT_BINS, STEPS_PER_TRACK, TRACK_COUNT};

pub const HISTORY_CAPACITY: usize = 96;

#[derive(Clone)]
pub struct SpectrumHistory {
    rows: [[f32; FFT_BINS]; HISTORY_CAPACITY],
    next: usize,
    len: usize,
}

impl Default for SpectrumHistory {
    fn default() -> Self {
        Self {
            rows: [[0.0; FFT_BINS]; HISTORY_CAPACITY],
            next: 0,
            len: 0,
        }
    }
}

impl SpectrumHistory {
    pub fn push(&mut self, row: &[f32; FFT_BINS]) {
        self.rows[self.next].copy_from_slice(row);
        self.next = (self.next + 1) % HISTORY_CAPACITY;
        self.len = (self.len + 1).min(HISTORY_CAPACITY);
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Read by age: zero is the newest retained spectrum.
    pub fn age(&self, age: usize) -> Option<&[f32; FFT_BINS]> {
        if age >= self.len {
            return None;
        }
        let index = (self.next + HISTORY_CAPACITY - 1 - age) % HISTORY_CAPACITY;
        Some(&self.rows[index])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Grid,
    Patch,
    Mixer,
}

pub struct ViewState<'a> {
    pub playing: bool,
    pub performance: bool,
    pub focus: Focus,
    pub selected_track: usize,
    pub selected_step: usize,
    pub glyph_mode: SubcellGlyphMode,
    pub seed: u64,
    pub modal: Option<&'a str>,
    pub audio_status: &'a str,
    pub fps: u32,
    pub events_seen: u64,
    pub key_events_seen: u64,
    pub input_profile: bool,
}

pub struct Scene {
    pub root: Node,
    pub surface_us: u64,
    pub generated_nodes: usize,
}

const TRACK_COLORS: [(u8, u8, u8); 4] = [
    (76, 232, 240),
    (255, 105, 154),
    (255, 203, 84),
    (171, 132, 255),
];

pub fn build(
    engine: &Engine,
    frame: &Frame,
    history: &SpectrumHistory,
    view: &ViewState<'_>,
    width: u16,
    height: u16,
) -> Scene {
    let surface_start = Instant::now();
    let raster = signal_surface(
        engine,
        frame,
        history,
        view,
        width,
        height.saturating_sub(4),
    );
    let surface_us = surface_start.elapsed().as_micros() as u64;

    let status = if view.playing { "RUN" } else { "HOLD" };
    let mode = if view.performance {
        "CONCERT / SIGNAL FIELD"
    } else {
        "ARRANGE / SIGNAL FLOW"
    };
    let input_receipts = if view.input_profile {
        format!(
            " EVT {:06} KEY {:06}",
            view.events_seen, view.key_events_seen
        )
    } else {
        String::new()
    };
    let header = if width >= 100 {
        format!(
            "SYNESTHESIA{input_receipts}   {mode}   {status}   {:03.0} BPM   STEP {:02}   {:02} FPS   SEED {:016X}",
            engine.bpm,
            frame.step_index as usize + 1,
            view.fps,
            view.seed
        )
    } else {
        format!(
            "SYNESTHESIA{input_receipts}  {status}  {:03.0} BPM  STEP {:02}/16  {}",
            engine.bpm,
            frame.step_index as usize + 1,
            if view.performance {
                "CONCERT"
            } else {
                "ARRANGE"
            },
        )
    };
    let secondary = if width >= 160 {
        format!(
            "OSC {:?}  VCF {:>5.0} Hz  Q {:0.2}  ADSR {:0.2}/{:0.2}/{:0.2}/{:0.2}   OUT {:0.2}  {}",
            engine.patch.waveform,
            engine.patch.cutoff_hz,
            engine.patch.resonance,
            engine.patch.attack,
            engine.patch.decay,
            engine.patch.sustain,
            engine.patch.release,
            frame.routing[6],
            view.audio_status,
        )
    } else if width >= 100 {
        format!(
            "OSC {:?}  VCF {:.0} Hz  Q {:.2}  OUT {:.2}  {}",
            engine.patch.waveform,
            engine.patch.cutoff_hz,
            engine.patch.resonance,
            frame.routing[6],
            view.audio_status,
        )
    } else {
        format!(
            "OSC {:?}  LP {:.0} Hz  {}",
            engine.patch.waveform, engine.patch.cutoff_hz, view.audio_status,
        )
    };
    let footer = if width >= 140 {
        format!(
            "{} T{} S{:02}  | Space play  Enter note  arrows step/track  Tab focus  [ ] BPM  F filter  E ADSR  W wave  M mute  S solo  P mode  Q quit",
            focus_name(view.focus),
            view.selected_track + 1,
            view.selected_step + 1,
        )
    } else if width >= 90 {
        format!(
            "{} T{} S{:02} | Space play  Enter note  arrows move  Tab focus  F/E/W sound  P concert  Q quit",
            focus_name(view.focus),
            view.selected_track + 1,
            view.selected_step + 1,
        )
    } else if width >= 70 {
        format!(
            "{} T{} S{:02} | Space  Enter  arrows  Tab  F/E/W  P mode  Q quit",
            focus_name(view.focus),
            view.selected_track + 1,
            view.selected_step + 1,
        )
    } else {
        format!(
            "{} T{} S{:02} | SPC ENT arrows TAB P mode Q quit",
            focus_name(view.focus),
            view.selected_track + 1,
            view.selected_step + 1,
        )
    };

    let root = Node::col()
        .percent_width(100.0)
        .percent_height(100.0)
        .child(Node::text(header, Style::new().fg(Color::rgb(236, 244, 255)).bold()).height(1.0))
        .child(Node::text(secondary, Style::new().fg(Color::rgb(123, 156, 185))).height(1.0))
        .child(Node::raster(raster).percent_width(100.0).flex_grow(1.0))
        .child(Node::text(footer, Style::new().fg(Color::rgb(148, 170, 194))).height(1.0));
    Scene {
        root,
        surface_us,
        generated_nodes: 5,
    }
}

fn focus_name(focus: Focus) -> &'static str {
    match focus {
        Focus::Grid => "GRID",
        Focus::Patch => "PATCH",
        Focus::Mixer => "MIXER",
    }
}

fn signal_surface(
    engine: &Engine,
    frame: &Frame,
    history: &SpectrumHistory,
    view: &ViewState<'_>,
    width: u16,
    height: u16,
) -> Surface {
    let mut canvas = BrailleCanvas::new(width, height);
    let (pw, ph) = (canvas.pixel_width() as i32, canvas.pixel_height() as i32);
    if pw < 96 || ph < 28 {
        let mut tiny = canvas.to_surface_mode(Style::default(), view.glyph_mode);
        tiny.print_str(
            0,
            0,
            "SYNESTHESIA // resize for signal field",
            Style::new().bold(),
            None,
        );
        return tiny;
    }

    let compact = ph < 88 || pw < 112;
    let seq_label_y = 0;
    let grid_top = if compact { 7 } else { 10 };
    let lane_h = if view.performance {
        if compact {
            4
        } else {
            5
        }
    } else if compact {
        4
    } else {
        ((ph * 7) / 100).clamp(5, 10)
    };
    let grid_bottom = grid_top + lane_h * TRACK_COUNT as i32;
    // Labels have a real gutter; notes and graph lines no longer run beneath
    // text. The narrow layout abbreviates labels to keep usable note columns.
    let gutter_cells = if width >= 90 { 21 } else { 12 };
    let x_left = gutter_cells * 2;
    let x_right = pw - 3;
    let grid_width = (x_right - x_left).max(STEPS_PER_TRACK as i32);
    let step_width = grid_width as f32 / STEPS_PER_TRACK as f32;

    draw_sequencer(
        &mut canvas,
        engine,
        frame,
        view,
        grid_top,
        grid_bottom,
        lane_h,
        x_left,
        grid_width,
    );

    let graph_top = (grid_bottom + if compact { 5 } else { 12 }).min(ph - 42);
    draw_routing(&mut canvas, frame, graph_top, x_left, grid_width);

    if view.performance {
        draw_performance_field(&mut canvas, frame, history, graph_top, ph);
    } else {
        draw_spectrogram(
            &mut canvas,
            history,
            graph_top + if compact { 19 } else { 24 },
            ph * 69 / 100,
            x_left,
            grid_width,
        );
        draw_spectrum(
            &mut canvas,
            frame,
            graph_top + if compact { 19 } else { 24 },
            ph * 69 / 100,
            x_left,
            grid_width,
        );
        draw_waveform(
            &mut canvas,
            frame,
            ph * 84 / 100,
            ph - 5,
            x_left,
            grid_width,
        );
        draw_mixer(&mut canvas, frame, engine, ph - 4, x_left, grid_width);
    }

    // A note event leaves the piano roll on the same current-step rail that
    // reaches the oscillator; the pulse position is tied to sequencer phase.
    let step_x = x_left + (frame.step_index as f32 * step_width) as i32;
    let pulse_y = graph_top + 1;
    canvas.line(step_x, grid_bottom + 1, step_x, pulse_y);

    // The output node descends into both the spectral and stereo fields. Its
    // junction size follows post-mixer signal energy rather than wall time.
    let bus_x = x_right - 1;
    let output_energy = frame.routing[6].clamp(0.0, 1.0);
    if output_energy > 0.005 {
        for y in (graph_top + 5..=ph - 4).step_by(if output_energy > 0.4 { 1 } else { 3 }) {
            canvas.set(bus_x, y);
        }
        canvas.circle(bus_x, ph * 69 / 100, 1 + (output_energy * 4.0) as i32);
        canvas.circle(bus_x, ph * 84 / 100, 1 + (output_energy * 4.0) as i32);
    }

    let mut surface = canvas.to_surface_mode(Style::default(), view.glyph_mode);
    colorize(
        &mut surface,
        &canvas,
        frame,
        ColorRegions {
            grid_top,
            grid_bottom,
            lane_h,
            graph_top,
            graph_left: x_left,
            ph,
        },
    );
    overlay_labels(
        &mut surface,
        engine,
        frame,
        view,
        seq_label_y,
        grid_top,
        lane_h,
        graph_top,
        ph,
        gutter_cells as u16,
    );
    if let Some(label) = view.modal {
        overlay_modal(&mut surface, label);
    }
    surface
}

#[allow(clippy::too_many_arguments)]
fn draw_sequencer(
    canvas: &mut BrailleCanvas,
    engine: &Engine,
    frame: &Frame,
    view: &ViewState<'_>,
    top: i32,
    bottom: i32,
    lane_h: i32,
    left: i32,
    width: i32,
) {
    let right = left + width;
    for step in 0..=STEPS_PER_TRACK {
        let x = left + (step as f32 * width as f32 / STEPS_PER_TRACK as f32).round() as i32;
        if step % 4 == 0 {
            canvas.line(x, top, x, bottom);
        } else {
            for y in (top..=bottom).step_by(4) {
                canvas.set(x, y);
            }
        }
    }
    for track in 0..=TRACK_COUNT {
        let y = top + track as i32 * lane_h;
        canvas.line(left, y, right, y);
    }
    let playhead_x =
        left + (frame.step_index as f32 * width as f32 / STEPS_PER_TRACK as f32) as i32;
    canvas.line(playhead_x, top - 3, playhead_x, bottom + 2);

    for track_index in 0..TRACK_COUNT {
        let track = &engine.tracks[track_index];
        let lane_top = top + track_index as i32 * lane_h;
        for step_index in 0..STEPS_PER_TRACK {
            let step = track.steps[step_index];
            let Some(note) = step.note else { continue };
            let sx0 = left
                + (step_index as f32 * width as f32 / STEPS_PER_TRACK as f32).round() as i32
                + 1;
            let sx1 = left
                + ((step_index + 1) as f32 * width as f32 / STEPS_PER_TRACK as f32).round() as i32
                - 1;
            let span = (sx1 - sx0 + 1).max(1);
            let thickness = (span as f32 * step.velocity.clamp(0.0, 1.0) * 0.55)
                .round()
                .max(1.0) as i32;
            let center_x = (sx0 + sx1) / 2;
            let midi_height = ((note.saturating_sub(36).min(48) as f32 / 48.0)
                * (lane_h - 2) as f32)
                .round() as i32;
            let note_y =
                (lane_top + lane_h - 2 - midi_height).clamp(lane_top + 1, lane_top + lane_h - 1);
            canvas.filled_rect(center_x - thickness / 2, note_y, thickness, 1);
            canvas.line(center_x, note_y + 1, center_x, lane_top + lane_h - 2);
        }
        let active_end = left
            + (track.pattern_length.clamp(1, STEPS_PER_TRACK as u8) as f32 * width as f32
                / STEPS_PER_TRACK as f32) as i32;
        canvas.line(active_end, lane_top + 1, active_end, lane_top + lane_h - 1);
    }
    let selected_x =
        left + ((view.selected_step as f32 + 0.5) * width as f32 / STEPS_PER_TRACK as f32) as i32;
    let selected_y = top + view.selected_track as i32 * lane_h + 1;
    if view.focus == Focus::Grid {
        canvas.rect(selected_x - 2, selected_y, 5, (lane_h - 1).max(2));
    }
}

fn draw_routing(canvas: &mut BrailleCanvas, frame: &Frame, y: i32, left: i32, width: i32) {
    const N: usize = 7;
    let mut xs = [0_i32; N];
    let mut ys = [0_i32; N];
    let base_y = y + 5;
    for i in 0..N {
        xs[i] = left + (i as f32 * width as f32 / (N - 1) as f32).round() as i32;
        let energy = frame.routing[i].clamp(0.0, 1.5) / 1.5;
        ys[i] = (base_y - (energy * 5.0) as i32).clamp(y + 1, y + 10);
        if i > 0 {
            let amp = (frame.routing[i].clamp(0.0, 1.0) * 5.0) as i32;
            for thick in 0..amp {
                canvas.line(xs[i - 1], ys[i - 1] + thick, xs[i], ys[i] + thick);
            }
            // One moving bead per graph edge, locked to the sixteenth-note phase.
            let phase = (frame.beat_phase * 4.0).fract();
            let px = (xs[i - 1] as f32 + (xs[i] - xs[i - 1]) as f32 * phase) as i32;
            let py = (ys[i - 1] as f32 + (ys[i] - ys[i - 1]) as f32 * phase) as i32;
            canvas.circle(px, py, 1 + (energy * 2.0) as i32);
        }
        canvas.circle(
            xs[i],
            ys[i],
            2 + (frame.routing[i].clamp(0.0, 1.0) * 3.0) as i32,
        );
    }
}

fn draw_spectrogram(
    canvas: &mut BrailleCanvas,
    history: &SpectrumHistory,
    top: i32,
    bottom: i32,
    left: i32,
    width: i32,
) {
    if history.is_empty() || bottom <= top || width <= 2 {
        return;
    }
    let ph = canvas.pixel_height() as i32;
    let top = top.clamp(0, ph - 1);
    let bottom = bottom.clamp(top + 1, ph - 1);
    let rows = history.len().min(width as usize / 2).max(1);
    let x0 = (width - rows as i32 * 2).max(0) + left;
    let bins = [
        1_usize,
        2,
        3,
        4,
        6,
        8,
        11,
        15,
        20,
        27,
        37,
        51,
        70,
        96,
        132,
        180,
        245,
        330,
        430,
        FFT_BINS - 1,
    ];
    for column in 0..rows {
        let Some(spectrum) = history.age(rows - column - 1) else {
            continue;
        };
        let x = x0 + column as i32 * 2;
        for (band, &bin) in bins.iter().enumerate() {
            if bin >= FFT_BINS {
                continue;
            }
            let magnitude = spectrum[bin].clamp(0.0, 1.0);
            let threshold = 0.007 + band as f32 * 0.0004;
            if magnitude <= threshold {
                continue;
            }
            let y =
                bottom - ((band as f32 / (bins.len() - 1) as f32) * (bottom - top) as f32) as i32;
            canvas.set(x, y);
            if magnitude > 0.035 {
                canvas.set(x, y - 1);
            }
            if magnitude > 0.12 {
                canvas.set(x + 1, y);
            }
        }
    }
}

fn draw_spectrum(
    canvas: &mut BrailleCanvas,
    frame: &Frame,
    top: i32,
    bottom: i32,
    left: i32,
    width: i32,
) {
    if bottom <= top || width < 4 {
        return;
    }
    let right = left + width;
    let bins = 48.min(FFT_BINS - 1);
    let mut points = Vec::with_capacity(bins);
    for i in 1..=bins {
        let ratio = i as f32 / bins as f32;
        let source_bin = (ratio * ratio * (FFT_BINS - 1) as f32) as usize;
        let magnitude = frame.spectrum[source_bin].clamp(0.0, 1.0);
        let x = left + (i as f32 * width as f32 / bins as f32) as i32;
        let y = bottom - (magnitude * (bottom - top) as f32 * 2.2).round() as i32;
        let y = y.clamp(top, bottom);
        points.push((x.clamp(left, right), y));
        canvas.line(x, bottom, x, y);
    }
    canvas.polyline(&points);
}

fn draw_waveform(
    canvas: &mut BrailleCanvas,
    frame: &Frame,
    top: i32,
    bottom: i32,
    left: i32,
    width: i32,
) {
    if bottom <= top || width < 4 {
        return;
    }
    let height = bottom - top;
    let center = top + height / 2;
    let half = (height / 2 - 1).max(1) as f32;
    let px_width = canvas.pixel_width() as i32;
    let right = (left + width).min(px_width - 1);
    let span = (right - left).max(1);
    let buckets = frame.waveform.min.len();
    let mut previous_l = None;
    let mut previous_r = None;
    for x in left..=right {
        let bucket = ((x - left) as usize * buckets / (span as usize + 1)).min(buckets - 1);
        let low = frame.waveform.min[bucket];
        let high = frame.waveform.max[bucket];
        let y_low = (center as f32 - high.clamp(-1.0, 1.0) * half).round() as i32;
        let y_high = (center as f32 - low.clamp(-1.0, 1.0) * half).round() as i32;
        canvas.line(x, y_low, x, y_high);

        let sample_index = bucket * (frame.fft_input.len() / buckets);
        let l = frame.samples[0][sample_index].clamp(-1.0, 1.0);
        let r = frame.samples[1][sample_index].clamp(-1.0, 1.0);
        let yl = (center as f32 - (height as f32 * 0.22) - l * half * 0.45).round() as i32;
        let yr = (center as f32 + (height as f32 * 0.22) - r * half * 0.45).round() as i32;
        if let Some((px, py)) = previous_l {
            canvas.line(px, py, x, yl);
        }
        if let Some((px, py)) = previous_r {
            canvas.line(px, py, x, yr);
        }
        previous_l = Some((x, yl));
        previous_r = Some((x, yr));
    }
}

fn draw_mixer(
    canvas: &mut BrailleCanvas,
    frame: &Frame,
    engine: &Engine,
    y: i32,
    left: i32,
    width: i32,
) {
    if width < 20 {
        return;
    }
    let right = left + width;
    canvas.line(left, y, right, y);
    let segment = width as f32 / TRACK_COUNT as f32;
    for track in 0..TRACK_COUNT {
        let x0 = left + (track as f32 * segment) as i32 + 2;
        let x1 = left + ((track + 1) as f32 * segment) as i32 - 2;
        let bar_width = (x1 - x0).max(2);
        let level = (frame.track_meters[track] * engine.tracks[track].gain * 10.0).clamp(0.0, 1.0);
        let filled = (bar_width as f32 * level).round() as i32;
        canvas.line(x0, y + 2, x1, y + 2);
        if filled > 0 {
            canvas.filled_rect(x0, y + 1, filled, 3);
        }
    }
}

fn draw_performance_field(
    canvas: &mut BrailleCanvas,
    frame: &Frame,
    history: &SpectrumHistory,
    graph_top: i32,
    ph: i32,
) {
    let width = canvas.pixel_width() as i32 - 4;
    let left = 2;
    let top = (graph_top + 20).clamp(0, ph - 8);
    let bottom = (ph - 3).max(top + 1);
    draw_spectrogram(canvas, history, top, bottom, left, width);
    // The live spectrum draws a rising harmonic terrain over the time field.
    let bins = 64.min(FFT_BINS - 1);
    let mut contour = Vec::with_capacity(bins);
    for i in 1..=bins {
        let source_bin = (i * i * (FFT_BINS - 1) / (bins * bins)).max(1);
        let magnitude = frame.spectrum[source_bin].clamp(0.0, 1.0);
        let x = left + i as i32 * width / bins as i32;
        let y = bottom - (magnitude * (bottom - top) as f32 * 1.75) as i32;
        contour.push((x, y.clamp(top, bottom)));
    }
    canvas.polyline(&contour);

    let wave_y = ph * 3 / 4;
    let amp = (ph as f32 * 0.12).max(3.0);
    let mut wave = Vec::with_capacity(frame.fft_input.len());
    let step = (frame.fft_input.len() / width.max(1) as usize).max(1);
    for (i, sample) in frame.fft_input.iter().step_by(step).enumerate() {
        let x = left + i as i32;
        let y = (wave_y as f32 - sample * amp).round() as i32;
        if x < width + left {
            wave.push((x, y));
        }
    }
    canvas.polyline(&wave);

    let pulse_x = 2 + ((width - 4) as f32 * frame.beat_phase) as i32;
    let pulse_y = top + ((bottom - top) as f32 * frame.beat_phase * 0.25) as i32;
    canvas.circle(
        pulse_x,
        pulse_y,
        2 + (frame.routing[6].clamp(0.0, 1.0) * 3.0) as i32,
    );
}

#[derive(Clone, Copy)]
struct ColorRegions {
    grid_top: i32,
    grid_bottom: i32,
    lane_h: i32,
    graph_top: i32,
    graph_left: i32,
    ph: i32,
}

fn colorize(surface: &mut Surface, canvas: &BrailleCanvas, frame: &Frame, regions: ColorRegions) {
    let ColorRegions {
        grid_top,
        grid_bottom,
        lane_h,
        graph_top,
        graph_left,
        ph,
    } = regions;
    for cy in 0..surface.height {
        for cx in 0..surface.width {
            let mask = canvas.mask_at(cx, cy);
            if mask == 0 {
                continue;
            }
            let y = cy as i32 * 4;
            let density = mask.count_ones() as f32 / 8.0;
            let (r, g, b) = if y < grid_bottom {
                let track = (((y - grid_top).max(0) / lane_h.max(1)) as usize).min(TRACK_COUNT - 1);
                let energy = (frame.track_meters[track] * 2.5).clamp(0.0, 1.0);
                mix(
                    (24, 30, 55),
                    TRACK_COLORS[track],
                    0.25 + density * 0.4 + energy * 0.35,
                )
            } else if y < graph_top + 16 {
                let graph_width = (canvas.pixel_width() as i32 - graph_left - 2).max(1);
                let node = (((cx as i32 * 2 - graph_left).max(0) as f32 / graph_width as f32) * 6.0)
                    .round()
                    .clamp(0.0, 6.0) as usize;
                let energy = frame.routing[node].clamp(0.0, 1.0);
                mix(
                    (60, 36, 126),
                    (54, 239, 210),
                    0.12 + energy * 0.65 + density * 0.2,
                )
            } else if y < ph * 78 / 100 {
                let bin = ((cx as f32 / surface.width.max(1) as f32).powi(2)
                    * (FFT_BINS - 1) as f32) as usize;
                heat(density * 0.65 + frame.spectrum[bin] * 0.35)
            } else {
                let energy = frame.routing[6].clamp(0.0, 1.0);
                mix(
                    (38, 58, 113),
                    (255, 186, 91),
                    0.25 + density * 0.5 + energy * 0.25,
                )
            };
            if let Some(cell) = surface.get_mut(cx, cy) {
                cell.style = Style::new().fg(Color::rgb(r, g, b)).bold_if(density > 0.72);
            }
        }
    }
}

trait BoldIf {
    fn bold_if(self, yes: bool) -> Self;
}

impl BoldIf for Style {
    fn bold_if(mut self, yes: bool) -> Self {
        self.bold = yes;
        self
    }
}

fn heat(value: f32) -> (u8, u8, u8) {
    let v = value.clamp(0.0, 1.0);
    if v < 0.33 {
        mix((3, 8, 28), (34, 72, 190), v / 0.33)
    } else if v < 0.66 {
        mix((34, 72, 190), (196, 45, 188), (v - 0.33) / 0.33)
    } else {
        mix((196, 45, 188), (255, 220, 130), (v - 0.66) / 0.34)
    }
}

fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    (
        (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t) as u8,
        (a.1 as f32 + (b.1 as f32 - a.1 as f32) * t) as u8,
        (a.2 as f32 + (b.2 as f32 - a.2 as f32) * t) as u8,
    )
}

#[allow(clippy::too_many_arguments)]
fn overlay_labels(
    surface: &mut Surface,
    engine: &Engine,
    frame: &Frame,
    view: &ViewState<'_>,
    label_y: i32,
    grid_top: i32,
    lane_h: i32,
    graph_top: i32,
    ph: i32,
    gutter_cells: u16,
) {
    let bright = Style::new().fg(Color::rgb(199, 217, 242)).bold();
    let dim = Style::new().fg(Color::rgb(112, 137, 170));
    surface.print_str(
        1,
        label_y as u16,
        "01 / SEQUENCER -> PITCH / VELOCITY",
        bright,
        None,
    );
    for track in 0..TRACK_COUNT {
        let lane_center = grid_top + track as i32 * lane_h + lane_h / 2;
        let step = engine.tracks[track].steps[view.selected_step];
        let name = ["KICK", "SNAR", "HAT ", "BASS"][track];
        let note = step.note.map(note_name).unwrap_or_else(|| "---".into());
        let state = if engine.tracks[track].muted {
            "M"
        } else if engine.tracks[track].solo {
            "S"
        } else {
            " "
        };
        let text = if surface.width >= 90 {
            format!(
                "{name} {state} {note} {:02} P{:02}",
                (step.velocity * 127.0).round() as u8,
                engine.tracks[track].pattern_length
            )
        } else {
            format!(
                "{name}{state}{note} {:02}",
                (step.velocity * 127.0).round() as u8
            )
        };
        surface.print_str(
            1,
            (lane_center / 4) as u16,
            &text,
            if track == view.selected_track {
                bright
            } else {
                dim
            },
            Some(gutter_cells.saturating_sub(1)),
        );
    }
    let graph_cell_y = (graph_top / 4).clamp(1, surface.height.saturating_sub(1) as i32) as u16;
    let route_label = if gutter_cells < 21 {
        "02 / ROUTE"
    } else {
        "02 / PATCH ROUTING"
    };
    surface.print_str(
        1,
        graph_cell_y,
        route_label,
        bright,
        Some(gutter_cells.saturating_sub(1)),
    );
    let names = ["SEQ", "OSC", "ENV", "VCF", "PAN", "MIX", "OUT"];
    for (i, name) in names.iter().enumerate() {
        let field_width = surface.width.saturating_sub(gutter_cells + 2);
        let label_span = field_width.saturating_sub(8);
        let x = gutter_cells + (i as u32 * label_span as u32 / 6) as u16;
        let energy = frame.routing[i];
        let s = if surface.width < 80 {
            name.to_string()
        } else {
            format!("{name} {:0.2}", energy)
        };
        surface.print_str(
            x,
            graph_cell_y.saturating_add(2),
            &s,
            dim,
            Some((label_span / 6).max(8).min(surface.width.saturating_sub(x))),
        );
    }
    if view.performance {
        surface.print_str(
            1,
            ((graph_top + 13) / 4).clamp(1, surface.height.saturating_sub(1) as i32) as u16,
            "03 / LIVE HARMONICS / STEREO WAVE",
            bright,
            None,
        );
    } else {
        surface.print_str(
            1,
            (graph_top + if ph < 88 { 13 } else { 17 }).max(0) as u16 / 4,
            "03 / FFT HISTORY -> FREQUENCY",
            bright,
            None,
        );
        surface.print_str(
            1,
            (ph * 79 / 100 / 4).clamp(0, surface.height.saturating_sub(1) as i32) as u16,
            "04 / L R  WAVEFORM",
            bright,
            None,
        );
        surface.print_str(
            1,
            surface.height.saturating_sub(1),
            if gutter_cells < 21 {
                "05 / MIX"
            } else {
                "05 / MIX  K S H B"
            },
            dim,
            Some(gutter_cells.saturating_sub(1)),
        );
    }
}

fn overlay_modal(surface: &mut Surface, label: &str) {
    if surface.width < 12 || surface.height < 3 {
        return;
    }
    let label_width = label.chars().count().min(u16::MAX as usize) as u16;
    let width = surface
        .width
        .saturating_sub(8)
        .min(label_width.saturating_add(4))
        .max(12.min(surface.width));
    let height = 3;
    let x = (surface.width - width) / 2;
    let y = (surface.height - height) / 2;
    let rect = Rect::new(x, y, width, height);
    let base = Style::new()
        .fg(Color::rgb(217, 231, 255))
        .bg(Color::rgb(10, 15, 32));
    surface.fill_rect(rect, Cell::space(base));
    surface.draw_border(
        rect,
        BorderType::Ascii,
        Style::new().fg(Color::rgb(71, 226, 222)).bold(),
    );
    surface.print_str(
        x + 2,
        y + 1,
        label,
        base.bold(),
        Some(width.saturating_sub(4)),
    );
}

fn note_name(midi: u8) -> String {
    const NOTES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    format!("{}{}", NOTES[midi as usize % 12], midi as i16 / 12 - 1)
}

pub fn history_fill(engine: &Engine, at_ms: u64, seed: u64, history: &mut SpectrumHistory) {
    let interval = 50_u64;
    for age in (0..HISTORY_CAPACITY).rev() {
        let t = at_ms.saturating_sub(age as u64 * interval);
        let frame = engine.render_at_ms(t, seed);
        history.push(&frame.spectrum);
    }
}
