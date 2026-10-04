//! Structural tests: what is drawn is what the analysis computed; the same input and
//! view give the same geometry; identity is carried across representations; and
//! degenerate inputs are handled honestly.

mod support;
use pulsar::analysis::{Engine, Stage};
use pulsar::app::render_frame;
use pulsar::render::{compose, draw, Env, Frame, Probe};
use pulsar::scenario::{SigId, CHUNK, FS, N_CP, N_TOTAL};
use pulsar::session::{secs_to_samples, Cmd, Focus, Model, View};
use support::*;

// ---------------------------------------------------------------------------
// 1. graph data == the application's observables
// ---------------------------------------------------------------------------

#[test]
fn spectrum_plot_series_is_exactly_the_whitened_spectrum() {
    let m = model_at(SEED, 300.0, View::Spectrum);
    let f = frame(&m, 120, 40);
    let cp = m.cp();
    let sp = cp.spec.as_ref().expect("spectrum exists at 300 s");
    let p = plot(&f, "spectrum");
    let base = &p.spec.series[0];
    assert_eq!(base.label, "spectrum");
    // Overview focus: every bin 1..nbins-1 in order, value for value.
    assert_eq!(base.points.len(), sp.nbins() - 1);
    for (i, &(fr, w)) in base.points.iter().enumerate() {
        let j = i + 1;
        assert_eq!(fr, j as f64 * sp.df, "frequency of bin {j}");
        assert_eq!(w, sp.w[j] as f64, "power of bin {j}");
    }
    // The receipt agrees: all seen, none lost to non-finite values, reducer applied.
    assert_eq!(p.report.reducers_requested, p.spec.series.iter().filter(|s| s.reduce != gibson::plot::Reduce::None).count());
    assert_eq!(p.report.reducers_declined, 0, "a spectrum is monotone in frequency");
    assert!(p.report.reduced_to < p.report.reduced_from, "16k bins squeezed into ~200 columns");
}

#[test]
fn growth_and_walk_plots_carry_the_analysis_arrays_verbatim() {
    let m = model_at(SEED, 300.0, View::Relation);
    let f = frame(&m, 120, 40);
    let cp = m.cp();
    let growth = plot(&f, "relation.growth");
    let walk = plot(&f, "relation.walk");
    for id in SigId::ALL {
        let prod = cp.signal(id);
        let g = growth.spec.series.iter().find(|s| s.label == id.name()).expect("growth series");
        assert_eq!(g.points, prod.growth, "{} growth", id.name());
        assert_eq!(g.color, id.color());
        let w = walk.spec.series.iter().find(|s| s.label == id.name()).expect("walk series");
        assert_eq!(w.points, prod.walk, "{} walk", id.name());
        assert_eq!(w.color, id.color());
    }
}

#[test]
fn fold_plot_is_the_folded_profile_with_its_error_band() {
    let mut m = model_at(SEED, 300.0, View::Fold);
    m.apply(Cmd::Select(SigId::Alpha));
    m.apply(Cmd::ToggleFocus); // two cycles for the selected identity
    let f = frame(&m, 120, 40);
    let p = plot(&f, "fold.A");
    let prod = m.cp().signal(SigId::Alpha).clone();
    let prof = p.spec.series.iter().find(|s| s.label == "profile").expect("profile");
    let nb = prod.profile.len();
    assert_eq!(prof.points.len(), 2 * nb, "two cycles");
    for c in 0..2 {
        for b in 0..nb {
            let (x, y) = prof.points[c * nb + b];
            assert!((x - (c as f64 + (b as f64 + 0.5) / nb as f64)).abs() < 1e-12);
            assert_eq!(y, prod.profile[b] as f64);
        }
    }
    let hi = p.spec.series.iter().find(|s| s.label == "+1σ").unwrap();
    for (i, &(_, y)) in hi.points.iter().enumerate() {
        assert!((y - (prod.profile[i % nb] as f64 + prod.profile_err as f64)).abs() < 1e-9);
    }
}

