//! Realtime audio output for the deterministic engine's editable pattern state.
//!
//! The UI publishes a bounded atomic snapshot. The CPAL callback owns all
//! transport, voice, and filter state and never takes a lock or allocates.

use std::f32::consts::TAU;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, SizedSample, Stream, StreamConfig};

use crate::engine::{seeded_phase, Engine, Waveform, TRACK_COUNT};

const STEPS: usize = 16;
const VOICE_COUNT: usize = TRACK_COUNT * 64;
const NO_NOTE: u32 = 256;

/// A running device stream. Construction fails cleanly when no usable output
/// device exists, allowing the caller to keep running its silent simulation.
pub struct AudioOutput {
    _stream: Stream,
    shared: SharedConfig,
    status: String,
    errors: Arc<AtomicU64>,
    timing: CallbackTiming,
}

#[derive(Clone, Debug, Default)]
struct CallbackTiming {
    callbacks: Arc<AtomicU64>,
    nonzero_callbacks: Arc<AtomicU64>,
    peak_sample: Arc<AtomicU32>,
    total_ns: Arc<AtomicU64>,
    max_ns: Arc<AtomicU64>,
    over_budget: Arc<AtomicU64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AudioTiming {
    pub callbacks: u64,
    pub nonzero_callbacks: u64,
    pub peak_sample: f32,
    pub mean_duration_ns: u64,
    pub max_duration_ns: u64,
    pub over_budget_callbacks: u64,
    pub backend_errors: u64,
}

impl AudioOutput {
    /// Opens the default output device and starts its callback.
    pub fn try_new(seed: u64) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "no default audio output device is available".to_owned())?;
        let supported = device
            .default_output_config()
            .map_err(|error| format!("could not query default audio output config: {error}"))?;
        let sample_format = supported.sample_format();
        let stream_config: StreamConfig = supported.into();
        let channels = stream_config.channels as usize;
        let sample_rate = stream_config.sample_rate.0.max(1);
        if channels == 0 {
            return Err("default audio output reports zero channels".to_owned());
        }

        let shared = SharedConfig::new();
        let errors = Arc::new(AtomicU64::new(0));
        let timing = CallbackTiming::default();
        let stream = match sample_format {
            SampleFormat::F32 => build_stream::<f32>(
                &device,
                &stream_config,
                seed,
                shared.clone(),
                errors.clone(),
                timing.clone(),
            )?,
            SampleFormat::I16 => build_stream::<i16>(
                &device,
                &stream_config,
                seed,
                shared.clone(),
                errors.clone(),
                timing.clone(),
            )?,
            SampleFormat::U16 => build_stream::<u16>(
                &device,
                &stream_config,
                seed,
                shared.clone(),
                errors.clone(),
                timing.clone(),
            )?,
            other => {
                return Err(format!(
                    "default audio output format {other:?} is unsupported (expected f32, i16, or u16)"
                ));
            }
        };
        stream
            .play()
            .map_err(|error| format!("could not start audio output stream: {error}"))?;

        Ok(Self {
            _stream: stream,
            shared,
            status: format!("audio active · {sample_rate} Hz · {channels} ch · {sample_format:?}"),
            errors,
            timing,
        })
    }

    /// Publishes the latest engine controls without waiting for the audio thread.
    pub fn update(&self, engine: &Engine, playing: bool) {
        self.shared.publish(engine, playing);
    }

    /// Human-readable device status captured when the stream was opened.
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Number of backend stream errors reported since startup.
    pub fn backend_errors(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }

    /// Callback duration telemetry; an over-budget callback is not itself proof
    /// that the device emitted an underrun.
    pub fn timing(&self) -> AudioTiming {
        let callbacks = self.timing.callbacks.load(Ordering::Relaxed);
        let total_ns = self.timing.total_ns.load(Ordering::Relaxed);
        AudioTiming {
            callbacks,
            nonzero_callbacks: self.timing.nonzero_callbacks.load(Ordering::Relaxed),
            peak_sample: f32::from_bits(self.timing.peak_sample.load(Ordering::Relaxed)),
            mean_duration_ns: total_ns.checked_div(callbacks).unwrap_or(0),
            max_duration_ns: self.timing.max_ns.load(Ordering::Relaxed),
            over_budget_callbacks: self.timing.over_budget.load(Ordering::Relaxed),
            backend_errors: self.backend_errors(),
        }
    }
}

