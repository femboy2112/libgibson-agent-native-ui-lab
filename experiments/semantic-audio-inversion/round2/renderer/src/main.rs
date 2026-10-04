//! External consumer of the reviewed HumanMusic release. All recordings stay local.
use gibson::audio::human_music::{
    contract::CompositionGrammar,
    cover::*,
    fingerprint::CanonicalFingerprint,
    functor::{perform_with_profile, Composition},
    performance::PerformanceOptions,
    plan::SectionFamily,
    policy::PerformanceProfile,
    rhythm::MetricPosition,
    score::{Role, Score},
    semantic,
    song::SongMap,
    synth::{HumanMusicSynth, StemMask},
    theory::Quality,
    world::MusicWorld,
};
use gibson::audio::{render::OfflineRenderer, time::SampleRate, wav::write_wav_i16};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

const ANCHOR: &str = "c2f6483d92fe2b351e6cd50936a97d8cdf73cb79";
const GRID: u32 = 256;
type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Deserialize, Clone)]
struct Source {
    source_id: String,
    source_sha256: String,
    blueprint_sha256: String,
    tempo_bpm: f64,
    meter_beats: f64,
    duration_beats: f64,
    #[serde(default)]
    key_root: Option<i32>,
    #[serde(default)]
    notes: Vec<NoteEvidence>,
    #[serde(default)]
    harmony: Vec<HarmonyEvidence>,
    #[serde(default)]
    drums: Vec<DrumEvidence>,
    #[serde(default)]
    sections: Vec<SectionEvidence>,
    #[serde(default)]
    evidence_status: Value,
    #[serde(default)]
    beat_origin_offset: Option<f64>,
    #[serde(skip)]
    native_pocket: bool,
    #[serde(skip)]
    normalized_input_sha256: String,
    #[serde(skip)]
    preroll_beats: f64,
}
#[derive(Debug, Deserialize, Clone)]
struct NoteEvidence {
    role: String,
    onset_beat: f64,
    duration_beats: f64,
    midi: f64,
    confidence: f64,
}
#[derive(Debug, Deserialize, Clone)]
struct HarmonyEvidence {
    start_beat: f64,
    end_beat: f64,
    root: i32,
    quality: String,
    confidence: f64,
}
#[derive(Debug, Deserialize, Clone)]
struct DrumEvidence {
    onset_beat: f64,
    #[serde(default)]
    metric_onset_beat: Option<f64>,
    #[serde(default)]
    pocket_member: bool,
    family: String,
    confidence: f64,
}
#[derive(Debug, Deserialize, Clone)]
struct SectionEvidence {
    start_beat: f64,
    end_beat: f64,
    label: String,
    confidence: f64,
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn write_json(path: &Path, value: &Value) -> Result<()> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
fn metric(x: f64) -> Result<MetricPosition> {
    if !x.is_finite() || !(0. ..=8192.).contains(&x) {
        return Err(format!("invalid metric domain {x}"));
    }
    MetricPosition::new((x * GRID as f64).round() as i64, GRID).ok_or("invalid rational".into())
}
fn confidence(x: f64) -> Result<()> {
    if !x.is_finite() || !(0.0..=1.).contains(&x) {
        Err("invalid confidence".into())
    } else {
        Ok(())
    }
}
fn hash_valid(x: &str) -> bool {
    x.len() == 64 && x.bytes().all(|b| b.is_ascii_hexdigit())
}
fn quality(q: &str) -> Result<Quality> {
    Ok(match q.to_lowercase().as_str() {
        "maj" | "major" => Quality::Maj,
        "min" | "minor" => Quality::Min,
        "dim" => Quality::Dim,
        "aug" => Quality::Aug,
        "maj7" => Quality::Maj7,
        "min7" => Quality::Min7,
        "7" | "dom7" => Quality::Dom7,
        "min7b5" => Quality::Min7b5,
        "dim7" => Quality::Dim7,
        "minmaj7" => Quality::MinMaj7,
        "sus2" => Quality::Sus2,
        "sus4" => Quality::Sus4,
        "maj9" => Quality::Maj9,
        "min9" => Quality::Min9,
        "9" | "dom9" => Quality::Dom9,
        "add9" => Quality::Add9,
        "maj6" => Quality::Maj6,
        "min6" => Quality::Min6,
        _ => return Err(format!("unsupported chord quality {q}")),
    })
}
fn status<'a>(s: &'a Source, axis: &str) -> &'a str {
    s.evidence_status
        .get(axis)
        .and_then(|v| {
            v.as_str()
                .or_else(|| v.get("relation").and_then(Value::as_str))
        })
        .unwrap_or("unknown")
}
fn line(s: &Source, role: &str, hm_role: Role, root: i32) -> Result<Option<CoverLine>> {
    let axis = if role == "lead" { "motif" } else { "bass" };
    if !matches!(status(s, axis), "metric" | "faithful") {
        return Ok(None);
    }
    let mut notes: Vec<_> = s.notes.iter().filter(|n| n.role == role).collect();
    notes.sort_by(|a, b| a.onset_beat.total_cmp(&b.onset_beat));
    if notes.is_empty() {
        return Err(format!("{axis} claims evidence but lane is empty"));
    }
    if notes.len() < 3 {
        return Ok(None);
    }
    let origin = (notes[0].midi.round() as i32 - root).div_euclid(12) * 12;
    let mut projected = Vec::new();
    for n in notes {
        let at = metric(n.onset_beat)?;
        if projected.last().is_some_and(|p: &CoverNote| p.at == at) {
            return Err(format!(
                "{axis}: ambiguous simultaneous notes after 1/{GRID} beat projection"
            ));
        }
        projected.push(CoverNote {
            at,
            relative_pitch: n.midi.round() as i32 - root - origin,
            reserved_until: if status(s, axis) == "faithful" {
                Some(metric(n.onset_beat + n.duration_beats)?)
            } else {
                None
            },
        });
    }
    Ok(Some(CoverLine {
        role: hm_role,
        notes: projected,
    }))
}
fn compile(s: &Source) -> Result<(CoverMap, Value)> {
    if !hash_valid(&s.source_sha256) || !hash_valid(&s.blueprint_sha256) {
        return Err("invalid source/blueprint SHA256".into());
    }
    if !s.tempo_bpm.is_finite() || !(20.0..=400.).contains(&s.tempo_bpm) {
        return Err("invalid tempo".into());
    }
    if s.meter_beats != 4. {
        return Err("HumanMusic supports 4/4 only; inferred source meter differs".into());
    }
    let length = metric(s.duration_beats)?;
    let root = s.key_root.unwrap_or(0);
    if !(0..12).contains(&root) {
        return Err("invalid root pitch class".into());
    }
    for n in &s.notes {
        confidence(n.confidence)?;
        if !n.midi.is_finite()
            || !(0.0..=127.).contains(&n.midi)
            || !n.duration_beats.is_finite()
            || n.duration_beats <= 0.
            || n.onset_beat < 0.
            || n.onset_beat + n.duration_beats > s.duration_beats + 1e-6
        {
            return Err("note outside finite source domain".into());
        }
        metric(n.onset_beat)?;
    }
    for h in &s.harmony {
        confidence(h.confidence)?;
        metric(h.start_beat)?;
        metric(h.end_beat)?;
        if h.end_beat <= h.start_beat
            || h.end_beat > s.duration_beats + 1e-6
            || !(0..12).contains(&h.root)
        {
            return Err("invalid harmonic span".into());
        }
    }
    for d in &s.drums {
        confidence(d.confidence)?;
        metric(d.onset_beat)?;
        if d.onset_beat >= s.duration_beats {
            return Err("drum outside domain".into());
        }
    }
    for f in &s.sections {
        confidence(f.confidence)?;
        metric(f.start_beat)?;
        metric(f.end_beat)?;
        if f.end_beat <= f.start_beat || f.end_beat > s.duration_beats + 1e-6 {
            return Err("invalid section".into());
        }
    }
    let motif = line(s, "lead", Role::Lead, root)?;
    let bass = line(s, "bass", Role::Bass, root)?;
    let mut profile = CoverFidelityProfile::FREE;
    profile.motif = if motif.is_some() {
        if status(s, "motif") == "faithful" {
            LineRelation::Faithful
        } else {
            LineRelation::Metric
        }
    } else {
        LineRelation::Free
    };
    profile.bass = if bass.is_some() {
        if status(s, "bass") == "faithful" {
            LineRelation::Faithful
        } else {
            LineRelation::Metric
        }
    } else {
        LineRelation::Free
    };
    let harmony = if matches!(status(s, "harmony"), "quality_family" | "exact") {
        profile.harmony = if status(s, "harmony") == "exact" {
            HarmonyRelation::Exact
        } else {
            HarmonyRelation::QualityFamily
        };
        Some(
            s.harmony
                .iter()
                .map(|h| {
                    Ok(CoverChord {
                        at: metric(h.start_beat)?,
                        end: metric(h.end_beat)?,
                        relative_root: (h.root - root).rem_euclid(12),
                        quality: quality(&h.quality)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        )
    } else {
        None
    };
    let groove = if status(s, "groove") == "kick_snare"
        || (s.native_pocket && status(s, "groove") == "pocket_skeleton")
    {
        let skeleton = status(s, "groove") == "pocket_skeleton";
        let mut out = Vec::new();
        for d in &s.drums {
            let voice = match d.family.as_str() {
                "kick" => GrooveVoice::Kick,
                "snare" => GrooveVoice::Snare,
                _ => continue,
            };
            // Skeleton membership must have been observed on the metric quarter grid. No offbeat
            // detection is moved onto the grid to manufacture a pocket.
            if skeleton && !d.pocket_member {
                continue;
            }
            let at = metric(if skeleton {
                d.metric_onset_beat
                    .ok_or("pocket member lacks canonical metric onset")?
            } else {
                d.onset_beat
            })?;
            if skeleton && at.subdivision() != 1 {
                return Err("canonical pocket not on global quarter grid".into());
            }
            out.push(CoverStroke { at, voice });
        }
        out.sort_by_key(|d| (d.at, matches!(d.voice, GrooveVoice::Snare)));
        if out.is_empty() {
            return Err("groove relation declared but no supported strokes".into());
        }
        profile.groove = if skeleton {
            GrooveRelation::PocketSkeleton
        } else {
            GrooveRelation::KickSnare
        };
        Some(out)
    } else {
        None
    };
    let form = if status(s, "form") == "exact" {
        profile.form = FormRelation::Exact;
        let mut labels = Vec::new();
        let mut out = Vec::new();
        for f in &s.sections {
            if f.start_beat % 4. != 0. || f.end_beat % 4. != 0. {
                return Err("exact form requires observed bar boundaries".into());
            }
            if !labels.contains(&f.label) {
                labels.push(f.label.clone())
            }
            out.push(CoverPhrase {
                start_bar: (f.start_beat / 4.) as u32,
                bars: ((f.end_beat - f.start_beat) / 4.) as u32,
                family: SectionFamily::Named {
                    identity: labels.iter().position(|x| x == &f.label).unwrap() as u32,
                },
            });
        }
        Some(out)
    } else {
        None
    };
    let spec = profile.spec();
    if !spec.has_song_identity() {
        return Err("no supported identity-bearing melodic, bass, or harmonic axis".into());
    }
    let map = CoverMap {
        unknown_axes: CoverAxis::ALL
            .into_iter()
            .filter(|a| {
                !spec.contains(*a)
                    && match a {
                        CoverAxis::Motif => status(s, "motif") == "unknown",
                        CoverAxis::BassFigure => status(s, "bass") == "unknown",
                        CoverAxis::HarmonicContour | CoverAxis::HarmonicLoop => {
                            status(s, "harmony") == "unknown"
                        }
                        CoverAxis::Groove => status(s, "groove") == "unknown",
                        CoverAxis::Form => status(s, "form") == "unknown",
                        _ => true,
                    }
            })
            .collect(),
        spec,
        ordered_chart: None,
        length: Some(length),
        form,
        motif,
        riff: None,
        harmony,
        groove,
        bass,
        orchestration: None,
        fidelity: Some(profile),
        projection: CoverProjection::Identity,
    };
    map.validate().map_err(|e| e.to_string())?;
    let max_quant = s
        .notes
        .iter()
        .map(|n| (metric(n.onset_beat).unwrap().beats() - n.onset_beat).abs())
        .fold(0., f64::max);
    let receipt = json!({"schema":"sai.cover_map_receipt/v2","source_id":s.source_id,"source_sha256":s.source_sha256,
        "blueprint_sha256":s.blueprint_sha256,"normalized_input_sha256":s.normalized_input_sha256,"libgibson_anchor":ANCHOR,"constraint_sha256":sha(format!("{map:?}").as_bytes()),
        "canonical_fingerprint":format!("{:016x}",map.canonical_fingerprint()),"relations":format!("{profile:?}"),
        "unknown_axes":map.unknown_axes.iter().map(|x|x.label()).collect::<Vec<_>>(),
        "metric_projection":{"denominator":GRID,"max_note_onset_error_beats":max_quant,"source_duration_beats":s.duration_beats-s.preroll_beats,"preroll_beats":s.preroll_beats,"native_pocket_treatment":s.native_pocket,"projected_duration_beats":length.beats()},
        "pitch_projection":{"rule":"nearest equal-tempered semitone; raw fractional MIDI retained in source object","max_pitch_error_semitones":s.notes.iter().filter(|n|matches!(n.role.as_str(),"lead"|"bass")).map(|n|(n.midi.round()-n.midi).abs()).fold(0.,f64::max)},"reference_tonic":root,"tonic_observed":s.key_root.is_some(),"map_debug":format!("{map:#?}"),
        "scope":"predeclared partial diagnostic lift; omitted observed axes are not certified preserved",
        "source_evidence_relations":s.evidence_status,"motif_scope":"complete selected monophonic candidate lane; no principal hook identification claimed","unrepresented_observed_axes":unsupported(s),
        "claim":"typed measurement constraint; not certified source correctness or acoustic conformance"});
    Ok((map, receipt))
}

fn transport_pocket(s: &mut Source) -> Result<Value> {
    if status(s, "groove") != "pocket_skeleton" {
        return Err("native-pocket treatment requires source pocket evidence".into());
    }
    let offset = s
        .beat_origin_offset
        .ok_or("source lacks beat origin offset")?;
    if !offset.is_finite() {
        return Err("nonfinite beat origin offset".into());
    }
    let shift = (-offset).rem_euclid(1.);
    s.preroll_beats = shift;
    s.native_pocket = true;
    s.duration_beats += shift;
    for n in &mut s.notes {
        n.onset_beat += shift;
    }
    for h in &mut s.harmony {
        h.start_beat += shift;
        h.end_beat += shift;
    }
    for f in &mut s.sections {
        f.start_beat += shift;
        f.end_beat += shift;
    }
    for d in &mut s.drums {
        d.onset_beat += shift;
        if let Some(x) = &mut d.metric_onset_beat {
            *x += shift;
        }
    }
    Ok(
        json!({"schema":"sai.metric_transport/v1","source_beat_origin_offset":offset,"preroll_beats":shift,
        "rule":"one additive positive phase transport applied to every note, raw drum, canonical drum, harmonic boundary, section boundary and domain endpoint",
        "microtiming_policy":"PocketSkeleton pins recurring observed canonical beat membership; raw acoustic drum onset remains separate",
        "source_seconds":"unchanged in renderer_source_snapshot.json","pocket_members":s.drums.iter().filter(|d|d.pocket_member).count()}),
    )
}
fn unsupported(s: &Source) -> Vec<Value> {
    let mut out = Vec::new();
    for (axis,relation,why) in [("harmony","ordered","metric CoverMap has no Ordered harmony combined with metric lines"),("form","topology","metric CoverMap has no Topology form combined with metric lines"),("groove","pocket_skeleton","source canonical beat phase and fill/core membership must be established before quarter-grid native pin")] {
        if status(s,axis)==relation && !(axis=="groove" && s.native_pocket) {out.push(json!({"axis":axis,"source_relation":relation,"status":"observed_but_unsupported_ingress","reason":why}));}
    }
    for (axis, role) in [("motif", "lead"), ("bass", "bass")] {
        let n = s.notes.iter().filter(|n| n.role == role).count();
        if matches!(status(s, axis), "metric" | "faithful") && n < 3 {
            out.push(json!({"axis":axis,"source_relation":status(s,axis),"status":"insufficient_identity_structure","observed_notes":n,"reason":"fewer than three observations cannot establish a structured identity line"}));
        }
    }
    out
}

fn truth(c: &Composition, w: &MusicWorld) -> Value {
    json!({"schema":"sai.generated_observer_truth/v2","libgibson_anchor":ANCHOR,"tempo_bpm":w.tempo_bpm,
        "tonic_pc":w.tonic_pc,"duration_beats":c.score.total_beats,
        "notes":c.score.notes.iter().map(|n|json!({"role":format!("{:?}",n.role).to_lowercase(),"onset_beat":n.start_beat,"duration_beats":n.dur_beats,"midi":n.pitch,"velocity":n.velocity})).collect::<Vec<_>>(),
        "drums":c.score.drums.iter().map(|d|json!({"family":format!("{:?}",d.voice).to_lowercase(),"onset_beat":d.start_beat,"velocity":d.velocity})).collect::<Vec<_>>(),
        "harmony":c.score.chords.iter().map(|h|json!({"start_beat":h.start_beat,"end_beat":h.start_beat+f64::from(h.dur_beats),"root":h.chord.root_pc,"quality":format!("{:?}",h.chord.quality).to_lowercase()})).collect::<Vec<_>>(),
        "sections":c.score.sections.iter().map(|s|json!({"start_beat":s.start_bar*4,"end_beat":(s.start_bar+s.bars)*4,"label":s.kind.label()})).collect::<Vec<_>>()})
}
fn render(score: &Score, w: &MusicWorld, out: &Path, sr: u32) -> Result<Value> {
    fs::create_dir_all(out.join("stems")).map_err(|e| e.to_string())?;
    let rate = SampleRate::new(sr).ok_or("invalid sample rate")?;
    let mut receipts = Vec::new();
    for name in [
        "full", "lead", "bass", "drums", "keys", "pad", "sfx", "support",
    ] {
        let mask = match name {
            "full" => StemMask::full(),
            "support" => StemMask::only(&["keys", "pad"]),
            _ => StemMask::solo(name),
        };
        let mut synth = HumanMusicSynth::new(score, w, rate);
        synth.set_stem_mask(mask);
        let frames = synth.total_samples();
        let rendered = OfflineRenderer::new(rate, 512).render(&mut synth, frames);
        let path = if name == "full" {
            out.join("raw_mix.wav")
        } else {
            out.join("stems").join(format!("{name}.wav"))
        };
        write_wav_i16(&path, &rendered.audio, rate).map_err(|e| e.to_string())?;
        let hash = sha(&fs::read(&path).map_err(|e| e.to_string())?);
        receipts
            .push(json!({"stem":name,"path":path,"sha256":hash,"frames":frames,"sample_rate":sr}));
    }
    Ok(Value::Array(receipts))
}
struct CandidateSettings<'a> {
    seed: u64,
    sr: u32,
    profile_name: &'a str,
    do_render: bool,
}
fn candidate(
    map: &CoverMap,
    s: &Source,
    receipt: &Value,
    out: &Path,
    w: MusicWorld,
    settings: CandidateSettings<'_>,
) -> Result<Value> {
    let CandidateSettings {
        seed,
        sr,
        profile_name,
        do_render,
    } = settings;
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let profile = profile_named(profile_name)?;
    let config = json!({"source_id":s.source_id,"source_sha256":s.source_sha256,"blueprint_sha256":s.blueprint_sha256,"normalized_input_sha256":s.normalized_input_sha256,
        "constraint_sha256":receipt["constraint_sha256"],"libgibson_anchor":ANCHOR,"world":w.name,"seed":seed,
        "scope":"partial_diagnostic","preroll_beats":s.preroll_beats,"native_pocket_treatment":s.native_pocket,"unrepresented_observed_axes":unsupported(s),"full_structural_success":false,"tempo_bpm":w.tempo_bpm,"tonic_pc":w.tonic_pc,"swing":w.swing,"profile":profile_name,"grammar":"HookArc","sample_rate":sr});
    write_json(&out.join("config.json"), &config)?;
    // This is exactly the public checked-cover boundary, decomposed to retain rejected takes.
    // cover() returns only the rejection receipt, so cover_candidate + both checks avoids a rerun.
    let c = match cover_candidate(
        map,
        CoverTarget {
            world: &w,
            seed,
            grammar: CompositionGrammar::HookArc,
            options: PerformanceOptions::default(),
            profile,
        },
    ) {
        Ok(c) => c,
        Err(e) => {
            let r = json!({"path":out,"status":"refused","admitted":false,"error":e.to_string(),"config":config});
            write_json(&out.join("conformance.json"), &r)?;
            return Ok(r);
        }
    };
    let conform = CoverConformance::check(map, &c, &w);
    let pipeline = PerformanceReceipt::measure_under(&c, &w, profile);
    let admitted = conform.passes() && pipeline.passes();
    let r = json!({"status":if admitted{"admitted"}else{"rejected"},"admitted":admitted,
        "conformance_passes":conform.passes(),"conformance":conform.report(),"pipeline_passes":pipeline.passes(),
        "pipeline":format!("{pipeline:#?}"),"failures":pipeline.failures(),"score_fingerprint":format!("{:016x}",c.score.canonical_fingerprint()),
        "performance_fingerprint":format!("{:016x}",c.perf.canonical_fingerprint()),"config":config});
    write_json(&out.join("conformance.json"), &r)?;
    write_json(&out.join("truth.json"), &truth(&c, &w))?;
    let temporal =
        gibson::audio::human_music::temporal::TemporalPitchDiagnostics::measure(&c.perf, &c.score);
    let false_rows=temporal.rows.iter().filter(|row|c.score.notes[row.note_index].function.is_some_and(|f|!row.supported.contains(&f))).map(|row|json!({"note":format!("{:?}",c.score.notes[row.note_index]),"proof":format!("{row:?}")})).collect::<Vec<_>>();
    write_json(
        &out.join("temporal_failures.json"),
        &json!({"reported_false_claims":temporal.false_function_claims,"unsupported_rows":false_rows}),
    )?;
    let audio = if do_render {
        render(&c.score, &w, out, sr)?
    } else {
        json!([])
    };
    write_json(&out.join("render_receipt.json"), &audio)?;
    Ok(
        json!({"path":out,"admitted":admitted,"status":r["status"],"failures":r["failures"],"audio":audio}),
    )
}
fn profile_named(name: &str) -> Result<PerformanceProfile> {
    Ok(match name {
        "BAND" => PerformanceProfile::BAND,
        "POCKET" => PerformanceProfile::POCKET,
        "PHRASED" => PerformanceProfile::PHRASED,
        "TEMPORAL" => PerformanceProfile::TEMPORAL,
        "WRITTEN" => PerformanceProfile::WRITTEN,
        _ => return Err("unknown performance profile".into()),
    })
}
fn probe(input: &Path, out: &Path) -> Result<()> {
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let source: Source = serde_json::from_slice(&fs::read(input).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let (map, receipt) = compile(&source)?;
    let mut rows = Vec::new();
    // Frozen diagnostic matrix before execution: identical map, source frame, seed, five public
    // realization profiles. No failed receipt is removed and no law is suppressed.
    for name in ["BAND", "POCKET", "PHRASED", "TEMPORAL", "WRITTEN"] {
        for mut world in MusicWorld::all() {
            world.tempo_bpm = source.tempo_bpm as f32;
            world.tonic_pc = source.key_root.unwrap_or(0);
            world.swing = 0.;
            let dir = out.join(format!("{}-{}", name, world.name));
            eprintln!("profile probe {} {}", name, world.name);
            rows.push(candidate(
                &map,
                &source,
                &receipt,
                &dir,
                world,
                CandidateSettings {
                    seed: 220901,
                    sr: 22050,
                    profile_name: name,
                    do_render: false,
                },
            )?);
        }
    }
    write_json(
        &out.join("profile_probe_manifest.json"),
        &json!({"constraint_sha256":receipt["constraint_sha256"],"rows":rows}),
    )
}
fn run_real(input: &Path, out: &Path, seeds: &[u64], sr: u32, native_pocket: bool) -> Result<()> {
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let bytes = fs::read(input).map_err(|e| e.to_string())?;
    let mut source: Source = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    source.normalized_input_sha256 = sha(&bytes);
    if native_pocket {
        let transform = transport_pocket(&mut source)?;
        write_json(&out.join("metric_transport_receipt.json"), &transform)?;
    }
    let gaps = unsupported(&source);
    if !gaps.is_empty() {
        write_json(
            &out.join("full_profile_ingress_refusal.json"),
            &json!({"status":"refused","reason":"full requested source relations cannot be expressed through this metric public ingress","unrepresented_observed_axes":gaps,"diagnostic_profile":"lead+bass and other natively supported metric pins; source object retained unchanged"}),
        )?;
    }
    let (map, receipt) = match compile(&source) {
        Ok(x) => x,
        Err(e) => {
            write_json(
                &out.join("compilation_refusal.json"),
                &json!({"source_id":source.source_id,"error":e,"input_sha256":sha(&bytes)}),
            )?;
            return Err(e);
        }
    };
    write_json(&out.join("cover_map_receipt.json"), &receipt)?;
    fs::write(out.join("renderer_source_snapshot.json"), &bytes).map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    let mut index = 0;
    for mut world in MusicWorld::all() {
        world.tempo_bpm = source.tempo_bpm as f32;
        world.tonic_pc = source.key_root.unwrap_or(0);
        // Source metric onsets already encode the observed timing; no extra swing is inferred.
        world.swing = 0.;
        for &seed in seeds {
            index += 1;
            let dir = out.join("candidates").join(format!("candidate-{index:03}"));
            eprintln!("{} {} seed {}", source.source_id, world.name, seed);
            rows.push(candidate(
                &map,
                &source,
                &receipt,
                &dir,
                world.clone(),
                CandidateSettings {
                    seed,
                    sr,
                    profile_name: "BAND",
                    do_render: true,
                },
            )?);
        }
    }
    write_json(
        &out.join("candidate_manifest.json"),
        &json!({"input_sha256":sha(&bytes),"constraint_sha256":receipt["constraint_sha256"],"candidates":rows}),
    )
}
fn generate(out: &Path, sr: u32, holdout: bool) -> Result<()> {
    if out.exists() && out.read_dir().map_err(|e| e.to_string())?.next().is_some() {
        return Err("development/holdout output must be a fresh empty directory".into());
    }
    let split = if holdout { "holdout" } else { "dev" };
    let seed_base = if holdout { 9_220_100 } else { 220_100 };
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for (wi, w) in MusicWorld::all().into_iter().enumerate() {
        for (ki, kind) in ["demo", "calm-loop"].into_iter().enumerate() {
            let seed = seed_base + (wi * 17 + ki) as u64;
            let beats = 32.;
            let trace = if kind == "demo" {
                semantic::demo_trace(beats)
            } else {
                semantic::calm_loop(beats)
            };
            let song = SongMap::build(&trace, seed, None);
            let c = perform_with_profile(
                &song,
                &w,
                PerformanceOptions::default(),
                PerformanceProfile::BAND,
            )
            .map_err(|e| e.to_string())?;
            let id = format!("{split}-{}-{kind}-{seed}", w.name.to_lowercase());
            let dir = out.join(&id);
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            write_json(&dir.join("truth.json"), &truth(&c, &w))?;
            let receipt = PerformanceReceipt::measure_under(&c, &w, PerformanceProfile::BAND);
            let audio = render(&c.score, &w, &dir, sr)?;
            write_json(&dir.join("render_receipt.json"), &audio)?;
            rows.push(json!({"id":id,"seed":seed,"world":w.name,"kind":kind,"path":dir,"pipeline_passes":receipt.passes(),"failures":receipt.failures(),"audio":audio}));
        }
    }
    write_json(
        &out.join("manifest.json"),
        &json!({"schema":if holdout{"sai.round2_holdout/v1"}else{"sai.round2_dev/v1"},"anchor":ANCHOR,"seed_family":format!("{seed_base}+world*17+kind"),"holdout":holdout,"items":rows}),
    )
}
fn main() {
    if let Err(e) = cli() {
        eprintln!("{e}");
        std::process::exit(1)
    }
}
fn cli() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("probe") => probe(
            Path::new(args.get(2).ok_or("probe INPUT OUT")?),
            Path::new(args.get(3).ok_or("probe INPUT OUT")?),
        ),
        Some("dev") | Some("holdout") => generate(
            Path::new(args.get(2).ok_or("dev OUT [SR]")?),
            args.get(3).map(|x| x.parse().unwrap()).unwrap_or(22050),
            args[1] == "holdout",
        ),
        Some("cover") | Some("native-pocket") => {
            let seeds = args
                .get(4)
                .map(|x| {
                    x.split(',')
                        .map(|s| s.parse::<u64>().map_err(|e| e.to_string()))
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()?
                .unwrap_or(vec![220901, 220902]);
            run_real(
                Path::new(args.get(2).ok_or("cover INPUT OUT [SEEDS] [SR]")?),
                Path::new(args.get(3).ok_or("missing output")?),
                &seeds,
                args.get(5).map(|x| x.parse().unwrap()).unwrap_or(22050),
                args[1] == "native-pocket",
            )
        }
        _ => {
            Err("usage: sai-round2-renderer dev OUT [SR] | cover INPUT OUT [SEED,SEED] [SR]".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source() -> Source {
        serde_json::from_value(json!({"source_id":"synthetic","source_sha256":"a".repeat(64),"blueprint_sha256":"b".repeat(64),"tempo_bpm":120,"meter_beats":4,"duration_beats":16,"key_root":0,
        "notes":[{"role":"lead","onset_beat":0,"duration_beats":1,"midi":60,"confidence":1},{"role":"lead","onset_beat":1,"duration_beats":1,"midi":64,"confidence":1},{"role":"lead","onset_beat":2,"duration_beats":1,"midi":67,"confidence":1}],"evidence_status":{"motif":"metric"}})).unwrap()
    }
    #[test]
    fn synthetic_microspan_counterexample_stays_rejected() {
        // Constructed fixture, not notes or chords transcribed from any recording.
        // A minimum 0.1-beat pad gate outlasts the 10/256-beat foreign chord.
        let mut s = source();
        s.duration_beats = 32.;
        for (i, n) in s.notes.iter_mut().enumerate() {
            n.onset_beat = i as f64 * 8.;
            n.midi = 67.;
        }
        s.evidence_status["harmony"] = json!("exact");
        s.harmony = vec![
            HarmonyEvidence {
                start_beat: 0.,
                end_beat: 4.,
                root: 7,
                quality: "min".into(),
                confidence: 1.,
            },
            HarmonyEvidence {
                start_beat: 4.,
                end_beat: 4. + 10. / 256.,
                root: 0,
                quality: "min".into(),
                confidence: 1.,
            },
            HarmonyEvidence {
                start_beat: 4. + 10. / 256.,
                end_beat: 32.,
                root: 7,
                quality: "min".into(),
                confidence: 1.,
            },
        ];
        let (map, _) = compile(&s).unwrap();
        let world = MusicWorld::black_ice();
        let target = CoverTarget {
            world: &world,
            seed: 220901,
            grammar: CompositionGrammar::HookArc,
            options: PerformanceOptions::default(),
            profile: PerformanceProfile::BAND,
        };
        match cover(&map, target) {
            Err(CoverError::Rejected(admission)) => {
                assert!(admission.conformance.passes());
                assert_eq!(admission.pipeline.temporal_false, 2);
                assert!(!admission.pipeline.passes());
            }
            _ => panic!("known-red microspan no longer returned a checked rejection"),
        }
    }
    #[test]
    fn native_pocket_transports_every_coordinate_once() {
        let mut s = source();
        s.beat_origin_offset = Some(0.25);
        s.evidence_status["groove"] = json!("pocket_skeleton");
        s.harmony.push(HarmonyEvidence {
            start_beat: 0.,
            end_beat: 16.,
            root: 0,
            quality: "maj".into(),
            confidence: 1.,
        });
        s.sections.push(SectionEvidence {
            start_beat: 0.,
            end_beat: 16.,
            label: "a".into(),
            confidence: 1.,
        });
        s.drums.push(DrumEvidence {
            onset_beat: 1.3,
            metric_onset_beat: Some(1.25),
            pocket_member: true,
            family: "kick".into(),
            confidence: 1.,
        });
        let before = s.clone();
        let r = transport_pocket(&mut s).unwrap();
        assert_eq!(r["preroll_beats"], 0.75);
        assert_eq!(s.duration_beats - before.duration_beats, 0.75);
        for (a, b) in s.notes.iter().zip(&before.notes) {
            assert_eq!(a.onset_beat - b.onset_beat, 0.75);
            assert_eq!(a.duration_beats, b.duration_beats);
            assert_eq!(a.midi, b.midi);
        }
        assert_eq!(s.harmony[0].start_beat, 0.75);
        assert_eq!(s.harmony[0].end_beat, 16.75);
        assert_eq!(s.sections[0].start_beat, 0.75);
        assert_eq!(s.sections[0].end_beat, 16.75);
        assert!((s.drums[0].onset_beat - before.drums[0].onset_beat - 0.75).abs() < 1e-12);
        assert_eq!(s.drums[0].metric_onset_beat, Some(2.));
        let (map, _) = compile(&s).unwrap();
        assert_eq!(map.groove.unwrap()[0].at.beats(), 2.);
    }
    #[test]
    fn checked_entry_agrees_with_retained_candidate_gate() {
        let (map, _) = compile(&source()).unwrap();
        let world = MusicWorld::black_ice();
        let target = || CoverTarget {
            world: &world,
            seed: 220901,
            grammar: CompositionGrammar::HookArc,
            options: PerformanceOptions::default(),
            profile: PerformanceProfile::BAND,
        };
        let expected = cover_candidate(&map, target())
            .map(|c| {
                CoverConformance::check(&map, &c, &world).passes()
                    && PerformanceReceipt::measure_under(&c, &world, PerformanceProfile::BAND)
                        .passes()
            })
            .unwrap_or(false);
        assert_eq!(cover(&map, target()).is_ok(), expected);
    }
    #[test]
    fn sparse_observation_does_not_become_bass_identity() {
        let mut s = source();
        s.evidence_status["bass"] = json!("metric");
        let mut n = s.notes[0].clone();
        n.role = "bass".into();
        s.notes.push(n);
        let (m, _) = compile(&s).unwrap();
        assert!(m.bass.is_none());
        assert_eq!(m.knowledge(CoverAxis::BassFigure), CoverKnowledge::Free);
        assert!(unsupported(&s)
            .iter()
            .any(|x| x["status"] == "insufficient_identity_structure"));
    }
    #[test]
    fn unknown_is_not_free_or_pin() {
        let (m, _) = compile(&source()).unwrap();
        assert_eq!(m.knowledge(CoverAxis::BassFigure), CoverKnowledge::Unknown);
        assert!(m.bass.is_none());
    }
    #[test]
    fn deterministic_and_production_independent() {
        let s = source();
        let (a, ra) = compile(&s).unwrap();
        let (b, rb) = compile(&s).unwrap();
        assert_eq!(a, b);
        assert_eq!(ra, rb);
    }
    #[test]
    fn missing_claimed_lane_fails() {
        let mut s = source();
        s.evidence_status["bass"] = json!("metric");
        assert!(compile(&s).is_err())
    }
    #[test]
    fn lower_ceiling_removes_pin() {
        let mut s = source();
        s.evidence_status["motif"] = json!("unknown");
        assert!(compile(&s).is_err());
    }
    #[test]
    fn invalid_domains_fail() {
        let mut s = source();
        s.notes[0].confidence = 1.1;
        assert!(compile(&s).is_err());
        s = source();
        s.source_sha256 = "missing".into();
        assert!(compile(&s).is_err());
    }
    #[test]
    fn simultaneous_pitch_rivals_are_not_silently_discarded() {
        let mut s = source();
        let mut n = s.notes[0].clone();
        n.midi = 67.;
        s.notes.push(n);
        assert!(compile(&s).is_err());
    }
    #[test]
    fn harmony_gaps_are_not_filled() {
        let mut s = source();
        s.evidence_status["harmony"] = json!("exact");
        s.harmony.push(HarmonyEvidence {
            start_beat: 1.,
            end_beat: 16.,
            root: 0,
            quality: "maj".into(),
            confidence: 1.,
        });
        assert!(compile(&s).is_err())
    }
    #[test]
    fn unrelated_json_production_does_not_enter_identity() {
        let s = source();
        let bytes=serde_json::to_value(json!({"source_id":"synthetic","source_sha256":"a".repeat(64),"blueprint_sha256":"b".repeat(64),"tempo_bpm":120,"meter_beats":4,"duration_beats":16,"key_root":0,"notes":[{"role":"lead","onset_beat":0,"duration_beats":1,"midi":60,"confidence":1},{"role":"lead","onset_beat":1,"duration_beats":1,"midi":64,"confidence":1},{"role":"lead","onset_beat":2,"duration_beats":1,"midi":67,"confidence":1}],"evidence_status":{"motif":"metric"},"production":{"gain":42}})).unwrap();
        let t: Source = serde_json::from_value(bytes).unwrap();
        assert_eq!(compile(&s).unwrap().0, compile(&t).unwrap().0);
    }
}