#[test]
fn stream_plots_carry_raw_samples_and_the_demodulated_phasor() {
    let m = model_at(SEED, 300.0, View::Stream);
    let f = frame(&m, 120, 40);
    let raw = plot(&f, "stream.raw");
    let s = &raw.spec.series[0];
    let x = m.engine.rx.station(0);
    let n = m.st.n;
    let lo = (((n as f64 / FS) - 24.0) * FS).floor() as usize;
    assert_eq!(s.points.len(), n - lo);
    for (i, &(t, v)) in s.points.iter().enumerate() {
        assert_eq!(t, (lo + i) as f64 / FS);
        assert_eq!(v, x[lo + i] as f64);
    }
    // gap-free, finite data: nothing may be lost
    assert_eq!(raw.report.nonfinite_rejected, 0);
    assert_eq!(raw.report.reducers_declined, 0);
    let iq = plot(&f, "stream.iq.A");
    let want = pulsar::render::stream::iq_series(m.cp().signal(SigId::Alpha));
    let got = iq.spec.series.iter().find(|s| s.label == "I").unwrap();
    assert_eq!(got.points.len(), want.len());
    for (g, w) in got.points.iter().zip(&want) {
        assert_eq!(*g, (w.0, w.1));
    }
}

#[test]
fn every_receipt_obeys_the_library_conservation_laws() {
    for view in View::ALL {
        for (w, h) in [(120, 40), (80, 24), (42, 15)] {
            let m = model_at(SEED, 300.0, view);
            let f = frame(&m, w, h);
            for p in plots(&f) {
                let r = &p.report;
                let total: usize = p.spec.series.iter().map(|s| s.points.len()).sum();
                assert_eq!(r.samples_seen, total, "{} {view:?} {w}x{h}", p.name);
                assert_eq!(r.finite_samples + r.nonfinite_rejected, r.samples_seen);
                assert_eq!(
                    r.segments_considered,
                    r.segments_emitted + r.segments_clipped,
                    "line conservation, {} {view:?} {w}x{h}",
                    p.name
                );
                let nan: usize = p
                    .spec
                    .series
                    .iter()
                    .flat_map(|s| s.points.iter())
                    .filter(|(x, y)| !x.is_finite() || !y.is_finite())
                    .count();
                assert_eq!(r.nonfinite_rejected, nan, "gaps are counted, not eaten");
            }
        }
    }
}

