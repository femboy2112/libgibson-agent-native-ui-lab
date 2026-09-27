//! Interaction tests: keys drive the app through the same dispatch the
//! interactive loop uses, then the *rendered output* is checked. This is
//! where focus restoration, filter stability and same-key content changes
//! are proven.

mod common;

use common::*;

use gibson::input::{KeyCode, KeyEvent, KeyModifiers};
use palimpsest::app::{keys, App, InputMode, View};

fn press(app: &mut App, code: KeyCode) {
    let k = KeyEvent {
        code,
        modifiers: KeyModifiers::empty(),
    };
    assert!(keys::handle_key(app, k));
}

fn press_char(app: &mut App, c: char) {
    press(app, KeyCode::Char(c));
}

#[test]
fn atlas_selection_steps_through_time_and_follows_camera() {
    let mut app = app_for("medium");
    let first_short = app.sel_row().unwrap().short.clone();
    let t0 = app.camera.t_center;

    press_char(&mut app, 'j'); // one commit older
    let second = app.sel_row().unwrap().short.clone();
    assert_ne!(first_short, second, "selection must move");

    // camera follows (animation target)
    let anim = app.anim.as_ref().expect("camera pan animation started");
    assert_ne!(anim.to_t, t0, "camera must follow the selection");

    press(&mut app, KeyCode::Up); // k: back to newest
    assert_eq!(app.sel_row().unwrap().short, first_short);
}

#[test]
fn lane_jump_moves_between_braids() {
    let mut app = app_for("medium");
    let lane0 = app.sel_row().unwrap().lane;
    // move to a commit in the middle where several lanes are alive
    for _ in 0..40 {
        press_char(&mut app, 'j');
    }
    let mid_lane = app.sel_row().unwrap().lane;
    press_char(&mut app, 'J'); // one lane down
    let lane_after = app.sel_row().unwrap().lane;
    assert!(
        lane_after != mid_lane || mid_lane == 0,
        "lane jump must change the lane (was {mid_lane}, now {lane_after}; start {lane0})"
    );
    if lane_after > 0 {
        press_char(&mut app, 'K');
        assert_eq!(app.sel_row().unwrap().lane, mid_lane);
    }
}

#[test]
fn enter_opens_diff_lens_and_esc_returns_to_atlas() {
    let mut app = app_for("tiny");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.view, View::Lens, "Enter must zoom into the lens");
    assert!(app.lens.diff.is_some(), "diff must be loaded");

    let (_, rows) = render_frame(&mut app, 100, 30);
    let text = frame_text(&rows);
    assert!(text.contains("DIFF LENS"));

    press(&mut app, KeyCode::Esc);
    assert_eq!(app.view, View::Atlas, "Esc must back out one level");
}

#[test]
fn search_keeps_selection_stable_while_filtering() {
    let mut app = app_for("medium");
    let selected_oid = app.sel_row().unwrap().oid.clone();

    // enter search mode and type a query matching a known word
    press_char(&mut app, '/');
    assert_eq!(app.mode, InputMode::Search);
    for c in "feat".chars() {
        press_char(&mut app, c);
    }
    assert!(
        !app.search.hits.is_empty(),
        "feat must match fixture commits"
    );
    // selection survives the filter (stable keys across filtering)
    assert_eq!(
        app.sel_row().unwrap().oid,
        selected_oid,
        "selection must be keyed and survive filtering"
    );

    // Enter accepts; n cycles through hits and moves selection
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, InputMode::Normal);
    let before = app.sel_row().unwrap().oid.clone();
    press_char(&mut app, 'n');
    let after = app.sel_row().unwrap().oid.clone();
    assert_ne!(before, after, "n must jump to the next hit");

    // halo markers render for hits
    let (_, rows) = render_frame(&mut app, 120, 40);
    let text = frame_text(&rows);
    assert!(
        text.contains('◎') || text.contains('◉'),
        "search halos must render in the atlas"
    );
}

#[test]
fn search_no_hits_reports_and_keeps_selection() {
    let mut app = app_for("medium");
    let selected = app.sel_row().unwrap().oid.clone();
    press_char(&mut app, '/');
    for c in "zzzzz".chars() {
        press_char(&mut app, c);
    }
    assert!(app.search.hits.is_empty());
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.sel_row().unwrap().oid,
        selected,
        "no-hit search must not disturb selection"
    );
    assert!(app.status.contains("no hits"));
}

#[test]
fn browser_filter_restores_focus_to_same_path() {
    let mut app = app_for("medium");
    press_char(&mut app, 'f');
    assert_eq!(app.mode, InputMode::FileBrowser, "f opens the file ledger");

    // walk down the list, remember the path under the cursor
    for _ in 0..3 {
        press(&mut app, KeyCode::Down);
    }
    let key_path = app
        .browser
        .as_ref()
        .and_then(|b| {
            let list: Vec<_> = if b.filter.is_empty() {
                b.entries.iter().collect()
            } else {
                b.entries
                    .iter()
                    .filter(|e| e.path.to_lowercase().contains(&b.filter.to_lowercase()))
                    .collect()
            };
            list.get(b.cursor).map(|e| e.path.clone())
        })
        .expect("cursor path");

    // type a filter that still matches the path
    let frag: String = key_path.chars().take(6).collect();
    for c in frag.chars() {
        press_char(&mut app, c);
    }
    let browser = app.browser.as_ref().unwrap();
    let list: Vec<_> = browser
        .entries
        .iter()
        .filter(|e| {
            e.path
                .to_lowercase()
                .contains(&browser.filter.to_lowercase())
        })
        .collect();
    let restored = list.get(browser.cursor).map(|e| e.path.clone());
    assert_eq!(
        restored,
        Some(key_path.clone()),
        "focus must restore to the same path across filtering (keyed selection)"
    );

    // Esc closes and restores the ring
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, InputMode::Normal);
}

