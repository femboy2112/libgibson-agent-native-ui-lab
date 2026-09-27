//! Key routing: the interaction model. Every key is dispatched here, per
//! input mode, with stable behavior across filtering and view changes.

use gibson::input::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::App;
use crate::app::{InputMode, View};
use crate::git::filelog::EntryKind;

/// Returns false when the application should exit.
pub fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return false;
    }

    match app.mode {
        InputMode::HelpOverlay => {
            app.close_overlay();
            true
        }
        InputMode::Search => handle_search_key(app, key),
        InputMode::LensSearch => handle_lens_search_key(app, key),
        InputMode::ZoomPreset => {
            app.mode = InputMode::Normal;
            if let KeyCode::Char(c @ '0'..='5') = key.code {
                let n = (c as u8 - b'0') as usize;
                app.zoom_preset(n);
                app.set_status(format!(
                    "lens preset {} — {}",
                    n,
                    crate::app::ZOOM_PRESETS[n].0
                ));
            } else if key.code != KeyCode::Esc {
                app.set_error("zoom preset: press z followed by 0–5");
            }
            app.mark();
            true
        }
        InputMode::Command => handle_command_key(app, key),
        InputMode::FileBrowser => handle_browser_key(app, key),
        InputMode::Normal => handle_normal_key(app, key),
    }
}

fn handle_search_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.search.clear();
            app.mode = InputMode::Normal;
            app.set_status("search cleared");
            app.mark();
        }
        KeyCode::Enter => {
            app.mode = InputMode::Normal;
            if app.search.hits.is_empty() {
                app.set_status(format!("no hits for `{}`", app.search.query));
            } else {
                // jump to the first hit, keep halos
                let first = app.search.hits[0];
                app.select(first);
                app.set_status(format!(
                    "{} hits for `{}` — n/N cycles",
                    app.search.hits.len(),
                    app.search.query
                ));
            }
            app.mark();
        }
        KeyCode::Backspace => {
            app.search.pop(&app.hist);
            app.mark();
        }
        KeyCode::Char(c) => {
            app.search.push(c, &app.hist);
            app.mark();
        }
        _ => {}
    }
    true
}

/// Search the loaded diff. History search is deliberately a separate mode:
/// its hits refer to commits, while these hits retain a file and hunk anchor.
fn handle_lens_search_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.lens.query.clear();
            app.lens.matches.clear();
            app.mode = InputMode::Normal;
            app.set_status("diff search cleared");
        }
        KeyCode::Enter => {
            app.mode = InputMode::Normal;
            if app.lens.matches.is_empty() {
                app.set_status(format!("no diff hits for `{}`", app.lens.query));
            } else {
                app.lens.match_cursor = 0;
                jump_lens_match(app);
                app.set_status(format!(
                    "{} diff hits for `{}` — m/M cycles",
                    app.lens.matches.len(),
                    app.lens.query
                ));
            }
        }
        KeyCode::Backspace => {
            app.lens.query.pop();
            refresh_lens_matches(app);
        }
        KeyCode::Char(c) => {
            app.lens.query.push(c);
            refresh_lens_matches(app);
        }
        _ => {}
    }
    app.mark();
    true
}

fn refresh_lens_matches(app: &mut App) {
    app.lens.matches = app
        .lens
        .diff
        .as_ref()
        .map(|d| crate::app::lens::find_matches(d, &app.lens.query, 200))
        .unwrap_or_default();
    app.lens.match_cursor = 0;
}

fn jump_lens_match(app: &mut App) {
    if let Some(&(file, hunk)) = app.lens.matches.get(app.lens.match_cursor) {
        focus_lens_location(
            app,
            file,
            if hunk == usize::MAX { None } else { Some(hunk) },
        );
    }
}

