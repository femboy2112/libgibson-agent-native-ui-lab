//! Model-level unit tests: lane scheduling, camera math, intraline emphasis,
//! date formatting, bounded indexing, and fixture determinism.

mod common;

use common::*;

use palimpsest::app::Camera;
use palimpsest::theme::{fmt_age, fmt_date, pad_left, pad_to, truncate};

#[test]
fn fixture_lanes_are_concurrent_and_braided() {
    let app = app_for("medium");
    // 4 strands + trunk + exp lane live concurrently: at least 5 lanes.
    assert!(
        app.hist.lanes >= 5,
        "interleaved strands must produce concurrent lanes, got {}",
        app.hist.lanes
    );
    // merges exist
    assert!(
        app.hist.rows.iter().any(|r| r.is_merge),
        "fixture must contain merge commits"
    );
    // every row's parents exist in the index (except roots)
    for r in app.hist.rows.iter() {
        if let Some(p) = &r.parent1 {
            assert!(
                app.hist.idx_of(p).is_some(),
                "parent {p} missing from index"
            );
        }
    }
}

#[test]
fn fixture_is_deterministic_across_builds() {
    let dir1 = tempfile::tempdir().unwrap();
    let dir2 = tempfile::tempdir().unwrap();
    let p1 =
        palimpsest::fixture::build(dir1.path(), &palimpsest::fixture::FixtureSpec::tiny()).unwrap();
    let p2 =
        palimpsest::fixture::build(dir2.path(), &palimpsest::fixture::FixtureSpec::tiny()).unwrap();
    let h1 = git2::Repository::open(&p1)
        .unwrap()
        .head()
        .unwrap()
        .target()
        .unwrap();
    let h2 = git2::Repository::open(&p2)
        .unwrap()
        .head()
        .unwrap()
        .target()
        .unwrap();
    assert_eq!(
        h1, h2,
        "fixtures built from the same spec must be byte-identical"
    );
}

#[test]
fn camera_math_is_inverse() {
    let cam = Camera {
        px_per_day: 10.0,
        t_center: 1_700_000_000,
    };
    let w = 100u16;
    let t = 1_700_086_400; // +1 day
    let x = cam.x_of(t, w);
    assert_eq!(x, 60, "1 day at 10px/day is 10 cols right of center");
    assert_eq!(cam.t_of(x, w), t, "x/t mapping must round-trip");
    assert_eq!(cam.days_visible(w), 10.0);
}

#[test]
fn history_window_is_bounded_and_ordered() {
    let app = app_for("large");
    let (t0, t1) = (app.hist.t_min + 86_400 * 10, app.hist.t_min + 86_400 * 40);
    let win = app.hist.window(t0, t1);
    assert!(!win.is_empty());
    let mut last = i64::MIN;
    for &i in &win {
        let t = app.hist.rows[i as usize].time;
        assert!(
            t >= t0 && t <= t1,
            "window must contain only in-range commits"
        );
        assert!(t >= last, "window must be ordered oldest-first");
        last = t;
    }
    // window must be much smaller than the whole index
    assert!(
        win.len() < app.hist.len() / 4,
        "window must be a small slice"
    );
}

#[test]
fn tier_a_stats_resolve_within_budget() {
    let mut app = app_for("large");
    let win: Vec<u32> = (0..100).collect();
    let resolved = app.hist.resolve_window_stats(&app.repo, &win);
    assert!(resolved <= 24, "per-frame stat budget must be respected");
    let resolved2 = app.hist.resolve_window_stats(&app.repo, &win);
    // second pass resolves more (budget resets per call cycle)
    assert!(resolved + resolved2 >= 24);
}

#[test]
fn tier_b_line_stat_matches_fixture_shape() {
    let mut app = app_for("tiny");
    let sel = app.selection.unwrap();
    let oid = app.hist.rows[sel as usize].oid.clone();
    let stat = app.hist.line_stat(&app.repo, &oid).expect("tier-B stat");
    assert!(stat.files >= 1, "rename commit touches files");
    assert!(stat.adds >= 1, "fixture adds lines");
}

#[test]
fn intraline_emphasis_marks_the_changed_interior() {
    let del = "fn alpha(a: i32, b: i32) -> i32 {";
    let add = "fn alpha(a: u64, b: i32) -> i32 {";
    let (start, len) =
        palimpsest::git::diff::interior_difference(del, add).expect("interior exists");
    assert_eq!(
        &del[start..start + len],
        "i32",
        "the changed interior must be located"
    );
    assert_eq!(&add[start..start + len], "u64");
}

#[test]
fn date_formatting_is_civil() {
    assert_eq!(fmt_date(0), "1970-01-01");
    assert_eq!(fmt_date(1_700_000_000), "2023-11-14");
    assert_eq!(fmt_date(1_704_067_200), "2024-01-01");
    assert_eq!(fmt_age(1_700_000_000, 1_700_000_000), "0s");
    assert_eq!(fmt_age(1_700_000_000, 1_700_003_600), "1h");
}

#[test]
fn text_width_helpers_respect_display_columns() {
    assert_eq!(truncate("hello", 10), "hello");
    assert_eq!(truncate("hello world", 5), "hell…");
    assert_eq!(pad_to("ab", 5), "ab   ");
    assert_eq!(pad_to("abcdef", 3), "abc");
    assert_eq!(pad_left("42", 5), "   42");
    // CJK: display columns, not chars (each glyph is 2 columns)
    assert_eq!(truncate("日本語テスト", 5), "日本…");
    assert_eq!(truncate("日本語テスト", 2), "…");
}

#[test]
fn search_hits_are_keyed_and_newest_first() {
    let app = app_for("medium");
    let hits = app.hist.search("feat");
    assert!(!hits.is_empty());
    // newest-first: row indices ascending
    let mut sorted = hits.clone();
    sorted.sort_unstable();
    assert_eq!(hits, sorted, "hits are already in row order (newest first)");
}

#[test]
fn blame_maps_lines_to_introducing_commits() {
    let app = app_for("tiny");
    let sel = app.selection.unwrap();
    let oid = app.hist.rows[sel as usize].oid.clone();
    let blame = palimpsest::git::blame::blame_file(&app.repo, &oid, "src/foundation.rs", 1000)
        .expect("blame");
    // 7 content lines (the trailing newline does not create a phantom line)
    assert_eq!(blame.lines.len(), 7, "fixture file body is 7 lines");
    // lines 1-3 were born in the initial commit; 4-7 in the rename commit
    let first = &blame.lines[0];
    assert_ne!(first.oid, oid, "anchor lines predate the rename commit");
    let last = &blame.lines[6];
    assert_eq!(last.oid, oid, "span() lines were born in the rename commit");
}

#[test]
fn file_history_follows_renames() {
    let app = app_for("medium");
    // src/foundation.rs was born as src/core.rs and renamed in the last commit
    let events =
        palimpsest::git::filelog::file_events(&app.repo, &app.hist, "src/foundation.rs", 3000);
    assert!(!events.is_empty(), "events must be recovered");
    assert!(
        events.iter().any(|e| e.status == 'R'),
        "the rename event must be recovered with status R"
    );
    assert!(
        events
            .iter()
            .any(|e| e.prev_path.as_deref() == Some("src/core.rs")),
        "rename chain must point at the previous path"
    );
}
