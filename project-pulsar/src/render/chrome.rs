//! Sparse chrome: status line, identity rail / strip, timeline, hint & readout.
//!
//! The information lives in the views; this is the least annotation that lets a
//! human drive them — and the one place identity is spelled out redundantly
//! (glyph + hue + slot + name), so it survives Mono.

use super::draw::*;
use super::{azel, Env, Layout, Probe};
use crate::analysis::{Checkpoint, EventKind, SignalProduct, Stage, Who};
use crate::scenario::{SigId, CHUNK, FS, N_TOTAL, T_END};
use crate::session::{Focus, Model, State, SPEEDS};
use gibson::{Rect, Style, Surface};

fn stage_color(stage: Stage) -> Rgb {
    match stage {
        Stage::Unseen => MUTED,
        Stage::Candidate => (240, 200, 110),
        Stage::Locked => (130, 220, 255),
        Stage::Localized => GOOD,
        Stage::Resolved => (190, 255, 190),
    }
}

pub fn fmt_hz(f: f64) -> String {
    if !f.is_finite() {
        "  —  ".to_string()
    } else if f >= 100.0 {
        format!("{f:.2}")
    } else {
        format!("{f:.4}")
    }
}

pub fn fmt_val(v: f64) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    let a = v.abs();
    if a == 0.0 {
        "0".into()
    } else if !(0.01..10_000.0).contains(&a) {
        format!("{v:.3e}")
    } else if a >= 100.0 {
        format!("{v:.1}")
    } else if a >= 1.0 {
        format!("{v:.3}")
    } else {
        format!("{v:.4}")
    }
}

pub fn status_row(model: &Model, cp: &Checkpoint, env: &Env) -> Surface {
    let st = &model.st;
    let w = env.width;
    let mut s = Surface::new(w.max(1), 1);
    let play = if st.playing {
        format!("▶ ×{}", SPEEDS[st.speed as usize])
    } else {
        format!("❚❚ ×{}", SPEEDS[st.speed as usize])
    };
    let focus = match st.focus {
        Focus::Overview => "overview".to_string(),
        Focus::Signal => format!("{} z{}", st.selected.name().to_ascii_lowercase(), st.zoom),
    };
    let t = model.t();
    let lag = (cp.n as f64) / FS;
    let mut x: i32 = 1;
    let mut seg = |s: &mut Surface, text: &str, st: Style| {
        put(s, x, 0, text, st);
        x += text.chars().count() as i32;
    };
    seg(&mut s, "PULSAR", style_b((150, 200, 255), true, false));
    seg(&mut s, " ┃ ", style(FAINT));
    seg(&mut s, st.view.name(), style_b(INK, true, false));
    if w >= 60 {
        seg(&mut s, &format!(" · {}", focus), style(MUTED));
    }
    seg(&mut s, " ┃ ", style(FAINT));
    if w >= 78 {
        seg(&mut s, &format!("t {t:6.1}s"), style_b(INK, false, false));
        seg(&mut s, &format!(" (analysis {lag:.0}s)"), style(MUTED));
    } else {
        seg(&mut s, &format!("t {t:.1}s"), style_b(INK, false, false));
    }
    seg(&mut s, " ┃ ", style(FAINT));
    seg(
        &mut s,
        &play,
        style_b(if st.playing { GOOD } else { MUTED }, st.playing, false),
    );
    if st.compare {
        seg(&mut s, " ┃ ", style(FAINT));
        seg(&mut s, "COMPARE", style_b(WARN, true, false));
    }
    if st.truth {
        seg(&mut s, " ┃ ", style(FAINT));
        seg(&mut s, "TRUTH", style_b(WARN, false, false));
    }
    if w >= 78 {
        seg(&mut s, " ┃ ", style(FAINT));
        seg(&mut s, &format!("seed {}", st.seed), style(FAINT));
    }
    // identity chips, right-aligned when there is room
    if w >= 100 {
        let mut chips: Vec<(String, Style)> = Vec::new();
        for id in SigId::ALL {
            let p = cp.signal(id);
            chips.push((
                format!("{}{} {}  ", id.glyph(), id.name(), p.stage.short()),
                style_b(id.color(), id == st.selected, false),
            ));
        }
        let total: i32 = chips.iter().map(|c| c.0.chars().count() as i32).sum();
        let mut cx = w as i32 - total;
        if cx > x + 1 {
            for (txt, stl) in chips {
                put(&mut s, cx, 0, &txt, stl);
                cx += txt.chars().count() as i32;
            }
        }
    }
    s
}

