//! HISTORY ATLAS — the braided temporal topology canvas.
//!
//! Git history rendered as a navigable object: time flows left to right,
//! branch lineages are horizontal braids at distinct depths, merges visibly
//! join histories, tags anchor, and change magnitude gives commits visual
//! mass. The density minimap at the bottom is the multiscale scrub
//! instrument: the live camera window is bracketed inside it.

use std::collections::HashSet;
use std::sync::Arc;

use gibson::cell::{Line as RLine, RichText, Span, Style};
use gibson::surface::Surface;

use crate::app::App;
use crate::theme::{fmt_age, fmt_date, glyphs, truncate, Palette};
use crate::views::widgets::*;

/// Draw the atlas into a fresh surface of exactly `w` x `h` cells.
pub fn draw_atlas(app: &mut App, w: u16, h: u16, pal: &Palette) -> Arc<Surface> {
    let mut s = Surface::new(w, h);
    if app.hist.is_empty() {
        let msg = "empty history — no commits reachable from HEAD";
        let x = ((w as usize).saturating_sub(msg.len())) / 2;
        s.print_str(x as u16, h / 2, msg, pal.s_faint(), Some(w));
        return Arc::new(s);
    }

    let cam = app.camera;
    let compact = w < 80 || h < 16;
    let hostile = w < 44;
    let show_minimap = h >= 12 && w >= 60 && !hostile;
    let show_ruler = h >= 6 && !hostile;

    let minimap_rows = if show_minimap { 1 } else { 0 };
    let lanes_top = if show_ruler { 1 } else { 0 };
    let lanes_bottom = h.saturating_sub(minimap_rows + 1); // last row reserved for the scale line
    let lanes_h = lanes_bottom.saturating_sub(lanes_top);

    // Lane geometry: roomy (2 rows per lane) when there is vertical slack.
    let lane_h: u16 = if compact { 1 } else { 2 };
    let lanes_visible = if lanes_h >= 1 {
        (lanes_h / lane_h).max(1)
    } else {
        1
    };
    let lane_y = |lane: u16| -> i32 {
        lanes_top as i32 + (lane.min(lanes_visible.saturating_sub(1))) as i32 * lane_h as i32
    };

    let sel = app.selection;
    let hits: HashSet<u32> = app.search.hits.iter().copied().collect();

    // Time window with one day of slack for edge stubs.
    let t0 = cam.t_left(w).saturating_sub(86_400);
    let t1 = cam.t_right(w).saturating_add(86_400);
    let win = app.hist.window(t0, t1);
    app.profile.windows_drawn += 1;

    // ---- ruler ---------------------------------------------------------------
    if show_ruler {
        draw_ruler(&mut s, cam, w, pal);
    }

    // ---- selected pin (behind everything else in the lanes region) ------------
    if let Some(i) = sel {
        let x = cam.x_of(app.hist.rows[i as usize].time, w);
        if x >= 0 && (x as u16) < w {
            let pin_bottom = h.saturating_sub(minimap_rows + 1);
            for y in lanes_top..pin_bottom.max(lanes_top) {
                vline_if_empty(
                    &mut s,
                    x as u16,
                    y as i32,
                    y as i32,
                    glyphs::PIN,
                    pal.s_accent(),
                );
            }
        }
    }

    // ---- edges (braids) --------------------------------------------------------
    for &i in &win {
        let row = &app.hist.rows[i as usize];
        let cx = cam.x_of(row.time, w);
        if cx < -4 || cx > w as i32 + 4 {
            continue;
        }
        let cy = lane_y(row.lane);
        let parent_slots = [
            row.parent1.as_deref(),
            if row.parent_count > 2 {
                None
            } else {
                row.parent2.as_deref()
            },
        ];
        for (slot, parent_oid) in parent_slots.iter().enumerate() {
            let Some(poid) = parent_oid else { continue };
            let Some(pidx) = app.hist.idx_of(poid) else {
                continue;
            };
            let prow = &app.hist.rows[pidx as usize];
            let mut px = cam.x_of(prow.time, w);
            let py = lane_y(prow.lane);
            // Clip edges that leave the left edge: draw a stub to the border.
            let leaving = px < -2;
            if leaving {
                px = -1;
            }
            if cx < -2 && !leaving {
                continue;
            }
            let style = if slot == 0 {
                pal.s_lane(row.lane as usize)
            } else {
                pal.s_merge()
            };
            draw_edge(&mut s, cx, cy, px, py, style, pal, slot > 0);
        }
    }

    // ---- nodes -----------------------------------------------------------------
    let mut tip_labels = Vec::new();
    let mut tag_labels = Vec::new();
    for &i in &win {
        let row = &app.hist.rows[i as usize];
        let x = cam.x_of(row.time, w);
        if x < -1 || x > w as i32 + 1 {
            continue;
        }
        let y = lane_y(row.lane);
        let is_sel = sel == Some(i);
        let is_hit = hits.contains(&i);
        let glyph = row.mass_glyph(is_sel, is_hit);
        let style = if is_sel {
            pal.s_selection()
        } else if row.is_merge {
            pal.s_merge()
        } else if !row.tags.is_empty() {
            pal.s_tag()
        } else {
            pal.s_lane(row.lane as usize)
        };
        put(&mut s, x, y, glyph, style);

        // HEAD marker to the left of the node.
        if row.oid == app.head.oid && x > 0 {
            let head_x = x - 1;
            put_if_space(&mut s, head_x, y, glyphs::HEAD_MARK, pal.s_head());
        }

        // labels (roomy layouts and generous widths only)
        if !compact && w >= 100 {
            let tip_label = app
                .hist
                .tips
                .get(&row.oid)
                .and_then(|names| names.first())
                .map(|name| {
                    if row.oid == app.head.oid {
                        format!("┤ {} ●HEAD", name)
                    } else {
                        format!("┤ {}", name)
                    }
                });
            if let Some(label) = &tip_label {
                tip_labels.push((
                    x + 1,
                    y,
                    label.clone(),
                    pal.s_lane(row.lane as usize).bold(),
                ));
            }
            if let Some(tag) = row.tags.first() {
                let label = format!("{}{}", glyphs::TAG, tag);
                // tags sit on the node row when the row is free (no tip label),
                // otherwise on the inter-lane row below
                if tip_label.is_none() {
                    tag_labels.push((x + 1, y, label, pal.s_tag()));
                } else if lane_h == 2 {
                    tag_labels.push((x + 1, y + 1, label, pal.s_tag()));
                }
            }
        }
    }
    // Complete labels are drawn after all topology, or omitted if their row
    // is occupied. Per-character vacancy checks used to produce pierced tags.
    for (x, y, label, style) in tip_labels.into_iter().chain(tag_labels) {
        print_label_if_clear(&mut s, x, y, &label, style);
    }

    // ---- hidden-lanes notice ----------------------------------------------------
    let total_lanes = app.hist.lanes;
    if total_lanes > lanes_visible {
        let note = format!("+{} lanes below", total_lanes - lanes_visible);
        let x = w.saturating_sub(note.len() as u16 + 2);
        if lanes_top < s.height {
            print_if_empty(&mut s, x, lanes_top, &note, pal.s_faint(), None);
        }
    }

    // A time-aligned activity cross-section uses the whitespace beneath
    // shallow branch topologies. Every column is a count of visible commits,
    // so the contour changes with the same pan/zoom as the braids and ruler.
    let section_top = lanes_top + app.hist.lanes.min(lanes_visible) * lane_h + 2;
    if section_top + 7 < lanes_bottom && w >= 70 {
        draw_activity_section(&mut s, app, &win, section_top, lanes_bottom, pal);
    }

    // ---- density minimap (the scrub instrument) ---------------------------------
    if show_minimap {
        let y = h - 2;
        draw_minimap(&mut s, app, y, pal);
    }

    // ---- scale line -----------------------------------------------------------------
    let days = cam.days_visible(w);
    let scale = if days < 3.0 {
        format!("lens {:.1}d @ {}px/d", days, cam.px_per_day)
    } else {
        format!("lens {:.0}d @ {:.2}px/d", days, cam.px_per_day)
    };
    let y = h - 1;
    if y < s.height {
        let left = format!(" {} commits in view", win.len());
        let combined = format!("{} {}", left, scale);
        s.print_str(
            0,
            y,
            &truncate(&combined, w as usize),
            pal.s_faint(),
            Some(w),
        );
    }

    Arc::new(s)
}

