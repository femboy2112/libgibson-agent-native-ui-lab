//! Wire-level probe: replay a scripted session through a headless context,
//! drain every emitted frame, and replay the accumulated bytes through a
//! real terminal model (the same flattener the render tests use). Each frame
//! is snapshotted — these tests assert what a user would actually *see* at
//! every step of a differential-update session, which is the strongest form
//! of "test actual rendered output".

mod common;

use common::*;

use gibson::input::{KeyCode, KeyEvent, KeyModifiers};
use palimpsest::app::{keys, App};

fn press(app: &mut App, code: KeyCode) {
    keys::handle_key(
        app,
        KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
        },
    );
}

/// Drain every frame emitted across a scripted session. Returns the
/// accumulated wire bytes and a screen snapshot *after each frame*.
fn replay_session(
    app: &mut App,
    script: &[(KeyCode, &str)],
    w: u16,
    h: u16,
) -> (String, Vec<Vec<String>>) {
    let mut ctx = gibson::context::Context::headless(gibson::context::RenderMode::Inline, w, h);
    ctx.set_color_depth(gibson::capability::ColorDepth::Ansi256);
    app.width = w;
    app.height = h;

    let mut wire = String::new();
    let mut snapshots: Vec<Vec<String>> = Vec::new();

    fn render_once(
        app: &mut App,
        ctx: &mut gibson::context::Context,
        wire: &mut String,
        snapshots: &mut Vec<Vec<String>>,
        w: u16,
    ) {
        app.mark();
        let pal = app.palette;
        let tree = palimpsest::views::build_ui(app, &pal);
        ctx.set_root(tree);
        app.dirty = false;
        let _ = ctx.render();
        wire.push_str(&ctx.take_output());
        snapshots.push(flatten(wire, w));
    }

    render_once(app, &mut ctx, &mut wire, &mut snapshots, w);
    for _ in 0..40 {
        app.tick_animation();
    }

    for (code, _label) in script {
        press(app, *code);
        for _ in 0..40 {
            app.tick_animation();
        }
        render_once(app, &mut ctx, &mut wire, &mut snapshots, w);
    }

    (wire, snapshots)
}

fn header_of(snap: &[String]) -> String {
    snap.iter()
        .find(|r| r.contains("PALIMPSEST"))
        .cloned()
        .unwrap_or_default()
}

#[test]
fn differential_frames_compose_faithfully_atlas_to_lens() {
    let mut app = app_for("tiny");
    let script = vec![
        (KeyCode::Char('j'), "step selection"),
        (KeyCode::Enter, "open the diff lens"),
        (KeyCode::Esc, "back to atlas"),
        (KeyCode::Char('5'), "health view"),
        (KeyCode::Char('R'), "commit report"),
        (KeyCode::Char('1'), "back to atlas"),
    ];
    let (wire, snapshots) = replay_session(&mut app, &script, 110, 32);

    // Every intermediate screen a user would have seen must be faithful.
    // The lens frame must show the lens title contiguously on screen...
    assert!(
        snapshots
            .iter()
            .any(|s| s.iter().any(|r| r.contains("DIFF LENS"))),
        "the lens frame must show DIFF LENS on screen ({} frames captured)",
        snapshots.len()
    );
    // ...the health frame its title...
    assert!(
        snapshots
            .iter()
            .any(|s| s.iter().any(|r| r.contains("REPOSITORY HEALTH"))),
        "the health frame must show its title"
    );
    // ...and the final screen must be a clean atlas, composed from a chain of
    // differential frames (no stale-cell chimera).
    let header = snapshots.last().map(|s| header_of(s)).unwrap_or_default();
    assert!(
        header.contains("HISTORY ATLAS"),
        "final header must read HISTORY ATLAS cleanly, got: {header:?}"
    );
    let _ = &wire;
}

#[test]
fn rapid_same_key_renders_stay_clean() {
    let mut app = app_for("medium");
    let script: Vec<(KeyCode, &str)> = (0..12)
        .map(|_| (KeyCode::Char('j'), "step"))
        .chain([(KeyCode::Enter, "lens"), (KeyCode::Esc, "esc")])
        .collect();
    let (_, snapshots) = replay_session(&mut app, &script, 90, 26);
    let header = snapshots.last().map(|s| header_of(s)).unwrap_or_default();
    assert!(
        header.contains("HISTORY ATLAS"),
        "after 12 steps + lens + esc, header must be clean, got: {header:?}"
    );
    // no duplicated selection marker from stale cells
    assert!(
        header.matches("◈").count() <= 2,
        "selection marker must not smear across differential frames: {header:?}"
    );
}

#[test]
fn every_step_frame_shows_the_selected_commit() {
    // The header is the identity anchor: whatever the selection is, the
    // on-screen header must carry that commit's short oid after every step.
    let mut app = app_for("medium");
    let script: Vec<(KeyCode, &str)> = (0..8).map(|_| (KeyCode::Char('j'), "step")).collect();
    let (_, snapshots) = replay_session(&mut app, &script, 100, 28);
    for (i, snap) in snapshots.iter().skip(1).enumerate() {
        let expected = app
            .hist
            .rows
            .get(i + 1)
            .map(|r| r.short.clone())
            .unwrap_or_default();
        let header = header_of(snap);
        assert!(
            header.contains(&expected),
            "after step {i} the screen must show commit {expected}, got: {header:?}"
        );
    }
}
