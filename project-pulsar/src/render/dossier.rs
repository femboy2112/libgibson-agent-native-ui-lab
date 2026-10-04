//! DOSSIER — the interpretation, in words, with the numbers that earn it.
//!
//! The only text-first view. It is deliberately a *report of the other views*: every
//! figure here is read from the same checkpoint the graphs were drawn from, and the
//! provisional/final status follows the clock, not a flag.

use super::draw::*;
use super::{azel, id_color, ViewIn, ViewOut};
use crate::analysis::{Checkpoint, EventKind, LineClass, SignalProduct, Stage, Who};
use crate::scenario::{SigId, CHUNK, FS, N_CP, STATIONS, T_END};
use crate::session::Focus;
use gibson::Style;

/// Least-squares slope of `ln snr` against `ln t` over the later part of a growth
/// curve (points with `snr ≥ 1`): 0.5 for a coherent source integrating against
/// white noise.
pub fn growth_slope(g: &[(f64, f64)]) -> Option<f64> {
    let pts: Vec<(f64, f64)> = g
        .iter()
        .filter(|(t, s)| *s >= 1.0 && *t > 0.0)
        .map(|(t, s)| (t.ln(), s.ln()))
        .collect();
    let n = pts.len();
    if n < 6 {
        return None;
    }
    let pts = &pts[n / 3..];
    let m = pts.len() as f64;
    let (sx, sy) = pts.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
    let (mx, my) = (sx / m, sy / m);
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for p in pts {
        sxy += (p.0 - mx) * (p.1 - my);
        sxx += (p.0 - mx).powi(2);
    }
    if sxx < 1e-9 {
        None
    } else {
        Some(sxy / sxx)
    }
}