/// Visible commit density in camera coordinates (distinct from the full-span
/// one-row minimap). Deliberately O(visible commits + viewport columns).
fn draw_activity_section(
    s: &mut Surface,
    app: &App,
    win: &[u32],
    top: u16,
    bottom: u16,
    pal: &Palette,
) {
    let w = s.width;
    let mut counts = vec![0u32; w as usize];
    for &i in win {
        let x = app.camera.x_of(app.hist.rows[i as usize].time, w);
        if (0..w as i32).contains(&x) {
            counts[x as usize] += 1;
        }
    }
    let peak = counts.iter().copied().max().unwrap_or(0).max(1);
    let chart_h = bottom.saturating_sub(top + 2).min(9);
    let title = format!(" COMMIT PRESSURE  /  viewport density · peak {peak} per column");
    s.print_str(
        0,
        top,
        &truncate(&title, w as usize),
        pal.s_muted(),
        Some(w),
    );
    let y_bottom = top + chart_h + 1;
    for (x, &count) in counts.iter().enumerate() {
        if count == 0 {
            continue;
        }
        let height = ((count as f32 / peak as f32) * chart_h as f32).ceil() as u16;
        for dy in 0..height {
            let style = if count == peak {
                pal.s_accent()
            } else {
                pal.s_muted()
            };
            s.set_cell(
                x as u16,
                y_bottom - dy,
                gibson::cell::Cell::new(
                    gibson::cell::Glyph::new(glyphs::DENSITY[(dy as usize + 2).min(7)]),
                    style,
                ),
            );
        }
    }
    if let Some(selected) = app.sel_row() {
        let x = app.camera.x_of(selected.time, w);
        if (0..w as i32).contains(&x) {
            s.set_cell(
                x as u16,
                y_bottom.saturating_sub(chart_h),
                gibson::cell::Cell::new(
                    gibson::cell::Glyph::new(glyphs::NODE_SELECTED),
                    pal.s_selection(),
                ),
            );
        }
    }
}

