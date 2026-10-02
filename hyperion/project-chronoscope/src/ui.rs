//! The screen: layout, the `gibson::ui` view tree, the timeline strip and the key map.

use crate::app::*;
use crate::epoch::*;
use crate::history::*;
use crate::view3d::{self, Params};
use crate::vm::*;
use gibson::ui::prelude::*;
use gibson::{Cell, Color, ColorDepth, Event, Glyph, KeyCode, KeyModifiers, Style, Surface};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub w: u16,
    pub h: u16,
    pub view_w: u16,
    pub view_h: u16,
    pub side_w: u16,
    pub strip_h: u16,
}

pub fn layout(w: u16, h: u16) -> Layout {
    let side_w = if w >= 100 { 36 } else { 0 };
    let strip_h = if h >= 34 {
        5
    } else if h >= 22 {
        4
    } else if h >= 14 {
        2
    } else {
        1
    };
    let view_w = w.saturating_sub(side_w).max(1);
    let view_h = h.saturating_sub(2 + strip_h).max(1);
    Layout {
        w,
        h,
        view_w,
        view_h,
        side_w,
        strip_h,
    }
}

pub fn epoch_tone(e: Epoch) -> Tone {
    match e {
        Epoch::Stable => Tone::Neutral,
        Epoch::Uncertain => Tone::Info,
        Epoch::Escalating => Tone::Warning,
        Epoch::Deadlock => Tone::Accent,
        Epoch::Contradiction => Tone::Danger,
        Epoch::Resolution => Tone::Success,
        Epoch::Convergence => Tone::Accent,
        Epoch::Catastrophe => Tone::Danger,
    }
}

fn fit(s: &str, w: usize) -> String {
    if s.chars().count() <= w {
        s.to_string()
    } else if w > 1 {
        let mut out: String = s.chars().take(w - 1).collect();
        out.push('…');
        out
    } else {
        s.chars().take(w).collect()
    }
}

/// Map a terminal event to a command. `None` = not ours.
pub fn key_cmd(ev: &Event) -> Option<Cmd> {
    let Event::Key(k) = ev else { return None };
    let shift = k.modifiers.contains(KeyModifiers::SHIFT);
    Some(match k.code {
        KeyCode::Char('q') => Cmd::Quit,
        KeyCode::Char(' ') | KeyCode::Char('p') => Cmd::TogglePlay,
        KeyCode::Char('.') | KeyCode::Right | KeyCode::Char('l') if !shift => Cmd::Step(1),
        KeyCode::Char(',') | KeyCode::Left | KeyCode::Char('h') if !shift => Cmd::Step(-1),
        KeyCode::Right | KeyCode::Char('L') => Cmd::Step(8),
        KeyCode::Left | KeyCode::Char('H') => Cmd::Step(-8),
        KeyCode::PageDown | KeyCode::Char(']') => Cmd::Step(32),
        KeyCode::PageUp | KeyCode::Char('[') => Cmd::Step(-32),
        KeyCode::Home | KeyCode::Char('0') => Cmd::Jump(Jump::Start),
        KeyCode::End | KeyCode::Char('$') | KeyCode::Char('c') => Cmd::Jump(Jump::End),
        KeyCode::Char('n') => Cmd::Jump(Jump::NextLandmark),
        KeyCode::Char('N') => Cmd::Jump(Jump::PrevLandmark),
        KeyCode::Char('d') => Cmd::Jump(Jump::NextDecision),
        KeyCode::Char('D') => Cmd::Jump(Jump::PrevDecision),
        KeyCode::Char('e') => Cmd::Jump(Jump::NextEpoch),
        KeyCode::Char('E') => Cmd::Jump(Jump::PrevEpoch),
        KeyCode::Char('g') => Cmd::Jump(Jump::NextInput),
        KeyCode::Char('G') => Cmd::Jump(Jump::PrevInput),
        KeyCode::Char('f') => Cmd::OpenFork,
        KeyCode::Char('b') => Cmd::CycleBranch(1),
        KeyCode::Char('B') => Cmd::CycleBranch(-1),
        KeyCode::Char('v') => Cmd::ToggleCompare,
        KeyCode::Char('V') => Cmd::CycleCompare,
        KeyCode::Char('a') => Cmd::ToggleAb,
        KeyCode::Char('x') => Cmd::ToggleCollapse,
        KeyCode::Char('+') | KeyCode::Char('=') => Cmd::Speed(1),
        KeyCode::Char('-') | KeyCode::Char('_') => Cmd::Speed(-1),
        KeyCode::Char('t') => Cmd::Turn,
        KeyCode::Char('o') => Cmd::ToggleInside,
        KeyCode::Char('m') => Cmd::ToggleMute,
        KeyCode::Char('i') | KeyCode::Enter => Cmd::Inspect,
        KeyCode::Char('?') => Cmd::Help,
        KeyCode::Esc => Cmd::CloseModal,
        _ => return None,
    })
}