/// Word-wrap `text` to `width`, continuation lines indented two further spaces.
fn wrap_into(out: &mut Vec<(String, Style)>, text: String, st: Style, width: usize) {
    if width < 12 || text.chars().count() <= width {
        out.push((text, st));
        return;
    }
    let indent: String = text.chars().take_while(|c| *c == ' ').collect();
    let cont = format!("{indent}  ");
    let mut line = indent.clone();
    let mut first = true;
    for word in text.trim_start().split(' ') {
        let need = line.chars().count() + if line.trim().is_empty() { 0 } else { 1 } + word.chars().count();
        if need > width && !line.trim().is_empty() {
            out.push((std::mem::take(&mut line), st));
            line = cont.clone();
            first = false;
        }
        if !line.trim().is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    let _ = first;
    if !line.trim().is_empty() {
        out.push((line, st));
    }
}

fn ts(k: Option<usize>) -> String {
    k.map(|c| format!("{:.0}s", (c * CHUNK) as f64 / FS))
        .unwrap_or_else(|| "—".into())
}

/// The structured interpretation of one identity (also used by the tests).
pub fn identity_lines(cp: &Checkpoint, id: SigId, width: usize) -> Vec<(String, Style)> {
    let p = cp.signal(id);
    let col = id.color();
    let mut v: Vec<(String, Style)> = Vec::new();
    v.push((
        format!(
            "{} {} — {} · {}",
            id.glyph(),
            id.name(),
            id.kind(),
            p.stage.label()
        ),
        style_b(col, true, false),
    ));
    if p.model.is_none() || p.stage == Stage::Unseen {
        v.push((
            "    nothing adopted yet: the search statistic has not left the noise".to_string(),
            style(MUTED),
        ));
        return v;
    }
    let m = p.model.unwrap();
    let per = if p.f_now > 0.0 { 1000.0 / p.f_now } else { f64::NAN };
    let l1 = format!(
        "    f = {:.5} Hz ± {:.1e}   P = {:.2} ms",
        p.f_now, p.f_sigma, per
    );
    v.push((l1, style(INK)));
    if id == SigId::Beta {
        v.push((
            format!(
                "    drift ḟ = {:+.2e} Hz/s ({:+.2} Hz over the whole run)",
                m.fdot,
                m.fdot * T_END
            ),
            style(INK),
        ));
    }
    v.push((
        format!(
            "    detection: log10 FAP {:.1} (≈{:.1}σ after trials) · confidence {:.0}%",
            p.log10fap,
            p.sigma_eq,
            p.conf * 100.0
        ),
        style(INK),
    ));
    match &p.sky {
        Some(sk) if sk.physical && sk.sig_major < 0.3 => {
            let (az, el) = azel(sk.lm);
            v.push((
                format!(
                    "    position: az {az:.0}° el {el:.0}°  1σ {:.3}×{:.3} ({:.1}°)",
                    sk.sig_major,
                    sk.sig_minor,
                    sk.sig_major.to_degrees()
                ),
                style(INK),
            ));
        }
        Some(sk) => {
            let lever = std::f64::consts::TAU * p.f_now * STATIONS[1].0 * 1e-3;
            v.push((
                format!(
                    "    position: unconstrained (1σ {:.2}): the {:.0} ms baseline spans only {:.2} rad of its cycle",
                    sk.sig_major, STATIONS[1].0, lever
                ),
                style(WARN),
            ));
        }
        None => v.push(("    position: unknown".to_string(), style(MUTED))),
    }
    let slope = growth_slope(&p.growth)
        .map(|s| format!("{s:.2}"))
        .unwrap_or_else(|| "—".into());
    v.push((
        format!(
            "    timeline: candidate {} · lock {} · fix {} · resolved {}",
            ts(p.cand_cp),
            ts(p.lock_cp),
            ts(p.loc_cp),
            ts(p.res_cp),
        ),
        style(MUTED),
    ));
    v.push((
        format!("    growth exponent {slope} (coherent integration ⇒ 0.5)"),
        style(MUTED),
    ));
    let mut out = Vec::new();
    for (t, s) in v {
        wrap_into(&mut out, t, s, width);
    }
    out
}

pub fn draw(vin: &ViewIn) -> ViewOut {
    let mut out = ViewOut::new(vin.rect);
    let r = vin.rect;
    if r.width < 12 || r.height < 3 {
        return out;
    }
    let cp = vin.cp;
    let st = vin.st;
    let final_state = cp.k >= N_CP;
    let w = r.width as usize;
    let mut lines: Vec<(String, Style)> = Vec::new();
    lines.push((
        if final_state {
            "FINAL INTERPRETATION — observation complete".to_string()
        } else {
            format!(
                "PROVISIONAL INTERPRETATION — {:.0} s of {:.0} s analysed",
                cp.t, T_END
            )
        },
        style_b(if final_state { GOOD } else { WARN }, true, false),
    ));
    lines.push((String::new(), style(INK)));

    if r.width >= 60 && r.height >= 12 {
        let ids: Vec<SigId> = if st.focus == Focus::Signal {
            vec![st.selected]
        } else {
            SigId::ALL.to_vec()
        };
        for id in ids {
            for (t, s) in identity_lines(cp, id, w.saturating_sub(1)) {
                lines.push((t, s));
            }
            lines.push((String::new(), style(INK)));
        }
        lines.push(("INTERFERENCE & MISLEADING CANDIDATES".to_string(), style_b(WARN, true, false)));
        let mut any = false;
        for l in cp.interference() {
            any = true;
            let (g, c, why) = match l.class {
                LineClass::Terrestrial => (
                    "✕",
                    WARN,
                    format!(
                        "terrestrial: amplitude differs between stations ({:.1}σ){}",
                        l.amp_z,
                        match &l.sky {
                            Some(sk) if !sk.physical => "; delays fit no far-field direction",
                            _ => "",
                        }
                    ),
                ),
                LineClass::Transient => {
                    let peak = l.growth.iter().map(|g| g.1).fold(0.0, f64::max);
                    let now = l.growth.last().map(|g| g.1).unwrap_or(0.0);
                    (
                        "~",
                        (200, 170, 120),
                        format!(
                            "transient: S/N grew to {peak:.1} then fell to {now:.1} — not a persistent source"
                        ),
                    )
                }
                LineClass::Unassigned => (
                    "?",
                    MUTED,
                    format!(
                        "unassigned, S/N {:.1} and still growing: watch it",
                        l.growth.last().map(|g| g.1).unwrap_or(0.0)
                    ),
                ),
            };
            wrap_into(&mut lines, format!("  {g} {:>6.2} Hz  {why}", l.f), style(c), w);
        }
        if !any {
            lines.push(("  none detected".to_string(), style(MUTED)));
        }
        for e in cp.events.iter().filter(|e| e.kind == EventKind::Revised) {
            if let Who::Sig(id) = e.who {
                wrap_into(
                    &mut lines,
                    format!(
                        "  {} {} candidate was revised at {:.0} s: its first frequency was built from interference",
                        id.glyph(),
                        id.name(),
                        e.t()
                    ),
                    style(MUTED),
                    w,
                );
            }
        }
        lines.push((String::new(), style(INK)));
        let n = |f: &dyn Fn(Stage) -> bool| {
            SigId::ALL.iter().filter(|i| f(cp.signal(**i).stage)).count()
        };
        // Localized/Resolved are fixed on the sky; Locked is phase-coherent only.
        wrap_into(
            &mut lines,
            format!(
                "VERDICT  {} of 3 sources fixed on the sky, {} locked in phase only, {} still candidate; {} interferer(s) rejected",
                n(&|s| s >= Stage::Localized),
                n(&|s| s == Stage::Locked),
                n(&|s| s == Stage::Candidate),
                cp.interference().iter().filter(|l| l.class != LineClass::Unassigned).count()
            ),
            style_b(INK, true, false),
            w,
        );
    } else {
        // compact: one or two lines per identity
        for id in SigId::ALL {
            let p = cp.signal(id);
            let pos = match &p.sky {
                Some(sk) if sk.physical && sk.sig_major < 0.3 => {
                    let (az, el) = azel(sk.lm);
                    format!("{az:.0}°/{el:.0}°")
                }
                _ => "pos ?".to_string(),
            };
            lines.push((
                truncate(
                    &format!(
                        "{}{} {} {} {:.0}%",
                        id.glyph(),
                        id.letter(),
                        p.stage.short(),
                        pos,
                        p.conf * 100.0
                    ),
                    w,
                ),
                style_b(id_color(id, id == st.selected, st.focus), true, false),
            ));
        }
        for l in cp.lines.iter().take(2) {
            let g = match l.class {
                LineClass::Terrestrial => "✕",
                LineClass::Transient => "~",
                LineClass::Unassigned => "?",
            };
            lines.push((truncate(&format!("{g}{:.1}Hz {}", l.f, l.class.label().to_ascii_lowercase()), w), style(WARN)));
        }
    }
    for (i, (t, s)) in lines.iter().enumerate() {
        if i as u16 >= r.height {
            break;
        }
        put(&mut out.surf, 1, i as i32, t, *s);
    }
    let _ = SignalProduct::clone;
    out
}