fn cycle_lens_match(app: &mut App, forward: bool) {
    let count = app.lens.matches.len();
    if count == 0 {
        app.set_status("no diff hits — press / to search this diff");
        return;
    }
    app.lens.match_cursor = if forward {
        (app.lens.match_cursor + 1) % count
    } else {
        (app.lens.match_cursor + count - 1) % count
    };
    jump_lens_match(app);
    app.set_status(format!("diff hit {}/{}", app.lens.match_cursor + 1, count));
    app.mark();
}

/// Keep the file and its hunk at the same position in the flattened source
/// stream, including when the file ledger is hidden by a narrow viewport.
fn focus_lens_location(app: &mut App, file: usize, hunk: Option<usize>) {
    let Some(diff) = app.lens.diff.as_ref() else {
        return;
    };
    let Some(f) = diff.files.get(file) else {
        return;
    };
    let hi = hunk.unwrap_or(0).min(f.hunks.len().saturating_sub(1));
    let target = crate::app::lens::flatten(diff, 4096)
        .iter()
        .position(|line| {
            line.file_idx == file
                && line.line_idx.is_none()
                && (hunk.is_none() || (line.hunk_idx == hi && line.text.starts_with("  @@")))
        })
        .unwrap_or(0);
    app.lens.file_cursor = file;
    app.lens.hunk_cursor = hi;
    app.lens.hunk_scroll = target.saturating_sub(1);
    let main_h = crate::views::layout_policy(app.width, app.height)
        .main_h
        .saturating_sub(1) as usize;
    if file < app.lens.file_scroll {
        app.lens.file_scroll = file;
    } else if file >= app.lens.file_scroll + main_h.max(1) {
        app.lens.file_scroll = file + 1 - main_h.max(1);
    }
    app.lens.focus_files = false;
    app.mark();
}

fn handle_command_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = InputMode::Normal;
            app.status.clear();
            app.mark();
        }
        KeyCode::Backspace => {
            app.status.pop();
            app.mark();
        }
        KeyCode::Enter => {
            let cmd = app.status.trim().to_string();
            app.mode = InputMode::Normal;
            app.status.clear();
            run_command(app, &cmd);
        }
        KeyCode::Char(c) => {
            app.status.push(c);
            app.mark();
        }
        _ => {}
    }
    true
}

fn handle_browser_key(app: &mut App, key: KeyEvent) -> bool {
    let Some(b) = &app.browser else {
        app.close_overlay();
        return true;
    };
    // The cursor indexes the filtered list, not the unfiltered tree. Preserve
    // the visible path before editing the filter, even after prior filtering.
    let key_path = key_path_of(b);

    match key.code {
        KeyCode::Esc => app.close_overlay(),
        // In a filter overlay every printable character (including j/k)
        // belongs to the query; navigation is arrows-only.
        KeyCode::Down => {
            if let Some(b) = &mut app.browser {
                let len = filtered_len(b);
                b.cursor = (b.cursor + 1).min(len.saturating_sub(1));
            }
            app.mark();
        }
        KeyCode::Up => {
            if let Some(b) = &mut app.browser {
                b.cursor = b.cursor.saturating_sub(1);
            }
            app.mark();
        }
        KeyCode::Backspace => {
            if let Some(b) = &mut app.browser {
                b.filter.pop();
                restore_cursor(&mut app.browser, &key_path);
            }
            app.mark();
        }
        KeyCode::Char(c) => {
            if let Some(b) = &mut app.browser {
                b.filter.push(c);
                restore_cursor(&mut app.browser, &key_path);
            }
            app.mark();
        }
        KeyCode::Enter => {
            let entry = app
                .browser
                .as_ref()
                .and_then(|b| filtered(b).into_iter().nth(b.cursor))
                .map(|e| (e.path.clone(), e.kind));
            match entry {
                Some((path, EntryKind::File)) => {
                    app.close_overlay();
                    app.ensure_strata(&path);
                    app.view = View::Strata;
                    app.set_status(format!("strata: {}", path));
                    app.mark();
                }
                Some((prefix, EntryKind::Dir)) => {
                    // drill into a directory by filtering on its prefix
                    if let Some(b) = &mut app.browser {
                        b.filter = prefix;
                    }
                    restore_cursor(&mut app.browser, &key_path);
                    app.mark();
                }
                None => {}
            }
        }
        _ => {}
    }
    true
}