fn put_if_space(s: &mut Surface, x: i32, y: i32, ch: &str, style: Style) {
    if x < 0 || y < 0 {
        return;
    }
    let empty = s
        .get(x as u16, y as u16)
        .map(|c| c.glyph.grapheme.as_str() == " " || c.glyph.is_empty())
        .unwrap_or(false);
    if empty {
        s.set_cell(
            x as u16,
            y as u16,
            gibson::cell::Cell::new(gibson::cell::Glyph::new(ch), style),
        );
    }
}

fn print_label_if_clear(s: &mut Surface, x: i32, y: i32, text: &str, style: Style) {
    let len = crate::theme::width_of(text) as i32;
    if x < 0 || y < 0 || x + len > s.width as i32 || y >= s.height as i32 {
        return;
    }
    // Tip cells can have one or two departing rail segments. Seek a nearby
    // clear run without moving the label so far that its owner is ambiguous.
    for start in x..=(x + 12).min(s.width as i32 - len) {
        if (start..start + len).all(|px| {
            s.get(px as u16, y as u16)
                .is_some_and(|c| c.glyph.grapheme.as_str() == " " || c.glyph.is_empty())
        }) {
            s.print_str(start as u16, y as u16, text, style, Some(len as u16));
            break;
        }
    }
}

/// Draw one braid edge from child (cx,cy) to parent (px,py). Parent is older,
/// so normally px < cx. Jog at the midpoint when lanes differ.
#[allow(clippy::too_many_arguments)]
fn draw_edge(
    s: &mut Surface,
    cx: i32,
    cy: i32,
    px: i32,
    py: i32,
    style: Style,
    pal: &Palette,
    is_merge: bool,
) {
    let _ = pal;
    if cy == py {
        hline_if_empty(s, px, cx, cy as u16, glyphs::RAIL, style);
        return;
    }
    let xj = (px + cx) / 2;
    // horizontal segments
    hline_if_empty(
        s,
        xj,
        cx,
        cy as u16,
        if is_merge {
            glyphs::RAIL_ACTIVE
        } else {
            glyphs::RAIL
        },
        style,
    );
    hline_if_empty(s, px, xj, py as u16, glyphs::RAIL, style);
    // vertical jog
    let (top, bot) = if py > cy { (cy, py) } else { (py, cy) };
    vline_if_empty(s, xj as u16, top, bot, glyphs::VERT, style);
    // corners
    if py > cy {
        put_if_space(s, xj, cy, glyphs::JOG_N, style);
        put_if_space(s, xj, py, glyphs::LANE_TURN_LEFT, style);
    } else {
        put_if_space(s, xj, cy, glyphs::JOG_S, style);
        put_if_space(s, xj, py, glyphs::LANE_TURN_RIGHT, style);
    }
}

