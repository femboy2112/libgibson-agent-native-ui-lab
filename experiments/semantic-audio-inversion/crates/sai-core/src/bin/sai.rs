//! `sai` — the Semantic Audio Inversion CLI.
//!
//! Small, dependency-light, deterministic. Every subcommand reads or writes the versioned
//! evidence/quotient/truth JSON. It never runs an ML model; that is the adapters' job.

use std::path::PathBuf;
use std::process::ExitCode;

use sai_core::evidence::EvidenceArtifact;
use sai_core::quotient::{recover, RecoveredQuotient, RequestedProfile};
use sai_core::receipt::sha256_file;
use sai_core::truth::TruthTrack;
use sai_core::SaiResult;

fn usage() -> String {
    "\
sai — Semantic Audio Inversion

USAGE:
  sai validate <evidence.json>
  sai recover --evidence <evidence.json> --profile <loose|interpretive|faithful|strict|free>
              [--out <quotient.json>]
  sai evaluate --quotient <quotient.json> --truth <truth.json> [--out <report.json>]
  sai hash <file>
  sai schema

The analyzer consumes evidence artifacts produced by adapters (own DSP baseline or an external
instrument). It never embeds a model runtime, so ordinary tests stay cheap."
        .to_string()
}

fn arg_value(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1).cloned())
}

fn write_out(path: Option<String>, text: &str) -> SaiResult<()> {
    match path {
        Some(p) => {
            std::fs::write(&p, text)?;
            eprintln!("wrote {p}");
            Ok(())
        }
        None => {
            println!("{text}");
            Ok(())
        }
    }
}

fn run() -> SaiResult<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first().map(String::as_str) else {
        println!("{}", usage());
        return Ok(());
    };
    match cmd {
        "schema" => {
            println!("evidence : {}", sai_core::evidence::SCHEMA);
            println!("receipt  : {}", sai_core::receipt::RunReceipt::SCHEMA);
            println!("quotient : {}", RecoveredQuotient::SCHEMA);
            println!("truth    : {}", sai_core::truth::TRUTH_SCHEMA);
            println!("metrics  : {}", sai_core::metrics::METRICS_SCHEMA);
            Ok(())
        }
        "hash" => {
            let Some(p) = args.get(1) else {
                return Err(sai_core::SaiError::Io("hash needs a file".into()));
            };
            println!("{}  {}", sha256_file(&PathBuf::from(p))?, p);
            Ok(())
        }
        "validate" => {
            let Some(p) = args.get(1) else {
                return Err(sai_core::SaiError::Io("validate needs an evidence file".into()));
            };
            let bytes = std::fs::read(p)?;
            let a = EvidenceArtifact::from_json_slice(&bytes)?;
            println!(
                "OK schema={} notes={} beats={} onsets={} refusals={} unknowns={}",
                a.schema,
                a.notes.len(),
                a.timing.as_ref().map_or(0, |t| t.beats.len()),
                a.onsets.len(),
                a.refusals.len(),
                a.unknowns.len()
            );
            Ok(())
        }
        "recover" => {
            let ev = arg_value(&args, "--evidence")
                .ok_or_else(|| sai_core::SaiError::Io("recover needs --evidence <file>".into()))?;
            let profile = arg_value(&args, "--profile").unwrap_or_else(|| "interpretive".into());
            let requested = RequestedProfile::from_name(&profile).ok_or_else(|| {
                sai_core::SaiError::Schema(format!("unknown profile '{profile}'"))
            })?;
            let bytes = std::fs::read(&ev)?;
            let artifact = EvidenceArtifact::from_json_slice(&bytes)?;
            let q = recover(&artifact, &requested)?;
            write_out(arg_value(&args, "--out"), &q.to_json()?)
        }
        "evaluate" => {
            let qp = arg_value(&args, "--quotient")
                .ok_or_else(|| sai_core::SaiError::Io("evaluate needs --quotient <file>".into()))?;
            let tp = arg_value(&args, "--truth")
                .ok_or_else(|| sai_core::SaiError::Io("evaluate needs --truth <file>".into()))?;
            let q = RecoveredQuotient::from_json_slice(&std::fs::read(&qp)?)?;
            let t = TruthTrack::from_json_slice(&std::fs::read(&tp)?)?;
            let report = sai_core::evaluate(&q, &t);
            write_out(arg_value(&args, "--out"), &report.to_json()?)
        }
        _ => {
            eprintln!("{}\nunknown command '{cmd}'", usage());
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