#[test]
fn lens_tab_toggles_pane_focus_and_files_scroll() {
    let mut app = app_for("medium");
    // walk to a merge commit: its diff spans many files
    let mut steps = 0;
    loop {
        app.ensure_lens();
        let n = app.lens.diff.as_ref().map(|d| d.files.len()).unwrap_or(0);
        if n > 1 || steps > 60 {
            break;
        }
        press_char(&mut app, 'j');
        app.ensure_lens();
        steps += 1;
    }
    app.view = View::Lens;
    press(&mut app, KeyCode::Tab);
    assert!(app.lens.focus_files, "Tab must focus the file ledger");
    let before = app.lens.file_cursor;
    press(&mut app, KeyCode::Down);
    assert_ne!(
        app.lens.file_cursor, before,
        "j must scroll the files (multi-file commit expected after {} steps)",
        steps
    );
    press(&mut app, KeyCode::Tab);
    assert!(!app.lens.focus_files);
}

#[test]
fn provenance_unfolds_fiber_and_jumps_to_origin() {
    let mut app = app_for("tiny");
    app.ensure_lens();
    let path = app
        .lens
        .diff
        .as_ref()
        .and_then(|d| d.files.first().map(|f| f.path().to_string()))
        .expect("lens file");
    app.ensure_prov(&path);
    app.view = View::Provenance;

    // walk to a line that predates the selection (line 1 was born in the
    // initial commit)
    press(&mut app, KeyCode::Down);
    press_char(&mut app, 'u');
    assert!(app.prov.unfolded, "u must unfold the fiber");

    let (_, rows) = render_frame(&mut app, 120, 40);
    let text = frame_text(&rows);
    assert!(text.contains("born in"), "unfolded fiber panel must render");

    // Enter jumps to the introducing commit
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.view,
        View::Lens,
        "Enter must jump into the origin's lens"
    );
}

#[test]
fn command_palette_goto_and_zoom() {
    let mut app = app_for("medium");
    press_char(&mut app, ':');
    assert_eq!(app.mode, InputMode::Command);
    for c in "zoom 4".chars() {
        press_char(&mut app, c);
    }
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, InputMode::Normal);
    let anim = app.anim.as_ref().expect("zoom animates");
    assert!((anim.to_px - palimpsest::app::ZOOM_PRESETS[4].1).abs() < 1e-3);

    // goto an abbreviated oid
    let short = app.hist.rows[10].short.clone();
    press_char(&mut app, ':');
    for c in format!("goto {}", short).chars() {
        press_char(&mut app, c);
    }
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.sel_row().unwrap().short,
        short,
        "goto must jump to the commit (status: {:?})",
        app.status
    );
}

#[test]
fn health_scroll_and_report_emission() {
    let mut app = app_for("medium");
    app.view = View::Health;
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    let scroll = app.health_scroll;
    assert_eq!(scroll, 4, "each Down scrolls two rows");
    press_char(&mut app, 'k');
    press_char(&mut app, 'k');
    assert_eq!(
        app.health_scroll,
        scroll - 4,
        "two k presses scroll back four"
    );

    press_char(&mut app, 'R');
    assert!(
        !app.scrollback_log.is_empty(),
        "R must queue the health report for scrollback"
    );
    let report = app.scrollback_log.remove(0);
    assert!(report.contains("REPOSITORY HEALTH"));
    assert!(report.contains("HOTTEST FILES"));
}

#[test]
fn quit_key_only_exits_at_top_level() {
    let mut app = app_for("tiny");
    // q in atlas quits
    let quit = {
        let k = KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::empty(),
        };
        // in Atlas view this returns false; simulate via keys::handle_key
        // but we don't want to exit the test process, so check the branch
        // indirectly: from a subview, q backs out instead
        app.view = View::Lens;
        keys::handle_key(&mut app, k)
    };
    assert!(quit, "q in a subview must not quit the process");
    assert_eq!(app.view, View::Atlas, "q backs out to the atlas");
}

#[test]
fn view_switching_by_number_keys() {
    let mut app = app_for("medium");
    press_char(&mut app, '5');
    assert_eq!(app.view, View::Health);
    press_char(&mut app, '1');
    assert_eq!(app.view, View::Atlas);
    press_char(&mut app, '3');
    assert_eq!(app.view, View::Lens);
    press_char(&mut app, '2');
    assert_eq!(app.view, View::Strata);
}

#[test]
fn strata_enter_selects_event_commit() {
    let mut app = app_for("medium");
    app.ensure_strata("src/hot/module_0.rs");
    app.view = View::Strata;
    press(&mut app, KeyCode::Down);
    let expected = app.strata.events[app.strata.cursor].oid.clone();
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.sel_row().unwrap().oid,
        expected,
        "Enter must select the event commit"
    );
}
