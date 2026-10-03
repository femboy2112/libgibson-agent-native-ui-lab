//! Frame composition: one dominant representation (the hero), a persistent
//! identity ledger, a time scrubber and sparse chrome. Responsive by SEMANTIC
//! ZOOM: smaller terminals drop secondary meaning and keep the hero.
//!
//!   Wide   (>=100x32)  header, hero, ledger header + 5 lanes, ticker, 3-row scrubber, hints
//!   Medium (>=60x19)   header, hero, 5 lanes, 2-row scrubber, hints
//!   Tiny   (otherwise) header, hero, 1-row identity strip, 1-row scrubber, hints

use gibson::{Cell, Rect, Style, SubcellGlyphMode, Surface};

use crate::identity;
use crate::sim::{DURATION, N_EPOCHS, SLOTS};
use crate::state::{Model, SPEEDS};
use crate::theme::*;
use crate::track::{EventKind, State};
use crate::views::{self, Anchor, Ctx, View, ViewOut};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Wide,
    Medium,
    Tiny,
}

pub fn classify(w: u16, h: u16) -> Class {
    if w >= 100 && h >= 32 {
        Class::Wide
    } else if w >= 60 && h >= 19 {
        Class::Medium
    } else {
        Class::Tiny
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub class: Class,
    pub header: u16,
    pub hero: Rect,
    pub lanes_y: u16,
    pub lanes_h: u16,
    pub ticker_y: Option<u16>,
    pub scrub_y: u16,
    pub scrub_h: u16,
    pub hint_y: u16,
}

pub fn layout(w: u16, h: u16) -> Layout {
    let class = classify(w, h);
    let (lanes_h, ticker, scrub_h) = match class {
        Class::Wide => (6u16, true, 3u16),
        Class::Medium => (5, false, 2),
        Class::Tiny => (1, false, 1),
    };
    let fixed = 1 + 1 + lanes_h + ticker as u16 + scrub_h;
    let hero_h = h.saturating_sub(fixed).max(1);
    let hint_y = h.saturating_sub(1);
    let scrub_y = hint_y.saturating_sub(scrub_h);
    let ticker_y = ticker.then_some(scrub_y.saturating_sub(1));
    let lanes_y = ticker_y.unwrap_or(scrub_y).saturating_sub(lanes_h);
    Layout {
        class,
        header: 0,
        hero: Rect::new(0, 1, w, hero_h),
        lanes_y,
        lanes_h,
        ticker_y,
        scrub_y,
        scrub_h,
        hint_y,
    }
}

pub struct Frame {
    pub surface: Surface,
    pub layout: Layout,
    pub hero: ViewOut,
    /// Anchors of every identity in the *previous* view while a transport runs.
    pub from_anchors: Option<Vec<Anchor>>,
}

pub fn ctx_for<'a>(m: &'a Model, mono: bool, glyphs: SubcellGlyphMode) -> Ctx<'a> {
    Ctx {
        obs: &m.obs,
        ep: m.ep(),
        t: m.t,
        sel: m.sel,
        ref_ep: m.ref_ep(),
        zoom: m.zoom,
        nudge: m.nudge,
        cursor: m.cursor_on.then_some(m.cursor),
        mono,
        glyphs,
    }
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