#[derive(Clone)]
struct SharedConfig {
    sequence: Arc<AtomicU64>,
    playing: Arc<AtomicBool>,
    bpm: Arc<AtomicU32>,
    waveform: Arc<AtomicU32>,
    attack: Arc<AtomicU32>,
    decay: Arc<AtomicU32>,
    sustain: Arc<AtomicU32>,
    release: Arc<AtomicU32>,
    cutoff_hz: Arc<AtomicU32>,
    resonance: Arc<AtomicU32>,
    patch_gain: Arc<AtomicU32>,
    patch_pan: Arc<AtomicU32>,
    tracks: [SharedTrack; TRACK_COUNT],
}

#[derive(Clone)]
struct SharedTrack {
    muted: Arc<AtomicBool>,
    solo: Arc<AtomicBool>,
    length: Arc<AtomicU32>,
    gain: Arc<AtomicU32>,
    pan: Arc<AtomicU32>,
    notes: [Arc<AtomicU32>; STEPS],
    velocities: [Arc<AtomicU32>; STEPS],
}

impl SharedConfig {
    fn new() -> Self {
        Self {
            sequence: arc_u64(0),
            playing: arc_bool(false),
            bpm: arc_f32(120.0),
            waveform: arc_u32(waveform_id(Waveform::Saw)),
            attack: arc_f32(0.008),
            decay: arc_f32(0.16),
            sustain: arc_f32(0.62),
            release: arc_f32(0.22),
            cutoff_hz: arc_f32(8_000.0),
            resonance: arc_f32(0.12),
            patch_gain: arc_f32(0.72),
            patch_pan: arc_f32(0.0),
            tracks: std::array::from_fn(|_| SharedTrack {
                muted: arc_bool(false),
                solo: arc_bool(false),
                length: arc_u32(STEPS as u32),
                gain: arc_f32(0.8),
                pan: arc_f32(0.0),
                notes: std::array::from_fn(|_| arc_u32(NO_NOTE)),
                velocities: std::array::from_fn(|_| arc_f32(0.0)),
            }),
        }
    }

    fn publish(&self, engine: &Engine, playing: bool) {
        // Odd means a publication is in progress. The callback simply keeps
        // using its previous complete snapshot during that short interval.
        self.sequence.fetch_add(1, Ordering::AcqRel);
        self.playing.store(playing, Ordering::Relaxed);
        self.bpm.store(engine.bpm.to_bits(), Ordering::Relaxed);
        self.waveform
            .store(waveform_id(engine.patch.waveform), Ordering::Relaxed);
        self.attack
            .store(engine.patch.attack.to_bits(), Ordering::Relaxed);
        self.decay
            .store(engine.patch.decay.to_bits(), Ordering::Relaxed);
        self.sustain
            .store(engine.patch.sustain.to_bits(), Ordering::Relaxed);
        self.release
            .store(engine.patch.release.to_bits(), Ordering::Relaxed);
        self.cutoff_hz
            .store(engine.patch.cutoff_hz.to_bits(), Ordering::Relaxed);
        self.resonance
            .store(engine.patch.resonance.to_bits(), Ordering::Relaxed);
        self.patch_gain
            .store(engine.patch.gain.to_bits(), Ordering::Relaxed);
        self.patch_pan
            .store(engine.patch.pan.to_bits(), Ordering::Relaxed);
        for (shared, track) in self.tracks.iter().zip(engine.tracks.iter()) {
            shared.muted.store(track.muted, Ordering::Relaxed);
            shared.solo.store(track.solo, Ordering::Relaxed);
            shared
                .length
                .store(track.pattern_length as u32, Ordering::Relaxed);
            shared.gain.store(track.gain.to_bits(), Ordering::Relaxed);
            shared.pan.store(track.pan.to_bits(), Ordering::Relaxed);
            for step in 0..STEPS {
                let cell = track.steps[step];
                shared.notes[step].store(
                    cell.note.map_or(NO_NOTE, |note| note.min(127) as u32),
                    Ordering::Relaxed,
                );
                shared.velocities[step].store(cell.velocity.to_bits(), Ordering::Relaxed);
            }
        }
        self.sequence.fetch_add(1, Ordering::Release);
    }