pub fn hint_row(model: &Model, env: &Env, readout: Option<&str>) -> Surface {
    let w = env.width;
    let mut s = Surface::new(w.max(1), 1);
    let st = &model.st;
    if let Some(buf) = &st.goto {
        let txt = format!(" goto t = {buf}▏  seconds 0–{T_END:.0}  ·  Enter go  ·  Esc cancel");
        put_max(&mut s, 0, 0, &txt, style_b(WARN, true, false), w);
        return s;
    }
    if let Some(r) = readout {
        put_max(&mut s, 0, 0, &format!(" ┼ {r}"), style_b(INK, false, false), w);
        return s;
    }
    let hint = if w >= 130 {
        " 1-6 view  ←→ ±8s  ,. ±1s  PgUp/Dn ±60s  n/N event  g goto  Tab/a b c signal  f focus  +/- zoom  p pin  m compare  i inspect(hjkl)  t truth  Space play [ ] speed  ? help  q quit"
    } else if w >= 90 {
        " 1-6 view  ←→ time  n/N event  g goto  Tab signal  f focus  +/- zoom  m compare  i inspect  Space play  ? help  q quit"
    } else if w >= 60 {
        " 1-6 view  ←→ time  Tab signal  f focus  m compare  i inspect  ? help  q quit"
    } else {
        " 1-6 view ←→ time Tab sig m cmp ? q"
    };
    put_max(&mut s, 0, 0, hint, style(MUTED), w);
    s
}

fn pos_text(p: &SignalProduct) -> String {
    match &p.sky {
        Some(sk) if sk.physical => {
            let (az, el) = azel(sk.lm);
            format!("az {az:3.0}° el {el:2.0}° ±{:.2}", sk.sig_major)
        }
        Some(sk) => format!("no far-field fix (σ {:.2})", sk.sig_major),
        None => "position unknown".to_string(),
    }
}