pub fn compose(m: &Model, w: u16, h: u16, mono: bool, glyphs: SubcellGlyphMode) -> Frame {
    let w = w.max(1);
    let h = h.max(1);
    let lay = layout(w, h);
    let mut s = Surface::new(w, h);
    let ctx = ctx_for(m, mono, glyphs);

    draw_header(&mut s, m, &lay, w);

    // hero (with identity transport while the view is changing)
    let hero_rect = Rect::new(0, 0, lay.hero.width, lay.hero.height);
    let hero = views::render(m.view, &ctx, hero_rect);
    let mut hero_surface = hero.surface.clone();
    let mut from_anchors = None;
    if let Some(from) = m.from {
        let old = views::render(from, &ctx, hero_rect);
        let p = smoothstep(m.trans);
        let seam = (p * lay.hero.width as f32) as u16;
        for y in 0..hero_rect.height {
            for x in seam..hero_rect.width {
                if let Some(c) = old.surface.get(x, y) {
                    hero_surface.set_cell(x, y, c.clone());
                }
            }
        }
        if seam < lay.hero.width {
            for y in 0..hero_rect.height {
                set_char(
                    &mut hero_surface,
                    seam as i32,
                    y as i32,
                    '\u{2502}',
                    st(ACCENT),
                );
            }
        }
        // the identities themselves travel: old anchor -> new anchor
        let mut moving = Vec::new();
        for a in &hero.anchors {
            if let Some(b) = old.anchor(a.slot) {
                let q = p;
                let x = b.x as f32 + (a.x - b.x) as f32 * q;
                let y = b.y as f32 + (a.y - b.y) as f32 * q;
                // conduit trail
                for i in 0..10 {
                    let f = i as f32 / 10.0 * q;
                    let tx = b.x as f32 + (a.x - b.x) as f32 * f;
                    let ty = b.y as f32 + (a.y - b.y) as f32 * f;
                    let state = m.state_of(a.slot);
                    set_char(
                        &mut hero_surface,
                        tx.round() as i32,
                        ty.round() as i32,
                        '\u{00B7}',
                        identity::style(a.slot, state),
                    );
                }
                let state = m.state_of(a.slot);
                put(
                    &mut hero_surface,
                    x.round() as i32,
                    y.round() as i32,
                    &identity::tag(a.slot, state),
                    identity::style(a.slot, state),
                );
                moving.push(Anchor {
                    slot: a.slot,
                    x: x.round() as i32,
                    y: y.round() as i32,
                });
            }
        }
        from_anchors = Some(old.anchors);
    }
    blit_all(&mut s, &hero_surface, 0, lay.hero.y);

    draw_lanes(&mut s, m, &lay, w);
    if let Some(ty) = lay.ticker_y {
        draw_ticker(&mut s, m, ty, w);
    }
    draw_scrubber(&mut s, m, &lay, w);
    draw_hints(&mut s, m, &lay, w);

    if m.help {
        draw_help(&mut s, &lay);
    }
    Frame {
        surface: s,
        layout: lay,
        hero,
        from_anchors,
    }
}

// ───────────────────────────────── header ─────────────────────────────────────

fn draw_header(s: &mut Surface, m: &Model, lay: &Layout, w: u16) {
    let play = if m.playing {
        format!("\u{25B6} \u{00D7}{:.0}", SPEEDS[m.speed])
    } else {
        "\u{275A}\u{275A} paused".to_string()
    };
    let ep = m.ep();
    match lay.class {
        Class::Wide | Class::Medium => {
            let brand = if lay.class == Class::Wide {
                "PROJECT PULSAR"
            } else {
                "PULSAR"
            };
            let rights = [
                format!(
                    "t {:6.1}/{:.0} s \u{00B7} epoch {:2}/{} \u{00B7} {} \u{00B7} seed {:X}",
                    m.t,
                    DURATION,
                    ep,
                    N_EPOCHS,
                    play,
                    m.obs.seed & 0xFFFF_FFFF
                ),
                format!(
                    "t {:6.1}/{:.0} s \u{00B7} epoch {:2}/{} \u{00B7} {}",
                    m.t, DURATION, ep, N_EPOCHS, play
                ),
                format!("t {:5.1} \u{00B7} {}", m.t, play),
            ];
            let tabs_of = |full: bool| -> Vec<String> {
                View::ALL
                    .iter()
                    .map(|v| {
                        let label = if full { v.name() } else { v.short() };
                        if *v == m.view {
                            format!("[{} {}]", v.key(), label)
                        } else {
                            format!(" {} {} ", v.key(), label)
                        }
                    })
                    .collect()
            };
            // choose the richest combination that fits
            let mut chosen: Option<(Vec<String>, usize)> = None;
            'outer: for full in [lay.class == Class::Wide, false] {
                let tabs = tabs_of(full);
                let tabs_w: usize = tabs.iter().map(|t| t.chars().count() + 1).sum();
                for (ri, r) in rights.iter().enumerate() {
                    let need = 1 + brand.chars().count() + 2 + tabs_w + r.chars().count() + 2;
                    if need <= w as usize {
                        chosen = Some((tabs.clone(), ri));
                        break 'outer;
                    }
                }
            }
            let (tabs, ri) = chosen.unwrap_or_else(|| (tabs_of(false), 2));
            let mut x = 1i32;
            put(s, x, 0, brand, st_bold(INK));
            x += brand.chars().count() as i32 + 2;
            for (v, text) in View::ALL.iter().zip(&tabs) {
                let style = if *v == m.view {
                    st_bold(ACCENT)
                } else {
                    st(DIM)
                };
                put(s, x, 0, text, style);
                x += text.chars().count() as i32 + 1;
            }
            let right = &rights[ri];
            let rx = w as i32 - right.chars().count() as i32 - 1;
            if rx > x {
                put(s, rx, 0, right, st(SOFT));
            }
            if m.compare && rx > x + 12 {
                put(s, x + 1, 0, "\u{25C6} COMPARE", st_bold(WARN));
            }
        }
        Class::Tiny => {
            let text = format!(
                "[{}] t{:.0} e{} {}{}",
                m.view.short(),
                m.t,
                ep,
                if m.playing {
                    "\u{25B6}"
                } else {
                    "\u{275A}\u{275A}"
                },
                if m.compare { " CMP" } else { "" }
            );
            put_clipped(s, 1, 0, &text, st_bold(ACCENT), w as usize);
        }
    }
}

