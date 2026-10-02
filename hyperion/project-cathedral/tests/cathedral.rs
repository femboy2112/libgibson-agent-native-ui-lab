//! Integration tests that drive the real `cathedral` binary through its public CLI.
//!
//! These are black-box: they assert the deterministic contracts Project Cathedral
//! promises to its own consumers (capability matrix, scripted incident, deterministic
//! replay, byte-stable text capture) without reaching into private modules.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_cathedral")
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(bin())
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("run cathedral {args:?}: {e}"))
}

fn stdout_of(args: &[&str]) -> String {
    let out = run(args);
    assert!(
        out.status.success(),
        "cathedral {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Parse the `CATHEDRAL_METRICS {...}` JSON line into a serde_json value.
fn metrics(args: &[&str]) -> serde_json::Value {
    let text = stdout_of(args);
    let line = text
        .lines()
        .find(|l| l.starts_with("CATHEDRAL_METRICS "))
        .unwrap_or_else(|| panic!("no metrics line in {args:?}:\n{text}"));
    serde_json::from_str(line.trim_start_matches("CATHEDRAL_METRICS "))
        .expect("metrics line is JSON")
}

fn unique_path(tag: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("cathedral-test-{tag}-{nanos}.json"))
}

#[test]
fn capability_matrix_preserves_state_affordance_and_exit_at_every_size() {
    let text = stdout_of(&["--capability"]);
    assert!(
        text.contains("all sizes preserve state + affordance + exit: yes"),
        "capability matrix did not pass:\n{text}"
    );
    // 5 sizes × 4 depths.
    let rows = text
        .lines()
        .filter(|l| {
            l.contains("TrueColor")
                || l.contains("Ansi256")
                || l.contains("Ansi16")
                || l.contains("Mono")
        })
        .filter(|l| l.split_whitespace().next().is_some_and(|s| s.contains('x')))
        .count();
    assert_eq!(rows, 20, "expected 20 capability rows, got {rows}");
}

#[test]
fn color_depth_degrades_monotonically() {
    let text = stdout_of(&["--capability"]);
    // For each size row group, color codes must not increase as depth falls.
    let mut by_size: std::collections::BTreeMap<String, Vec<(String, usize)>> =
        std::collections::BTreeMap::new();
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        // size depth bytes exact state cluster afford exit colorcodes
        if cols.len() < 9 || !cols[0].contains('x') {
            continue;
        }
        let size = cols[0].to_string();
        let depth = cols[1].to_string();
        let codes: usize = cols[8].parse().expect("colorcodes column");
        by_size.entry(size).or_default().push((depth, codes));
    }
    assert_eq!(by_size.len(), 5, "expected five sizes");
    for (size, rows) in &by_size {
        assert_eq!(rows.len(), 4, "{size}: four depths");
        assert!(
            rows[0].1 >= rows[1].1 && rows[1].1 >= rows[2].1 && rows[2].1 >= rows[3].1,
            "{size}: color codes not monotone across TrueColor/256/16/Mono: {rows:?}"
        );
        // Mono must strip at least three quarters of the escape sequences, and stay
        // small in absolute terms. (An absolute cap alone is brittle: it tracks the
        // exact chrome, not the degradation.)
        assert!(
            rows[3].1 * 4 <= rows[0].1,
            "{size}: Mono kept too many escapes vs TrueColor: {rows:?}"
        );
        assert!(
            rows[3].1 <= 200,
            "{size}: Mono should be nearly escape-free"
        );
    }
}

#[test]
fn scripted_incident_reaches_restored_and_commits_the_report() {
    let record = unique_path("incident");
    let out = Command::new(bin())
        .args([
            "--wtf",
            "--frames=650",
            "--no-music",
            &format!("--record={}", record.display()),
        ])
        .env("CATHEDRAL_TRACE", "1")
        .output()
        .expect("run scripted incident");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let trace = String::from_utf8_lossy(&out.stderr);
    for needle in [
        "LOCAL FAULT",
        "CASCADE",
        "ROLLBACK",
        "REROUTE",
        "RESTORED",
        "INCIDENT REPORT",
    ] {
        assert!(trace.contains(needle), "trace missing {needle:?}:\n{trace}");
    }
    let m = metrics(&["--wtf", "--frames=650", "--no-music"]);
    assert_eq!(
        m["phase"], "NORMAL",
        "system should have settled after recovery"
    );
    assert_eq!(m["incident_events"], 11);
    let _ = std::fs::remove_file(record);
}

