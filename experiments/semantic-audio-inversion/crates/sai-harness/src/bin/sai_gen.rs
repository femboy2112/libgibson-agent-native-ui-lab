//! `sai_gen` — generate the blind dataset and the mutation suite.
//!
//! Subcommands:
//!   sai_gen dataset   --out DIR --split dev|holdout [--per-kind N] [--seed-base S] [--total-beats B]
//!   sai_gen mutations --dev DIR --out DIR [--limit N]
//!
//! The holdout is generated under its own seed base and is never analyzed by the development
//! loop. The analyzer only ever receives the WAV path plus a receipt.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gibson::audio::human_music::world::WorldId;
use gibson::audio::wav::write_wav_i16;
use sai_core::receipt::sha256_hex;
use sai_harness::dataset::{
    build_composition, render_score, truth_from_score, ItemSpec, Manifest, ManifestItem,
    Split, TraceKind, ANCHOR_REV, SAMPLE_RATE,
};
use sai_harness::mutations::{apply_audio, apply_score, Mutation};

fn arg_value(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1).cloned())
}

fn worlds() -> [(WorldId, &'static str); 3] {
    [
        (WorldId::BlackIce, "black-ice"),
        (WorldId::Vapor95, "vapor95"),
        (WorldId::SwissSignal, "swiss-signal"),
    ]
}

fn ensure_dirs(out: &Path) -> Result<(PathBuf, PathBuf), String> {
    let wav = out.join("wav");
    let truth = out.join("truth");
    std::fs::create_dir_all(&wav).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&truth).map_err(|e| e.to_string())?;
    Ok((wav, truth))
}

fn write_wav_and_hash(path: &Path, audio: &gibson::audio::StereoBlock, sr: u32) -> Result<String, String> {
    let sample_rate = gibson::audio::time::SampleRate::new(sr).ok_or("invalid sample rate")?;
    write_wav_i16(path, audio, sample_rate).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(sha256_hex(&bytes))
}

fn cmd_dataset(args: &[String]) -> Result<(), String> {
    let out = PathBuf::from(arg_value(args, "--out").ok_or("dataset needs --out DIR")?);
    let split = match arg_value(args, "--split").as_deref() {
        Some("holdout") => Split::Holdout,
        _ => Split::Dev,
    };
    let per_kind: usize = arg_value(args, "--per-kind")
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);
    let seed_base: u64 = arg_value(args, "--seed-base")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1000);
    let total_beats: f64 = arg_value(args, "--total-beats")
        .and_then(|s| s.parse().ok())
        .unwrap_or(32.0);

    let (wav_dir, truth_dir) = ensure_dirs(&out)?;
    let mut items = Vec::new();
    for (world, _wname) in worlds() {
        for kind in TraceKind::ALL {
            for k in 0..per_kind {
                let seed = seed_base + (world as u64) * 10_000 + (kind as u64) * 100 + k as u64;
                let spec = ItemSpec::canonical(world, kind, seed, total_beats, split);
                let comp = build_composition(&spec);
                let w = spec.world();
                let rendered = render_score(&comp.score, &w, sample_rate());
                let wav_path = wav_dir.join(format!("{}.wav", spec.id));
                let wav_sha = write_wav_and_hash(&wav_path, &rendered.audio, SAMPLE_RATE)?;
                let truth = truth_from_score(
                    &spec,
                    &comp.score,
                    rendered.duration_seconds(),
                    wav_sha.clone(),
                );
                let truth_json = truth.to_json().map_err(|e| e.to_string())?;
                let truth_path = truth_dir.join(format!("{}.json", spec.id));
                std::fs::write(&truth_path, &truth_json).map_err(|e| e.to_string())?;
                items.push(ManifestItem {
                    id: spec.id.clone(),
                    world: format!("{:?}", spec.world),
                    kind: spec.kind.name().to_string(),
                    seed,
                    total_beats,
                    split: split.as_str().to_string(),
                    wav_sha256: wav_sha,
                    truth_sha256: sha256_hex(truth_json.as_bytes()),
                    duration_seconds: truth.duration_seconds,
                    established_axes: truth.established_axes.clone(),
                });
                eprintln!("generated {}", spec.id);
            }
        }
    }
    let manifest = Manifest {
        schema: Manifest::SCHEMA.into(),
        anchor_rev: ANCHOR_REV.into(),
        sample_rate_hz: SAMPLE_RATE,
        items,
    };
    std::fs::write(
        out.join("manifest.json"),
        manifest.to_json().map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    eprintln!("wrote {} items to {}", manifest.items.len(), out.display());
    Ok(())
}