fn key_path_of(b: &crate::app::BrowserState) -> Option<String> {
    filtered(b).get(b.cursor).map(|e| e.path.clone())
}

fn filtered(b: &crate::app::BrowserState) -> Vec<&crate::git::filelog::TreeEntry> {
    if b.filter.is_empty() {
        b.entries.iter().collect()
    } else {
        let n = b.filter.to_lowercase();
        b.entries
            .iter()
            .filter(|e| e.path.to_lowercase().contains(&n))
            .collect()
    }
}

fn filtered_len(b: &crate::app::BrowserState) -> usize {
    filtered(b).len()
}

/// Focus restoration: after a filter change, put the cursor back on the same
/// path if it survived; otherwise clamp to the list end.
fn restore_cursor(browser: &mut Option<crate::app::BrowserState>, key_path: &Option<String>) {
    let Some(b) = browser else { return };
    let list = filtered(b);
    if let Some(p) = key_path {
        if let Some(i) = list.iter().position(|e| e.path == *p) {
            b.cursor = i;
            return;
        }
    }
    b.cursor = b.cursor.min(list.len().saturating_sub(1));
}

fn handle_normal_key(app: &mut App, key: KeyEvent) -> bool {
    // view switching: '1'..'5'
    if let KeyCode::Char(d) = key.code {
        if let Some(n) = d.to_digit(10).filter(|n| (1..=5).contains(n)) {
            if let Some(v) = crate::app::View::from_num(n as u8) {
                return switch_view(app, v);
            }
        }
    }
    if key.code == KeyCode::Tab {
        return tab_focus(app);
    }

    match key.code {
        KeyCode::Char('?') => {
            app.ring.capture();
            app.mode = InputMode::HelpOverlay;
            app.mark();
            return true;
        }
        KeyCode::Char(':') => {
            app.mode = InputMode::Command;
            app.status.clear();
            app.mark();
            return true;
        }
        KeyCode::Char('/') => {
            if app.view == View::Lens {
                app.mode = InputMode::LensSearch;
                app.lens.query.clear();
                app.lens.matches.clear();
                app.lens.match_cursor = 0;
            } else {
                app.mode = InputMode::Search;
                app.search.clear();
                app.set_status("");
            }
            app.mark();
            return true;
        }
        KeyCode::Char('n') if app.view == View::Atlas => return cycle_hit(app, true),
        KeyCode::Char('N') if app.view == View::Atlas => return cycle_hit(app, false),
        KeyCode::Char('g') => {
            if let Some(first) = app.hist.rows.first().map(|_| 0u32) {
                app.select(first);
            }
            return true;
        }
        KeyCode::Char('G') => {
            if let Some(last) = app.hist.len().checked_sub(1).map(|i| i as u32) {
                app.select(last);
            }
            return true;
        }
        KeyCode::Char('z') if app.view == View::Atlas || app.view == View::Strata => {
            app.mode = InputMode::ZoomPreset;
            app.mark();
            return true;
        }
        _ => {}
    }

    match app.view {
        View::Atlas => atlas_key(app, key),
        View::Strata => strata_key(app, key),
        View::Lens => lens_key(app, key),
        View::Provenance => prov_key(app, key),
        View::Health => health_key(app, key),
    }
}