// --- timeline strip ---------------------------------------------------------------------

pub fn strip_surface(m: &mut Model, w: u16, h: u16, depth: ColorDepth) -> Surface {
    let mut s = Surface::new(w, h);
    let bg = if depth == ColorDepth::Mono {
        Style::new()
    } else {
        Style::new().bg(Color::Rgb(5, 7, 13))
    };
    for y in 0..h {
        for x in 0..w {
            s.set_cell(x, y, Cell::new(Glyph::space(), bg));
        }
    }
    let ids = m.visible_branches(h.saturating_sub(1).max(1) as usize);
    let max_end = ids
        .iter()
        .map(|b| m.hist.branch(*b).end())
        .max()
        .unwrap_or(1)
        .max(64) as f32;
    let col_of = |step: f32| -> i32 { ((step / max_end) * (w as f32 - 1.0)).round() as i32 };
    // row 0: ruler with landmarks and the cursor
    let cur_col = col_of(m.cursor as f32);
    m.hist.ensure_chain(m.cur_b);
    let lm = landmarks(&m.hist, m.cur_b);
    for l in &lm {
        let c = col_of(l.pos as f32);
        if c >= 0 && c < w as i32 {
            let ch = match l.kind {
                LandmarkKind::Fork => '◢',
                LandmarkKind::Input => '↓',
                LandmarkKind::Fate => '◆',
                LandmarkKind::Catastrophe => '✹',
                LandmarkKind::Checkpoint => '┬',
                _ => '·',
            };
            let fg = if depth == ColorDepth::Mono {
                Style::new()
            } else {
                Style::new()
                    .fg(Color::Rgb(150, 160, 180))
                    .bg(Color::Rgb(5, 7, 13))
            };
            if matches!(
                l.kind,
                LandmarkKind::Fork
                    | LandmarkKind::Input
                    | LandmarkKind::Fate
                    | LandmarkKind::Catastrophe
                    | LandmarkKind::Checkpoint
            ) {
                s.set_cell(c as u16, 0, Cell::new(Glyph::from_char(ch), fg));
            }
        }
    }
    let cur_style = if depth == ColorDepth::Mono {
        Style::new().bold().reverse()
    } else {
        Style::new()
            .fg(Color::Rgb(255, 255, 255))
            .bg(Color::Rgb(5, 7, 13))
            .bold()
    };
    if cur_col >= 0 && cur_col < w as i32 {
        s.set_cell(
            cur_col as u16,
            0,
            Cell::new(Glyph::from_char('▼'), cur_style),
        );
    }
    // branch rows
    for (row, &b) in ids.iter().enumerate() {
        let y = row as u16 + 1;
        if y >= h {
            break;
        }
        let br = m.hist.branch(b);
        let (first, end) = (br.fork_at, br.end());
        let is_cur = b == m.cur_b;
        for x in 0..w {
            let step = ((x as f32 / (w as f32 - 1.0).max(1.0)) * max_end) as u32;
            if step < first || step >= end {
                continue;
            }
            let ep = m
                .hist
                .rec_at(b, step)
                .map(|r| r.epoch)
                .unwrap_or(Epoch::Stable);
            let past = step < m.cursor && is_cur;
            let glyph = if depth == ColorDepth::Mono {
                ep.letter()
            } else if is_cur {
                if past {
                    '█'
                } else {
                    '▓'
                }
            } else {
                '░'
            };
            let st = if depth == ColorDepth::Mono {
                if is_cur {
                    Style::new().bold()
                } else {
                    Style::new().dim()
                }
            } else {
                let c = ep.rgb();
                let k = if is_cur {
                    if past {
                        1.0
                    } else {
                        0.7
                    }
                } else {
                    0.45
                };
                Style::new()
                    .fg(Color::Rgb(
                        (c.0 as f32 * k) as u8,
                        (c.1 as f32 * k) as u8,
                        (c.2 as f32 * k) as u8,
                    ))
                    .bg(Color::Rgb(5, 7, 13))
            };
            s.set_cell(x, y, Cell::new(Glyph::from_char(glyph), st));
        }
        // fork connector and label
        if br.parent.is_some() {
            let c = col_of(first as f32);
            if c >= 0 && c < w as i32 {
                let st = if depth == ColorDepth::Mono {
                    Style::new().bold()
                } else {
                    Style::new()
                        .fg(Color::Rgb(255, 240, 255))
                        .bg(Color::Rgb(5, 7, 13))
                };
                s.set_cell(c as u16, y, Cell::new(Glyph::from_char('◢'), st));
            }
        }
        let lab = br.label.to_string();
        for (i, ch) in lab.chars().enumerate() {
            let x = end.min(max_end as u32) as f32 / max_end * (w as f32 - 1.0);
            let xx = (x as i32 + 1 + i as i32).min(w as i32 - 1);
            if xx >= 0 {
                let st = if depth == ColorDepth::Mono {
                    Style::new().bold()
                } else {
                    Style::new()
                        .fg(Color::Rgb(230, 235, 245))
                        .bg(Color::Rgb(5, 7, 13))
                        .bold()
                };
                if m.hist.branch(b).terminal == Some(Terminal::Meltdown)
                    && i == 0
                    && x as i32 + 1 < w as i32
                {
                    // catastrophe mark at the end of a dead future
                }
                s.set_cell(xx as u16, y, Cell::new(Glyph::from_char(ch), st));
            }
        }
    }
    s
}