fn sample_rate() -> gibson::audio::time::SampleRate {
    gibson::audio::time::SampleRate::new(SAMPLE_RATE).expect("valid sample rate")
}

fn mutation_suite() -> Vec<Mutation> {
    vec![
        Mutation::Transpose { semitones: 5 },
        Mutation::Transpose { semitones: -3 },
        Mutation::TempoScale { factor: 1.25 },
        Mutation::TempoScale { factor: 0.8 },
        Mutation::DeleteLeadNote { index: 0 },
        Mutation::ShiftLeadNote { index: 2, delta_beats: 0.25 },
        Mutation::ChordEdit { index: 0 },
        Mutation::MuteRole { role: "lead".into() },
        Mutation::MuteRole { role: "bass".into() },
        Mutation::MuteDrums,
        Mutation::Eq,
        Mutation::Compress,
        Mutation::Reverb,
        Mutation::Noise,
    ]
}

fn spec_from_manifest(item: &ManifestItem) -> Result<ItemSpec, String> {
    let world = match item.world.as_str() {
        "BlackIce" => WorldId::BlackIce,
        "Vapor95" => WorldId::Vapor95,
        "SwissSignal" => WorldId::SwissSignal,
        other => return Err(format!("unknown world '{other}'")),
    };
    let kind = TraceKind::from_name(&item.kind).ok_or_else(|| format!("unknown kind '{}'", item.kind))?;
    let split = if item.split == "holdout" { Split::Holdout } else { Split::Dev };
    Ok(ItemSpec {
        id: item.id.clone(),
        world,
        seed: item.seed,
        kind,
        total_beats: item.total_beats,
        split,
    })
}

fn cmd_mutations(args: &[String]) -> Result<(), String> {
    let dev = PathBuf::from(arg_value(args, "--dev").ok_or("mutations needs --dev DIR")?);
    let out = PathBuf::from(arg_value(args, "--out").ok_or("mutations needs --out DIR")?);
    let limit: usize = arg_value(args, "--limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    let manifest = Manifest::from_json_slice(
        &std::fs::read(dev.join("manifest.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let (wav_dir, truth_dir) = ensure_dirs(&out)?;
    let mut index = Vec::new();
    let suite = mutation_suite();
    for item in manifest.items.iter().take(limit) {
        let spec = spec_from_manifest(item)?;
        let comp = build_composition(&spec);
        let world = spec.world();
        for m in &suite {
            let label = format!("{}__{}", spec.id, m.label());
            // Score-level mutation first; PCM-only leaves the score unchanged.
            let mut score = comp.score.clone();
            apply_score(&mut score, m);
            let mut rendered = render_score(&score, &world, sample_rate());
            if m.is_pcm_only() {
                apply_audio(&mut rendered.audio, m, SAMPLE_RATE);
            }
            let wav_path = wav_dir.join(format!("{label}.wav"));
            let wav_sha = write_wav_and_hash(&wav_path, &rendered.audio, SAMPLE_RATE)?;
            let mut truth = truth_from_score(&spec, &score, rendered.duration_seconds(), wav_sha.clone());
            truth.id = label.clone();
            let truth_path = truth_dir.join(format!("{label}.json"));
            std::fs::write(&truth_path, truth.to_json().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            let expectation = m.expectation();
            index.push(serde_json::json!({
                "id": label,
                "source_id": spec.id,
                "mutation": m,
                "expectation": expectation,
                "wav_sha256": wav_sha,
                "truth_path": format!("truth/{label}.json"),
                "wav_path": format!("wav/{label}.wav"),
            }));
        }
        eprintln!("mutated {}", spec.id);
    }
    let doc = serde_json::json!({
        "schema": "sai.mutations/v1",
        "anchor_rev": ANCHOR_REV,
        "source_manifest": manifest.schema,
        "items": index,
    });
    std::fs::write(
        out.join("mutations.json"),
        serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    eprintln!("wrote {} mutations to {}", index.len(), out.display());
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("dataset") => cmd_dataset(&args),
        Some("mutations") => cmd_mutations(&args),
        _ => {
            eprintln!(
                "sai_gen — blind HumanMusic dataset + mutation generator\n\n\
                 USAGE:\n  \
                 sai_gen dataset   --out DIR --split dev|holdout [--per-kind N] [--seed-base S]\n  \
                 sai_gen mutations --dev DIR --out DIR [--limit N]"
            );
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