// ───────────────────────────────── lanes ───────────────────────────────────────

fn spark(profile: &[f64], width: usize) -> String {
    const BARS: [char; 8] = [
        '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}',
        '\u{2588}',
    ];
    let n = profile.len();
    if n == 0 || width == 0 {
        return String::new();
    }
    let pooled: Vec<f64> = (0..width)
        .map(|i| {
            let a = i * n / width;
            let b = ((i + 1) * n / width).max(a + 1).min(n);
            profile[a..b].iter().cloned().fold(f64::MIN, f64::max)
        })
        .collect();
    let lo = pooled.iter().cloned().fold(f64::MAX, f64::min);
    let hi = pooled.iter().cloned().fold(f64::MIN, f64::max);
    let span = (hi - lo).max(1e-9);
    pooled
        .iter()
        .map(|v| BARS[(((v - lo) / span) * 7.0).round().clamp(0.0, 7.0) as usize])
        .collect()
}

fn zbar(z: f64, width: usize) -> String {
    let filled = ((z / 16.0).clamp(0.0, 1.0) * width as f64).round() as usize;
    (0..width)
        .map(|i| if i < filled { '\u{25AE}' } else { '\u{25AF}' })
        .collect()
}

fn lane_profile(m: &Model, slot: usize) -> Vec<f64> {
    let ctx = ctx_for(m, false, SubcellGlyphMode::Braille2x4);
    views::fold::profile_for(&ctx, slot, m.ep(), 64, 0)
        .map(|(f, _)| f.mean)
        .unwrap_or_default()
}