fn switch_view(app: &mut App, v: View) -> bool {
    if app.view == v {
        return true;
    }
    if v == View::Lens {
        app.ensure_lens();
    }
    if v == View::Provenance && app.prov.blame.is_none() {
        app.set_status("provenance needs a file — open the lens (3), focus a file, press p");
    }
    if v == View::Strata && app.strata.events.is_empty() && app.strata.path.is_empty() {
        app.set_status("strata needs a file — press f to browse files at the selection");
    }
    if v == View::Health {
        app.ensure_metrics();
    }
    app.view = v;
    app.mark();
    true
}

fn tab_focus(app: &mut App) -> bool {
    if app.view == View::Lens {
        app.lens.focus_files = !app.lens.focus_files;
        app.set_status(if app.lens.focus_files {
            "focus: file ledger"
        } else {
            "focus: hunk stream"
        });
        app.mark();
    }
    true
}

fn cycle_hit(app: &mut App, forward: bool) -> bool {
    if let Some(row) = app.search.cycle(forward) {
        app.select(row);
        app.set_status(format!(
            "hit {}/{} — {}",
            app.search.hit_cursor + 1,
            app.search.hits.len(),
            app.hist.rows[row as usize].summary
        ));
    } else {
        app.set_status("no search hits — press / to search");
    }
    true
}

fn atlas_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Right => {
            app.step_selection(1);
            true
        }
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Left => {
            app.step_selection(-1);
            true
        }
        KeyCode::Char('J') => lane_step(app, 1),
        KeyCode::Char('K') => lane_step(app, -1),
        KeyCode::Char(',') => {
            app.pan(-0.5);
            true
        }
        KeyCode::Char('.') => {
            app.pan(0.5);
            true
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            app.zoom(1.6);
            true
        }
        KeyCode::Char('-') => {
            app.zoom(1.0 / 1.6);
            true
        }
        KeyCode::Enter => {
            app.ensure_lens();
            app.view = View::Lens;
            app.mark();
            true
        }
        KeyCode::Char('f') => {
            if let Some(sel) = app.selection {
                app.open_browser(sel);
                app.set_status("file ledger — type to filter, Enter to open strata");
            }
            true
        }
        KeyCode::Char('c') => {
            emit_dossier(app);
            true
        }
        KeyCode::Char('r') => {
            app.center_on_selection();
            app.set_status("camera recentered on selection");
            app.mark();
            true
        }
        KeyCode::Char('q') => false,
        _ => true,
    }
}

fn lane_step(app: &mut App, dir: i32) -> bool {
    let Some(cur) = app.selection else {
        return true;
    };
    let cur_lane = app.hist.rows[cur as usize].lane as i32;
    let target = (cur_lane + dir).max(0) as u16;
    // walk away from the selection in time (older first, then newer)
    let n = app.hist.len() as u32;
    let mut found = None;
    for step in 1..n {
        if let Some(i) = cur.checked_sub(step) {
            if app.hist.rows[i as usize].lane == target {
                found = Some(i);
                break;
            }
        }
        if let Some(i) = cur.checked_add(step) {
            if (i as usize) < app.hist.len() && app.hist.rows[i as usize].lane == target {
                found = Some(i);
                break;
            }
        }
    }
    match found {
        Some(i) => {
            app.select(i);
            true
        }
        None => {
            app.set_status(format!(
                "no lane {} from here",
                if dir > 0 { "below" } else { "above" }
            ));
            true
        }
    }
}

fn strata_key(app: &mut App, key: KeyEvent) -> bool {
    let n = app.strata.events.len();
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            app.strata.cursor = (app.strata.cursor + 1).min(n.saturating_sub(1));
            follow_scroll_strata(app);
            app.mark();
            true
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.strata.cursor = app.strata.cursor.saturating_sub(1);
            follow_scroll_strata(app);
            app.mark();
            true
        }
        KeyCode::Enter => {
            let hit = app
                .strata
                .events
                .get(app.strata.cursor)
                .map(|e| (e.oid.clone(), e.summary.clone()));
            if let Some((oid, summary)) = hit {
                if app.select_oid(&oid) {
                    app.ensure_lens();
                    app.view = View::Lens;
                    app.set_status(format!("selected {} — {}", &oid[..7], summary));
                } else {
                    app.set_error("commit is outside the indexed window");
                }
            }
            true
        }
        KeyCode::Char('r') => {
            if let Some(e) = app.strata.events.get(app.strata.cursor) {
                app.camera.t_center = e.time;
                app.mark();
            }
            true
        }
        KeyCode::Char('f') => {
            if let Some(sel) = app.selection {
                app.open_browser(sel);
            }
            true
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.view = View::Atlas;
            app.mark();
            true
        }
        _ => true,
    }
}

