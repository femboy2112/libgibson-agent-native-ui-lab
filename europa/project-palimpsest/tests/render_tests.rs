//! Rendered-output tests: the actual visible frames at every responsive
//! target size, in every view, in every color mode. These test what a user
//! sees — the flattened terminal grid — not internal model state.

mod common;

use common::*;

use palimpsest::app::View;
use palimpsest::theme::Palette;

#[test]
fn atlas_renders_braids_at_all_target_sizes() {
    for (w, h) in [
        (160u16, 50u16),
        (120, 40),
        (100, 30),
        (80, 24),
        (60, 20),
        (42, 15),
    ] {
        let mut app = app_for("medium");
        app.ensure_metrics();
        let (raw, rows) = render_frame(&mut app, w, h);
        let text = frame_text(&rows);
        assert!(text.contains("PALIMPSEST"), "header missing at {w}x{h}");
        // semantic identity: full title when roomy, compact code when hostile
        if w >= 60 {
            assert!(
                text.contains("HISTORY ATLAS"),
                "view title missing at {w}x{h}"
            );
        } else {
            assert!(
                text.contains("ATLAS"),
                "compact view code missing at {w}x{h}"
            );
        }
        // the braid vocabulary is present: nodes and rails
        assert!(
            text.contains('●') || text.contains('◈'),
            "commit nodes missing at {w}x{h}"
        );
        // responsive invariants
        for (i, row) in rows.iter().enumerate() {
            assert!(
                common::strip_ansi(row).chars().count() <= w as usize + 2,
                "row {i} exceeds {w} cols at {w}x{h}: {:?}",
                row
            );
        }
        let _ = raw;
    }
}

#[test]
fn atlas_overview_shows_full_history_ruler() {
    let mut app = app_for("medium");
    let (_, rows) = render_frame(&mut app, 120, 40);
    let text = frame_text(&rows);
    // ruler dates for the medium fixture window (2024-01 .. 2024-03)
    assert!(
        text.contains("2024-01"),
        "ruler start missing: {}",
        text.lines().take(3).collect::<Vec<_>>().join("\n")
    );
    assert!(text.contains("lens"), "scale line missing");
    assert!(text.contains("in view"), "window commit count missing");
}

#[test]
fn atlas_braids_show_strand_labels_when_wide() {
    let mut app = app_for("medium");
    let (_, rows) = render_frame(&mut app, 160, 50);
    let text = frame_text(&rows);
    assert!(
        text.contains("feature/strand-1"),
        "strand labels missing at 160 cols"
    );
    // merge joins visible: merge glyph or join verticals
    assert!(
        text.contains('◉') || text.contains('│'),
        "merge topology missing"
    );
}

#[test]
fn atlas_degrades_to_focus_lens_at_hostile_width() {
    let mut app = app_for("medium");
    let (_, rows) = render_frame(&mut app, 42, 15);
    let text = frame_text(&rows);
    // hostile mode: no ruler labels, but semantic identity survives
    assert!(
        !text.contains("2024-01-05"),
        "ruler labels should collapse at 42 cols"
    );
    assert!(text.contains("PALIMPSEST"));
    assert!(text.contains('◈'), "selection pin must survive");
}

#[test]
fn minimap_brackets_camera_window() {
    let mut app = app_for("medium");
    let (_, rows) = render_frame(&mut app, 120, 40);
    let text = frame_text(&rows);
    assert!(text.contains('❰'), "minimap left bracket missing");
    assert!(text.contains('❱'), "minimap right bracket missing");
}

#[test]
fn selected_commit_pin_and_dossier_present() {
    let mut app = app_for("medium");
    let short = app.sel_row().unwrap().short.clone();
    let (_, rows) = render_frame(&mut app, 120, 40);
    let text = frame_text(&rows);
    assert!(text.contains(&short), "selected oid {short} missing");
    assert!(text.contains('┊'), "selection pin missing");
    assert!(text.contains("SELECTION"), "dossier panel missing");
}

