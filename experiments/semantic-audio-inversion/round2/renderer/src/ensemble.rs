//! Measurement playback, deliberately outside CoverMap / checked composition admission.
use super::{sha, write_json, Result, ANCHOR};
use gibson::audio::{
    human_music::{
        form::SectionKind,
        instrument::Patch,
        score::{DrumHit, DrumVoice, Note, Provenance, Role, Score},
        synth::{HumanMusicSynth, ProductionControl},
        world::MusicWorld,
    },
    render::OfflineRenderer,
    time::SampleRate,
    wav::write_wav_i16,
    StereoBlock,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Deserialize)]
struct Event {
    onset_sec: f64,
    offset_sec: f64,
    midi: f64,
    #[serde(default = "default_velocity")]
    velocity: f32,
    confidence: f64,
}
fn default_velocity() -> f32 {
    0.75
}
#[derive(Deserialize)]
struct Lane {
    lane_id: String,
    events: Vec<Event>,
    stem_identity: Value,
}
#[derive(Deserialize)]
struct Stroke {
    onset_sec: f64,
    family: String,
    confidence: f64,
}
#[derive(Deserialize)]
struct Source {
    source_id: String,
    source_sha256: String,
    blueprint_sha256: String,
    duration_sec: f64,
    lanes: Vec<Lane>,
    drums: Vec<Stroke>,
    production: Value,
}
fn selected_lanes(s: &Source) -> Vec<&Lane> {
    let six = s.lanes.iter().any(|l| {
        l.lane_id.starts_with("htdemucs_6s:")
            && matches!(label(l), "guitar" | "piano" | "other")
            && !l.events.is_empty()
    });
    s.lanes
        .iter()
        .filter(|l| {
            !l.events.is_empty()
                && match l.lane_id.split_once(':') {
                    Some(("htdemucs_ft", "vocals" | "bass")) => true,
                    Some(("htdemucs_6s", "guitar" | "piano" | "other")) => six,
                    Some(("htdemucs_ft", "other")) => !six,
                    _ => false,
                }
        })
        .collect()
}
fn label(l: &Lane) -> &str {
    l.lane_id.split_once(':').map_or("unknown", |x| x.1)
}
fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(f64::total_cmp);
    xs.get(xs.len() / 2).copied().unwrap_or(0.)
}
fn behavior(l: &Lane) -> (Role, Value) {
    let pitch = median(l.events.iter().map(|n| n.midi).collect());
    let duration = median(
        l.events
            .iter()
            .map(|n| n.offset_sec - n.onset_sec)
            .collect(),
    );
    let mut times: Vec<_> = l
        .events
        .iter()
        .map(|n| (n.onset_sec, n.offset_sec))
        .collect();
    times.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut end = 0f64;
    let mut overlaps = 0;
    for &(a, b) in &times {
        if a < end - 1e-5 {
            overlaps += 1
        }
        end = end.max(b);
    }
    let fraction = overlaps as f64 / l.events.len().max(1) as f64;
    // Stem identity is a prior only: low, mostly monophonic material is bass; high
    // mostly monophonic material lead; sustained/block polyphony pad/keys.
    let role = if pitch < 50. && fraction < 0.25 {
        Role::Bass
    } else if fraction < 0.15 {
        Role::Lead
    } else if duration >= 0.75 {
        Role::Pad
    } else {
        Role::Keys
    };
    (
        role,
        json!({"median_midi":pitch,"median_gate_seconds":duration,"overlap_fraction":fraction,"role":format!("{role:?}"),"stem_label_prior":label(l),"rule":"register + event overlap + gate duration; no song identity branches"}),
    )
}
fn patch_mut(w: &mut MusicWorld, r: Role) -> &mut Patch {
    match r {
        Role::Lead => &mut w.lead,
        Role::Bass => &mut w.bass,
        Role::Pad => &mut w.pad,
        Role::Keys => &mut w.keys,
    }
}
fn capacity(role: Role) -> usize {
    match role {
        Role::Bass | Role::Lead => 3,
        _ => 6,
    }
}
fn pack(events: &[Event], capacity: usize, sr: u32) -> Vec<Vec<Event>> {
    let mut sorted = events.to_vec();
    sorted.sort_by(|a, b| {
        a.onset_sec
            .total_cmp(&b.onset_sec)
            .then(a.midi.total_cmp(&b.midi))
    });
    let mut groups: Vec<(Vec<f64>, Vec<Event>)> = Vec::new();
    for event in sorted {
        let tail_end = event.offset_sec + 0.06 + 2. / sr as f64;
        if let Some((ends, notes)) = groups
            .iter_mut()
            .find(|(ends, _)| ends.iter().any(|x| *x <= event.onset_sec))
        {
            let ix = ends.iter().position(|x| *x <= event.onset_sec).unwrap();
            ends[ix] = tail_end;
            notes.push(event);
        } else {
            let mut ends = vec![0.; capacity];
            ends[0] = tail_end;
            groups.push((ends, vec![event]));
        }
    }
    groups.into_iter().map(|(_, n)| n).collect()
}
fn score(events: &[Event], role: Role, duration: f64) -> Result<Score> {
    // A 60 BPM rendering clock makes one Score beat exactly one source second.
    // It is not a recovered musical tempo and introduces no tempo normalization.
    let mut s = Score::new(60., 4., duration);
    for n in events {
        if !n.onset_sec.is_finite()
            || !n.offset_sec.is_finite()
            || n.onset_sec < 0.
            || n.offset_sec > duration + 1e-6
            || n.offset_sec <= n.onset_sec
            || !n.midi.is_finite()
            || !(0. ..=127.).contains(&n.midi)
            || !(0. ..=1.).contains(&n.confidence)
            || !n.velocity.is_finite()
            || !(0. ..=1.).contains(&n.velocity)
        {
            return Err("invalid raw-time lane event".into());
        }
        s.notes.push(Note::new(
            n.onset_sec,
            (n.offset_sec - n.onset_sec) as f32,
            n.midi.round() as i32,
            n.velocity,
            role,
            Provenance::new(SectionKind::A),
        ));
    }
    s.validate()?;
    Ok(s)
}
fn synth(s: &Score, w: &MusicWorld, sr: SampleRate) -> StereoBlock {
    // Keep the world's oscillator/filter character but remove time-smearing and nonlinear
    // mix treatments for this discriminating measurement playback. No note is generated.
    let control = ProductionControl {
        clean_waves: false,
        zero_detune: false,
        no_saturation: true,
        dry: true,
        no_bus_comp: true,
        short_release: true,
        no_chorus: true,
        no_echo: true,
        full_band_space: false,
    };
    let mut synth = HumanMusicSynth::with_production(s, w, sr, control);
    let frames = synth.total_samples();
    OfflineRenderer::new(sr, 512)
        .render(&mut synth, frames)
        .audio
}
fn target_rms(s: &Source, id: &str) -> Option<f64> {
    let stem = s.production.get("stems")?.get(id)?;
    if let Some(db) = stem
        .get("stereo")?
        .get("mid_rms_db")
        .and_then(Value::as_f64)
    {
        let side = stem["stereo"]["side_rms_db"].as_f64().unwrap_or(-120.);
        Some((10f64.powf(db / 10.) + 10f64.powf(side / 10.)).sqrt())
    } else {
        None
    }
}
fn finish_audio(path: &Path, a: &StereoBlock, sr: SampleRate) -> Result<Value> {
    write_wav_i16(path, a, sr).map_err(|e| e.to_string())?;
    Ok(
        json!({"path":path,"sha256":sha(&fs::read(path).map_err(|e|e.to_string())?),"frames":a.frames(),"sample_rate":sr.get(),"rms":a.rms(),"peak":a.peak()}),
    )
}
pub(super) fn run(input: &Path, out: &Path, sample_rate: u32) -> Result<()> {
    if out.exists() && out.read_dir().map_err(|e| e.to_string())?.next().is_some() {
        return Err("ensemble output must be a fresh empty directory".into());
    }
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let bytes = fs::read(input).map_err(|e| e.to_string())?;
    let s: Source = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if !s.duration_sec.is_finite() || s.duration_sec <= 0. {
        return Err("invalid source duration".into());
    }
    fs::write(out.join("source_snapshot.json"), &bytes).map_err(|e| e.to_string())?;
    let sr = SampleRate::new(sample_rate).ok_or("invalid sample rate")?;
    let lanes = selected_lanes(&s);
    let centroids: Vec<_> = lanes
        .iter()
        .filter_map(|l| s.production["stems"][&l.lane_id]["spectral_centroid_hz"].as_f64())
        .filter(|v| v.is_finite() && *v > 0.)
        .collect();
    let median_centroid = median(centroids).max(1.);
    let mut worlds = Vec::new();
    for world in MusicWorld::all() {
        let dir = out.join(world.name);
        fs::create_dir_all(dir.join("stems")).map_err(|e| e.to_string())?;
        fs::create_dir_all(dir.join("lane_wavs")).map_err(|e| e.to_string())?;
        let mut mix: Option<StereoBlock> = None;
        let mut receipts = Vec::new();
        let mut role_audio: BTreeMap<String, StereoBlock> = BTreeMap::new();
        for lane in &lanes {
            let (role, role_receipt) = behavior(lane);
            let mut w = world.clone();
            let centroid = s.production["stems"][&lane.lane_id]["spectral_centroid_hz"].as_f64();
            let ratio = centroid
                .filter(|v| v.is_finite() && *v > 0.)
                .map(|v| (v / median_centroid).clamp(0.5, 2.))
                .unwrap_or(1.);
            let patch = patch_mut(&mut w, role);
            // Source targets are retained as a deferred recipe; no source EQ fit before structure passes.
            let patch_receipt = format!("{patch:?}");
            let groups = pack(&lane.events, capacity(role), sample_rate);
            let mut audio: Option<StereoBlock> = None;
            for events in &groups {
                let score = score(events, role, s.duration_sec)?;
                let pcm = synth(&score, &w, sr);
                if let Some(a) = &mut audio {
                    a.add(&pcm)
                } else {
                    audio = Some(pcm)
                }
            }
            let mut audio = audio.ok_or("selected empty lane")?;
            let measured = audio.rms();
            let target = target_rms(&s, &lane.lane_id);
            let deferred_gain = target
                .filter(|_| measured > 1e-8)
                .map(|t| (t / f64::from(measured)).clamp(0.01, 10.))
                .unwrap_or(1.);
            let gain = 1.0;
            audio.scale(gain as f32);
            if let Some(m) = &mut mix {
                m.add(&audio)
            } else {
                mix = Some(audio.clone())
            }
            let role_name = format!("{role:?}").to_lowercase();
            if let Some(a) = role_audio.get_mut(&role_name) {
                a.add(&audio)
            } else {
                role_audio.insert(role_name, audio.clone());
            }
            let artifact = finish_audio(
                &dir.join("lane_wavs")
                    .join(format!("{}.wav", lane.lane_id.replace(':', "-"))),
                &audio,
                sr,
            )?;
            receipts.push(json!({"lane_id":lane.lane_id,"source_events":lane.events.len(),"score_events":groups.iter().map(Vec::len).sum::<usize>(),
                "synthetic_voice_groups":groups.len(),"group_purpose":"avoid native fixed-pool voice stealing; musical lane count unchanged",
                "source_provenance":lane.stem_identity,"role_observation":role_receipt,"patch":patch_receipt,"applied_spectral_ratio":1.,"deferred_spectral_ratio":ratio,
                "source_rms_target":target,"measured_unscaled_rms":measured,"gain":gain,"deferred_gain":deferred_gain,
                "max_onset_error_seconds":0.,"max_gate_representation_error_seconds":lane.events.iter().map(|n|((n.offset_sec-n.onset_sec)as f32 as f64-(n.offset_sec-n.onset_sec)).abs()).fold(0.,f64::max),
                "max_pitch_rounding_semitones":lane.events.iter().map(|n|(n.midi.round()-n.midi).abs()).fold(0.,f64::max),"audio":artifact}));
        }
        let mut drums = Score::new(60., 4., s.duration_sec);
        let mut omitted = Vec::new();
        for d in &s.drums {
            if !d.onset_sec.is_finite()
                || d.onset_sec < 0.
                || d.onset_sec >= s.duration_sec
                || !(0. ..=1.).contains(&d.confidence)
            {
                return Err("invalid raw drum event".into());
            }
            let voice = match d.family.as_str() {
                "kick" => DrumVoice::Kick,
                "snare" => DrumVoice::Snare,
                "hat" | "hihat" | "closed_hat" | "closedhat" => DrumVoice::ClosedHat,
                "open_hat" | "openhat" => DrumVoice::OpenHat,
                "clap" => DrumVoice::Clap,
                _ => {
                    omitted.push(json!({"onset_sec":d.onset_sec,"family":d.family,"reason":"unsupported native drum voice"}));
                    continue;
                }
            };
            drums.drums.push(DrumHit {
                start_beat: d.onset_sec,
                voice,
                velocity: 0.75,
                prov: Provenance::new(SectionKind::A),
            });
        }
        drums.validate()?;
        let mut audio = synth(&drums, &world, sr);
        let measured = audio.rms();
        let target = target_rms(&s, "htdemucs_ft:drums");
        let deferred_gain = target
            .filter(|_| measured > 1e-8)
            .map(|v| (v / f64::from(measured)).clamp(0.01, 10.))
            .unwrap_or(1.);
        let gain = 1.0;
        audio.scale(gain as f32);
        if let Some(m) = &mut mix {
            m.add(&audio)
        } else {
            mix = Some(audio.clone())
        }
        role_audio.insert("drums".into(), audio.clone());
        let drum_audio = finish_audio(&dir.join("lane_wavs/drums.wav"), &audio, sr)?;
        receipts.push(json!({"lane_id":"observed-drums","source_events":s.drums.len(),"score_events":drums.drums.len(),"omitted":omitted,"max_onset_error_seconds":0.,"gain":gain,"deferred_gain":deferred_gain,"source_rms_target":target,"audio":drum_audio}));
        let mut mix = mix.ok_or("no source events")?;
        let peak = mix.peak();
        let safety_gain = (0.9 / (lanes.len() + usize::from(!s.drums.is_empty())).max(1) as f32)
            .min(if peak > 0.95 { 0.95 / peak } else { 1. });
        mix.scale(safety_gain);
        let full = finish_audio(&dir.join("measurement_playback.wav"), &mix, sr)?;
        finish_audio(&dir.join("raw_mix.wav"), &mix, sr)?;
        let mut aggregate = Vec::new();
        for name in ["lead", "bass", "drums", "keys", "pad", "sfx"] {
            let a = role_audio
                .entry(name.into())
                .or_insert_with(|| StereoBlock::new(mix.frames()));
            a.scale(safety_gain);
            aggregate.push(finish_audio(
                &dir.join("stems").join(format!("{name}.wav")),
                a,
                sr,
            )?);
        }
        let mut support = role_audio["keys"].clone();
        support.add(&role_audio["pad"]);
        aggregate.push(finish_audio(&dir.join("stems/support.wav"), &support, sr)?);
        let config = json!({"scope":"measurement_playback_unfitted_control","source_production_fit_applied":false,"transport":"source_seconds","source_id":s.source_id,"input_sha256":sha(&bytes),"libgibson_anchor":ANCHOR,"world":world.name,"bypasses_composition_checks":true,"public_cover_admission":"not_applicable","admitted":false,"render_clock_bpm":60,"sample_rate":sample_rate});
        write_json(&dir.join("config.json"), &config)?;
        write_json(
            &dir.join("conformance.json"),
            &json!({"status":"measurement_playback","admitted":false,"public_cover_admission":"not_applicable","reason":"raw measurement playback bypasses composition; no fabricated pitch function or cover conformance","config":config}),
        )?;
        write_json(
            &dir.join("aggregate_render_receipt.json"),
            &json!(aggregate),
        )?;
        let row = json!({"schema":"sai.dynamic_ensemble_measurement_playback/v1","source_id":s.source_id,"source_sha256":s.source_sha256,"blueprint_sha256":s.blueprint_sha256,
            "input_sha256":sha(&bytes),"libgibson_anchor":ANCHOR,"world":world.name,"admitted":false,"admission_status":"not_applicable_measurement_playback_bypasses_composition",
            "musical_lanes":lanes.len()+usize::from(!s.drums.is_empty()),"safety_gain":safety_gain,"render_clock_bpm":60,"source_timing":"raw seconds, no inferred tempo normalization",
            "generated_backing_notes":0,"invented_chords":0,"unclassified_functions":"all pitched notes intentionally remain unclassified measurements",
            "production_control":"native oscillator/filter character, dry, no echo/chorus/saturation/compression,60ms release cap",
            "relational_timbre":"source targets recorded but NOT FIT; native role patches unchanged; structural gate precedes fitting","source_production_fit_applied":false,
            "lanes":receipts,"mix":full});
        write_json(&dir.join("receipt.json"), &row)?;
        worlds.push(row);
    }
    write_json(
        &out.join("manifest.json"),
        &json!({"source_id":s.source_id,"claim":"discriminating evidence-playback probe; not admitted fresh HumanMusic cover",
        "selection_policy":"ft vocals+bass;6s guitar+piano+other when nonempty;ft other only without6s harmonic lanes;never add both separator families for the same support content",
        "selected_lanes":lanes.iter().map(|l|&l.lane_id).collect::<Vec<_>>(),"omitted_lane_ids":s.lanes.iter().filter(|l|!lanes.iter().any(|x|x.lane_id==l.lane_id)).map(|l|&l.lane_id).collect::<Vec<_>>(),"worlds":worlds}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(at: f64, off: f64, pitch: f64) -> Event {
        Event {
            onset_sec: at,
            offset_sec: off,
            midi: pitch,
            velocity: 0.75,
            confidence: 0.8,
        }
    }
    #[test]
    fn raw_clock_preserves_syncopation_and_gates() {
        let es = vec![event(0.173, 0.431, 60.1), event(0.783, 1.127, 64.2)];
        let s = score(&es, Role::Lead, 2.).unwrap();
        assert_eq!(s.notes.len(), 2);
        for (n, e) in s.notes.iter().zip(&es) {
            assert_eq!(n.start_beat, e.onset_sec);
            assert!((f64::from(n.dur_beats) - (e.offset_sec - e.onset_sec)).abs() < 1e-6);
            assert!(n.function.is_none())
        }
        assert!(s.chords.is_empty());
    }
    #[test]
    fn packing_preserves_all_events_without_overflow() {
        let es = (0..14)
            .map(|i| event(0., 1., 48. + i as f64))
            .collect::<Vec<_>>();
        let groups = pack(&es, 6, 22050);
        assert_eq!(groups.len(), 3);
        assert_eq!(groups.iter().map(Vec::len).sum::<usize>(), es.len());
        assert!(groups.iter().all(|g| g.len() <= 6));
    }
    #[test]
    fn missing_lanes_create_no_backing() {
        let s = score(&[], Role::Keys, 2.).unwrap();
        assert!(s.notes.is_empty());
        assert!(s.chords.is_empty());
    }
    #[test]
    fn role_comes_from_behavior() {
        let l = Lane {
            lane_id: "htdemucs_6s:guitar".into(),
            events: vec![event(0., 1., 40.), event(2., 3., 43.)],
            stem_identity: json!({}),
        };
        assert_eq!(behavior(&l).0, Role::Bass);
    }
    #[test]
    fn rival_support_separators_are_not_doubled() {
        let lane = |id: &str| Lane {
            lane_id: id.into(),
            events: vec![event(0., 1., 60.)],
            stem_identity: json!({}),
        };
        let mut s = Source {
            source_id: "synthetic".into(),
            source_sha256: "a".repeat(64),
            blueprint_sha256: "b".repeat(64),
            duration_sec: 2.,
            lanes: vec![
                lane("htdemucs_ft:other"),
                lane("htdemucs_6s:guitar"),
                lane("htdemucs_ft:vocals"),
            ],
            drums: Vec::new(),
            production: json!({}),
        };
        let selected = selected_lanes(&s);
        assert!(!selected.iter().any(|l| l.lane_id == "htdemucs_ft:other"));
        assert_eq!(selected.len(), 2);
        s.lanes[1].events.clear();
        assert!(selected_lanes(&s)
            .iter()
            .any(|l| l.lane_id == "htdemucs_ft:other"));
    }
}