fn follow_scroll_strata(app: &mut App) {
    // keep the cursor row inside a nominal 12-row window of the ledger
    let half = 6usize;
    if app.strata.cursor < app.strata.scroll {
        app.strata.scroll = app.strata.cursor;
    } else if app.strata.cursor >= app.strata.scroll + half * 2 {
        app.strata.scroll = app.strata.cursor.saturating_sub(half * 2) + 1;
    }
    if let Some(e) = app.strata.events.get(app.strata.cursor) {
        let cam = app.camera;
        let t = e.time;
        if t < cam.t_left(app.width) || t > cam.t_right(app.width) {
            app.animate_camera_to(t, cam.px_per_day);
        }
    }
}

fn lens_key(app: &mut App, key: KeyEvent) -> bool {
    let Some(diff) = app.lens.diff.clone() else {
        return esc_back(app, key);
    };
    let files = diff.files.len();
    match key.code {
        KeyCode::Tab => tab_focus(app),
        KeyCode::Down | KeyCode::Char('j') => {
            if app.lens.focus_files {
                let next = (app.lens.file_cursor + 1).min(files.saturating_sub(1));
                focus_lens_location(app, next, None);
                app.lens.focus_files = true;
            } else {
                app.lens.hunk_scroll = app.lens.hunk_scroll.saturating_add(3);
            }
            app.mark();
            true
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.lens.focus_files {
                let next = app.lens.file_cursor.saturating_sub(1);
                focus_lens_location(app, next, None);
                app.lens.focus_files = true;
            } else {
                app.lens.hunk_scroll = app.lens.hunk_scroll.saturating_sub(3);
            }
            app.mark();
            true
        }
        KeyCode::Char('n') => {
            // next hunk of the focused file
            if let Some(f) = diff.files.get(app.lens.file_cursor) {
                let hunks = f.hunks.len();
                let next = (app.lens.hunk_cursor + 1).min(hunks.saturating_sub(1));
                let file = app.lens.file_cursor;
                focus_lens_location(app, file, Some(next));
            }
            true
        }
        KeyCode::Char('N') => {
            let file = app.lens.file_cursor;
            focus_lens_location(app, file, Some(app.lens.hunk_cursor.saturating_sub(1)));
            true
        }
        KeyCode::Char('m') => {
            cycle_lens_match(app, true);
            true
        }
        KeyCode::Char('M') => {
            cycle_lens_match(app, false);
            true
        }
        KeyCode::Char('p') => {
            // provenance of the focused file
            if let Some(f) = diff.files.get(app.lens.file_cursor) {
                let path = f.path().to_string();
                app.ensure_prov(&path);
                app.view = View::Provenance;
                app.set_status(format!("provenance: {}", path));
                app.mark();
            }
            true
        }
        KeyCode::Char('s') => {
            if let Some(f) = diff.files.get(app.lens.file_cursor) {
                let path = f.path().to_string();
                app.ensure_strata(&path);
                app.view = View::Strata;
                app.set_status(format!("strata: {}", path));
                app.mark();
            }
            true
        }
        KeyCode::Char('E') => {
            emit_diff(app);
            true
        }
        KeyCode::Char('[') => {
            app.step_selection(-1);
            app.ensure_lens();
            true
        }
        KeyCode::Char(']') => {
            app.step_selection(1);
            app.ensure_lens();
            true
        }
        KeyCode::Enter => {
            if app.lens.focus_files {
                app.lens.focus_files = false;
                let file = app.lens.file_cursor;
                focus_lens_location(app, file, Some(0));
                app.mark();
            }
            true
        }
        _ => esc_back(app, key),
    }
}

