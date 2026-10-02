//! Application state, the quotient pipeline, typed refusal and the evidence log.
//!
//! The load-bearing boundary: `regenerate` extracts a `CoverMap` from the
//! `ReferenceSong`, then hands **the map and the target only** to `cover(...)`.
//! The source composition is not passed to the generator and is not retained in
//! any field the generator reads.

use std::path::PathBuf;
use std::time::Duration;

use gibson::audio::buffer::StereoBlock;
use gibson::audio::device::AudioDevice;
use gibson::audio::human_music::{
    contract::CompositionGrammar,
    cover::{
        cover, CoverAdmission, CoverAxis, CoverConformance, CoverError, CoverFidelityPreset,
        CoverFidelityProfile, CoverKnowledge, CoverMap, CoverTarget, FormRelation, GrooveRelation,
        HarmonyAxis, HarmonyRelation, LineRelation, OrchestrationRelation,
    },
    fingerprint::CanonicalFingerprint,
    functor::Composition,
    performance::PerformanceOptions,
    policy::PerformanceProfile,
    reference_song::{DerivedHarmony, ReferenceSong},
    receipt::PerformanceReceipt,
    synth::HumanMusicSynth,
    world::MusicWorld,
};
use gibson::audio::render::{AudioSource, OfflineRenderer, RenderCtx};
use gibson::audio::time::{SampleRate, SampleTime};
use gibson::audio::wav::write_wav_i16;
use gibson::particles::ParticleSystem;

use crate::visual::{GlyphMode, VisualParams};