/// The identity rail (wide) or strip (narrow).
pub fn draw_identity(hero: &mut Surface, lay: &Layout, cp: &Checkpoint, st: &State, env: &Env) {
    let mode = env.glyphs;
    if lay.strip {
        // one compact row: glyph+letter, stage, 3-cell confidence meter
        let mut x = 1i32;
        for id in SigId::ALL {
            let p = cp.signal(id);
            let sel = id == st.selected;
            let col = id.color();
            let head = format!("{}{}", id.glyph(), id.letter());
            put(hero, x, 0, &head, style_b(col, true, !sel && st.focus == Focus::Signal));
            x += 2;
            let stage = format!(" {} ", p.stage.short());
            put(hero, x, 0, &stage, style_b(stage_color(p.stage), sel, false));
            x += stage.chars().count() as i32;
            put(hero, x, 0, &meter(p.conf, 3, mode), style_b(col, false, !sel));
            x += 4;
        }
        return;
    }
    if lay.rail_w == 0 {
        return;
    }
    let rw = lay.rail_w as i32;
    let h = lay.hero_h.saturating_sub(1) as i32; // rows above the timeline
    let compact = h < 18;
    let card_h = if compact { 3 } else { 5 };
    let mut y = 0i32;
    for id in SigId::ALL {
        let p = cp.signal(id);
        let sel = id == st.selected;
        let col = id.color();
        let marker = if sel { "▶" } else { " " };
        put(hero, 0, y, marker, style_b(col, true, false));
        put(hero, 1, y, id.glyph(), style_b(col, true, false));
        put(hero, 3, y, id.name(), style_b(col, sel, !sel && st.focus == Focus::Signal));
        put_right(
            hero,
            rw,
            y,
            p.stage.label(),
            style_b(stage_color(p.stage), sel, false),
        );
        // frequency line
        let fline = if p.f_now.is_finite() && p.model.is_some() {
            let unit = if id == SigId::Beta {
                match p.model {
                    Some(m) => format!(" Hz ḟ{:+.1e}", m.fdot),
                    None => " Hz".into(),
                }
            } else {
                " Hz".into()
            };
            let sig = if p.f_sigma.is_finite() {
                format!(" ±{:.1e}", p.f_sigma)
            } else {
                String::new()
            };
            let base = format!("{}{}", fmt_hz(p.f_now), unit);
            // the ± only when it fits whole: a clipped error bar is a lie
            if !compact && base.chars().count() + sig.chars().count() <= (rw - 3) as usize {
                format!("{base}{sig}")
            } else {
                base
            }
        } else {
            "no estimate".to_string()
        };
        put_max(hero, 3, y + 1, &fline, style(if sel { INK } else { MUTED }), (rw - 3) as u16);
        // confidence meter
        let mw = (rw - 9).max(4) as usize;
        put(hero, 3, y + 2, &meter(p.conf, mw, mode), style_b(col, false, !sel));
        put_right(
            hero,
            rw,
            y + 2,
            &format!("{:3.0}%", p.conf * 100.0),
            style(if sel { INK } else { MUTED }),
        );
        if !compact {
            put_max(hero, 3, y + 3, &pos_text(p), style(if sel { INK } else { MUTED }), (rw - 3) as u16);
        }
        y += card_h;
    }
    // interference
    if y + 2 < h {
        put(hero, 1, y, "INTERFERENCE", style_b(WARN, true, false));
        y += 1;
        let mut shown = 0;
        for l in cp.interference().into_iter().take(3) {
            if y >= h {
                break;
            }
            let (glyph, col) = match l.class {
                crate::analysis::LineClass::Terrestrial => ("✕", WARN),
                crate::analysis::LineClass::Transient => ("~", (200, 170, 120)),
                crate::analysis::LineClass::Unassigned => ("?", MUTED),
            };
            let short = match l.class {
                crate::analysis::LineClass::Terrestrial => "RFI",
                crate::analysis::LineClass::Transient => "transient",
                crate::analysis::LineClass::Unassigned => "unassigned",
            };
            let txt = format!("{glyph} {:>5.2} Hz {}", l.f, short);
            put_max(hero, 1, y, &txt, style(col), (rw - 1) as u16);
            y += 1;
            shown += 1;
        }
        if shown == 0 && y < h {
            put(hero, 1, y, "none flagged", style(FAINT));
            y += 1;
        }
        y += 1;
    }
    // event log (newest at the bottom)
    if y + 2 < h {
        put(hero, 1, y, "EVENTS", style_b(MUTED, true, false));
        y += 1;
        let room = (h - y).max(0) as usize;
        let evs: Vec<_> = cp.events.iter().collect();
        let start = evs.len().saturating_sub(room);
        for e in &evs[start..] {
            let (glyph, col, name) = match e.who {
                Who::Sig(id) => (id.glyph().to_string(), id.color(), id.name().to_string()),
                Who::Line(f) => (
                    if e.kind == EventKind::FlagTerrestrial { "✕".into() } else { "~".into() },
                    WARN,
                    format!("{f:.1}Hz"),
                ),
            };
            let txt = format!("{:>3.0}s {} {} {}", e.t(), glyph, name, e.kind.label());
            put_max(hero, 1, y, &txt, style(col), (rw - 1) as u16);
            y += 1;
        }
    }
}

