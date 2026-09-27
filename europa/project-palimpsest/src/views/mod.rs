//! View assembly: the responsive layout policy and the frame builder.
//!
//! Layout ladder (authored for the mission's target sizes):
//!   160x50, 120x40 — two-column atlas (canvas + dossier column)
//!   100x30        — atlas + dossier band
//!   80x24         — compact: labels off, dossier band shrinks
//!   60x20         — focused single-lens: minimap off, hint line short
//!   ~40 cols      — hostile: ruler off, single pin column, text-first

pub mod atlas;
pub mod chrome;
pub mod health;
pub mod lens;
pub mod prov;
pub mod strata;
pub mod widgets;

use gibson::node::Node;
use gibson::surface::Surface;

use crate::app::{App, InputMode, View};
use crate::theme::Palette;

pub struct Layout {
    pub header_h: u16,
    pub hint_h: u16,
    pub cmd_h: u16,
    pub main_h: u16,
    pub two_col: bool,
    pub dossier_w: u16,
    pub dossier_h: u16,
}

pub fn layout_policy(w: u16, h: u16) -> Layout {
    let header_h: u16 = 1;
    let cmd_h: u16 = 1;
    let hint_h: u16 = if h >= 20 && w >= 50 { 1 } else { 0 };
    let main_h = h.saturating_sub(header_h + cmd_h + hint_h).max(1);
    let two_col = w >= 150 && main_h >= 22;
    let dossier_w: u16 = if two_col { 46 } else { 0 };
    let dossier_h: u16 = if !two_col {
        match (h, w) {
            (0..=9, _) => 0,
            (10..=15, _) => 3,
            _ if w < 70 => 3,
            _ => 6,
        }
    } else {
        0
    };
    Layout {
        header_h,
        hint_h,
        cmd_h,
        main_h,
        two_col,
        dossier_w,
        dossier_h,
    }
}

/// Build the complete UI tree for the current frame.
pub fn build_ui(app: &mut App, pal: &Palette) -> Node {
    let w = app.width;
    let h = app.height;
    let lay = layout_policy(w, h);

    let main = build_main(app, &lay, pal);

    let mut base = Node::col()
        .percent_width(100.0)
        .percent_height(100.0)
        .child(chrome::header_node(app, w, pal))
        .child(main.flex_grow(1.0))
        .child(chrome::cmdline_node(app, w, pal));
    if lay.hint_h > 0 {
        base = base.child(chrome::hintline_node(app, w, pal));
    }

    match app.mode {
        InputMode::HelpOverlay => {
            let dim = Node::dim().percent_width(100.0).percent_height(100.0);
            let card = chrome::help_overlay(app, w, app.height.saturating_sub(2), pal);
            Node::stack()
                .percent_width(100.0)
                .percent_height(100.0)
                .child(base)
                .child(dim)
                .child(card)
        }
        InputMode::FileBrowser => {
            let dim = Node::dim().percent_width(100.0).percent_height(100.0);
            let card = chrome::browser_card(app, w, app.height, pal);
            Node::stack()
                .percent_width(100.0)
                .percent_height(100.0)
                .child(base)
                .child(dim)
                .child(card)
        }
        _ => base,
    }
}

fn build_main(app: &mut App, lay: &Layout, pal: &Palette) -> Node {
    let w = app.width;
    let main_h = lay.main_h;

    match app.view {
        View::Atlas => {
            // resolve tier-A stats for the visible window (bounded per frame)
            let win = {
                let t0 = app.camera.t_left(w).saturating_sub(86_400);
                let t1 = app.camera.t_right(w).saturating_add(86_400);
                app.hist.window(t0, t1)
            };
            app.profile.tier_a_resolved += app.hist.resolve_window_stats(&app.repo, &win);

            // two-column: the dossier takes a fixed column, the canvas the rest
            let canvas_w = if lay.two_col {
                w.saturating_sub(lay.dossier_w).max(40)
            } else {
                w
            };
            let canvas_h = main_h.saturating_sub(lay.dossier_h);
            let surface = atlas::draw_atlas(app, canvas_w, canvas_h, pal);

            if lay.two_col {
                Node::row()
                    .child(Node::surface(surface).flex_grow(1.0))
                    .child(dossier_node(app, lay.dossier_w, pal))
            } else if lay.dossier_h > 0 {
                Node::col()
                    .child(Node::surface(surface).flex_grow(1.0))
                    .child(dossier_node(app, w, pal))
            } else {
                Node::col().child(Node::surface(surface).flex_grow(1.0))
            }
        }
        View::Strata => {
            let surface = strata::draw_strata(app, w, main_h, pal);
            Node::col().child(Node::surface(surface).flex_grow(1.0))
        }
        View::Lens => {
            app.ensure_lens();
            let surface = lens::draw_lens(app, w, main_h, pal);
            Node::col().child(Node::surface(surface).flex_grow(1.0))
        }
        View::Provenance => {
            let surface = prov::draw_prov(app, w, main_h, pal);
            Node::col().child(Node::surface(surface).flex_grow(1.0))
        }
        View::Health => {
            let surface = health::draw_health(app, w, main_h, pal);
            Node::col().child(Node::surface(surface).flex_grow(1.0))
        }
    }
}

/// The dossier column/band — the editorial detail card of the selection.
fn dossier_node(app: &mut App, w: u16, pal: &Palette) -> Node {
    let rich = atlas::dossier_rich(app, pal);
    Node::panel(
        " SELECTION ",
        gibson::surface::BorderType::Single,
        pal.s_border(),
    )
    .width(w as f32)
    .child(Node::rich_text_wrapped(
        rich,
        gibson::node::WrapMode::WordWrap,
    ))
}

/// Render a plain surface dump to text lines (debug helper).
#[allow(dead_code)]
pub fn surface_to_lines(s: &Surface) -> Vec<String> {
    (0..s.height)
        .map(|y| {
            (0..s.width)
                .filter_map(|x| s.get(x, y).map(|c| c.glyph.grapheme.to_string()))
                .collect::<String>()
        })
        .collect()
}