#[test]
fn a_one_bin_line_survives_the_extrema_reducer_in_the_drawn_frame() {
    // The brightest spectral bin must still be drawn after ~16k bins are reduced to
    // ~200 columns (law J, exercised through the application, not just the library).
    let m = model_at(SEED, 300.0, View::Spectrum);
    let f = frame(&m, 120, 40);
    let p = plot(&f, "spectrum");
    let (fpk, wpk) = p.spec.series[0]
        .points
        .iter()
        .cloned()
        .filter(|(f, _)| *f < 16.0)
        .fold((0.0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
    let cell = p.cell_of(fpk, wpk).expect("the peak is inside the view");
    let ch = f.hero.get(cell.0, cell.1).unwrap().glyph.grapheme.chars().next().unwrap();
    assert!(('\u{2800}'..='\u{28FF}').contains(&ch) && ch != '\u{2800}', "peak cell shows ink, got {ch:?}");
}

// ---------------------------------------------------------------------------
// 2. determinism of state, geometry and capability naturality
// ---------------------------------------------------------------------------

fn probe_fingerprint(f: &Frame) -> String {
    let mut s = String::new();
    for p in &f.probes {
        match p {
            Probe::Plot(pp) => {
                s.push_str(&format!("{}|{:?}|{:?}\n", pp.name, pp.layout, pp.report));
            }
            Probe::Raster(r) => s.push_str(&format!("{}|{:?}|{:?}\n", r.name, r.origin, r.z)),
            Probe::Sky(k) => s.push_str(&format!("{k:?}\n")),
        }
    }
    s
}

#[test]
fn same_seed_time_and_view_give_identical_geometry_and_text() {
    for view in View::ALL {
        let a = frame(&model_at(SEED, 211.5, view), 100, 30);
        let b = frame(&model_at(SEED, 211.5, view), 100, 30);
        assert_eq!(probe_fingerprint(&a), probe_fingerprint(&b), "{view:?} geometry");
        assert_eq!(text(&a), text(&b), "{view:?} text");
    }
    // …and a different time or seed is a different frame.
    let a = text(&frame(&model_at(SEED, 211.5, View::Sky), 100, 30));
    assert_ne!(a, text(&frame(&model_at(SEED, 250.0, View::Sky), 100, 30)));
    assert_ne!(a, text(&frame(&model_at(SEED + 1, 211.5, View::Sky), 100, 30)));
}

#[test]
fn terminal_size_changes_realization_not_data_or_acceptance_counts() {
    // law E, as exercised by the app: same observables, three sizes
    let mk = |w, h| frame(&model_at(SEED, 300.0, View::Relation), w, h);
    let (a, b, c) = (mk(120, 40), mk(80, 24), mk(42, 15));
    for name in ["relation.growth", "relation.walk"] {
        let (pa, pb, pc) = (plot(&a, name), plot(&b, name), plot(&c, name));
        for p in [pb, pc] {
            assert_eq!(pa.report.samples_seen, p.report.samples_seen);
            assert_eq!(pa.report.finite_samples, p.report.finite_samples);
            assert_eq!(pa.report.scale_domain_rejected, p.report.scale_domain_rejected);
            assert_eq!(pa.report.nonfinite_rejected, p.report.nonfinite_rejected);
            assert_eq!(pa.spec.series.len(), p.spec.series.len());
            for (sa, sp) in pa.spec.series.iter().zip(&p.spec.series) {
                assert_eq!(sa.points, sp.points, "data must not depend on size");
            }
        }
    }
}

#[test]
fn colour_depth_and_glyph_mode_change_cells_not_geometry() {
    // law F: capability naturality
    let m = model_at(SEED, 300.0, View::Spectrum);
    let mut tc = Env::new(100, 30);
    let mut mono = Env::new(100, 30).mono();
    let a = compose(&m, &tc);
    let b = compose(&m, &mono);
    assert_eq!(probe_fingerprint(&a), probe_fingerprint(&b));
    tc.glyphs = gibson::SubcellGlyphMode::Ascii;
    mono.glyphs = gibson::SubcellGlyphMode::HalfBlock1x2;
    let c = compose(&m, &tc);
    let d = compose(&m, &mono);
    // layouts (geometry) identical; only the realised glyphs may differ
    let layouts = |f: &Frame| plots(f).iter().map(|p| format!("{:?}", p.layout)).collect::<Vec<_>>();
    assert_eq!(layouts(&a), layouts(&c));
    assert_eq!(layouts(&a), layouts(&d));
    assert_ne!(text(&a), text(&c), "ascii realisation differs from braille");
}

// ---------------------------------------------------------------------------
// 3. identity across representations
// ---------------------------------------------------------------------------

#[test]
fn identities_are_distinct_and_redundant() {
    let glyphs: Vec<_> = SigId::ALL.iter().map(|i| i.glyph()).collect();
    let letters: Vec<_> = SigId::ALL.iter().map(|i| i.letter()).collect();
    let colors: Vec<_> = SigId::ALL.iter().map(|i| i.color()).collect();
    for (a, b) in [(0, 1), (0, 2), (1, 2)] {
        assert_ne!(glyphs[a], glyphs[b]);
        assert_ne!(letters[a], letters[b]);
        assert_ne!(colors[a], colors[b]);
    }
}

#[test]
fn each_identity_is_findable_in_every_representation_in_colour_and_mono() {
    for mono in [false, true] {
        for view in View::ALL {
            for (w, h) in [(120u16, 40u16), (80, 24), (42, 15)] {
                let m = model_at(SEED, 300.0, view);
                let env = if mono { Env::new(w, h).mono() } else { Env::new(w, h) };
                let f = compose(&m, &env);
                let txt = text(&f);
                for id in SigId::ALL {
                    assert!(
                        txt.contains(id.glyph()),
                        "{} glyph missing in {view:?} {w}x{h} mono={mono}",
                        id.name()
                    );
                }
            }
        }
    }
}

#[test]
fn one_frequency_is_shared_by_rail_fold_spectrum_and_sky() {
    // The same semantic signal must be the same *number* in every representation.
    let m = model_at(SEED, 300.0, View::Fold);
    let cp = m.cp();
    for id in SigId::ALL {
        let p = cp.signal(id);
        let fmodel = p.model.unwrap().freq(cp.t);
        assert_eq!(p.f_now, fmodel);
        // fold lane header carries P = 1000/f
        let f = frame(&m, 120, 40);
        let txt = text(&f);
        let per = format!("P = {:.2} ms", 1000.0 / p.f_now);
        assert!(txt.contains(&per), "{}: fold header `{per}` missing", id.name());
    }
    // spectrum: the identity's overlay series sits on its model frequency
    let ms = model_at(SEED, 300.0, View::Spectrum);
    let fs = frame(&ms, 120, 40);
    let sp = plot(&fs, "spectrum");
    for id in [SigId::Alpha, SigId::Gamma] {
        let p = cp.signal(id);
        let ov = sp.spec.series.iter().find(|s| s.label == id.name()).expect("overlay");
        assert_eq!(ov.color, id.color());
        let f0 = p.model.unwrap().f0;
        assert!(
            ov.points.iter().any(|&(x, _)| (x - f0).abs() < 0.01),
            "{} overlay does not contain its fundamental",
            id.name()
        );
    }
    let ob = sp.spec.series.iter().find(|s| s.label == "BETA").expect("beta overlay");
    let pb = cp.signal(SigId::Beta);
    assert!(ob.points.iter().any(|&(x, _)| (x - pb.f_now).abs() < 0.02));
    // sky: the mark for an identity is its posterior mean
    let mk = model_at(SEED, 300.0, View::Sky);
    let fk = frame(&mk, 120, 40);
    let sky = fk
        .probes
        .iter()
        .find_map(|p| if let Probe::Sky(s) = p { Some(s) } else { None })
        .unwrap();
    for id in [SigId::Alpha, SigId::Beta] {
        let want = cp.signal(id).sky.unwrap().lm;
        let (_, got, _) = sky.marks.iter().find(|(n, _, _)| n == id.name()).expect("mark");
        assert_eq!(*got, want);
    }
}

#[test]
fn focus_keeps_the_selected_identity_and_dims_the_others() {
    let mut m = model_at(SEED, 300.0, View::Relation);
    m.apply(Cmd::Select(SigId::Beta));
    m.apply(Cmd::ToggleFocus);
    let f = frame(&m, 120, 40);
    let g = plot(&f, "relation.growth");
    let labels: Vec<&str> = g.spec.series.iter().map(|s| s.label.as_str()).collect();
    assert!(labels.contains(&"BETA"));
    assert!(!labels.contains(&"ALPHA") && !labels.contains(&"GAMMA"));
    assert_eq!(m.st.focus, Focus::Signal);
}

// ---------------------------------------------------------------------------
// 4. seek == run, cold == warm, replay is exact
// ---------------------------------------------------------------------------

#[test]
fn direct_seek_equals_running_forward() {
    let target = secs_to_samples(217.0);
    let mut seek = Model::new(SEED);
    seek.apply(Cmd::Seek(target));
    let mut run = Model::new(SEED);
    let mut left = target;
    while left > 0 {
        let k = left.min(77);
        run.apply(Cmd::Advance(k as u32));
        left -= k;
    }
    assert_eq!(seek.st, run.st);
    let env = Env::new(80, 24);
    assert_eq!(
        render_frame(&seek, &env).unwrap().lines,
        render_frame(&run, &env).unwrap().lines
    );
    // stepping back and forth also lands on the same state
    let mut walk = Model::new(SEED);
    walk.apply(Cmd::Seek(target + 5000));
    walk.apply(Cmd::Step(-5000));
    assert_eq!(walk.st.n, target);
}

#[test]
fn cold_analysis_equals_warm_analysis() {
    // A freshly built engine computing checkpoint 40 *directly* must agree bit-for-bit
    // with the shared engine that reached it incrementally: no hidden path dependence.
    let warm = Engine::shared(SEED);
    let a = warm.checkpoint(40);
    let cold = Engine::new(SEED);
    let b = cold.checkpoint(40);
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
    // and dropping the cache and recomputing gives the same again
    cold.reset();
    let c = cold.checkpoint(40);
    assert_eq!(format!("{b:?}"), format!("{c:?}"));
}

/// A tiny deterministic generator of arbitrary command sequences.
fn random_commands(n: usize, seed: u64) -> Vec<Cmd> {
    let mut s = seed;
    let mut next = move || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (s >> 33) as u32
    };
    let mut v = Vec::new();
    for _ in 0..n {
        let r = next();
        v.push(match r % 17 {
            0 => Cmd::Seek((next() as usize) % (N_TOTAL + 100)),
            1 => Cmd::Step((next() % 4000) as i64 - 2000),
            2 => Cmd::Advance(next() % 500),
            3 => Cmd::SetView(View::ALL[(next() % 6) as usize]),
            4 => Cmd::Select(SigId::from_idx((next() % 3) as usize)),
            5 => Cmd::CycleSelect(((next() % 3) as i8) - 1),
            6 => Cmd::ToggleFocus,
            7 => Cmd::Zoom(((next() % 3) as i8) - 1),
            8 => Cmd::PinHere,
            9 => Cmd::ToggleCompare,
            10 => Cmd::CursorMove(((next() % 3) as i8) - 1, ((next() % 3) as i8) - 1),
            11 => Cmd::ToggleCursor,
            12 => Cmd::ToggleTruth,
            13 => Cmd::JumpEvent(if next() % 2 == 0 { 1 } else { -1 }),
            14 => Cmd::Speed(((next() % 3) as i8) - 1),
            15 => Cmd::Frame(next() % 12),
            _ => Cmd::TogglePlay,
        });
    }
    v
}

#[test]
fn a_recorded_session_replays_to_the_identical_state_and_frame() {
    for seed in [1u64, 2, 3] {
        let cmds = random_commands(120, seed);
        let mut live = pulsar::session::Session::new(SEED);
        for c in &cmds {
            live.apply(c.clone());
        }
        // through the text log and back
        let text = live.log_text();
        let parsed = pulsar::session::parse_script(&text).expect("log parses");
        let replay = pulsar::session::Session::replay(SEED, &parsed);
        assert_eq!(live.model.st, replay.model.st, "state after replay (seed {seed})");
        for env in [Env::new(80, 24), Env::new(42, 15).mono()] {
            assert_eq!(
                render_frame(&live.model, &env).unwrap().lines,
                render_frame(&replay.model, &env).unwrap().lines
            );
        }
        // the log is compact: consecutive advances/frames fold
        assert!(live.log.len() <= cmds.len());
    }
}

// ---------------------------------------------------------------------------
// 5. cursor / inspection uses the library's own transform
// ---------------------------------------------------------------------------

#[test]
fn inspect_readout_matches_the_plot_transform_and_round_trips() {
    for view in [View::Stream, View::Spectrum, View::Fold, View::Relation] {
        let m = model_at(SEED, 300.0, view);
        let f = frame(&m, 120, 40);
        for p in plots(&f) {
            let r = p.layout.plot_rect;
            let t = p.layout.transform.as_ref().expect("transform");
            for cy in (r.y..r.y + r.height).step_by(3) {
                for cx in (r.x..r.x + r.width).step_by(5) {
                    let cell = (p.origin.0 + cx, p.origin.1 + cy);
                    let (x, y) = p.data_at(cell).expect("cell inside plot");
                    // project(unproject(cell centre)) must land back in the same cell
                    let back = p.cell_of(x, y).unwrap_or_else(|| panic!("{}: {x},{y} not drawable", p.name));
                    assert_eq!(back, cell, "{}: round trip of cell {cell:?} via ({x},{y})", p.name);
                    let _ = t;
                }
            }
        }
    }
}

#[test]
fn inspect_readout_text_reports_the_data_coordinates_under_the_cursor() {
    let mut m = model_at(SEED, 300.0, View::Spectrum);
    m.apply(Cmd::ToggleCursor);
    m.apply(Cmd::CursorMove(2, -3));
    let f = frame(&m, 120, 40);
    let r = f.readout.clone().expect("a readout");
    assert!(r.contains("frequency [Hz]") || r.contains("dynspec"), "{r}");
    let hint = f.hint.to_visible_lines().join("");
    assert!(hint.contains("┼"), "readout is shown on the hint line: {hint}");
}

// ---------------------------------------------------------------------------
// 6. honest handling of degenerate input
// ---------------------------------------------------------------------------

#[test]
fn nothing_received_yet_is_stated_not_faked() {
    let m = model_at(SEED, 0.0, View::Stream);
    let f = frame(&m, 100, 30);
    assert!(text(&f).contains("no data yet"));
    let raw = plot(&f, "stream.raw");
    assert_eq!(raw.report.samples_seen, 0);
    // before the first checkpoint no identity claims anything
    let cp = m.cp();
    for id in SigId::ALL {
        assert_eq!(cp.signal(id).stage, Stage::Unseen);
        assert!(cp.signal(id).sky.is_none());
    }
    // sub-chunk time: stream has data, analysis lags and says so
    let mut m2 = Model::new(SEED);
    m2.apply(Cmd::Seek(CHUNK - 1));
    assert_eq!(m2.cp().k, 0);
    let f2 = frame(&m2, 100, 30);
    assert!(text(&f2).contains("analysis 0s"));
}

#[test]
fn every_view_survives_every_size_and_never_overflows_its_box() {
    let sizes = [(1u16, 1u16), (2, 2), (3, 2), (10, 3), (20, 5), (30, 8), (41, 14), (42, 15), (60, 12), (79, 23), (120, 40), (200, 60)];
    let times = [0.0, 0.03, 5.0, 64.0, 480.0];
    for &(w, h) in &sizes {
        for view in View::ALL {
            for &t in &times {
                for (focus, cmp) in [(false, false), (true, false), (false, true)] {
                    let mut m = model_at(SEED, t, view);
                    if focus {
                        m.apply(Cmd::ToggleFocus);
                        m.apply(Cmd::Zoom(1));
                    }
                    if cmp {
                        m.apply(Cmd::ToggleCompare);
                    }
                    m.apply(Cmd::ToggleCursor);
                    let r = render_frame(&m, &Env::new(w, h)).expect("renders");
                    assert_eq!(r.lines.len(), h as usize, "{view:?} {w}x{h} height");
                    for (i, l) in r.lines.iter().enumerate() {
                        assert!(
                            l.chars().count() <= w as usize,
                            "{view:?} {w}x{h} t={t}: row {i} has {} cells: {l:?}",
                            l.chars().count()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn an_invalid_axis_configuration_is_reported_never_silently_blanked() {
    use gibson::plot::{AxisScale, AxisSpec, PlotSpec, PlotView, Series};
    use gibson::{Rect, Surface};
    // Log10 axis over a non-positive view: a *configuration* fault, not bad samples.
    let spec = PlotSpec::new(
        AxisSpec::new(AxisScale::Linear, "x"),
        AxisSpec::new(AxisScale::Log10, "y"),
    )
    .series(Series::line(vec![(0.0, 1.0), (1.0, 10.0)]));
    let bad = PlotView::new(
        draw::range(0.0, 1.0),
        gibson::plot::FiniteRange::new(-1.0, 1.0).unwrap(),
    );
    let mut s = Surface::new(40, 10);
    let probe = draw::draw_plot(&mut s, Rect::new(0, 0, 40, 10), "demo", spec, bad, gibson::SubcellGlyphMode::Braille2x4);
    assert!(probe.is_none());
    let t = s.to_visible_lines().join("\n");
    assert!(t.contains("invalid y-axis domain"), "{t}");
    // degenerate ranges requested by our own views are widened, not dropped
    let r = draw::range(5.0, 5.0);
    assert!(r.max() > r.min());
    let r = draw::range(f64::NAN, 3.0);
    assert!(r.max() > r.min());
    let r = draw::log_range(-1.0, 0.0);
    assert!(r.min() > 0.0 && r.max() > r.min());
}

#[test]
fn all_nan_and_flat_series_plot_without_panic_and_are_counted() {
    use gibson::plot::{AxisScale, AxisSpec, PlotSpec, PlotView, Series};
    use gibson::{Rect, Surface};
    let spec = PlotSpec::new(
        AxisSpec::new(AxisScale::Linear, "x"),
        AxisSpec::new(AxisScale::Linear, "y"),
    )
    .series(Series::line(vec![(f64::NAN, f64::NAN); 5]))
    .series(Series::line((0..50).map(|i| (i as f64, 1.0)).collect()));
    let mut s = Surface::new(40, 10);
    let p = draw::draw_plot(
        &mut s,
        Rect::new(0, 0, 40, 10),
        "degenerate",
        spec,
        PlotView::new(draw::range(0.0, 49.0), draw::range(1.0, 1.0)),
        gibson::SubcellGlyphMode::Braille2x4,
    )
    .expect("plots");
    assert_eq!(p.report.nonfinite_rejected, 5);
    assert_eq!(p.report.finite_samples, 50);
}

#[test]
fn analysis_checkpoint_index_clamps() {
    let eng = Engine::shared(SEED);
    assert_eq!(eng.checkpoint(10_000).k, N_CP);
    assert_eq!(eng.checkpoint_at_sample(N_TOTAL).k, N_CP);
    assert_eq!(eng.checkpoint_at_sample(0).k, 0);
}

// ---------------------------------------------------------------------------
// 7. small structural facts about the custom graphics
// ---------------------------------------------------------------------------

#[test]
fn phasor_coherence_separates_a_ray_from_a_random_walk() {
    use pulsar::render::relation::coherence;
    let ray: Vec<(f64, f64)> = (0..40).map(|i| (i as f64, 0.5 * i as f64)).collect();
    let (c, _) = coherence(&ray).unwrap();
    assert!((c - 1.0).abs() < 1e-12, "a straight ray is perfectly coherent");
    // an alternating walk returns to the origin: zero coherence
    let osc: Vec<(f64, f64)> = (0..41).map(|i| ((i % 2) as f64, 0.0)).collect();
    assert!(coherence(&osc).unwrap().0 < 0.05);
    assert!(coherence(&[(0.0, 0.0)]).is_none(), "degenerate walks have no coherence");
}

#[test]
fn posterior_cloud_is_deterministic_inside_the_sky_and_contracts() {
    use pulsar::render::sky::{sample, CLOUD_N};
    let eng = Engine::shared(SEED);
    let early = eng.checkpoint_at_sample(secs_to_samples(104.0));
    let late = eng.checkpoint_at_sample(N_TOTAL);
    let spread = |cp: &pulsar::analysis::Checkpoint| {
        let p = cp.signal(SigId::Beta);
        let pts: Vec<_> = (0..CLOUD_N).map(|i| sample(SEED, SigId::Beta, p, i)).collect();
        for (i, &(x, y)) in pts.iter().enumerate() {
            assert!(x * x + y * y <= 1.0, "sample {i} below the horizon");
            assert_eq!(pts[i], sample(SEED, SigId::Beta, p, i), "sample {i} not a pure function");
        }
        let (mx, my) = (
            pts.iter().map(|p| p.0).sum::<f64>() / CLOUD_N as f64,
            pts.iter().map(|p| p.1).sum::<f64>() / CLOUD_N as f64,
        );
        (pts.iter().map(|p| (p.0 - mx).powi(2) + (p.1 - my).powi(2)).sum::<f64>() / CLOUD_N as f64).sqrt()
    };
    assert!(spread(&late) < 0.7 * spread(&early));
}