fn prov_key(app: &mut App, key: KeyEvent) -> bool {
    let Some(b) = app.prov.blame.clone() else {
        return esc_back(app, key);
    };
    let n = b.lines.len().saturating_sub(1);
    let body_h = (app.height.saturating_sub(6)).max(4) as usize;
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            app.prov.line_cursor = (app.prov.line_cursor + 1).min(n);
            app.prov.unfolded = false;
            follow_scroll_prov(app, body_h);
            app.mark();
            true
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.prov.line_cursor = app.prov.line_cursor.saturating_sub(1);
            app.prov.unfolded = false;
            follow_scroll_prov(app, body_h);
            app.mark();
            true
        }
        KeyCode::Char('u') => {
            if app.prov.unfolded {
                app.prov.unfolded = false;
            } else {
                crate::views::prov::unfold_fiber(app);
            }
            app.mark();
            true
        }
        KeyCode::Enter => {
            // jump to the introducing commit — the line unfolds backward
            if let Some(l) = b.lines.get(app.prov.line_cursor) {
                let oid = l.oid.clone();
                if app.select_oid(&oid) {
                    app.set_status(format!("fiber origin {} — {}", &oid[..7], l.author));
                    app.ensure_lens();
                    app.view = View::Lens;
                } else {
                    app.set_error("origin commit is outside the indexed window");
                }
            }
            app.mark();
            true
        }
        KeyCode::Char('f') => {
            // cycle through fiber stations
            if app.prov.unfolded && !app.prov.fiber.is_empty() {
                let oid = app.prov.fiber[app.prov.fiber_cursor].oid.clone();
                app.prov.fiber_cursor = (app.prov.fiber_cursor + 1) % app.prov.fiber.len();
                if app.select_oid(&oid) {
                    app.ensure_lens();
                    app.view = View::Lens;
                }
            } else {
                app.set_status("unfold first (u) to cycle fiber stations");
            }
            true
        }
        KeyCode::Char('c') => {
            let text = crate::views::prov::blame_summary_text(app);
            if !text.is_empty() {
                app.note_scrollback(text);
            }
            true
        }
        _ => esc_back(app, key),
    }
}

fn follow_scroll_prov(app: &mut App, body_h: usize) {
    if app.prov.line_cursor < app.prov.line_scroll {
        app.prov.line_scroll = app.prov.line_cursor;
    } else if app.prov.line_cursor >= app.prov.line_scroll + body_h {
        app.prov.line_scroll = app.prov.line_cursor + 1 - body_h;
    }
}

fn health_key(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => {
            app.health_scroll = app.health_scroll.saturating_add(2);
            app.mark();
            true
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.health_scroll = app.health_scroll.saturating_sub(2);
            app.mark();
            true
        }
        KeyCode::Char('R') => {
            app.ensure_metrics();
            let text = crate::views::health::report_text(app);
            app.note_scrollback(text);
            app.set_status("report committed to scrollback");
            true
        }
        _ => esc_back(app, key),
    }
}

fn esc_back(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.view = View::Atlas;
            app.mark();
            true
        }
        KeyCode::Char('Q') => false,
        _ => true,
    }
}

// ---- command palette ---------------------------------------------------------