/// The time ruler: adaptive tick granularity, labels at every tick.
/// Public so the strata view shares the exact same axis (visual continuity).
pub fn draw_ruler_pub(s: &mut Surface, cam: crate::app::Camera, w: u16, pal: &Palette) {
    draw_ruler(s, cam, w, pal);
}

/// The time ruler: adaptive tick granularity, labels at every tick.
fn draw_ruler(s: &mut Surface, cam: crate::app::Camera, w: u16, pal: &Palette) {
    let steps_days: [i64; 10] = [1, 2, 7, 14, 30, 91, 182, 365, 730, 3650];
    let px = cam.px_per_day as f64;
    let mut step = *steps_days.last().unwrap();
    for &sd in &steps_days {
        // A tick must leave enough space for its complete date label.
        let label_cells = if sd >= 90 { 9.0 } else { 12.0 };
        if px * sd as f64 >= label_cells {
            step = sd;
            break;
        }
    }
    let step_secs = step * 86_400;
    let t_left = cam.t_left(w);
    let t_right = cam.t_right(w);
    let first = t_left.div_euclid(step_secs) * step_secs;
    let mut t = first;
    while t <= t_right {
        let x = cam.x_of(t, w);
        if x >= 0 && (x as u16) < w {
            let label = if step >= 90 {
                fmt_date(t)[..7].to_string()
            } else {
                fmt_date(t)
            };
            // A lone tick without its date suggests a missing event. Paint
            // both only when the complete label fits the viewport.
            if x + 1 + label.len() as i32 <= w as i32 {
                s.set_cell(
                    x as u16,
                    0,
                    gibson::cell::Cell::new(
                        gibson::cell::Glyph::new(glyphs::RULER_TICK),
                        pal.s_border(),
                    ),
                );
                print_if_empty(s, (x + 1) as u16, 0, &label, pal.s_muted(), None);
            }
        }
        t += step_secs;
    }
}

/// Full-span commit density with the live camera window bracketed.
fn draw_minimap(s: &mut Surface, app: &App, y: u16, pal: &Palette) {
    let w = s.width;
    let span = (app.hist.t_max - app.hist.t_min).max(1);
    let t_left = app.camera.t_left(w);
    let t_right = app.camera.t_right(w);

    // density columns; sparse bucket indices reconstructed via the persisted
    // per-bucket width
    let bucket_width = app.hist.density_width.max(1);
    let max_count = app.hist.density.iter().map(|(_, c)| *c).max().unwrap_or(1);
    for &(bucket, count) in &app.hist.density {
        let bt = app.hist.t_min + bucket * bucket_width;
        let frac = ((bt - app.hist.t_min) as f32 / span as f32).clamp(0.0, 1.0);
        let x = (frac * w as f32) as u16;
        if x >= w || y >= s.height {
            continue;
        }
        let level = ((count as f32 / max_count as f32) * 7.0).round() as usize;
        s.set_cell(
            x,
            y,
            gibson::cell::Cell::new(
                gibson::cell::Glyph::new(glyphs::DENSITY[level.min(7)]),
                pal.s_muted(),
            ),
        );
    }

    // camera window: replace inside cells with faint dots, edges with brackets
    let wl = (((t_left - app.hist.t_min) as f32 / span as f32 * w as f32) as i16)
        .clamp(0, w as i16 - 1) as u16;
    let wr = (((t_right - app.hist.t_min) as f32 / span as f32 * w as f32) as i16)
        .clamp(wl as i16, w as i16 - 1) as u16;
    for x in wl..=wr {
        if x < w && y < s.height {
            s.set_cell(
                x,
                y,
                gibson::cell::Cell::new(
                    gibson::cell::Glyph::new(glyphs::SCRUB_INSIDE),
                    pal.s_border(),
                ),
            );
        }
    }
    if wl < w {
        s.set_cell(
            wl,
            y,
            gibson::cell::Cell::new(
                gibson::cell::Glyph::new(glyphs::SCRUB_EDGE_L),
                pal.s_accent(),
            ),
        );
    }
    if wr < w {
        s.set_cell(
            wr,
            y,
            gibson::cell::Cell::new(
                gibson::cell::Glyph::new(glyphs::SCRUB_EDGE_R),
                pal.s_accent(),
            ),
        );
    }

    // selected commit marker on the minimap
    if let Some(i) = app.selection {
        let t = app.hist.rows[i as usize].time;
        let frac = ((t - app.hist.t_min) as f32 / span as f32).clamp(0.0, 1.0);
        let x = (frac * w as f32) as u16;
        if x < w && y < s.height {
            s.set_cell(
                x,
                y,
                gibson::cell::Cell::new(
                    gibson::cell::Glyph::new(glyphs::NODE_SELECTED),
                    pal.s_selection(),
                ),
            );
        }
    }
}