    fn snapshot(&self, current: &mut Config) {
        let before = self.sequence.load(Ordering::Acquire);
        if before & 1 != 0 {
            return;
        }
        let next = Config {
            playing: self.playing.load(Ordering::Relaxed),
            bpm: load_f32(&self.bpm),
            waveform: decode_waveform(self.waveform.load(Ordering::Relaxed)),
            attack: load_f32(&self.attack),
            decay: load_f32(&self.decay),
            sustain: load_f32(&self.sustain),
            release: load_f32(&self.release),
            cutoff_hz: load_f32(&self.cutoff_hz),
            resonance: load_f32(&self.resonance),
            patch_gain: load_f32(&self.patch_gain),
            patch_pan: load_f32(&self.patch_pan),
            tracks: std::array::from_fn(|index| {
                let track = &self.tracks[index];
                TrackConfig {
                    muted: track.muted.load(Ordering::Relaxed),
                    solo: track.solo.load(Ordering::Relaxed),
                    length: track.length.load(Ordering::Relaxed).clamp(1, STEPS as u32) as usize,
                    gain: load_f32(&track.gain),
                    pan: load_f32(&track.pan),
                    notes: std::array::from_fn(|step| {
                        let note = track.notes[step].load(Ordering::Relaxed);
                        if note == NO_NOTE {
                            None
                        } else {
                            Some(note.min(127) as u8)
                        }
                    }),
                    velocities: std::array::from_fn(|step| {
                        finite_clamp(load_f32(&track.velocities[step]), 0.0, 1.0, 0.0)
                    }),
                }
            }),
        };
        let after = self.sequence.load(Ordering::Acquire);
        if before == after {
            *current = next;
        }
    }
}

#[derive(Clone, Copy)]
struct TrackConfig {
    muted: bool,
    solo: bool,
    length: usize,
    gain: f32,
    pan: f32,
    notes: [Option<u8>; STEPS],
    velocities: [f32; STEPS],
}

#[derive(Clone, Copy)]
struct Config {
    playing: bool,
    bpm: f32,
    waveform: Waveform,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    cutoff_hz: f32,
    resonance: f32,
    patch_gain: f32,
    patch_pan: f32,
    tracks: [TrackConfig; TRACK_COUNT],
}

impl Default for Config {
    fn default() -> Self {
        Self {
            playing: false,
            bpm: 120.0,
            waveform: Waveform::Saw,
            attack: 0.008,
            decay: 0.16,
            sustain: 0.62,
            release: 0.22,
            cutoff_hz: 8_000.0,
            resonance: 0.12,
            patch_gain: 0.72,
            patch_pan: 0.0,
            tracks: [TrackConfig {
                muted: false,
                solo: false,
                length: STEPS,
                gain: 0.8,
                pan: 0.0,
                notes: [None; STEPS],
                velocities: [0.0; STEPS],
            }; TRACK_COUNT],
        }
    }
}

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    track: usize,
    velocity: f32,
    age_seconds: f32,
    phase: f32,
    phase_step: f32,
    gate_seconds: f32,
}

impl Voice {
    const EMPTY: Self = Self {
        active: false,
        track: 0,
        velocity: 0.0,
        age_seconds: 0.0,
        phase: 0.0,
        phase_step: 0.0,
        gate_seconds: 0.0,
    };
}

struct Synth {
    config: Config,
    seed: u64,
    voices: [Voice; VOICE_COUNT],
    filter_a: [f32; TRACK_COUNT],
    filter_b: [f32; TRACK_COUNT],
    sample_rate: f32,
    transport_seconds: f64,
    previous_tick: Option<u64>,
    was_playing: bool,
}