#[test]
fn strata_event_and_ruler_share_the_atlas_camera() {
    let mut app = app_for("medium");
    app.ensure_strata("src/hot/module_0.rs");
    let t = app.strata.events[0].time;
    app.camera.t_center = t;
    let pal = app.palette;
    let s = palimpsest::views::strata::draw_strata(&mut app, 120, 30, &pal);
    let x = app.camera.x_of(t, 120) as u16;
    assert_eq!(s.get(x, 2).unwrap().glyph.grapheme, "◈");

    app.camera.t_center = t + 2 * 86_400;
    let s2 = palimpsest::views::strata::draw_strata(&mut app, 120, 30, &pal);
    let shifted_x = app.camera.x_of(t, 120) as u16;
    assert_ne!(x, shifted_x, "the file event moves on the shared time axis");
    assert_eq!(s2.get(shifted_x, 2).unwrap().glyph.grapheme, "◈");
}

#[test]
fn diff_lens_renders_hunks_with_line_numbers() {
    let mut app = app_for("medium");
    app.ensure_lens();
    app.view = View::Lens;
    let (_, rows) = render_frame(&mut app, 120, 40);
    let text = frame_text(&rows);
    assert!(text.contains("DIFF LENS"), "lens title missing");
    assert!(text.contains("FILES"), "file ledger missing");
    assert!(text.contains("@@"), "hunk headers missing");
    assert!(text.contains("pub fn"), "source text missing");
}

#[test]
fn provenance_renders_fibers_and_origins() {
    let mut app = app_for("tiny");
    app.ensure_lens();
    if let Some(diff) = app.lens.diff.clone() {
        if let Some(f) = diff.files.first() {
            app.ensure_prov(f.path());
        }
    }
    app.view = View::Provenance;
    let (_, rows) = render_frame(&mut app, 120, 40);
    let text = frame_text(&rows);
    assert!(text.contains("FIBERS"), "fibers header missing");
    assert!(text.contains('◆'), "age glyphs missing");
    // every visible source line carries its origin
    assert!(text.contains("pub fn anchor"), "source text missing");
}

#[test]
fn strata_renders_worldline_and_ledger() {
    let mut app = app_for("medium");
    app.ensure_strata("src/hot/module_0.rs");
    app.view = View::Strata;
    let (_, rows) = render_frame(&mut app, 120, 40);
    let text = frame_text(&rows);
    assert!(text.contains("FILE STRATA"), "strata title missing");
    assert!(text.contains("events"), "event ledger header missing");
    assert!(text.contains('━'), "existence band missing");
    // the event ledger has at least one dated row
    assert!(text.contains("2024-01"), "event dates missing");
}

#[test]
fn health_renders_editorial_report() {
    let mut app = app_for("medium");
    app.ensure_metrics();
    let m = app.metrics.as_ref().unwrap();
    assert!(
        m.complete_scan,
        "medium fixture should have complete file metrics"
    );
    assert_eq!(m.sample_size, m.commits);
    assert!(
        !m.largest.is_empty(),
        "largest commits must be available before lazy tier-A stats"
    );
    let largest_oid = m.largest[0].short.clone();
    assert!(m
        .largest
        .windows(2)
        .all(|pair| pair[0].files >= pair[1].files));
    assert!((0.0..=1.0).contains(&m.concentration));
    assert_eq!(m.branches.len(), app.branches.len());
    assert!(m
        .branches
        .iter()
        .any(|b| b.name == "exp/never-merges" && !b.in_window));
    assert!(app.now >= m.branches.iter().map(|b| b.last_commit).max().unwrap());
    assert!(
        palimpsest::views::health::report_text(&app).contains("exp/never-merges*"),
        "report includes the disconnected branch with an explicit index marker"
    );
    app.view = View::Health;
    let (_, rows) = render_frame(&mut app, 120, 40);
    let text = frame_text(&rows);
    assert!(text.contains("REPOSITORY HEALTH"));
    assert!(text.contains("AUTHOR DISTRIBUTION"));
    assert!(text.contains("HOTTEST FILES"));
    assert!(text.contains("LARGEST COMMITS"));
    assert!(
        text.contains(&largest_oid),
        "first sampled largest commit missing from rendered report"
    );
    assert!(text.contains("BRANCH AGES"));
}