/// The scrubber: progress, event glyphs, pin and the playhead on one row.
pub fn draw_timeline(hero: &mut Surface, lay: &Layout, model: &Model, cp: &Checkpoint, env: &Env) {
    let st = &model.st;
    let y = lay.timeline_row as i32;
    let w = lay.width as i32;
    let x0 = 6i32;
    let x1 = w - 7;
    if x1 <= x0 + 8 {
        return;
    }
    let span = (x1 - x0) as f64;
    let xt = |t: f64| x0 + ((t / T_END).clamp(0.0, 1.0) * span).round() as i32;
    let now_x = xt(model.t());
    put(hero, 1, y, "0s", style(MUTED));
    put_right(hero, w - 1, y, &format!("{:.0}s", T_END), style(MUTED));
    let (bar_a, bar_b) = if matches!(env.glyphs, gibson::SubcellGlyphMode::Ascii) {
        ('=', '-')
    } else {
        ('━', '─')
    };
    for x in x0..=x1 {
        let (ch, c) = if x <= now_x { (bar_a, (110, 150, 200)) } else { (bar_b, FAINT) };
        put(hero, x, y, &ch.to_string(), style(c));
    }
    for e in cp.events.iter() {
        let x = xt(e.t());
        let (g, col) = match (e.kind, e.who) {
            (EventKind::FlagTerrestrial, _) => ("✕".to_string(), WARN),
            (EventKind::FlagTransient, _) => ("~".to_string(), (200, 170, 120)),
            (EventKind::Candidate, Who::Sig(id)) | (EventKind::Revised, Who::Sig(id)) => {
                ("·".to_string(), id.color())
            }
            (_, Who::Sig(id)) => (id.glyph().to_string(), id.color()),
            _ => continue,
        };
        put(hero, x, y, &g, style_b(col, true, false));
    }
    if let Some(p) = st.pin {
        put(hero, xt(p as f64 / FS), y, "◇", style_b(WARN, true, false));
    }
    put(hero, now_x, y, "┃", style_b(INK, true, false));
    // the analysis horizon: data have arrived, products lag by < one chunk
    let _ = (CHUNK, N_TOTAL);
}

/// Human description of what lies under `cell`, from the probes drawn this frame.
pub fn readout_at(probes: &[Probe], cell: (u16, u16), st: &State) -> Option<String> {
    for p in probes {
        match p {
            Probe::Plot(pp) => {
                if let Some((x, y)) = pp.data_at(cell) {
                    let xl = axis_text(&pp.spec.x);
                    let yl = axis_text(&pp.spec.y);
                    return Some(format!(
                        "{}  {xl} = {}   {yl} = {}",
                        pp.name,
                        fmt_val(x),
                        fmt_val(y)
                    ));
                }
            }
            Probe::Raster(r) => {
                if let Some((x, y, z)) = r.at(cell) {
                    return Some(format!(
                        "{}  {} = {}   {} = {}   {} = {}",
                        r.name,
                        r.x_label,
                        fmt_val(x),
                        r.y_label,
                        fmt_val(y),
                        r.z_label,
                        fmt_val(z)
                    ));
                }
            }
            Probe::Sky(sk) => {
                if let Some((l, m)) = sk.lm_at(cell) {
                    let (az, el) = azel((l, m));
                    let near = sk
                        .marks
                        .iter()
                        .map(|(n, p, s)| {
                            let d = ((p.0 - l).powi(2) + (p.1 - m).powi(2)).sqrt();
                            (n, d, *s)
                        })
                        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                    let mut s = if l * l + m * m <= 1.0 {
                        format!("sky  l = {l:+.3}  m = {m:+.3}   az {az:.1}°  el {el:.1}°")
                    } else {
                        format!("sky  l = {l:+.3}  m = {m:+.3}   (below the horizon)")
                    };
                    if let Some((n, d, sg)) = near {
                        let z = if sg > 0.0 { d / sg } else { f64::NAN };
                        s.push_str(&format!("   nearest {n}: {d:.3} ({z:.1}σ)"));
                    }
                    return Some(s);
                }
            }
        }
    }
    let _ = st;
    None
}

fn axis_text(a: &gibson::plot::AxisSpec) -> String {
    match &a.unit {
        Some(u) if !u.is_empty() => format!("{} [{}]", a.label, u),
        _ => a.label.clone(),
    }
}

#[allow(unused)]
fn _rect(_: Rect) {}