impl Synth {
    fn new(sample_rate: u32, seed: u64) -> Self {
        Self {
            config: Config::default(),
            seed,
            voices: [Voice::EMPTY; VOICE_COUNT],
            filter_a: [0.0; TRACK_COUNT],
            filter_b: [0.0; TRACK_COUNT],
            sample_rate: sample_rate.max(1) as f32,
            transport_seconds: 0.0,
            previous_tick: None,
            was_playing: false,
        }
    }

    fn render(&mut self) -> (f32, f32) {
        let cfg = self.config;
        if !cfg.playing {
            if self.was_playing {
                self.voices.fill(Voice::EMPTY);
                self.filter_a.fill(0.0);
                self.filter_b.fill(0.0);
            }
            self.was_playing = false;
            self.previous_tick = None;
            return (0.0, 0.0);
        }

        let bpm = finite_clamp(cfg.bpm, 20.0, 300.0, 120.0);
        let step_seconds = 15.0 / bpm;
        let tick = (self.transport_seconds / step_seconds as f64).floor() as u64;
        let any_solo = any_solo(&cfg);
        if !self.was_playing {
            self.previous_tick = None;
        }
        if self.previous_tick != Some(tick) {
            let first = self
                .previous_tick
                .map_or(tick, |previous| previous.saturating_add(1));
            // A sudden tempo edit can skip ticks. Only the current grid cell is
            // started; replaying a backlog would make a UI edit burst of notes.
            let start = if tick.saturating_sub(first) > 1 {
                tick
            } else {
                first
            };
            for event_tick in start..=tick {
                for track_index in 0..TRACK_COUNT {
                    let track = cfg.tracks[track_index];
                    let step = event_tick as usize % track.length;
                    if let Some(note) = track.notes[step] {
                        let muted = track.muted || (any_solo && !track.solo);
                        if !muted {
                            self.start_voice(
                                track_index,
                                note,
                                track.velocities[step],
                                step_seconds * 0.82,
                                event_tick,
                            );
                        }
                    }
                }
            }
            self.previous_tick = Some(tick);
        }

        let mut track_mix = [0.0_f32; TRACK_COUNT];
        let attack = finite_clamp(cfg.attack, 0.001, 10.0, 0.008);
        let decay = finite_clamp(cfg.decay, 0.0, 10.0, 0.16);
        let sustain = finite_clamp(cfg.sustain, 0.0, 1.0, 0.62);
        let release = finite_clamp(cfg.release, 0.001, 3.0, 0.22);
        let patch_gain = finite_clamp(cfg.patch_gain, 0.0, 2.0, 0.72);
        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }
            // A zero envelope at age zero is the start of a positive attack,
            // not the end of the note. Only the completed release retires it.
            if voice.age_seconds >= voice.gate_seconds + release {
                voice.active = false;
                continue;
            }
            let env = envelope(
                voice.age_seconds,
                voice.gate_seconds,
                attack,
                decay,
                sustain,
                release,
            );
            let sample = oscillator(cfg.waveform, voice.phase) * voice.velocity * env * patch_gain;
            voice.phase = (voice.phase + voice.phase_step).rem_euclid(TAU);
            voice.age_seconds += 1.0 / self.sample_rate;
            track_mix[voice.track] += sample;
        }

        let cutoff = finite_clamp(cfg.cutoff_hz, 20.0, self.sample_rate * 0.45, 8_000.0);
        let alpha = 1.0 - (-TAU * cutoff / self.sample_rate).exp();
        let resonance = finite_clamp(cfg.resonance, 0.0, 1.0, 0.0) * 0.45;
        let mut left = 0.0;
        let mut right = 0.0;
        for (track_index, track_sample) in track_mix.iter().enumerate() {
            let track = cfg.tracks[track_index];
            let audible = !track.muted && (!any_solo || track.solo);
            let input = if audible {
                *track_sample * finite_clamp(track.gain, 0.0, 2.0, 0.0)
            } else {
                0.0
            };
            self.filter_a[track_index] += alpha * (input - self.filter_a[track_index]);
            self.filter_b[track_index] +=
                alpha * (self.filter_a[track_index] - self.filter_b[track_index]);
            let filtered = self.filter_b[track_index]
                + resonance * (self.filter_a[track_index] - self.filter_b[track_index]);
            if !audible {
                continue;
            }
            let pan = finite_clamp(cfg.patch_pan + track.pan, -1.0, 1.0, 0.0);
            left += filtered * ((1.0 - pan) * 0.5).sqrt();
            right += filtered * ((1.0 + pan) * 0.5).sqrt();
        }
        self.transport_seconds += 1.0 / self.sample_rate as f64;
        self.was_playing = true;
        (soft_clip(left), soft_clip(right))
    }

    fn start_voice(&mut self, track: usize, note: u8, velocity: f32, gate_seconds: f32, tick: u64) {
        let slot = self
            .voices
            .iter()
            .position(|voice| !voice.active)
            .or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .max_by(|(_, left), (_, right)| left.age_seconds.total_cmp(&right.age_seconds))
                    .map(|(index, _)| index)
            });
        if let Some(index) = slot {
            self.voices[index] = Voice {
                active: true,
                track,
                velocity: finite_clamp(velocity, 0.0, 1.0, 0.0),
                age_seconds: 0.0,
                phase: seeded_phase(self.seed, track, tick),
                phase_step: TAU * 440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
                    / self.sample_rate,
                gate_seconds,
            };
        }
    }
}