fn draw_lanes(s: &mut Surface, m: &Model, lay: &Layout, w: u16) {
    let ep = m.ep();
    let epoch = m.obs.timeline.epoch(ep);
    let y0 = lay.lanes_y as i32;
    let active: Vec<usize> = epoch
        .map(|e| {
            (0..SLOTS)
                .filter(|i| e.slots[*i].state.is_active())
                .collect()
        })
        .unwrap_or_default();
    let sel = views::effective_selection(&ctx_for(m, false, SubcellGlyphMode::Braille2x4));

    if lay.class == Class::Tiny {
        let mut x = 1;
        if active.is_empty() {
            put(s, x, y0, "searching: noise floor only", st(DIM));
            return;
        }
        for i in active {
            let sl = epoch.map(|e| e.slots[i]).expect("epoch");
            let tag = identity::tag(i, sl.state);
            let text = if sel == Some(i) {
                format!("[{tag}]")
            } else {
                format!(" {tag} ")
            };
            put(s, x, y0, &text, identity::style(i, sl.state));
            x += text.chars().count() as i32;
        }
        if let Some(i) = sel {
            let sl = epoch.map(|e| e.slots[i]).expect("epoch");
            let z = format!(" Z{:.0}", sl.z);
            put(s, x, y0, &z, st(SOFT));
        }
        return;
    }

    let mut y = y0;
    if lay.class == Class::Wide {
        let head = "  ID   STATE       FREQUENCY     SIGNIFICANCE    FOLDED PROFILE    DELAYS \u{03C4}21 \u{03C4}31     SKY (az, el \u{00B1}\u{03C3})";
        put_clipped(s, 1, y, head, st(DIM), w as usize);
        y += 1;
    }
    if active.is_empty() {
        put(
            s,
            2,
            y,
            "no candidates yet \u{2014} integrating noise; the first lines appear as the spectrum resolves",
            st(DIM),
        );
        return;
    }
    for i in active {
        if y >= lay.lanes_y as i32 + lay.lanes_h as i32 {
            break;
        }
        let sl = epoch.map(|e| e.slots[i]).expect("epoch");
        let sty = identity::style(i, sl.state);
        let mark = if sel == Some(i) { "\u{25B8}" } else { " " };
        let prof = lane_profile(m, i);
        let mut x = 1i32;
        let seg = |s: &mut Surface, text: &str, style: Style, x: &mut i32| {
            put(s, *x, y, text, style);
            *x += text.chars().count() as i32;
        };
        seg(s, mark, st_bold(INK), &mut x);
        seg(s, &identity::tag(i, sl.state), sty, &mut x);
        x = if lay.class == Class::Wide { 7 } else { 6 };
        seg(s, &format!("{:<10}", sl.state.label()), sty, &mut x);
        if lay.class == Class::Wide {
            seg(s, &format!("{:>9.5} Hz  ", sl.f), st(SOFT), &mut x);
            seg(s, &format!("Z{:>5.1}\u{03C3} ", sl.z), st(INK), &mut x);
            seg(s, &zbar(sl.z, 8), sty, &mut x);
            x += 3;
            seg(s, &spark(&prof, 16), sty, &mut x);
            x += 3;
            let dly = match sl.delays {
                Some(d) if d.tau[0].is_finite() && d.tau[1].is_finite() => {
                    format!("{:+6.1} {:+6.1} ms", d.tau[0] * 1000.0, d.tau[1] * 1000.0)
                }
                _ => "      \u{2014}            ".to_string(),
            };
            seg(s, &format!("{dly:<16}"), st(SOFT), &mut x);
            x += 1;
            let sky = match sl.sky {
                _ if sl.state == State::RejectedCw => "local \u{00B7} common-mode".to_string(),
                Some(k) => {
                    let (az, el) = k.best();
                    let amb = if k.resolved_by_s4 && k.confidence_of_best() > 0.9 {
                        "\u{2713}"
                    } else {
                        "\u{00B1}ghost"
                    };
                    format!(
                        "{az:5.1}\u{00B0} {el:+5.1}\u{00B0} \u{00B1}{:.1}\u{00B0} {amb}",
                        k.sigma_deg
                    )
                }
                None => "\u{2014}".to_string(),
            };
            seg(s, &sky, st(SOFT), &mut x);
        } else {
            seg(s, &format!("{:>7.4}Hz ", sl.f), st(SOFT), &mut x);
            seg(s, &format!("Z{:>4.1} ", sl.z), st(INK), &mut x);
            seg(s, &zbar(sl.z, 5), sty, &mut x);
            x += 1;
            seg(s, &spark(&prof, 10), sty, &mut x);
            x += 1;
            if let Some(k) = sl.sky {
                let (az, el) = k.best();
                seg(
                    s,
                    &format!("{az:3.0}\u{00B0}{el:+3.0}\u{00B0}"),
                    st(SOFT),
                    &mut x,
                );
            }
        }
        y += 1;
    }
}

// ───────────────────────────────── ticker ──────────────────────────────────────

fn event_text(ev: &crate::track::Event) -> String {
    match ev.slot {
        Some(s) => {
            let st_ = match ev.kind {
                EventKind::Locked | EventKind::MirrorResolved => State::Locked,
                EventKind::RejectedCw => State::RejectedCw,
                EventKind::RejectedTransient => State::RejectedTransient,
                EventKind::Tracking => State::Tracking,
                _ => State::Candidate, // Detected / Withdrawn read as an unconfirmed identity
            };
            format!("{:>5.0}s {} {}", ev.t, identity::tag(s, st_), ev.describe())
        }
        None => format!("{:>5.0}s \u{2534}  {}", ev.t, ev.describe()),
    }
}