pub const DISPLAY_AXES: [CoverAxis; 8] = [
    CoverAxis::Motif,
    CoverAxis::Riff,
    CoverAxis::Groove,
    CoverAxis::HarmonicContour,
    CoverAxis::HarmonicLoop,
    CoverAxis::Form,
    CoverAxis::Orchestration,
    CoverAxis::BassFigure,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioMode {
    Stopped,
    Cover,
    Source,
}

#[derive(Debug, Clone)]
pub struct Refusal {
    pub kind: &'static str,
    pub reason: String,
    pub axes: [bool; 8],
    pub checks: Vec<(String, bool, String)>,
}

#[derive(Debug, Clone)]
pub enum Outcome {
    Covered {
        notes: usize,
        perf_fp: u64,
        score_hash: u64,
        conformance_pass: bool,
        receipt_pass: bool,
        wav_hash: Option<u64>,
    },
    Refused {
        kind: String,
        reason: String,
    },
    ExtractError(String),
}

#[derive(Debug, Clone)]
pub struct EvidenceRecord {
    pub step: u64,
    pub world: String,
    pub seed: u64,
    pub fidelity: String,
    pub axes: String,
    pub map_fp: Option<u64>,
    pub relation: String,
    pub outcome: Outcome,
    pub envelope_hash: u64,
}

/// Seamlessly loops an audio source indefinitely.
pub struct LoopingSynth {
    pub score: gibson::audio::human_music::score::Score,
    pub world: MusicWorld,
    pub sr: SampleRate,
    synth: HumanMusicSynth,
    loop_offset: u64,
}

impl LoopingSynth {
    pub fn new(score: gibson::audio::human_music::score::Score, world: MusicWorld, sr: SampleRate) -> Self {
        let synth = HumanMusicSynth::new(&score, &world, sr);
        Self { score, world, sr, synth, loop_offset: 0 }
    }
}

impl AudioSource for LoopingSynth {
    fn render(&mut self, out: &mut StereoBlock, ctx: &RenderCtx) {
        let relative_start = ctx.start.0.saturating_sub(self.loop_offset);
        let rel_ctx = RenderCtx { sr: ctx.sr, start: SampleTime(relative_start) };
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

pub struct Model {
    pub source: ReferenceSong,
    pub source_label: String,
    pub derived: Option<DerivedHarmony>,
    pub world: MusicWorld,
    pub seed: u64,
    pub preset: CoverFidelityPreset,
    pub glyph: GlyphMode,
    pub axes_on: [bool; 8],
    pub quotient: Option<CoverMap>,
    pub relations: String,
    pub knowledge: [CoverKnowledge; 8],
    /// What the current fidelity regime *could* know with every axis pinned —
    /// the ceiling against which a removed axis is visibly `PURGED` rather than
    /// merely `UNKNOWN`.
    pub ceiling_known: [bool; 8],
    pub conformance: [Option<bool>; 8],
    pub admission_checks: Vec<(String, bool, String)>,
    pub receipt_pass: Option<bool>,
    pub cover_comp: Option<Composition>,
    pub refusal: Option<Refusal>,
    pub envelope: Vec<f32>,
    pub envelope_source: Vec<f32>,
    pub evidence: Vec<EvidenceRecord>,
    pub step: u64,
    pub elapsed: Duration,
    pub particles: ParticleSystem,
    pub audio: Option<AudioDevice>,
    pub audio_mode: AudioMode,
    pub status: String,
    pub help: bool,
    pub no_audio: bool,
    pub export_dir: PathBuf,
    pub cover_wav_hash: Option<u64>,
}

impl Model {
    pub fn active_count(&self) -> usize {
        self.axes_on.iter().filter(|&&a| a).count()
    }

    pub fn theseus_pct(&self) -> f32 {
        let known = self.ceiling_known.iter().filter(|&&k| k).count();
        if known == 0 {
            return 0.0;
        }
        let preserved = (0..8)
            .filter(|&i| self.axes_on[i] && self.ceiling_known[i])
            .count();
        preserved as f32 / known as f32 * 100.0
    }

    /// Eight axis bits as a compact string (`1` = preserved).
    pub fn axes_string(&self) -> String {
        self.axes_on
            .iter()
            .map(|&a| if a { '1' } else { '0' })
            .collect()
    }

    /// A short, machine-readable tag for the current pipeline outcome.
    pub fn outcome_tag(&self) -> String {
        if let Some(r) = &self.refusal {
            format!("REFUSED({})", r.kind)
        } else if self.cover_comp.is_some() {
            "COVERED".to_string()
        } else {
            "NONE".to_string()
        }
    }

    /// Visual parameters snapshot for the current frame.
    pub fn visual(&self, time: f32, cap: gibson::capability::ColorDepth) -> VisualParams {
        let (cover_bars, cover_beats, cover_notes) = match &self.cover_comp {
            Some(c) => (
                c.score
                    .notes
                    .iter()
                    .map(|n| (n.start_beat as f32, n.dur_beats, n.pitch))
                    .collect::<Vec<_>>(),
                c.score.total_beats as f32,
                c.score.notes.len(),
            ),
            None => (Vec::new(), 1.0, 0),
        };
        let source_bars = self
            .source
            .melody()
            .map(|m| m.iter().map(|n| (n.at.beats() as f32, n.duration.beats() as f32, n.pitch)).collect())
            .unwrap_or_default();
        let mut refusal_axes = [false; 8];
        if let Some(r) = &self.refusal {
            refusal_axes = r.axes;
        }
        VisualParams {
            axes_on: self.axes_on,
            axis_known: std::array::from_fn(|i| self.knowledge[i] != CoverKnowledge::Unknown),
            axis_conformance: self.conformance,
            refusal_axes,
            refusal: self.refusal.is_some(),
            time,
            seed: self.seed,
            palette: crate::visual::palette_for(self.world.name),
            color_depth: cap,
            glyph: self.glyph,
            particles: self.particles.particles.clone(),
            cover_notes,
            source_notes: self.source.melody().map(|m| m.len()).unwrap_or(0),
            cover_bars,
            source_bars,
            cover_beats,
            source_beats: self.source.length.beats() as f32,
            envelope: if self.refusal.is_some() {
                self.envelope_source.clone()
            } else {
                self.envelope.clone()
            },
            playhead: (self.elapsed.as_secs_f32() * 0.25) % 1.0,
            status: self.status.clone(),
            world_name: self.world.name.to_string(),
            fidelity: self.preset.label().to_string(),
            theseus_pct: self.theseus_pct(),
        }
    }

    /// Spawn a deterministic sever burst at the ring for `axis`.
    pub fn spawn_sever(&mut self, axis: usize) {
        let a = -std::f32::consts::PI / 2.0 + axis as f32 * std::f32::consts::TAU / 8.0;
        let x = 0.5 + a.cos() * 0.40;
        let y = 0.5 + a.sin() * 0.40;
        self.particles
            .burst_directional(90, x, y, 0.55, 1.7, a, std::f32::consts::TAU / 3.0);
        self.particles.burst(40, x, y, 0.25, 2.2);
    }

    pub fn tick(&mut self, dt: f32) {
        self.particles.update(dt);
    }

    /// Re-run extraction + cover generation. The reference is never handed to
    /// `cover`; only the extracted quotient and the target are.
    pub fn regenerate(&mut self) {
        self.step += 1;
        self.refusal = None;
        self.admission_checks.clear();
        self.conformance = [None; 8];

        let mut profile = CoverFidelityProfile::preset(self.preset);
        if !self.axes_on[0] {
            profile.motif = LineRelation::Free;
        }
        if !self.axes_on[1] {
            profile.riff = LineRelation::Free;
        }
        if !self.axes_on[2] {
            profile.groove = GrooveRelation::Free;
        }
        if !self.axes_on[3] && !self.axes_on[4] {
            profile.harmony = HarmonyRelation::Free;
        } else {
            // `HarmonicContour` and `HarmonicLoop` share one pinned harmony; the
            // `harmony_axis` label selects which identity the pin carries, which
            // is what makes axes 4 and 5 independently meaningful.
            profile.harmony_axis = match (self.axes_on[3], self.axes_on[4]) {
                (true, true) => HarmonyAxis::Both,
                (true, false) => HarmonyAxis::Contour,
                (false, true) => HarmonyAxis::Loop,
                (false, false) => HarmonyAxis::Contour,
            };
        }
        if !self.axes_on[5] {
            profile.form = FormRelation::Free;
        }
        if !self.axes_on[6] {
            profile.orchestration = OrchestrationRelation::Free;
        }
        if !self.axes_on[7] {
            profile.bass = LineRelation::Free;
        }

        let derived = self.derived.clone();
        let extracted = self
            .source
            .extract_fidelity(&profile, Some(self.preset), derived.as_ref());

        let (map, _report) = match extracted {
            Ok(v) => v,
            Err(e) => {
                self.quotient = None;
                self.cover_comp = None;
                self.relations.clear();
                self.knowledge = [CoverKnowledge::Unknown; 8];
                let (kind, reason) = classify_extract_error(&e);
                self.set_refusal(kind, reason.clone(), &e);
                self.refresh_envelope();
                self.record_evidence(None, Outcome::ExtractError(format!("{kind}: {reason}")));
                return;
            }
        };

        // Ceiling: what this fidelity regime *could* know with every axis pinned.
        // The active map (toggles applied) is what is preserved now; the ceiling
        // says whether a removed axis was actually available, so the console can
        // distinguish PURGED (information was there and was removed) from
        // UNKNOWN (the reference never established it).
        let ceiling_profile = CoverFidelityProfile::preset(self.preset);
        let ceiling = self
            .source
            .extract_fidelity(&ceiling_profile, Some(self.preset), derived.as_ref())
            .ok()
            .map(|(m, _)| m);
        for i in 0..8 {
            let axis = DISPLAY_AXES[i];
            let known = ceiling
                .as_ref()
                .map(|m| m.knowledge(axis))
                .unwrap_or_else(|| map.knowledge(axis));
            self.knowledge[i] = known;
            self.ceiling_known[i] = known != CoverKnowledge::Unknown;
        }
        self.relations = summarize_relations(&map);

        let target = CoverTarget {
            world: &self.world,
            seed: self.seed,
            grammar: CompositionGrammar::DeflectedLift,
            options: PerformanceOptions::default(),
            profile: PerformanceProfile::BAND,
        };

        let map_fp = map.canonical_fingerprint();
        match cover(&map, target) {
            Ok(comp) => {
                let law = CoverConformance::check(&map, &comp, &self.world);
                for c in &law.checks {
                    if let Some(i) = display_index(c.axis) {
                        self.conformance[i] = Some(c.passed);
                    }
                }
                let receipt = PerformanceReceipt::measure_under(
                    &comp,
                    &self.world,
                    PerformanceProfile::BAND,
                );
                self.receipt_pass = Some(receipt.passes());
                let comp_fp = comp.perf.canonical_fingerprint();
                let score_hash = score_hash(&comp);
                let notes = comp.score.notes.len();
                if let Some(r) = &self.cover_comp {
                    let _ = r;
                }
                self.cover_comp = Some(comp);
                self.quotient = Some(map);
                self.status = format!(
                    "cover admitted · {} notes · conformance {} · receipts {}",
                    notes,
                    if law.passes() { "PASS" } else { "FAIL" },
                    if receipt.passes() { "PASS" } else { "FAIL" }
                );
                self.cover_wav_hash = None;
                self.refresh_envelope();
                let env_hash = self.envelope_hash();
                self.record_evidence(
                    Some(map_fp),
                    Outcome::Covered {
                        notes,
                        perf_fp: comp_fp,
                        score_hash,
                        conformance_pass: law.passes(),
                        receipt_pass: receipt.passes(),
                        wav_hash: self.cover_wav_hash,
                    },
                );
                let _ = env_hash;
            }
            Err(CoverError::Rejected(admission)) => {
                self.cover_comp = None;
                self.quotient = Some(map);
                self.set_admission_refusal(*admission);
                self.refresh_envelope();
                let reason = self
                    .refusal
                    .as_ref()
                    .map(|r| r.reason.clone())
                    .unwrap_or_default();
                self.record_evidence(
                    Some(map_fp),
                    Outcome::Refused { kind: "Rejected".into(), reason },
                );
            }
            Err(e) => {
                self.cover_comp = None;
                self.quotient = Some(map);
                let (kind, reason) = classify_extract_error(&e);
                self.set_refusal(kind, reason.clone(), &e);
                self.refresh_envelope();
                self.record_evidence(
                    Some(map_fp),
                    Outcome::Refused { kind: kind.to_string(), reason },
                );
            }
        }
    }

    fn set_admission_refusal(&mut self, admission: CoverAdmission) {
        let mut axes = [false; 8];
        let mut checks = Vec::new();
        for c in &admission.conformance.checks {
            if let Some(i) = display_index(c.axis) {
                axes[i] = !c.passed;
            }
            checks.push((format!("{:?}", c.axis), c.passed, c.detail.clone()));
        }
        for f in admission.pipeline.failures() {
            checks.push(("pipeline".into(), false, f));
        }
        let reason = admission
            .conformance
            .checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| format!("{}: {}", c.axis.label(), c.detail))
            .next()
            .or_else(|| admission.pipeline.failures().first().cloned())
            .unwrap_or_else(|| "cover admission failed".into());
        self.refusal = Some(Refusal { kind: "CoverAdmission", reason, axes, checks });
        self.status = "LAWFUL REFUSAL — the target world rejected the admitted bridge".into();
    }

    fn set_refusal(&mut self, kind: &'static str, reason: String, e: &CoverError) {
        let mut axes = [false; 8];
        let low = reason.to_lowercase();
        let mut hit = false;
        let key_axes: [(usize, &[&str]); 8] = [
            (0, &["motif", "lead"]),
            (1, &["riff"]),
            (2, &["groove", "drum"]),
            (3, &["harmonic", "harmony", "contour"]),
            (4, &["loop", "cadence"]),
            (5, &["form", "phrase"]),
            (6, &["orchestration", "seat", "arrangement"]),
            (7, &["bass", "sub-root"]),
        ];
        for (i, keys) in key_axes {
            if keys.iter().any(|k| low.contains(k)) {
                axes[i] = true;
                hit = true;
            }
        }
        if !hit {
            // no keyword: blame the axes currently pinned
            for i in 0..8 {
                axes[i] = self.axes_on[i];
            }
        }
        let detail = match e {
            CoverError::MissingAxis(a) => format!("missing axis: {}", a.label()),
            CoverError::Invalid(s) => (*s).to_string(),
            CoverError::ConflictingPins => "conflicting pins".into(),
            CoverError::UnprojectableTiming => "unprojectable timing".into(),
            CoverError::Policy(s) => s.clone(),
            CoverError::Rejected(a) => a.conformance.report(),
        };
        self.refusal = Some(Refusal { kind, reason, axes, checks: vec![("typed".into(), false, detail)] });
        self.status = format!("LAWFUL REFUSAL — typed {kind}");
    }

    fn refresh_envelope(&mut self) {
        // Real offline render of the current cover (or source when refused).
        let (score, world) = match &self.cover_comp {
            Some(c) => (Some(c.score.clone()), self.world.clone()),
            None => (self.source.melody_score().ok(), self.world.clone()),
        };
        if let Some(score) = score {
            let mut synth = HumanMusicSynth::new(&score, &world, SampleRate::STUDIO);
            let renderer = OfflineRenderer::new(SampleRate::STUDIO, 1024);
            let result = renderer.render_seconds(&mut synth, 2.0);
            let env = envelope_of(&result.audio, 256);
            let hash = hash_audio(&result.audio);
            if self.cover_comp.is_some() {
                self.envelope = env;
                self.cover_wav_hash = Some(hash);
            } else {
                self.envelope_source = env;
            }
        }
    }

    fn envelope_hash(&self) -> u64 {
        let env = if self.cover_comp.is_some() {
            &self.envelope
        } else {
            &self.envelope_source
        };
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for &v in env {
            for b in v.to_le_bytes() {
                h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
            }
        }
        h
    }

    fn record_evidence(&mut self, map_fp: Option<u64>, outcome: Outcome) {
        let axes: String = self.axes_on.iter().map(|&a| if a { '1' } else { '0' }).collect();
        self.evidence.push(EvidenceRecord {
            step: self.step,
            world: self.world.name.to_string(),
            seed: self.seed,
            fidelity: self.preset.label().to_string(),
            axes,
            map_fp,
            relation: self.relations.clone(),
            outcome,
            envelope_hash: self.envelope_hash(),
        });
    }

    /// Export the current cover + source to WAVs and write the evidence log.
    pub fn export(&mut self) -> Result<Vec<String>, String> {
        std::fs::create_dir_all(&self.export_dir).map_err(|e| e.to_string())?;
        let mut written = Vec::new();
        if let Some(comp) = &self.cover_comp {
            let mut synth = HumanMusicSynth::new(&comp.score, &self.world, SampleRate::STUDIO);
            let renderer = OfflineRenderer::new(SampleRate::STUDIO, 1024);
            let secs = comp.score.total_beats as f64 * (60.0 / self.world.tempo_bpm as f64);
            let result = renderer.render_seconds(&mut synth, secs);
            let path = self.export_dir.join("theseus_cover.wav");
            if write_wav_i16(&path, &result.audio, SampleRate::STUDIO).is_ok() {
                self.cover_wav_hash = Some(hash_audio(&result.audio));
                written.push(path.display().to_string());
            }
        }
        if let Ok(score) = self.source.melody_score() {
            let mut synth = HumanMusicSynth::new(&score, &self.world, SampleRate::STUDIO);
            let renderer = OfflineRenderer::new(SampleRate::STUDIO, 1024);
            let secs = score.total_beats * (60.0 / 100.0);
            let result = renderer.render_seconds(&mut synth, secs);
            let path = self.export_dir.join("theseus_source.wav");
            if write_wav_i16(&path, &result.audio, SampleRate::STUDIO).is_ok() {
                written.push(path.display().to_string());
            }
        }
        let json = self.evidence_json();
        let path = self.export_dir.join("theseus_evidence.json");
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        written.push(path.display().to_string());
        self.status = format!("exported {} artifact(s) to {}", written.len(), self.export_dir.display());
        Ok(written)
    }

    pub fn evidence_json(&self) -> String {
        let mut out = String::from("{\n  \"application\": \"project-theseus\",\n  \"libgibson\": \"v0.4.0 (c2f6483d92fe2b351e6cd50936a97d8cdf73cb79)\",\n  \"records\": [\n");
        for (i, r) in self.evidence.iter().enumerate() {
            out.push_str("    {");
            out.push_str(&format!(
                "\"step\":{},\"world\":\"{}\",\"seed\":{},\"fidelity\":\"{}\",\"axes\":\"{}\",",
                r.step, r.world, r.seed, r.fidelity, r.axes
            ));
            if let Some(fp) = r.map_fp {
                out.push_str(&format!("\"covermap_fingerprint\":\"{fp:016x}\","));
            }
            out.push_str(&format!("\"relations\":\"{}\",", escape(&r.relation)));
            out.push_str(&format!("\"envelope_hash\":\"{:016x}\",", r.envelope_hash));
            match &r.outcome {
                Outcome::Covered { notes, perf_fp, score_hash, conformance_pass, receipt_pass, wav_hash } => {
                    out.push_str(&format!(
                        "\"outcome\":\"covered\",\"notes\":{notes},\"perf_fingerprint\":\"{perf_fp:016x}\",\"score_hash\":\"{score_hash:016x}\",\"conformance\":{conformance_pass},\"receipt\":{receipt_pass},\"wav_hash\":{}",
                        wav_hash.map(|h| format!("\"{h:016x}\"")).unwrap_or_else(|| "null".into())
                    ));
                }
                Outcome::Refused { kind, reason } => {
                    out.push_str(&format!(
                        "\"outcome\":\"refused\",\"kind\":\"{}\",\"reason\":\"{}\"",
                        escape(kind),
                        escape(reason)
                    ));
                }
                Outcome::ExtractError(e) => {
                    out.push_str(&format!("\"outcome\":\"extract_error\",\"detail\":\"{}\"", escape(e)));
                }
            }
            out.push('}');
            if i + 1 < self.evidence.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  ]\n}\n");
        out
    }

    pub fn play_cover(&mut self) {
        if self.no_audio {
            self.status = "audio disabled (--no-audio)".into();
            return;
        }
        self.audio = None;
        if let Some(comp) = &self.cover_comp {
            let looper = LoopingSynth::new(comp.score.clone(), self.world.clone(), SampleRate::STUDIO);
            let source: Box<dyn AudioSource + Send> = Box::new(looper);
            match AudioDevice::play(source, SampleRate::STUDIO) {
                Ok(dev) => {
                    self.audio = Some(dev);
                    self.audio_mode = AudioMode::Cover;
                    self.status = format!("streaming cover [{}] · looping", self.world.name);
                }
                Err(e) => {
                    self.audio_mode = AudioMode::Stopped;
                    self.status = format!("audio device error: {e:?}");
                }
            }
        } else {
            self.status = "cover refused — nothing to stream".into();
        }
    }

    pub fn play_source(&mut self) {
        if self.no_audio {
            self.status = "audio disabled (--no-audio)".into();
            return;
        }
        self.audio = None;
        if let Ok(score) = self.source.melody_score() {
            let looper = LoopingSynth::new(score, self.world.clone(), SampleRate::STUDIO);
            let source: Box<dyn AudioSource + Send> = Box::new(looper);
            match AudioDevice::play(source, SampleRate::STUDIO) {
                Ok(dev) => {
                    self.audio = Some(dev);
                    self.audio_mode = AudioMode::Source;
                    self.status = "auditioning the reference melody (A/B)".into();
                }
                Err(e) => {
                    self.audio_mode = AudioMode::Stopped;
                    self.status = format!("audio device error: {e:?}");
                }
            }
        }
    }

    pub fn stop_audio(&mut self) {
        self.audio = None;
        self.audio_mode = AudioMode::Stopped;
        self.status = "audio muted".into();
    }
}

pub fn display_index(axis: CoverAxis) -> Option<usize> {
    match axis {
        CoverAxis::Motif => Some(0),
        CoverAxis::Riff => Some(1),
        CoverAxis::Groove => Some(2),
        CoverAxis::HarmonicContour => Some(3),
        CoverAxis::HarmonicLoop => Some(4),
        CoverAxis::Form => Some(5),
        CoverAxis::Orchestration => Some(6),
        CoverAxis::BassFigure => Some(7),
    }
}

fn summarize_relations(map: &CoverMap) -> String {
    let r = map.relations();
    format!(
        "motif={:?} riff={:?} bass={:?} harmony={:?} groove={:?} form={:?} orch={:?}",
        r.motif, r.riff, r.bass, r.harmony, r.groove, r.form, r.orchestration
    )
}

fn classify_extract_error(e: &CoverError) -> (&'static str, String) {
    match e {
        CoverError::Invalid(s) => ("Invalid", (*s).to_string()),
        CoverError::MissingAxis(a) => ("MissingAxis", a.label().to_string()),
        CoverError::ConflictingPins => ("ConflictingPins", "conflicting pins".into()),
        CoverError::UnprojectableTiming => ("UnprojectableTiming", "unprojectable timing".into()),
        CoverError::Policy(s) => ("Policy", s.clone()),
        CoverError::Rejected(a) => ("Rejected", a.conformance.report()),
    }
}

fn score_hash(c: &Composition) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let mut mix = |v: u64| {
        h = (h ^ v).wrapping_mul(0x100_0000_01b3);
    };
    for n in &c.score.notes {
        mix(n.pitch as u64);
        mix(n.start_beat.to_bits() as u64);
        mix(n.dur_beats.to_bits() as u64);
        mix(n.role as u64);
    }
    h
}

fn envelope_of(block: &StereoBlock, buckets: usize) -> Vec<f32> {
    let n = block.frames();
    if n == 0 || buckets == 0 {
        return Vec::new();
    }
    let size = (n / buckets).max(1);
    let mut out = Vec::with_capacity(buckets);
    for b in 0..buckets {
        let start = b * size;
        let end = (start + size).min(n);
        let mut peak = 0.0f32;
        for i in start..end {
            let v = block.left[i].abs().max(block.right[i].abs());
            if v > peak {
                peak = v;
            }
        }
        out.push(peak.min(1.0));
    }
    out
}

pub fn hash_audio(block: &StereoBlock) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for i in 0..block.frames() {
        for s in [block.left[i], block.right[i]] {
            for b in s.to_le_bytes() {
                h = (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
            }
        }
    }
    h
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', " ")
}