#[test]
fn mono_mode_emits_no_color_escapes() {
    let mut app = app_for("tiny");
    app.palette = Palette::mono();
    let (raw, rows) = render_frame(&mut app, 100, 30);
    let text = frame_text(&rows);
    assert!(
        text.contains('●') || text.contains('◈'),
        "mono keeps the topology glyphs"
    );
    assert!(!raw.contains("38;5;"), "mono must not emit 256-color codes");
    assert!(!raw.contains("38;2;"), "mono must not emit truecolor codes");
}

#[test]
fn mono_diff_and_provenance_preserve_source_without_color() {
    let mut app = app_for("tiny");
    app.palette = Palette::mono();
    app.ensure_lens();
    let path = app.lens.diff.as_ref().unwrap().files[0].path().to_string();
    for view in [View::Lens, View::Strata, View::Provenance] {
        match view {
            View::Strata => app.ensure_strata(&path),
            View::Provenance => app.ensure_prov(&path),
            _ => {}
        }
        app.view = view;
        let (raw, rows) = render_frame(&mut app, 80, 24);
        assert!(
            !raw.contains("38;5;") && !raw.contains("38;2;"),
            "{view:?} emitted color in mono"
        );
        assert!(
            frame_text(&rows).contains(&path),
            "{view:?} lost file identity"
        );
    }
}

#[test]
fn ansi16_mode_emits_no_truecolor() {
    let mut app = app_for("tiny");
    app.palette = Palette::ansi16();
    let (raw, _) = render_frame(&mut app, 100, 30);
    assert!(
        !raw.contains("38;2;"),
        "ansi16 must not emit truecolor codes"
    );
    assert!(
        !raw.contains("38;5;"),
        "ansi16 must not emit 256-color codes"
    );
}

#[test]
fn scrollback_artifact_reaches_output() {
    let mut app = app_for("tiny");
    palimpsest::app::keys::emit_dossier(&mut app);
    assert!(!app.scrollback_log.is_empty(), "dossier queued");
    let mut ctx = gibson::context::Context::headless(gibson::context::RenderMode::Inline, 100, 30);
    // flush through the same path the interactive loop uses
    {
        let mut taken = Vec::new();
        while let Some(text) = app.scrollback_log.first().cloned() {
            let _ = ctx.insert_text_before_live("───── palimpsest artifact ─────");
            let _ = ctx.insert_text_before_live(&text);
            taken.push(text);
            app.scrollback_log.remove(0);
        }
        let out = ctx.take_output();
        let all = common::strip_ansi(&out);
        assert!(all.contains("palimpsest artifact"), "artifact rule missing");
        assert!(all.contains("oid "), "dossier oid missing from scrollback");
    }
}

#[test]
fn empty_repository_renders_graceful_state() {
    let dir = tempfile::tempdir().unwrap();
    let repo = palimpsest::git::repo::Repo::open(dir.path()).unwrap_or_else(|_| {
        // not a git dir: init one
        git2::Repository::init(dir.path()).unwrap();
        palimpsest::git::repo::Repo::open(dir.path()).unwrap()
    });
    let mut app = palimpsest::app::App::load(repo, Palette::color(), 5000).expect("empty repo app");
    let (_, rows) = render_frame(&mut app, 80, 24);
    let text = frame_text(&rows);
    assert!(
        text.contains("empty history"),
        "empty repo must explain itself, got: {}",
        text.lines().take(4).collect::<Vec<_>>().join(" | ")
    );
}

#[test]
fn truncation_notice_when_limit_hits() {
    let path = fixture_path("large");
    let repo = palimpsest::git::repo::Repo::open(&path).unwrap();
    let mut app = palimpsest::app::App::load(repo, Palette::color(), 200).unwrap();
    assert_eq!(app.hist.len(), 200, "index bound respected");
    assert!(app.hist.truncated, "the bound must be flagged honestly");
    let (_, rows) = render_frame(&mut app, 120, 40);
    let text = frame_text(&rows);
    assert!(text.contains("200c"), "loaded count shown");
}