// --- the screen --------------------------------------------------------------------------------

pub struct Screen {
    pub tree: Element<Action>,
    pub segments: u32,
    pub plotted: u64,
    pub layout: Layout,
}

fn gauge_line(name: &str, v: i32, hi: i32) -> Element<Action> {
    progress(
        format!("{name} {v}"),
        (v as f32 / hi as f32).clamp(0.0, 1.0),
    )
}

/// Build the whole screen for the current model state.
pub fn build_screen(m: &mut Model, env: UiEnvironment) -> Screen {
    let lay = layout(env.width, env.height);
    let depth = env.color_depth;
    let comparing = m.compare.is_some();
    let lod = view3d::lod_for(lay.view_w, lay.view_h);
    let snap = m.snapshot(lod);
    let prm = Params {
        cw: lay.view_w,
        ch: lay.view_h,
        depth,
        glyphs: env.glyph_mode,
    };
    let frame = if comparing {
        view3d::render_compare(&snap, prm)
    } else {
        view3d::render_navigate(&snap, prm)
    };
    let (segments, plotted) = (frame.stats.segments, frame.stats.plotted);
    let epoch = m.epoch_here();
    let director = m.director();
    let forked = director.facts().bool("forked");
    let banner = crate::director::banner_for(epoch, forked, comparing);
    let (vp_node, _p) = m.atmo.compose(
        &director,
        Arc::new(frame.surface),
        &banner,
        lay.view_w,
        lay.view_h,
    );

    let br = m.hist.branch(m.cur_b).clone();
    let end = m.end();
    let w = env.width as usize;
    // --- top bar
    let play = if m.playing {
        format!("▶ {}×", SPEEDS[m.speed])
    } else {
        "❚❚".to_string()
    };
    let aud = if m.mute {
        "♪ off".to_string()
    } else {
        match m.status_of(m.audio_branch_now()) {
            "♪" => {
                if m.player.available() {
                    "♪ live".to_string()
                } else {
                    "♪ silent".to_string()
                }
            }
            "…" => "♪ composing…".to_string(),
            _ => "♪ —".to_string(),
        }
    };
    let pos = format!("{}/{}", m.cursor, end);
    let mut top = row().gap(1).height(1);
    top = top.child(heading("◷ CHRONOSCOPE"));
    top = top.child(badge(br.label.to_string()).tone(Tone::Accent));
    top = top.child(label(pos).emphasis(Emphasis::Strong));
    top = top.child(status(epoch.short()).tone(epoch_tone(epoch)).emphasis(
        if epoch == Epoch::Catastrophe {
            Emphasis::Strong
        } else {
            Emphasis::Normal
        },
    ));
    if w >= 56 {
        top = top.child(label(play));
        top = top.child(label(aud));
    }
    if let Some((a, b)) = m.compare {
        if w >= 70 {
            top = top.child(
                badge(format!(
                    "{}⇄{}",
                    m.hist.branch(a).label,
                    m.hist.branch(b).label
                ))
                .tone(Tone::Info),
            );
        }
    }

    // --- bottom bar
    let hint = if w >= 110 {
        "space run · ←→ scrub · ,. step · n/N landmark · d decision · f fork · b branch · v compare · a A/B · x collapse · i inspect · ? help · q quit"
    } else if w >= 76 {
        "␣ run ←→ scrub n landmark d decision f fork b branch v compare a A/B i inspect ? q"
    } else if w >= 50 {
        "␣ run ←→ scrub f fork b branch v cmp a A/B ? q"
    } else {
        "␣ ←→ f b v a ? q"
    };
    let bottom = label(fit(hint, w)).emphasis(Emphasis::Muted).height(1);

    // --- main column: viewport + strip
    let strip = strip_surface(m, lay.view_w, lay.strip_h, depth);
    let main = column()
        .width(lay.view_w)
        // The viewport is a keyed, focusable control with an `on_event` sink: while it has focus
        // the arrows scrub time instead of walking the focus ring (see FRICTION.md).
        .child(
            raw(vp_node)
                .width(lay.view_w)
                .height(lay.view_h)
                .key("viewport")
                .on_event(Action::Key),
        )
        .child(raster(strip).width(lay.view_w).height(lay.strip_h));

    let mut body = row().height(lay.view_h + lay.strip_h).child(main);

    // --- side panel
    if lay.side_w > 0 {
        let mut hist_panel = panel("HISTORIES").width(lay.side_w).gap(0);
        let mut ids: Vec<BranchId> = (0..m.hist.branches.len() as BranchId).collect();
        // keep the list bounded: the cursor's lineage first, then the most recent
        ids.sort_by_key(|b| {
            std::cmp::Reverse((
                *b == m.cur_b,
                m.visits
                    .iter()
                    .position(|v| v == b)
                    .map(|p| p as i32)
                    .unwrap_or(-1),
            ))
        });
        for &id in ids.iter().take(6) {
            let b = m.hist.branch(id);
            let line = fit(
                &format!("{} {} {}", b.label, m.status_of(id), m.branch_desc(id)),
                lay.side_w as usize - 6,
            );
            hist_panel = hist_panel.child(
                choice(line, id == m.cur_b)
                    .key(format!("branch.{id}"))
                    .on_press(Action::Pick(id)),
            );
        }
        if m.hist.branches.len() > 6 {
            hist_panel = hist_panel.child(
                label(format!("… {} more", m.hist.branches.len() - 6)).emphasis(Emphasis::Faint),
            );
        }
        let mut now = panel("NOW").width(lay.side_w).gap(0);
        if let Some(r) = m.rec_here() {
            let v = r.vars;
            now = now
                .child(label(fit(
                    &describe_event(&m.hist, m.cur_b, m.cursor - 1),
                    lay.side_w as usize - 4,
                )))
                .child(gauge_line("HEAT", v[V_HEAT as usize], 100))
                .child(gauge_line("LOAD", v[V_LOAD as usize], 9))
                .child(gauge_line("COOL", v[V_COOL as usize], 9))
                .child(label(format!(
                    "CREDITS {} / LEDGER {}",
                    v[V_CREDITS as usize], v[V_LEDGER as usize]
                )))
                .child(label(format!(
                    "ESC {}  MODE {}  FAULTS {}",
                    v[V_ESC as usize], v[V_MODE as usize], v[V_FAULTS as usize]
                )));
        } else {
            now = now.child(label("t = 0: nothing has executed yet"));
        }
        if let Some((a, b)) = m.compare {
            if let Some(c) = m.compare_result().cloned() {
                let row_ = c.rows.get(m.cursor.saturating_sub(1) as usize);
                if let Some(r) = row_ {
                    let (tone, txt) = match r.verdict {
                        crate::sem::Verdict::Identical => (Tone::Success, "≡ byte-identical state"),
                        crate::sem::Verdict::ComputationallyEqual => {
                            (Tone::Success, "= same computation, different provenance")
                        }
                        crate::sem::Verdict::Equivalent => {
                            (Tone::Info, "≈ semantically equivalent (bytes differ)")
                        }
                        crate::sem::Verdict::Divergent => (Tone::Danger, "≠ divergent"),
                    };
                    now = now.child(status(fit(txt, lay.side_w as usize - 6)).tone(tone));
                    let apart = r
                        .rel
                        .iter()
                        .filter(|x| **x == crate::sem::Rel::Apart)
                        .count();
                    now = now.child(label(format!(
                        "{} dims apart · first divergence {}",
                        apart,
                        c.first_divergence
                            .map(|s| s.to_string())
                            .unwrap_or("-".into())
                    )));
                }
                let _ = (a, b);
            }
        }
        body = body.child(column().width(lay.side_w).child(hist_panel).child(now));
    }

    let mut screen_el = screen().height(lay.h).child(top).child(body).child(bottom);

    // --- modals / toast
    match m.modal.clone() {
        Modal::None => {}
        Modal::Inspect => {
            let step = m.cursor.saturating_sub(1);
            let mut md = modal(format!("INSPECT · {} · step {}", br.label, step))
                .key("modal.inspect")
                .on_dismiss(Action::CloseModal);
            md = md.child(text(fit(&describe_event(&m.hist, m.cur_b, step), 60)));
            md = md.child(text(format!("epoch: {}", epoch.name())));
            if let Some(r) = m.rec_here() {
                md = md.child(text(format!(
                    "HEAT {} LOAD {} COOL {} ESC {} MODE {}",
                    r.vars[0], r.vars[1], r.vars[2], r.vars[7], r.vars[8]
                )));
                md = md.child(text(format!(
                    "tasks: {}",
                    (0..MAX_TASKS)
                        .map(|t| format!(
                            "{}={}",
                            &TASK_NAMES[t][..3],
                            ["run", "slp", "rcv", "snd", "LCK", "DEAD", "end"]
                                [r.tcode[t] as usize % 7]
                        ))
                        .collect::<Vec<_>>()
                        .join(" ")
                )));
            }
            md = md.child(label("because (causal parents):").emphasis(Emphasis::Muted));
            let anc = ancestry(&m.hist, m.cur_b, step, 6);
            for (s, d) in anc.iter().skip(1).take(5) {
                md = md.child(text(fit(
                    &format!(
                        "{}{} {}",
                        "  ".repeat(*d as usize),
                        s,
                        describe_event(&m.hist, m.cur_b, *s)
                    ),
                    62,
                )));
            }
            md = md.child(
                row()
                    .gap(2)
                    .child(
                        button("Fork here")
                            .key("inspect.fork")
                            .on_press(Action::OpenFork),
                    )
                    .child(
                        button("Close")
                            .key("inspect.close")
                            .on_press(Action::CloseModal),
                    ),
            );
            screen_el = screen_el.overlay(md);
        }
        Modal::Fork(opts) => {
            let mut md = modal(format!(
                "FORK · {} @ {} · change one thing",
                br.label, m.cursor
            ))
            .key("modal.fork")
            .on_dismiss(Action::CloseModal);
            md = md.child(text(
                "The old future stays as a ghost. Pick the single change:",
            ));
            for (i, (lab, _)) in opts.iter().enumerate() {
                md = md.child(
                    button(fit(lab, 56))
                        .key(format!("fork.{i}"))
                        .on_press(Action::Fork(i)),
                );
            }
            screen_el = screen_el.overlay(md);
        }
        Modal::Help => {
            let mut md = modal("KEYS")
                .key("modal.help")
                .on_dismiss(Action::CloseModal);
            for l in [
                "space/p run·pause   +/- speed   ←→ , . step   H L ±8   [ ] ±32   Home End c",
                "n/N landmark  d/D decision  e/E epoch  g/G input",
                "f fork here   b/B branch   v compare   V next target   a A/B audio",
                "x collapse/rejoin  i inspect  m mute  q quit",
            ] {
                md = md.child(text(fit(l, 70)));
            }
            md = md.child(
                button("Close")
                    .key("help.close")
                    .on_press(Action::CloseModal),
            );
            screen_el = screen_el.overlay(md);
        }
    }
    if let Some((t, _)) = &m.toast {
        screen_el = screen_el.overlay(toast(fit(t, w.saturating_sub(6).max(8))).key("toast"));
    }
    Screen {
        tree: screen_el,
        segments,
        plotted,
        layout: lay,
    }
}