fn draw_ticker(s: &mut Surface, m: &Model, y: u16, w: u16) {
    let past: Vec<&crate::track::Event> = m
        .obs
        .timeline
        .events
        .iter()
        .filter(|e| e.t <= m.t + 1e-9)
        .collect();
    let mut x = 1i32;
    put(s, x, y as i32, "events", st(DIM));
    x += 8;
    if past.is_empty() {
        put(s, x, y as i32, "\u{2014}", st(DIM));
        return;
    }
    let take = past.len().min(3);
    for (i, ev) in past[past.len() - take..].iter().enumerate() {
        let latest = i + 1 == take;
        let text = event_text(ev);
        let slot_style = match ev.slot {
            Some(sl) => identity::style(sl, m.state_of(sl)),
            None => st(SOFT),
        };
        put(
            s,
            x,
            y as i32,
            &text,
            if latest { slot_style } else { st(DIM) },
        );
        x += text.chars().count() as i32 + 3;
        if x > w as i32 - 10 {
            break;
        }
    }
}

// ──────────────────────────────── scrubber ─────────────────────────────────────

fn t_to_x(t: f64, w: u16) -> i32 {
    let span = (w as i32 - 3).max(1) as f64;
    1 + ((t / DURATION).clamp(0.0, 1.0) * span).round() as i32
}

fn draw_scrubber(s: &mut Surface, m: &Model, lay: &Layout, w: u16) {
    let y = lay.scrub_y as i32;
    let tiny = lay.class == Class::Tiny;
    let (ev_row, bar_row) = if tiny { (None, y) } else { (Some(y), y + 1) };
    let head = t_to_x(m.t, w);
    // bar
    for x in 1..(w as i32 - 1) {
        let on = x <= head;
        put(
            s,
            x,
            bar_row,
            if on { "\u{2501}" } else { "\u{2500}" },
            st(if on { ACCENT } else { FAINT }),
        );
    }
    // S4 online tick (and its label on the event row when there is one)
    let s4 = t_to_x(crate::sim::T_S4_ON, w);
    put(s, s4, bar_row, "\u{2534}", st(SOFT));
    if let Some(r) = ev_row {
        put(s, s4 - 1, r, "S4", st(SOFT));
    }
    // reference mark
    if let Some(mk) = m.mark {
        let x = t_to_x(mk, w);
        put(s, x, bar_row, "\u{2503}", st_bold(WARN));
    }
    // identity events
    for ev in &m.obs.timeline.events {
        let Some(sl) = ev.slot else { continue };
        let x = t_to_x(ev.t, w);
        let (text, state) = match ev.kind {
            EventKind::Detected => (identity::IDENT[sl].hollow.to_string(), State::Candidate),
            EventKind::Locked => (identity::IDENT[sl].solid.to_string(), State::Locked),
            EventKind::RejectedCw | EventKind::RejectedTransient => {
                ("\u{00D7}".to_string(), State::RejectedCw)
            }
            _ => continue,
        };
        let reached = ev.t <= m.t + 1e-9;
        let sty = if reached {
            identity::style(sl, state)
        } else {
            st(FAINT)
        };
        match ev_row {
            Some(r) => put(s, x, r, &text, sty),
            None => {
                if ev.kind == EventKind::Locked {
                    put(s, x, bar_row, &text, sty)
                }
            }
        }
    }
    // playhead
    put(s, head, bar_row, "\u{25AE}", st_bold(INK));
    if lay.scrub_h >= 3 {
        let r = y + 2;
        for (t, label) in [
            (0.0, "0"),
            (64.0, "64"),
            (128.0, "128"),
            (192.0, "192"),
            (256.0, "256 s"),
        ] {
            let x = t_to_x(t, w);
            let n = label.chars().count() as i32;
            let xx = if t >= 256.0 { x - n + 1 } else { x };
            put(s, xx, r, label, st(DIM));
        }
        let now = format!("\u{25B2} t {:.1}", m.t);
        let nx = (head - 2).clamp(6, w as i32 - 24);
        if (nx - t_to_x(128.0, w)).abs() > 8
            && (nx - t_to_x(64.0, w)).abs() > 8
            && (nx - t_to_x(192.0, w)).abs() > 8
        {
            put(s, nx, r, &now, st(INK));
        }
        let _ = r;
    }
}

