//! Bounded, deterministic synthesizer and render-frame generator.
//!
//! This module has no audio-device or UI dependencies. `render_at_ms` is a
//! pure simulation of the public engine state at an absolute time, so frames
//! can be reproduced by passing the same engine, timestamp, and seed.

use std::f32::consts::{PI, TAU};

pub const SAMPLE_RATE: u32 = 48_000;
pub const FRAME_SAMPLES: usize = 1_024;
pub const FFT_BINS: usize = FRAME_SAMPLES / 2 + 1;
pub const TRACK_COUNT: usize = 4;
pub const STEPS_PER_TRACK: usize = 16;

// At the supported tempo/release limits, 64 prior step events cover every
// voice that can still be in its release tail.
const MAX_RELEASE_TICKS: u64 = 64;
const MAX_RELEASE_SECONDS: f32 = 3.0;
const FILTER_WARMUP_SAMPLES: usize = 2_048;
const WAVEFORM_BUCKETS: usize = 128;
const SAMPLES_PER_BUCKET: usize = FRAME_SAMPLES / WAVEFORM_BUCKETS;
pub const MAX_ACTIVE_NOTES: usize = TRACK_COUNT * (MAX_RELEASE_TICKS as usize + 1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waveform {
    Sine,
    Triangle,
    Saw,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub note: Option<u8>,
    /// Linear velocity in the range 0.0 to 1.0.
    pub velocity: f32,
}

impl Step {
    pub const EMPTY: Self = Self {
        note: None,
        velocity: 0.0,
    };

    pub const fn note(note: u8, velocity: f32) -> Self {
        Self {
            note: Some(note),
            velocity,
        }
    }
}

impl Default for Step {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Patch {
    pub waveform: Waveform,
    /// Envelope times are in seconds.
    pub attack: f32,
    pub decay: f32,
    /// Sustain level is linear, in the range 0.0 to 1.0.
    pub sustain: f32,
    pub release: f32,
    pub cutoff_hz: f32,
    /// Resonance control is normalized to the range 0.0 to 1.0.
    pub resonance: f32,
    pub gain: f32,
    /// Pan is -1.0 (left) to 1.0 (right).
    pub pan: f32,
}

impl Default for Patch {
    fn default() -> Self {
        Self {
            waveform: Waveform::Saw,
            attack: 0.008,
            decay: 0.16,
            sustain: 0.62,
            release: 0.22,
            cutoff_hz: 8_000.0,
            resonance: 0.12,
            gain: 0.72,
            pan: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    pub steps: [Step; STEPS_PER_TRACK],
    /// Pattern length is clamped to 1..=16 when rendered.
    pub pattern_length: u8,
    pub muted: bool,
    pub solo: bool,
    pub gain: f32,
    pub pan: f32,
}

impl Default for Track {
    fn default() -> Self {
        Self {
            steps: [Step::EMPTY; STEPS_PER_TRACK],
            pattern_length: STEPS_PER_TRACK as u8,
            muted: false,
            solo: false,
            gain: 0.8,
            pan: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Engine {
    /// Tempo is clamped to 20..=300 BPM when rendered.
    pub bpm: f32,
    pub patch: Patch,
    pub tracks: [Track; TRACK_COUNT],
}

impl Default for Engine {
    fn default() -> Self {
        let mut tracks = [Track::default(); TRACK_COUNT];
        tracks[0].gain = 0.9;
        tracks[0].steps[0] = Step::note(36, 1.0);
        tracks[0].steps[8] = Step::note(36, 0.9);

        tracks[1].gain = 0.72;
        tracks[1].pan = -0.12;
        tracks[1].steps[4] = Step::note(38, 0.88);
        tracks[1].steps[12] = Step::note(38, 0.88);

        tracks[2].gain = 0.38;
        tracks[2].pan = 0.14;
        for step in (0..STEPS_PER_TRACK).step_by(2) {
            tracks[2].steps[step] = Step::note(42, if step % 4 == 0 { 0.58 } else { 0.34 });
        }

        tracks[3].gain = 0.62;
        tracks[3].steps[0] = Step::note(48, 0.76);
        tracks[3].steps[6] = Step::note(55, 0.7);
        tracks[3].steps[8] = Step::note(43, 0.78);
        tracks[3].steps[14] = Step::note(55, 0.68);

        Self {
            bpm: 120.0,
            patch: Patch::default(),
            tracks,
        }
    }
}

impl Engine {
    pub fn set_step(&mut self, track: usize, step: usize, value: Step) -> bool {
        if track >= TRACK_COUNT || step >= STEPS_PER_TRACK {
            return false;
        }
        self.tracks[track].steps[step] = value;
        true
    }

    pub fn toggle_step(&mut self, track: usize, step: usize, note: u8, velocity: f32) -> bool {
        if track >= TRACK_COUNT || step >= STEPS_PER_TRACK {
            return false;
        }
        let cell = &mut self.tracks[track].steps[step];
        if cell.note.is_some() {
            *cell = Step::EMPTY;
        } else {
            *cell = Step::note(note, velocity.clamp(0.0, 1.0));
        }
        true
    }

    /// Render an absolute-time stereo frame without retaining history.
    pub fn render_at_ms(&self, at_ms: u64, seed: u64) -> Frame {
        let bpm = finite_or(self.bpm, 120.0).clamp(20.0, 300.0);
        let beat_seconds = 60.0 / bpm as f64;
        let step_seconds = beat_seconds / 4.0;
        let frame_start_seconds = at_ms as f64 / 1_000.0;
        let patch = self.patch;
        let cutoff = finite_or(patch.cutoff_hz, 8_000.0).clamp(20.0, SAMPLE_RATE as f32 * 0.45);
        let filter_alpha = 1.0 - (-TAU * cutoff / SAMPLE_RATE as f32).exp();
        let resonance = finite_or(patch.resonance, 0.0).clamp(0.0, 1.0);
        let patch_gain = finite_or(patch.gain, 0.0).clamp(0.0, 2.0);
        let release = finite_or(patch.release, 0.22).clamp(0.001, MAX_RELEASE_SECONDS);
        let attack = finite_or(patch.attack, 0.008).clamp(0.001, 10.0);
        let decay = finite_or(patch.decay, 0.16).clamp(0.0, 10.0);
        let sustain = finite_or(patch.sustain, 0.62).clamp(0.0, 1.0);
        let mut lp_one = [0.0_f32; TRACK_COUNT];
        let mut lp_two = [0.0_f32; TRACK_COUNT];
        let mut frame = Frame::default();
        let mut meter_square = [0.0_f64; TRACK_COUNT];
        let mut sequence_square = 0.0_f64;
        let mut oscillator_square = 0.0_f64;
        let mut envelope_square = 0.0_f64;
        let mut filter_square = 0.0_f64;
        let mut spatial_square = 0.0_f64;
        let mut mixer_square = 0.0_f64;
        let mut output_square = 0.0_f64;
        let any_solo = self.tracks.iter().any(|track| track.solo && !track.muted);

        for work_index in 0..FILTER_WARMUP_SAMPLES + FRAME_SAMPLES {
            let frame_index = work_index as isize - FILTER_WARMUP_SAMPLES as isize;
            let seconds = (frame_start_seconds + frame_index as f64 / SAMPLE_RATE as f64).max(0.0);
            let mut left = 0.0_f32;
            let mut right = 0.0_f32;

            for track_index in 0..TRACK_COUNT {
                let track = &self.tracks[track_index];
                if track.muted || (any_solo && !track.solo) {
                    continue;
                }
                let (osc, env, gate_energy) = track_components(
                    track,
                    track_index,
                    seconds,
                    step_seconds,
                    seed,
                    patch.waveform,
                    attack,
                    decay,
                    sustain,
                    release,
                    patch_gain,
                );
                let track_gain = finite_or(track.gain, 0.0).clamp(0.0, 2.0);
                let oscillator = osc * track_gain;
                let envelope = env * track_gain;
                let input = envelope;

                if work_index == 0 {
                    // Start the bounded pre-roll at a steady first input sample.
                    lp_one[track_index] = input;
                    lp_two[track_index] = input;
                } else {
                    lp_one[track_index] += filter_alpha * (input - lp_one[track_index]);
                    lp_two[track_index] +=
                        filter_alpha * (lp_one[track_index] - lp_two[track_index]);
                }
                let filtered = lp_two[track_index]
                    + resonance * 0.45 * (lp_one[track_index] - lp_two[track_index]);
                let pan = (finite_or(patch.pan, 0.0) + finite_or(track.pan, 0.0)).clamp(-1.0, 1.0);
                let left_gain = ((1.0 - pan) * 0.5).sqrt();
                let right_gain = ((1.0 + pan) * 0.5).sqrt();
                let track_left = filtered * left_gain;
                let track_right = filtered * right_gain;
                left += track_left;
                right += track_right;

                if frame_index >= 0 {
                    let i = frame_index as usize;
                    meter_square[track_index] += (track_left as f64 * track_left as f64
                        + track_right as f64 * track_right as f64)
                        * 0.5;
                    sequence_square += gate_energy as f64 * gate_energy as f64;
                    oscillator_square += oscillator as f64 * oscillator as f64;
                    envelope_square += envelope as f64 * envelope as f64;
                    filter_square += filtered as f64 * filtered as f64;
                    spatial_square += (track_left as f64 * track_left as f64
                        + track_right as f64 * track_right as f64)
                        * 0.5;
                    frame.fft_input[i] = 0.5 * (left + right);
                    frame.samples[0][i] = left;
                    frame.samples[1][i] = right;
                    mixer_square += (left as f64 * left as f64 + right as f64 * right as f64) * 0.5;
                }
            }

            if frame_index >= 0 {
                let i = frame_index as usize;
                left = soft_clip(left);
                right = soft_clip(right);
                frame.samples[0][i] = left;
                frame.samples[1][i] = right;
                frame.fft_input[i] = 0.5 * (left + right);
                output_square += (left as f64 * left as f64 + right as f64 * right as f64) * 0.5;
            }
        }

        for (track, square) in meter_square.iter().enumerate() {
            frame.track_meters[track] = (*square / FRAME_SAMPLES as f64).sqrt() as f32;
        }
        let count = FRAME_SAMPLES as f64;
        frame.routing = [
            rms(sequence_square, count),
            rms(oscillator_square, count),
            rms(envelope_square, count),
            rms(filter_square, count),
            rms(spatial_square, count),
            rms(mixer_square, count),
            rms(output_square, count),
        ];
        frame.beat_phase = ((frame_start_seconds % beat_seconds) / beat_seconds) as f32;
        let tick = (frame_start_seconds / step_seconds).floor() as u64;
        frame.step_index = (tick % STEPS_PER_TRACK as u64) as u8;
        collect_active_notes(
            self,
            frame_start_seconds,
            step_seconds,
            attack,
            decay,
            sustain,
            release,
            &mut frame,
        );
        summarize_waveform(&frame.fft_input, &mut frame.waveform);
        calculate_spectrum(&frame.fft_input, &mut frame.spectrum);
        frame
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaveformSummary {
    pub min: [f32; WAVEFORM_BUCKETS],
    pub max: [f32; WAVEFORM_BUCKETS],
    pub rms: [f32; WAVEFORM_BUCKETS],
}

impl Default for WaveformSummary {
    fn default() -> Self {
        Self {
            min: [0.0; WAVEFORM_BUCKETS],
            max: [0.0; WAVEFORM_BUCKETS],
            rms: [0.0; WAVEFORM_BUCKETS],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ActiveNote {
    pub track: u8,
    pub note: u8,
    pub velocity: f32,
    pub age_ms: f32,
    pub level: f32,
    pub muted: bool,
}

#[derive(Clone, Debug)]
pub struct Frame {
    /// Channel-major stereo frame: `samples[0]` is left and `samples[1]` is right.
    pub samples: [[f32; FRAME_SAMPLES]; 2],
    /// Mono downmix ready for an external FFT or visual analyzer.
    pub fft_input: [f32; FRAME_SAMPLES],
    /// Hann-windowed, one-sided magnitude spectrum, including DC and Nyquist.
    pub spectrum: [f32; FFT_BINS],
    pub waveform: WaveformSummary,
    pub track_meters: [f32; TRACK_COUNT],
    /// RMS energy through `[sequencer, oscillator, envelope, filter, spatial, mixer, output]`.
    pub routing: [f32; 7],
    pub beat_phase: f32,
    pub step_index: u8,
    pub active_notes: [ActiveNote; MAX_ACTIVE_NOTES],
    pub active_note_count: usize,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            samples: [[0.0; FRAME_SAMPLES]; 2],
            fft_input: [0.0; FRAME_SAMPLES],
            spectrum: [0.0; FFT_BINS],
            waveform: WaveformSummary::default(),
            track_meters: [0.0; TRACK_COUNT],
            routing: [0.0; 7],
            beat_phase: 0.0,
            step_index: 0,
            active_notes: [ActiveNote::default(); MAX_ACTIVE_NOTES],
            active_note_count: 0,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn track_components(
    track: &Track,
    track_index: usize,
    seconds: f64,
    step_seconds: f64,
    seed: u64,
    waveform: Waveform,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    patch_gain: f32,
) -> (f32, f32, f32) {
    let length = (track.pattern_length as usize).clamp(1, STEPS_PER_TRACK);
    let current_tick = (seconds / step_seconds).floor() as u64;
    let gate_seconds = (step_seconds * 0.82) as f32;
    let first_tick = first_relevant_tick(current_tick, step_seconds, gate_seconds, release);
    let velocity_gain = patch_gain;
    let mut oscillator_sum = 0.0;
    let mut envelope_sum = 0.0;
    let mut scheduled_level = 0.0;

    for tick in first_tick..=current_tick {
        let event = track.steps[tick as usize % length];
        let Some(note) = event.note else { continue };
        let age = (seconds - tick as f64 * step_seconds) as f32;
        let velocity = finite_or(event.velocity, 0.0).clamp(0.0, 1.0);
        let level = envelope_at(age, gate_seconds, attack, decay, sustain, release);
        if level <= 0.0 || velocity <= 0.0 {
            continue;
        }
        let phase = seeded_phase(seed, track_index, tick);
        let hz = midi_hz(note.min(127));
        let angle = (TAU as f64 * hz as f64 * age.max(0.0) as f64 + phase as f64) as f32;
        let sample = oscillator_sample(waveform, angle);
        oscillator_sum += sample * velocity * velocity_gain;
        envelope_sum += sample * velocity * level * velocity_gain;
        if tick == current_tick {
            scheduled_level = velocity * level;
        }
    }
    (oscillator_sum, envelope_sum, scheduled_level)
}

#[allow(clippy::too_many_arguments)]
fn collect_active_notes(
    engine: &Engine,
    seconds: f64,
    step_seconds: f64,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    frame: &mut Frame,
) {
    let current_tick = (seconds / step_seconds).floor() as u64;
    let gate_seconds = (step_seconds * 0.82) as f32;
    let first_tick = first_relevant_tick(current_tick, step_seconds, gate_seconds, release);
    for track_index in 0..TRACK_COUNT {
        let track = &engine.tracks[track_index];
        let length = (track.pattern_length as usize).clamp(1, STEPS_PER_TRACK);
        for tick in first_tick..=current_tick {
            let event = track.steps[tick as usize % length];
            let Some(note) = event.note else { continue };
            let age = (seconds - tick as f64 * step_seconds) as f32;
            let level = envelope_at(age, gate_seconds, attack, decay, sustain, release);
            let velocity = finite_or(event.velocity, 0.0).clamp(0.0, 1.0);
            if level <= 0.0 || velocity <= 0.0 || frame.active_note_count >= MAX_ACTIVE_NOTES {
                continue;
            }
            frame.active_notes[frame.active_note_count] = ActiveNote {
                track: track_index as u8,
                note: note.min(127),
                velocity,
                age_ms: age.max(0.0) * 1_000.0,
                level: level * velocity,
                muted: track.muted
                    || (engine
                        .tracks
                        .iter()
                        .any(|candidate| candidate.solo && !candidate.muted)
                        && !track.solo),
            };
            frame.active_note_count += 1;
        }
    }
}

fn first_relevant_tick(
    current_tick: u64,
    step_seconds: f64,
    gate_seconds: f32,
    release: f32,
) -> u64 {
    let history =
        (((gate_seconds + release) as f64 / step_seconds).ceil() as u64).min(MAX_RELEASE_TICKS);
    current_tick.saturating_sub(history)
}

fn envelope_at(age: f32, gate: f32, attack: f32, decay: f32, sustain: f32, release: f32) -> f32 {
    if age < 0.0 || age >= gate + release {
        return 0.0;
    }
    let at_gate = envelope_before_release(gate, attack, decay, sustain);
    if age >= gate {
        return at_gate * (1.0 - (age - gate) / release).clamp(0.0, 1.0);
    }
    envelope_before_release(age, attack, decay, sustain)
}

fn envelope_before_release(age: f32, attack: f32, decay: f32, sustain: f32) -> f32 {
    if attack > 0.0 && age < attack {
        return (age / attack).clamp(0.0, 1.0);
    }
    let after_attack = (age - attack).max(0.0);
    if decay > 0.0 && after_attack < decay {
        return 1.0 + (sustain - 1.0) * (after_attack / decay).clamp(0.0, 1.0);
    }
    sustain
}

fn oscillator_sample(waveform: Waveform, phase: f32) -> f32 {
    let angle = phase.rem_euclid(TAU);
    match waveform {
        Waveform::Sine => angle.sin(),
        Waveform::Triangle => (angle.sin().asin() * (2.0 / PI)).clamp(-1.0, 1.0),
        Waveform::Saw => angle / PI - 1.0,
        Waveform::Square => {
            if angle < PI {
                1.0
            } else {
                -1.0
            }
        }
    }
}

fn midi_hz(note: u8) -> f32 {
    440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
}

pub(crate) fn seeded_phase(seed: u64, track: usize, tick: u64) -> f32 {
    let mut value = seed ^ (track as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ tick;
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    let unit = (value >> 40) as f32 / (1_u32 << 24) as f32;
    unit * TAU
}

fn summarize_waveform(input: &[f32; FRAME_SAMPLES], output: &mut WaveformSummary) {
    for bucket in 0..WAVEFORM_BUCKETS {
        let start = bucket * SAMPLES_PER_BUCKET;
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        let mut squares = 0.0_f64;
        for &sample in &input[start..start + SAMPLES_PER_BUCKET] {
            min = min.min(sample);
            max = max.max(sample);
            squares += sample as f64 * sample as f64;
        }
        output.min[bucket] = min;
        output.max[bucket] = max;
        output.rms[bucket] = (squares / SAMPLES_PER_BUCKET as f64).sqrt() as f32;
    }
}

fn calculate_spectrum(input: &[f32; FRAME_SAMPLES], output: &mut [f32; FFT_BINS]) {
    let mut bins = [Complex::default(); FRAME_SAMPLES];
    let mut window_sum = 0.0_f64;
    for i in 0..FRAME_SAMPLES {
        let window = (0.5 - 0.5 * (TAU * i as f32 / (FRAME_SAMPLES - 1) as f32).cos()) as f64;
        window_sum += window;
        bins[i].re = input[i] * window as f32;
    }
    fft(&mut bins);
    for (i, value) in output.iter_mut().enumerate() {
        let factor = if i == 0 || i == FRAME_SAMPLES / 2 {
            1.0
        } else {
            2.0
        };
        let magnitude =
            (bins[i].re as f64 * bins[i].re as f64 + bins[i].im as f64 * bins[i].im as f64).sqrt();
        *value = (magnitude * factor / window_sum) as f32;
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Complex {
    re: f32,
    im: f32,
}

fn fft(values: &mut [Complex; FRAME_SAMPLES]) {
    let mut reversed = 0;
    for i in 1..FRAME_SAMPLES {
        let mut bit = FRAME_SAMPLES >> 1;
        while reversed & bit != 0 {
            reversed ^= bit;
            bit >>= 1;
        }
        reversed ^= bit;
        if i < reversed {
            values.swap(i, reversed);
        }
    }

    let mut span = 2;
    while span <= FRAME_SAMPLES {
        let half = span / 2;
        let angle_step = -TAU / span as f32;
        for base in (0..FRAME_SAMPLES).step_by(span) {
            for offset in 0..half {
                let angle = angle_step * offset as f32;
                let wr = angle.cos();
                let wi = angle.sin();
                let even = values[base + offset];
                let odd_value = values[base + offset + half];
                let odd = Complex {
                    re: odd_value.re * wr - odd_value.im * wi,
                    im: odd_value.re * wi + odd_value.im * wr,
                };
                values[base + offset] = Complex {
                    re: even.re + odd.re,
                    im: even.im + odd.im,
                };
                values[base + offset + half] = Complex {
                    re: even.re - odd.re,
                    im: even.im - odd.im,
                };
            }
        }
        span <<= 1;
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn rms(square_sum: f64, count: f64) -> f32 {
    (square_sum / count).sqrt() as f32
}

fn soft_clip(value: f32) -> f32 {
    value / (1.0 + value.abs() * 0.35)
}