fn build_stream<T>(
    device: &Device,
    config: &StreamConfig,
    seed: u64,
    shared: SharedConfig,
    errors: Arc<AtomicU64>,
    timing: CallbackTiming,
) -> Result<Stream, String>
where
    T: SizedSample + OutputSample,
{
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate.0.max(1);
    let mut synth = Synth::new(sample_rate, seed);
    let buffer_budget_ns = (1_000_000_000_u128 / sample_rate as u128).max(1) as u64;
    device
        .build_output_stream::<T, _, _>(
            config,
            move |output: &mut [T], _| {
                let callback_start = Instant::now();
                // Snapshot controls once per backend buffer. Device cadence
                // remains independent of UI redraws and no lock is taken.
                shared.snapshot(&mut synth.config);
                let mut peak = 0.0_f32;
                for frame in output.chunks_mut(channels) {
                    let (left, right) = synth.render();
                    peak = peak.max(left.abs()).max(right.abs());
                    let mono = (left + right) * 0.5;
                    for (channel, sample) in frame.iter_mut().enumerate() {
                        *sample = T::from_float(match channel {
                            0 => left,
                            1 => right,
                            _ => mono,
                        });
                    }
                }
                if peak > 1.0e-5 {
                    timing.nonzero_callbacks.fetch_add(1, Ordering::Relaxed);
                    update_peak(&timing.peak_sample, peak);
                }
                let frames = output.len() / channels;
                let budget_ns = buffer_budget_ns.saturating_mul(frames as u64);
                let elapsed_ns = callback_start.elapsed().as_nanos().min(u64::MAX as u128) as u64;
                timing.callbacks.fetch_add(1, Ordering::Relaxed);
                timing.total_ns.fetch_add(elapsed_ns, Ordering::Relaxed);
                update_max(&timing.max_ns, elapsed_ns);
                if elapsed_ns > budget_ns {
                    timing.over_budget.fetch_add(1, Ordering::Relaxed);
                }
            },
            move |_| {
                errors.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|error| format!("could not build audio output stream: {error}"))
}

fn update_max(target: &AtomicU64, value: u64) {
    let mut current = target.load(Ordering::Relaxed);
    while value > current {
        match target.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

fn update_peak(target: &AtomicU32, value: f32) {
    // Nonnegative IEEE-754 bits have the same ordering as their finite value.
    let bits = value.to_bits();
    let mut current = target.load(Ordering::Relaxed);
    while bits > current {
        match target.compare_exchange_weak(current, bits, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

trait OutputSample: Sized {
    fn from_float(value: f32) -> Self;
}

impl OutputSample for f32 {
    fn from_float(value: f32) -> Self {
        value
    }
}

impl OutputSample for i16 {
    fn from_float(value: f32) -> Self {
        (value.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
    }
}

impl OutputSample for u16 {
    fn from_float(value: f32) -> Self {
        ((value.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16
    }
}

fn envelope(age: f32, gate: f32, attack: f32, decay: f32, sustain: f32, release: f32) -> f32 {
    if age < 0.0 || age >= gate + release {
        return 0.0;
    }
    let at_gate = envelope_before_gate(gate, attack, decay, sustain);
    if age >= gate {
        at_gate * (1.0 - (age - gate) / release).clamp(0.0, 1.0)
    } else {
        envelope_before_gate(age, attack, decay, sustain)
    }
}

fn envelope_before_gate(age: f32, attack: f32, decay: f32, sustain: f32) -> f32 {
    if attack > 0.0 && age < attack {
        return (age / attack).clamp(0.0, 1.0);
    }
    let after_attack = (age - attack).max(0.0);
    if decay > 0.0 && after_attack < decay {
        1.0 + (sustain - 1.0) * (after_attack / decay).clamp(0.0, 1.0)
    } else {
        sustain
    }
}

fn oscillator(waveform: Waveform, phase: f32) -> f32 {
    match waveform {
        Waveform::Sine => phase.sin(),
        Waveform::Triangle => (phase.sin().asin() * (2.0 / std::f32::consts::PI)).clamp(-1.0, 1.0),
        Waveform::Saw => phase / std::f32::consts::PI - 1.0,
        Waveform::Square => {
            if phase < std::f32::consts::PI {
                1.0
            } else {
                -1.0
            }
        }
    }
}

fn soft_clip(sample: f32) -> f32 {
    sample / (1.0 + sample.abs())
}

fn any_solo(config: &Config) -> bool {
    config.tracks.iter().any(|track| track.solo && !track.muted)
}

fn finite_clamp(value: f32, low: f32, high: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(low, high)
    } else {
        fallback
    }
}

fn waveform_id(waveform: Waveform) -> u32 {
    match waveform {
        Waveform::Sine => 0,
        Waveform::Triangle => 1,
        Waveform::Saw => 2,
        Waveform::Square => 3,
    }
}

fn decode_waveform(value: u32) -> Waveform {
    match value {
        0 => Waveform::Sine,
        1 => Waveform::Triangle,
        3 => Waveform::Square,
        _ => Waveform::Saw,
    }
}

fn arc_bool(value: bool) -> std::sync::Arc<AtomicBool> {
    std::sync::Arc::new(AtomicBool::new(value))
}

fn arc_u32(value: u32) -> std::sync::Arc<AtomicU32> {
    std::sync::Arc::new(AtomicU32::new(value))
}

fn arc_u64(value: u64) -> std::sync::Arc<AtomicU64> {
    std::sync::Arc::new(AtomicU64::new(value))
}

fn arc_f32(value: f32) -> std::sync::Arc<AtomicU32> {
    arc_u32(value.to_bits())
}

fn load_f32(value: &AtomicU32) -> f32 {
    f32::from_bits(value.load(Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_published_pattern_produces_pcm_and_stops_cleanly() {
        let shared = SharedConfig::new();
        let engine = Engine::default();
        let mut synth = Synth::new(48_000, 2112);
        shared.publish(&engine, true);
        shared.snapshot(&mut synth.config);

        // The first sample of a positive attack is exactly zero. That must
        // not terminate a newly-triggered voice before it can make sound.
        assert_eq!(synth.render(), (0.0, 0.0));
        assert!(synth.voices.iter().any(|voice| voice.active));

        let peak = (0..2_048)
            .flat_map(|_| {
                let (left, right) = synth.render();
                [left.abs(), right.abs()]
            })
            .fold(0.0_f32, f32::max);
        assert!(
            peak > 0.01,
            "published pattern made no audible PCM: peak={peak}"
        );

        shared.publish(&engine, false);
        shared.snapshot(&mut synth.config);
        assert_eq!(synth.render(), (0.0, 0.0));
        assert!(synth.voices.iter().all(|voice| !voice.active));
    }
}