// ─────────────────────────────────── hints ─────────────────────────────────────

fn draw_hints(s: &mut Surface, m: &Model, lay: &Layout, w: u16) {
    let text = match lay.class {
        Class::Wide => "1-5 view  \u{2190}\u{2192} scrub  \u{21E7}\u{2190}\u{2192} 16s  \u{2191}\u{2193} pick  space play  +/- speed  m mark  c compare  z zoom  ,. detune  hjkl cursor  n/p event  r replay  ? help",
        Class::Medium => "1-5 view \u{2190}\u{2192} scrub \u{2191}\u{2193} pick spc play m/c compare z zoom ,. detune hjkl cursor ? help",
        Class::Tiny => "1-5 \u{2190}\u{2192} \u{2191}\u{2193} spc m c z ? q",
    };
    let _ = m;
    put_clipped(s, 1, lay.hint_y as i32, text, st(DIM), w as usize - 1);
}

// ──────────────────────────────────── help ─────────────────────────────────────

fn draw_help(s: &mut Surface, lay: &Layout) {
    let lines = [
        "PROJECT PULSAR \u{2014} three buried signals, five representations",
        "",
        "1..5 / Tab      TRACE  SPECTRUM  FOLD  RELATE  SKY",
        "\u{2190} \u{2192}  (\u{21E7})    scrub time by one epoch (4 s)  /  16 s",
        "home end  g     start / end of the mission",
        "\u{2191} \u{2193}            pick a candidate (focus)",
        "space  + -      play / pause   speed",
        "m  c            mark an earlier state  /  compare it with now",
        "z               zoom: all-sky \u{2192} local, spectrum, trace window",
        "\u{2024} ,  .          detune the fold period (watch the ridge shear)",
        "h j k l  x      move / hide the inspect cursor",
        "n  p            next / previous pipeline event",
        "r               replay from t = 0     ?  close help",
        "q  esc          quit (prints the replay script)",
    ];
    let hero = lay.hero;
    let bw = lines.iter().map(|l| l.chars().count()).max().unwrap_or(10) as i32 + 4;
    let bh = lines.len() as i32 + 2;
    if (hero.width as i32) < bw + 2 || (hero.height as i32) < bh + 1 {
        // too small for the box: a one-line version in the hero
        put_clipped(s, 1, hero.y as i32 + 1, "1-5 view  \u{2190}\u{2192} scrub  \u{2191}\u{2193} pick  space play  m/c compare  z zoom  q quit", st_bold(WARN), hero.width as usize - 1);
        return;
    }
    let x0 = (hero.width as i32 - bw) / 2;
    let y0 = hero.y as i32 + (hero.height as i32 - bh) / 2;
    for yy in 0..bh {
        for xx in 0..bw {
            s.set_cell(
                (x0 + xx) as u16,
                (y0 + yy) as u16,
                Cell::space(Style::new()),
            );
        }
    }
    for xx in 0..bw {
        set_char(s, x0 + xx, y0, '\u{2500}', st(ACCENT));
        set_char(s, x0 + xx, y0 + bh - 1, '\u{2500}', st(ACCENT));
    }
    for yy in 0..bh {
        set_char(s, x0, y0 + yy, '\u{2502}', st(ACCENT));
        set_char(s, x0 + bw - 1, y0 + yy, '\u{2502}', st(ACCENT));
    }
    set_char(s, x0, y0, '\u{250C}', st(ACCENT));
    set_char(s, x0 + bw - 1, y0, '\u{2510}', st(ACCENT));
    set_char(s, x0, y0 + bh - 1, '\u{2514}', st(ACCENT));
    set_char(s, x0 + bw - 1, y0 + bh - 1, '\u{2518}', st(ACCENT));
    for (i, l) in lines.iter().enumerate() {
        put(
            s,
            x0 + 2,
            y0 + 1 + i as i32,
            l,
            st(if i == 0 { INK } else { SOFT }),
        );
    }
}

/// All visible text of a frame, one string per row (trailing blanks trimmed).
pub fn frame_lines(f: &Frame) -> Vec<String> {
    f.surface.to_visible_lines()
}