fn run_command(app: &mut App, cmd: &str) {
    let (head, rest) = match cmd.split_once(' ') {
        Some((h, r)) => (h, r.trim()),
        None => (cmd, ""),
    };
    match head {
        "goto" if !rest.is_empty() => match app.repo.resolve(rest) {
            Ok(oid) => {
                if app.select_oid(&oid) {
                    app.set_status(format!("goto {}", &oid[..7]));
                } else {
                    app.set_error("commit resolved but outside the indexed window");
                }
            }
            Err(e) => app.set_error(e),
        },
        "author" if !rest.is_empty() => {
            app.search.query = rest.to_string();
            app.search.recompute(&app.hist);
            let n = app.search.hits.len();
            app.set_status(format!("author `{}`: {} hits — n/N cycles", rest, n));
            app.mark();
        }
        "file" if !rest.is_empty() => {
            if let Some(sel) = app.selection {
                app.open_browser(sel);
                if let Some(b) = &mut app.browser {
                    b.filter = rest.to_string();
                }
                app.mark();
            }
        }
        "zoom" => {
            let n: usize = rest.parse().unwrap_or(2);
            app.zoom_preset(n);
            app.set_status(format!("lens preset z{}", n));
        }
        "export" => {
            app.ensure_lens();
            emit_diff(app);
        }
        "report" => {
            app.ensure_metrics();
            let text = crate::views::health::report_text(app);
            app.note_scrollback(text);
        }
        "help" => {
            app.ring.capture();
            app.mode = InputMode::HelpOverlay;
            app.mark();
        }
        "quit" | "exit" => std::process::exit(0),
        "" => {}
        _ => app.set_error(format!("unknown command `{}` — try help", head)),
    }
}

// ---- scrollback emissions ------------------------------------------------------

/// The dossier block committed to native scrollback.
pub fn emit_dossier(app: &mut App) {
    let Some(sel) = app.selection else { return };
    let (oid, short, summary, author, email, time, lane, is_merge, tags) = {
        let r = &app.hist.rows[sel as usize];
        (
            r.oid.clone(),
            r.short.clone(),
            r.summary.clone(),
            r.author.clone(),
            r.email.clone(),
            r.time,
            r.lane,
            r.is_merge,
            r.tags.clone(),
        )
    };
    let stat = app.hist.line_stat(&app.repo, &oid);
    let msg = app.dossier_message();
    let tip_names = app.hist.tips.get(&oid).cloned();
    let mut out = String::new();
    out.push_str(&format!("◈ {}  {}\n", short, summary));
    out.push_str(&format!(
        "  {} <{}>  {}  lane {}{}\n",
        author,
        email,
        crate::theme::fmt_date(time),
        lane,
        if is_merge { "  (merge)" } else { "" }
    ));
    match stat {
        Some(s) => out.push_str(&format!("  {} files  +{} −{}\n", s.files, s.adds, s.dels)),
        None => out.push_str("  stats not resolved yet\n"),
    }
    if !tags.is_empty() {
        out.push_str(&format!("  tags: {}\n", tags.join(" ")));
    }
    if let Some(names) = tip_names {
        out.push_str(&format!("  branch tips: {}\n", names.join(" ")));
    }
    if !msg.is_empty() {
        out.push_str(&format!("  {}\n", msg));
    }
    out.push_str(&format!("  oid {}\n", oid));
    app.note_scrollback(out);
    app.set_status("dossier committed to scrollback");
}

/// The full diff export committed to native scrollback.
pub fn emit_diff(app: &mut App) {
    let Some(sel) = app.selection else { return };
    let oid = app.hist.rows[sel as usize].oid.clone();
    match crate::git::diff::export_diff_text(&app.repo, &oid, 400) {
        Ok(text) => {
            let Some(diff) = &app.lens.diff else {
                app.set_error("no diff loaded");
                return;
            };
            let head = format!(
                "◈ DIFF {}  {}  +{} −{}\n",
                &oid[..7],
                diff.files.len(),
                diff.total_adds,
                diff.total_dels
            );
            app.note_scrollback(head + &text);
            app.set_status("diff committed to scrollback");
        }
        Err(e) => app.set_error(e),
    }
}
