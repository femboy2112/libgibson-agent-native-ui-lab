//! The sustained workload, short enough for CI: determinism, bounded retention, no refused
//! forks, and the bench's own accounting.

use project_chronoscope::bench::*;

#[test]
fn sustained_run_is_deterministic_and_bounded() {
    let cfg = SustainedConfig {
        frames: 1500,
        max_resident_recs: 12_000,
        ..SustainedConfig::default()
    };
    let a = run_sustained(&cfg);
    let b = run_sustained(&cfg);
    assert_eq!(
        a.digest, b.digest,
        "same seed, same autopilot, same bytes and cursors"
    );
    assert_eq!(a.total_bytes, b.total_bytes);
    assert_eq!(a.branches, b.branches);
    assert_eq!(a.frames, 1500);
    assert!(
        a.forks_made > 40,
        "the autopilot really forks ({})",
        a.forks_made
    );
    assert_eq!(a.fork_failures, 0);
    // retention: resident records never exceed the budget by more than the protected set
    // (a handful of full branches: the cursor's lineage, compare target, visible ghosts)
    assert!(
        a.max_resident_recs_seen < 12_000 + 8 * 640,
        "resident peak {}",
        a.max_resident_recs_seen
    );
    assert!(a.fossilized_total > 0, "old branches were fossilized");
    assert!(
        a.rehydrated_total > 0,
        "…and rebuilt by replay when revisited"
    );
    // every frame fits the renderer's own accounting
    assert!(a.bytes_per_frame.max > 0.0);
    assert!(
        a.full_repaints <= a.resizes + 20,
        "full repaints ({}) only on resize/first paint",
        a.full_repaints
    );
    println!("{}", a.to_markdown());
}

#[test]
fn a_different_autopilot_seed_gives_a_different_history_but_the_same_bounds() {
    let cfg = SustainedConfig {
        frames: 600,
        seed: 12345,
        max_resident_recs: 8_000,
        ..SustainedConfig::default()
    };
    let a = run_sustained(&cfg);
    let b = run_sustained(&SustainedConfig {
        seed: 54321,
        ..cfg.clone()
    });
    assert_ne!(a.digest, b.digest);
    assert!(
        a.max_resident_recs_seen < 8_000 + 8 * 640 && b.max_resident_recs_seen < 8_000 + 8 * 640
    );
}