/// The dossier: editorial detail card of the selected commit. RichText so the
/// layout engine wraps long messages gracefully at any width.
pub fn dossier_rich(app: &mut App, pal: &Palette) -> RichText {
    let mut rich = RichText::new();
    let Some(sel) = app.selection else {
        let l = RLine::styled("no commit selected", pal.s_faint());
        rich.push_line(l);
        return rich;
    };
    let row = &app.hist.rows[sel as usize];
    let oid = row.oid.clone();
    let short = row.short.clone();
    let summary = row.summary.clone();
    let author = row.author.clone();
    let time = row.time;
    let lane = row.lane;
    let is_merge = row.is_merge;
    let is_root = row.is_root();
    let files = row
        .files
        .map(|f| f.to_string())
        .unwrap_or_else(|| "…".to_string());
    let tags = row.tags.clone();
    let has_tip = app.hist.tips.contains_key(&oid);
    let ls = app.hist.line_stat(&app.repo, &oid);
    let msg = app.dossier_message();

    // Header line: short oid + summary
    let mut head = RLine::new();
    head.push(Span::styled(short, pal.s_selection()));
    head.push(Span::styled("  ", pal.s_faint()));
    head.push(Span::styled(truncate(&summary, 72), Style::new().bold()));
    rich.push_line(head);

    // Author + date + age
    let mut meta = RLine::new();
    meta.push(Span::styled(author, pal.s_author()));
    meta.push(Span::styled(" · ", pal.s_faint()));
    meta.push(Span::styled(crate::theme::fmt_date(time), pal.s_muted()));
    meta.push(Span::styled(
        format!(" ({} ago)", fmt_age(time, app.now)),
        pal.s_faint(),
    ));
    rich.push_line(meta);

    // Stats row (tier A files; tier B +/- on demand, cached).
    let mut stat = RLine::new();
    match ls {
        Some(st) => {
            stat.push(Span::styled(
                format!("{} file{}", st.files, if st.files == 1 { "" } else { "s" }),
                pal.s_muted(),
            ));
            stat.push(Span::styled("  ", pal.s_muted()));
            stat.push(Span::styled(format!("+{}", st.adds), pal.s_added()));
            stat.push(Span::styled(" / ", pal.s_faint()));
            stat.push(Span::styled(format!("−{}", st.dels), pal.s_removed()));
        }
        None => {
            stat.push(Span::styled(format!("{} paths", files), pal.s_muted()));
        }
    }
    if is_merge {
        stat.push(Span::styled("  merge", pal.s_merge()));
    }
    if is_root {
        stat.push(Span::styled("  root", pal.s_faint()));
    }
    rich.push_line(stat);

    // Tags & lanes
    if !tags.is_empty() || has_tip {
        let mut anchors = RLine::new();
        if let Some(names) = app.hist.tips.get(&oid) {
            anchors.push(Span::styled("tips ", pal.s_faint()));
            anchors.push(Span::styled(
                names.join(" "),
                pal.s_lane(lane as usize).bold(),
            ));
        }
        if !tags.is_empty() {
            anchors.push(Span::styled("  tags ", pal.s_faint()));
            anchors.push(Span::styled(
                tags.iter()
                    .map(|t| format!("{}{}", glyphs::TAG, t))
                    .collect::<Vec<_>>()
                    .join(" "),
                pal.s_tag(),
            ));
        }
        rich.push_line(anchors);
    }

    // Full message body
    if !msg.is_empty() {
        let body = RLine::styled(msg, pal.s_text());
        rich.push_line(body);
    }

    rich
}