#[test]
fn deterministic_replay_reconstructs_equivalent_state_with_music() {
    let record = unique_path("replay");
    let recorded = run(&[
        "--wtf",
        "--frames=240",
        "--music",
        &format!("--record={}", record.display()),
    ]);
    assert!(
        recorded.status.success(),
        "record failed: {}",
        String::from_utf8_lossy(&recorded.stderr)
    );
    let before = metrics(&["--wtf", "--frames=240", "--music"]);
    let replay = stdout_of(&[&format!("--replay={}", record.display())]);
    assert!(replay.contains("REPLAY OK"), "replay output:\n{replay}");
    assert!(
        replay.contains(before["final_digest"].as_str().unwrap()),
        "replay digest does not match the recorded run\n{replay}\nvs {}",
        before["final_digest"]
    );
    let _ = std::fs::remove_file(record);
}

#[test]
fn same_seed_reproduces_and_a_different_input_diverges() {
    let a = metrics(&["--wtf", "--frames=220", "--no-music"]);
    let b = metrics(&["--wtf", "--frames=220", "--no-music"]);
    assert_eq!(a["final_digest"], b["final_digest"], "same seed diverged");
    // The same fixture and seed with the scripted incident withheld reaches a
    // different semantic state at the same frame: the ordered action journal, not
    // wall time, is the input that moves the system.
    let baseline = metrics(&["--frames=220", "--no-music"]);
    assert_ne!(
        a["final_digest"], baseline["final_digest"],
        "withholding the scenario did not change state"
    );
    assert_eq!(baseline["incident_events"], 0);
    assert_eq!(baseline["actions"], 0);
}

#[test]
fn text_capture_is_byte_stable_and_shows_the_contract() {
    let args = [
        "--wtf",
        "--at=220",
        "--text",
        "--no-music",
        "--width=100",
        "--height=30",
    ];
    let first = run(&args);
    let second = run(&args);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    // The metrics line carries wall-time telemetry; only the rendered frame is a
    // determinism contract, so compare everything before CATHEDRAL_METRICS.
    let frame = |bytes: &[u8]| -> Vec<u8> {
        let marker = b"\nCATHEDRAL_METRICS";
        match bytes.windows(marker.len()).position(|w| w == marker) {
            Some(i) => bytes[..i].to_vec(),
            None => bytes.to_vec(),
        }
    };
    assert_eq!(
        frame(&first.stdout),
        frame(&second.stdout),
        "readable text frame is not byte-stable"
    );
    let text = String::from_utf8_lossy(&frame(&first.stdout)).into_owned();
    for needle in ["CATHEDRAL", "hot ", "inject-fault", "[Q]uit"] {
        assert!(
            text.contains(needle),
            "text frame missing {needle:?}:\n{text}"
        );
    }
    // The readable surface must be free of ANSI (that is the point of --text).
    assert!(!text.contains('\u{1b}'), "--text leaked ANSI escapes");
}

#[test]
fn footer_does_not_collide_with_the_music_axis_at_narrow_widths() {
    // 80x24 is the classic default terminal, and it is exactly where the
    // right-aligned music axis used to overwrite the left footer run.
    let out = run(&[
        "--wtf",
        "--at=220",
        "--text",
        "--no-music",
        "--width=80",
        "--height=24",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .find(|l| l.contains("TNeutral EMuted"))
        .expect("music-axis footer line missing");
    assert!(
        line.chars().count() <= 80,
        "footer overflowed 80 columns: {line:?}"
    );
    let idx = line.find("TNeutral").unwrap();
    assert!(
        line[..idx].chars().last().is_none_or(|c| c == ' '),
        "music axis collided with the left footer run: {line:?}"
    );
    assert!(
        !line.contains("joTNeutral"),
        "footer collision regressed: {line:?}"
    );
}

#[test]
fn long_run_stays_bounded_and_reports_music_receipts() {
    let m = metrics(&[
        "--wtf",
        "--frames=650",
        "--music",
        "--width=160",
        "--height=50",
    ]);
    assert_eq!(m["frames"], 650);
    assert_eq!(
        m["music_rejections"], 0,
        "checked BAND route rejected a take"
    );
    assert!(
        m["rss_end_kib"].as_u64().unwrap() < 200_000,
        "RSS grew unexpectedly: {}",
        m["rss_end_kib"]
    );
    assert!(
        m["history_insertions"].as_u64().unwrap() <= 32,
        "scrollback history is not bounded: {}",
        m["history_insertions"]
    );
}
