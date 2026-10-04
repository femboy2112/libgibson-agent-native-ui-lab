//! Semantic and scientific invariants of Project Pulsar. None of these tests asks
//! "does it render?": each one states a property of the observatory that must
//! hold if the model, the analysis and the representations are telling the truth.

// Slots are identities: indexing `IDENT[s]` / `slots[s]` by the same `s` is the point.
#![allow(clippy::needless_range_loop)]

use std::sync::{Arc, OnceLock};

use gibson::plot::{self, AxisScale, AxisSpec, PlotSpec, PlotView, Series};
use gibson::{KeyCode, KeyEvent, KeyModifiers, Rect, SubcellGlyphMode};

use project_pulsar::cli::{self, Capture};
use project_pulsar::compose::{self, Class};
use project_pulsar::dsp;
use project_pulsar::identity::{self, IDENT};
use project_pulsar::observatory::{Observatory, DEFAULT_SEED};
use project_pulsar::sim::{self, DURATION, EPOCH_S, FS, N_EPOCHS, SLOTS, T_S4_ON};
use project_pulsar::state::{run_script, Model};
use project_pulsar::track::{epoch_at, EventKind, State};
use project_pulsar::views::{self, common, trace, Ctx, View};

const BRAILLE: SubcellGlyphMode = SubcellGlyphMode::Braille2x4;

fn obs() -> Arc<Observatory> {
    static O: OnceLock<Arc<Observatory>> = OnceLock::new();
    O.get_or_init(|| Arc::new(Observatory::new(DEFAULT_SEED)))
        .clone()
}

fn model_at(t: f64, view: View) -> Model {
    let mut m = Model::new(obs());
    m.seek(t);
    m.view = view;
    m
}

fn lines(f: &compose::Frame) -> Vec<String> {
    f.surface.to_visible_lines()
}

fn text(m: &Model, w: u16, h: u16, mono: bool) -> String {
    lines(&compose::compose(m, w, h, mono, BRAILLE)).join("\n")
}

fn ctx<'a>(m: &'a Model) -> Ctx<'a> {
    compose::ctx_for(m, false, BRAILLE)
}

fn circ(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(1.0);
    d.min(1.0 - d)
}

// ───────────────────────────── determinism ─────────────────────────────────────

#[test]
fn world_is_deterministic_from_the_seed() {
    let a = Observatory::new(DEFAULT_SEED);
    let b = Observatory::new(DEFAULT_SEED);
    assert_eq!(
        a.digest(),
        b.digest(),
        "same seed => bit-identical data and timeline"
    );
    assert_eq!(a.digest(), obs().digest());
    let c = Observatory::new(DEFAULT_SEED ^ 1);
    assert_ne!(
        a.digest(),
        c.digest(),
        "a different seed is a different world"
    );
}

#[test]
fn identical_input_gives_byte_identical_frames_in_every_view_and_size() {
    for view in View::ALL {
        for (w, h) in [(120, 40), (80, 24), (42, 15)] {
            for mono in [false, true] {
                let m = model_at(177.0, view);
                let a = text(&m, w, h, mono);
                let b = text(&m, w, h, mono);
                assert_eq!(a, b, "{view:?} {w}x{h} mono={mono}");
            }
        }
    }
}

#[test]
fn capture_through_the_ui_runtime_is_byte_stable_and_matches_the_composer() {
    let mut m = model_at(200.0, View::Sky);
    m.sel = Some(2);
    for (w, h, mono) in [(120u16, 40u16, false), (80, 24, true), (42, 15, false)] {
        let cap = Capture {
            w,
            h,
            mono,
            color: false,
        };
        let a = cli::capture_text(&m, &cap, BRAILLE).expect("capture");
        let b = cli::capture_text(&m, &cap, BRAILLE).expect("capture");
        assert_eq!(a, b);
        let composed = compose::compose(&m, w, h, mono, BRAILLE)
            .surface
            .to_visible_lines();
        let runtime: Vec<&str> = a.lines().collect();
        for (i, row) in composed.iter().enumerate() {
            assert_eq!(
                runtime.get(i).copied().unwrap_or("").trim_end(),
                row.trim_end(),
                "row {i} of {w}x{h}: the real UI path must show what the composer drew"
            );
        }
    }
}

#[test]
fn seeking_equals_running_and_a_key_log_replays_exactly() {
    // direct seek == stepping with the scrub key
    let mut stepped = Model::new(obs());
    for _ in 0..8 {
        stepped.key(KeyEvent::new(KeyCode::Right, KeyModifiers::empty()));
    }
    let seeked = model_at(32.0, View::Trace);
    assert_eq!(stepped.t, 32.0);
    assert_eq!(
        text(&stepped, 100, 34, false),
        text(&seeked, 100, 34, false)
    );

    // direct seek == playing (dyadic steps: 16 x 0.25 s x 8 s/s = 32 s exactly)
    let mut played = Model::new(obs());
    played.playing = true;
    for _ in 0..16 {
        played.advance(0.25);
    }
    assert_eq!(played.t, 32.0);
    played.playing = false; // only the transport glyph in the header differs while playing
    assert_eq!(text(&played, 100, 34, false), text(&seeked, 100, 34, false));

    // a recorded session replays to the identical frame
    let mut original = Model::new(obs());
    run_script(&mut original, "5 @150 down c z @200 3 , , 2 m 4 left left").expect("script");
    let log = original.replay_script();
    let mut replay = Model::new(obs());
    run_script(&mut replay, &log).expect("replay of the log");
    assert_eq!(replay.t, original.t);
    assert_eq!(replay.view, original.view);
    assert_eq!(replay.sel, original.sel);
    assert_eq!(replay.nudge, original.nudge);
    assert_eq!(
        text(&replay, 120, 40, false),
        text(&original, 120, 40, false)
    );
}

#[test]
fn the_pipeline_is_causal_the_future_cannot_change_a_published_solution() {
    let k0 = 30usize;
    let n0 = k0 * sim::EPOCH_SAMPLES;
    let base = sim::generate(DEFAULT_SEED, true);
    let mut altered = base.clone();
    for s in altered.streams.iter_mut() {
        for (i, v) in s.iter_mut().enumerate().skip(n0) {
            *v = if i % 3 == 0 {
                f64::NAN
            } else {
                -2.5 * *v + 7.0
            };
        }
    }
    let a = Observatory::from_raw(DEFAULT_SEED, base);
    let b = Observatory::from_raw(DEFAULT_SEED, altered);
    for k in 1..=k0 {
        let (ea, eb) = (a.timeline.epoch(k).unwrap(), b.timeline.epoch(k).unwrap());
        for s in 0..SLOTS {
            let (x, y) = (ea.slots[s], eb.slots[s]);
            assert_eq!(x.state, y.state, "epoch {k} slot {s}");
            assert_eq!(x.f.to_bits(), y.f.to_bits(), "epoch {k} slot {s} f");
            assert_eq!(x.z.to_bits(), y.z.to_bits(), "epoch {k} slot {s} z");
        }
    }
    // ...and the future DID change, so the comparison above was not vacuous
    assert_ne!(a.digest(), b.digest());
}

// ─────────────────────── scientific recovery of the injected world ─────────────────

#[test]
fn the_fold_recovers_every_injected_period() {
    let o = obs();
    let last = o.timeline.epoch(N_EPOCHS).unwrap();
    for s in 0..3 {
        let truth = &o.truth()[s];
        let sl = last.slots[s];
        assert_eq!(
            sl.state,
            State::Locked,
            "{} must be locked",
            IDENT[s].letter
        );
        assert!(
            (sl.f - truth.f).abs() < 4e-4,
            "{}: f {} vs injected {}",
            IDENT[s].letter,
            sl.f,
            truth.f
        );
        let period_err = (1.0 / sl.f - truth.period()).abs() / truth.period();
        assert!(
            period_err < 3e-4,
            "{}: period error {period_err}",
            IDENT[s].letter
        );
    }
}

#[test]
fn the_folded_pulse_lands_on_the_injected_phase_and_survives_only_at_the_right_period() {
    let o = obs();
    let last = o.timeline.epoch(N_EPOCHS).unwrap();
    for s in 0..3 {
        let truth = &o.truth()[s];
        let sl = last.slots[s];
        // the fold peak is one of the injected components (a double-peaked profile
        // may legitimately report either of them as its maximum)
        let nearest = truth
            .comps
            .iter()
            .map(|c| circ(sl.peak_phase, c.phase))
            .fold(1.0, f64::min);
        assert!(
            nearest < 0.04,
            "{}: fold peak {} is {nearest:.3} cycles from every injected component",
            IDENT[s].letter,
            sl.peak_phase
        );
        // single-component profiles pin the phase exactly
        if truth.comps.len() == 1 {
            assert!(circ(sl.peak_phase, truth.fold_peak_phase()) < 0.02);
        }
        // the drift-free recent-half phase (used to extrapolate arrival times) is on a pulse too
        let nearest_recent = truth
            .comps
            .iter()
            .map(|c| circ(sl.peak_phase_recent, c.phase))
            .fold(1.0, f64::min);
        assert!(
            nearest_recent < 0.05,
            "{}: recent peak {} is {nearest_recent:.3} cycles off",
            IDENT[s].letter,
            sl.peak_phase_recent
        );
        let c = &o.cleans[0];
        let right = dsp::fold(c, 0, sim::N, sl.f, 64).snr();
        let wrong = dsp::fold(c, 0, sim::N, sl.f * 1.004, 64).snr();
        assert!(right > 10.0, "{}: on-period fold {right}", IDENT[s].letter);
        assert!(
            wrong < 0.6 * right,
            "{}: detuned fold {wrong} vs {right}",
            IDENT[s].letter
        );
    }
}

#[test]
fn trace_pulse_markers_land_on_the_true_pulse_arrival_times() {
    let o = obs();
    let last = o.timeline.epoch(N_EPOCHS).unwrap();
    for s in 0..3 {
        let truth = &o.truth()[s];
        let sl = last.slots[s];
        let epochs = trace::pulse_epochs(sl.f, sl.peak_phase_recent, 232.0, 256.0);
        assert!(epochs.len() as f64 > 0.9 * 24.0 * truth.f - 2.0);
        let top = (0..400)
            .map(|i| truth.profile_at(i as f64 / 400.0))
            .fold(0.0, f64::max);
        for t in epochs {
            // the TRUE waveform, evaluated at the predicted arrival time, is on a pulse
            let v = truth.profile_at(truth.f * t);
            assert!(
                v > 0.45 * top,
                "{}: marker at {t:.3}s sees {:.2} of a {:.2} pulse",
                IDENT[s].letter,
                v,
                top
            );
        }
    }
}

#[test]
fn spectral_lines_sit_where_the_tracker_says_and_are_significant() {
    let o = obs();
    let spec = o.spectrum(N_EPOCHS);
    let last = o.timeline.epoch(N_EPOCHS).unwrap();
    for s in 0..SLOTS {
        let sl = last.slots[s];
        let truth = &o.truth()[s];
        let line = project_pulsar::track::line_power(&spec, sl.f);
        if s == 4 {
            continue; // the burst is gone by the end
        }
        assert!(line > 12.0, "{}: line power {line}", IDENT[s].letter);
        assert!((sl.f - truth.f).abs() < 1e-3);
        // and nothing else is brighter nearby
        let k = spec.bin(truth.f);
        let around = spec.power[k - 3..=k + 3]
            .iter()
            .cloned()
            .fold(0.0, f64::max);
        assert!(around >= line * 0.99);
    }
    // the periodogram itself is a calibrated instrument: floor ~ 1 away from lines
    let lo = spec.bin(5.6);
    let hi = spec.bin(30.0);
    let mean: f64 = spec.power[lo..hi].iter().sum::<f64>() / (hi - lo) as f64;
    assert!((mean - 1.0).abs() < 0.08, "mean floor {mean}");
}

#[test]
fn sky_localisation_matches_the_injected_geometry() {
    let o = obs();
    let last = o.timeline.epoch(N_EPOCHS).unwrap();
    for s in 0..3 {
        let truth = &o.truth()[s];
        let fix = last.slots[s].sky.expect("locked sources are localised");
        let off = sim::separation_deg(fix.best(), (truth.az, truth.el));
        assert!(
            off < 10.0f64.max(1.5 * fix.sigma_deg),
            "{}: {off:.1} deg off, sigma {:.1}",
            IDENT[s].letter,
            fix.sigma_deg
        );
        // the mirror ambiguity was resolved toward the correct hemisphere
        let up = truth.el > 0.0;
        assert!(fix.resolved_by_s4);
        if up {
            assert!(fix.w_plus > 0.95, "{}: w+ {}", IDENT[s].letter, fix.w_plus);
        } else {
            assert!(fix.w_plus < 0.05, "{}: w+ {}", IDENT[s].letter, fix.w_plus);
        }
    }
}

#[test]
fn a_planar_array_cannot_tell_a_source_from_its_mirror_until_station_four_arrives() {
    let o = obs();
    let before = o.timeline.epoch(epoch_at(T_S4_ON - 8.0)).unwrap();
    for s in 0..3 {
        let fix = before.slots[s].sky.expect("fix");
        assert!(!fix.resolved_by_s4);
        assert!(
            (fix.w_plus - 0.5).abs() < 1e-9,
            "{}: planar array => 50/50",
            IDENT[s].letter
        );
    }
    // the picture shows it: every locked identity has a ghost twin before S4...
    let m = model_at(T_S4_ON - 8.0, View::Sky);
    let pre = text(&m, 120, 40, false);
    for s in 0..3 {
        assert!(
            pre.contains(&format!(
                "{}{}\u{00B7}50%",
                IDENT[s].hollow, IDENT[s].letter
            )),
            "ghost of {} missing before S4",
            IDENT[s].letter
        );
    }
    // ...and none after the collapse.
    let post = text(&model_at(DURATION, View::Sky), 120, 40, false);
    for s in 0..3 {
        assert!(
            !post.contains(&format!("{}{}\u{00B7}", IDENT[s].hollow, IDENT[s].letter)),
            "ghost of {} survived S4",
            IDENT[s].letter
        );
    }
}

#[test]
fn confidence_grows_with_integration_time() {
    let o = obs();
    for s in 0..3 {
        let z = |t: f64| o.timeline.epoch(epoch_at(t)).unwrap().slots[s].z;
        assert!(
            z(64.0) < z(128.0) && z(128.0) < z(256.0),
            "{}: Z must grow",
            IDENT[s].letter
        );
    }
    // localisation tightens as timing accumulates (A is localised from ~96 s)
    let sig = |t: f64| {
        o.timeline.epoch(epoch_at(t)).unwrap().slots[0]
            .sky
            .unwrap()
            .sigma_deg
    };
    assert!(sig(112.0) > sig(184.0) && sig(184.0) > sig(256.0) * 0.99);
    assert!(
        sig(256.0) < 0.85 * sig(112.0),
        "{} vs {}",
        sig(256.0),
        sig(112.0)
    );
}

#[test]
fn false_candidates_are_rejected_and_never_locked() {
    let o = obs();
    let (mut d_cw, mut d_common) = (false, false);
    for e in &o.timeline.epochs {
        assert_ne!(
            e.slots[3].state,
            State::Locked,
            "interference must never lock"
        );
        assert_ne!(
            e.slots[4].state,
            State::Locked,
            "the transient must never lock"
        );
        d_cw |= e.slots[3].state == State::RejectedCw;
        d_common |= e.slots[3].common_mode;
    }
    assert!(d_cw && d_common);
    let last = o.timeline.epoch(N_EPOCHS).unwrap();
    assert_eq!(last.slots[3].state, State::RejectedCw);
    assert!(last.slots[3].pulsy < 0.1, "a sinusoid has no harmonics");
    assert_eq!(last.slots[4].state, State::RejectedTransient);
    let zmax = o
        .timeline
        .epochs
        .iter()
        .map(|e| e.slots[4].z)
        .fold(0.0, f64::max);
    assert!(
        last.slots[4].z < 0.9 * zmax,
        "the burst's significance must have decayed"
    );
    let locked: Vec<usize> = (0..SLOTS)
        .filter(|s| last.slots[*s].state == State::Locked)
        .collect();
    assert_eq!(
        locked,
        vec![0, 1, 2],
        "exactly the three persistent sources survive"
    );
}

#[test]
fn noise_only_data_raises_no_candidates() {
    for seed in [11u64, 12, 13, 14] {
        let o = Observatory::from_raw(seed, sim::generate(seed, false));
        for e in &o.timeline.epochs {
            for s in 0..SLOTS {
                assert_eq!(
                    e.slots[s].state,
                    State::Quiet,
                    "seed {seed} epoch {} slot {s}",
                    e.k
                );
            }
        }
    }
}

#[test]
fn identities_appear_in_a_sensible_order_and_events_are_consistent() {
    let o = obs();
    let ev = &o.timeline.events;
    assert!(
        ev.windows(2).all(|w| w[0].epoch <= w[1].epoch),
        "events are time ordered"
    );
    for s in 0..SLOTS {
        let det = o.timeline.first_event(s, EventKind::Detected);
        if let Some(lock) = o.timeline.first_event(s, EventKind::Locked) {
            assert!(det.expect("detected first").epoch <= lock.epoch);
        }
    }
    let t_lock = |s| {
        o.timeline
            .first_event(s, EventKind::Locked)
            .expect("locked")
            .t
    };
    assert!(t_lock(2) <= 100.0 && t_lock(0) <= 120.0 && t_lock(1) <= 150.0);
    // a mirror collapses only after station 4 is online
    for e in ev.iter().filter(|e| e.kind == EventKind::MirrorResolved) {
        assert!(e.t > T_S4_ON);
    }
    assert!(ev.iter().any(|e| e.kind == EventKind::StationOnline));
}

// ───────────────────────────── invalid data handling ───────────────────────────────

#[test]
fn invalid_samples_are_masked_counted_and_never_reach_the_analysis() {
    let o = obs();
    let raw = &o.raw.streams[0];
    assert!(
        raw.iter().any(|v| v.is_nan()),
        "the data really contains NaN"
    );
    assert!(raw.iter().any(|v| v.is_infinite()), "...and infinity");
    let c = &o.cleans[0];
    assert!(c.x.iter().all(|v| v.is_finite()));
    for &n in &o.raw.corrupted {
        assert!(!c.ok[n], "corrupted sample {n} must be masked");
    }
    assert!(c.masked >= o.raw.corrupted.len());
    for ep in [1, 16, 40, 64] {
        assert!(
            o.spectrum(ep).power.iter().all(|v| v.is_finite()),
            "epoch {ep}"
        );
    }
    for e in &o.timeline.epochs {
        for s in &e.slots {
            assert!(s.z.is_finite() && s.f.is_finite() && s.pulsy.is_finite());
        }
    }
}

#[test]
fn gaps_in_the_trace_break_the_line_and_appear_on_the_plot_receipt() {
    let o = obs();
    // the second receiver dropout (176.0 .. 177.4 s) sits inside this window
    let (lo, hi) = (170.0, 190.0);
    let pts = trace::trace_points(&o.cleans[0], lo, hi);
    let masked = trace::masked_in(&o.cleans[0], lo, hi);
    assert!(masked >= 85, "dropout is {masked} samples");
    let spec = PlotSpec::new(
        AxisSpec::new(AxisScale::Linear, "t").unit("s"),
        AxisSpec::new(AxisScale::Linear, "a").unit("sigma"),
    )
    .series(Series::line(pts.clone()));
    let view = PlotView::new(common::range(lo, hi), common::range(-6.0, 6.0));
    let (_, rep) = plot::compile(&spec, &view, Rect::new(0, 0, 100, 30));
    assert_eq!(rep.samples_seen, pts.len());
    assert_eq!(
        rep.nonfinite_rejected, masked,
        "the receipt accounts for every masked sample"
    );
    // gap law: segments exist only between consecutive finite samples
    let consecutive = pts
        .windows(2)
        .filter(|w| w[0].1.is_finite() && w[1].1.is_finite())
        .count();
    assert_eq!(rep.segments_considered, consecutive);
    assert!(
        consecutive < pts.len() - masked,
        "a gap removes the bridging segment"
    );
}

#[test]
fn hostile_command_lines_are_rejected_with_a_message_not_a_panic() {
    let bad: &[&[&str]] = &[
        &["--capture", "0x0"],
        &["--capture", "7x3"],
        &["--capture", "wide"],
        &["--capture", "80x24:neon"],
        &["--at", "nan"],
        &["--at", "inf"],
        &["--at", "soon"],
        &["--view", "bogus"],
        &["--select", "Z"],
        &["--cursor", "0.5"],
        &["--glyphs", "emoji"],
        &["--no-such-flag"],
        &["--seed", "0xZZ"],
    ];
    for args in bad {
        let v: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        assert!(cli::parse_args(&v).is_err(), "{args:?} must be rejected");
    }
    let ok: Vec<String> = [
        "--capture",
        "120x40:mono",
        "--at",
        "300",
        "--view",
        "sky",
        "--select",
        "b",
        "--compare",
        "60",
        "--cursor",
        "2,-1",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let o = cli::parse_args(&ok).expect("valid");
    assert_eq!(o.at, Some(DURATION), "--at clamps into the mission");
    assert_eq!(
        o.cursor,
        Some((1.0, 0.0)),
        "--cursor clamps into the unit square"
    );
    assert_eq!(o.select, Some(1));
    assert!(o.capture.as_ref().unwrap().mono);
    // a script with an unknown token is an error too
    let mut m = Model::new(obs());
    assert!(run_script(&mut m, "1 2 frobnicate").is_err());
}

// ───────────────────────── axes, transforms and cursors ────────────────────────────

#[test]
fn axis_captions_name_the_modelled_quantities() {
    let expected: [(View, &[(&str, &str)]); 4] = [
        (
            View::Trace,
            &[("receiver time (s)", "S1 amplitude (\u{03C3})")],
        ),
        (
            View::Spectrum,
            &[("frequency (Hz)", "power (\u{00D7} floor)")],
        ),
        (
            View::Fold,
            &[("rotation phase (cycles)", "folded amplitude (\u{03C3})")],
        ),
        (
            View::Relate,
            &[
                ("delay \u{03C4}21 (ms)", "delay \u{03C4}31 (ms)"),
                ("integration (s)", "coherent significance (\u{03C3})"),
            ],
        ),
    ];
    for (view, axes) in expected {
        let mut m = model_at(200.0, view);
        m.sel = Some(0);
        let c = ctx(&m);
        let out = views::render(view, &c, Rect::new(0, 0, 118, 28));
        assert_eq!(out.axes.len(), axes.len(), "{view:?}");
        for (got, want) in out.axes.iter().zip(axes) {
            assert_eq!((got.0.as_str(), got.1.as_str()), *want, "{view:?}");
        }
        // and the captions are actually on screen
        let shown = text(&m, 120, 40, false);
        for (x, y) in axes {
            assert!(shown.contains(x), "{view:?}: {x} not drawn");
            assert!(shown.contains(y), "{view:?}: {y} not drawn");
        }
    }
    // the sky has no plot axes but states its coordinates
    let sky = text(&model_at(200.0, View::Sky), 120, 40, false);
    assert!(sky.contains("az") && sky.contains("el"));
}

#[test]
fn the_plot_transform_round_trips_between_data_cells_and_the_cursor() {
    let spec = PlotSpec::new(
        AxisSpec::new(AxisScale::Linear, "f").unit("Hz"),
        AxisSpec::new(AxisScale::Log10, "p"),
    );
    let view = PlotView::new(common::range(0.3, 5.5), common::range(0.3, 1000.0));
    for (w, h) in [(118u16, 24u16), (78, 12), (40, 9)] {
        let po = common::draw_plot(&spec, &view, w, h, BRAILLE);
        let pr = po.layout.plot_rect;
        for i in 0..=20 {
            for j in 0..=10 {
                let (u, v) = (i as f64 / 20.0, j as f64 / 10.0);
                let (x, y) = common::data_at(&po.layout, u, v).expect("data");
                assert!((0.3 - 1e-9..=5.5 + 1e-9).contains(&x));
                assert!(y > 0.0);
                let (cx, cy) = common::cell_of(&po.layout, x, y).expect("cell");
                let ex = pr.x as i32 + ((u * pr.width as f64).floor() as i32).min(pr.width as i32);
                let ey = pr.y as i32
                    + (((1.0 - v) * pr.height as f64).floor() as i32).min(pr.height as i32);
                assert!(
                    (cx - ex).abs() <= 1 && (cy - ey).abs() <= 1,
                    "({u},{v}) -> {cx},{cy} vs {ex},{ey}"
                );
            }
        }
    }
}

#[test]
fn cursor_readouts_report_the_quantities_under_the_cursor() {
    // spectrum: the cursor column IS a frequency
    let mut m = model_at(256.0, View::Spectrum);
    m.cursor_on = true;
    m.cursor = ((1.2974 - 0.3) / 5.2, 0.5);
    let c = ctx(&m);
    let out = views::render(View::Spectrum, &c, Rect::new(0, 0, 118, 28));
    assert!(out.readout.starts_with("f 1.2974 Hz"), "{}", out.readout);
    assert!(
        out.readout.contains(&IDENT[0].solid.to_string()),
        "nearest identity is named: {}",
        out.readout
    );

    // delay plane: u,v are tau21, tau31 in ms
    let mut m = model_at(256.0, View::Relate);
    m.cursor_on = true;
    m.cursor = (0.75, 0.25);
    let out = views::render(View::Relate, &ctx(&m), Rect::new(0, 0, 118, 28));
    assert!(
        out.readout.contains("+35.0 ms") && out.readout.contains("-35.0 ms"),
        "{}",
        out.readout
    );

    // sky: the centre of the Hammer map is az 180, el 0
    let mut m = model_at(256.0, View::Sky);
    m.cursor_on = true;
    m.cursor = (0.5, 0.5);
    let out = views::render(View::Sky, &ctx(&m), Rect::new(0, 0, 118, 28));
    assert!(
        out.readout.starts_with("az 180.0\u{00B0}  el +0.0\u{00B0}"),
        "{}",
        out.readout
    );

    // fold: the cursor column is a rotation phase
    let mut m = model_at(256.0, View::Fold);
    m.sel = Some(0);
    m.cursor_on = true;
    m.cursor = (0.30, 0.5);
    let out = views::render(View::Fold, &ctx(&m), Rect::new(0, 0, 118, 28));
    assert!(out.readout.starts_with("phase 0.300"), "{}", out.readout);
    let ph_peak: f64 = o_peak_phase(0);
    assert!((ph_peak - 0.32).abs() < 0.05);
}

fn o_peak_phase(slot: usize) -> f64 {
    obs().timeline.epoch(N_EPOCHS).unwrap().slots[slot].peak_phase
}

// ─────────────────────────────── identity & atlas ──────────────────────────────────

fn cell_char(f: &compose::Frame, x: i32, y: i32) -> String {
    let y = y + f.layout.hero.y as i32;
    f.surface
        .get(x as u16, y as u16)
        .map(|c| c.glyph.grapheme.to_string())
        .unwrap_or_default()
}

#[test]
fn every_identity_is_the_same_glyph_in_every_representation() {
    for mono in [false, true] {
        for (w, h) in [(120u16, 40u16), (80, 24), (42, 15)] {
            for view in View::ALL {
                let m = model_at(220.0, view);
                let f = compose::compose(&m, w, h, mono, BRAILLE);
                for s in 0..3 {
                    // the three persistent sources are locked: anchored, solid, in bounds
                    let a = f.hero.anchor(s).unwrap_or_else(|| {
                        panic!("{view:?} {w}x{h}: no anchor for {}", IDENT[s].letter)
                    });
                    assert!(
                        a.x >= 0 && a.x < w as i32 && a.y >= 0 && a.y < f.layout.hero.height as i32,
                        "{view:?} {w}x{h} anchor {a:?} outside the hero"
                    );
                    assert_eq!(
                        cell_char(&f, a.x, a.y),
                        IDENT[s].solid,
                        "{view:?} {w}x{h} mono={mono}: wrong glyph at the anchor of {}",
                        IDENT[s].letter
                    );
                }
                // the persistent ledger names every identity too
                let txt = lines(&f).join("\n");
                for s in 0..3 {
                    assert!(
                        txt.contains(&format!("{}{}", IDENT[s].solid, IDENT[s].letter)),
                        "{view:?} {w}x{h}"
                    );
                }
            }
        }
    }
}

#[test]
fn lock_acquisition_turns_the_hollow_identity_solid_in_every_view() {
    // B: tracking (hollow) at 100 s, locked (solid) at 200 s; the letter never changes.
    let o = obs();
    assert_eq!(
        o.timeline.epoch(epoch_at(100.0)).unwrap().slots[1].state,
        State::Tracking
    );
    assert_eq!(
        o.timeline.epoch(epoch_at(200.0)).unwrap().slots[1].state,
        State::Locked
    );
    for view in View::ALL {
        let early = compose::compose(&model_at(100.0, view), 120, 40, false, BRAILLE);
        let late = compose::compose(&model_at(200.0, view), 120, 40, false, BRAILLE);
        let a = early.hero.anchor(1).expect("B anchored while tracking");
        assert_eq!(
            cell_char(&early, a.x, a.y),
            IDENT[1].hollow,
            "{view:?} early"
        );
        let b = late.hero.anchor(1).expect("B anchored when locked");
        assert_eq!(cell_char(&late, b.x, b.y), IDENT[1].solid, "{view:?} late");
        assert!(lines(&early)
            .join("\n")
            .contains(&identity::tag(1, State::Tracking)));
        assert!(lines(&late)
            .join("\n")
            .contains(&identity::tag(1, State::Locked)));
    }
}

#[test]
fn anchor_mapping_is_total_every_visible_identity_has_a_site_in_every_view() {
    for t in [48.0, 100.0, 140.0, 200.0, 256.0] {
        let m = model_at(t, View::Trace);
        let e = m.obs.timeline.epoch(m.ep()).unwrap();
        for view in View::ALL {
            let mm = model_at(t, view);
            let f = compose::compose(&mm, 120, 40, false, BRAILLE);
            for s in 0..SLOTS {
                if e.slots[s].state.is_active()
                    && e.slots[s].state != State::RejectedCw
                    && e.slots[s].state != State::RejectedTransient
                {
                    assert!(
                        f.hero.anchor(s).is_some(),
                        "t={t} {view:?}: active identity {} has no site",
                        IDENT[s].letter
                    );
                }
            }
        }
    }
}

#[test]
fn identity_transport_moves_the_identities_between_representations() {
    let mut m = model_at(200.0, View::Sky);
    m.from = Some(View::Spectrum);
    m.trans = 0.5;
    let mid = compose::compose(&m, 120, 40, false, BRAILLE);
    assert!(mid.from_anchors.is_some());
    // the seam is drawn at the transport progress
    let seam = (0.5f32 * 0.5 * (3.0 - 2.0 * 0.5) * 118.0) as i32;
    let _ = seam;
    let row = lines(&mid)[5].clone();
    assert!(
        row.contains('\u{2502}'),
        "a transport seam must be visible: {row:?}"
    );
    // each persistent identity is drawn on the line between its old and new sites
    let old = views::render(
        View::Spectrum,
        &ctx(&m),
        Rect::new(0, 0, 120, mid.layout.hero.height),
    );
    let new = &mid.hero;
    for s in 0..3 {
        let (a, b) = (old.anchor(s).unwrap(), new.anchor(s).unwrap());
        let (mx, my) = ((a.x + b.x) as f32 / 2.0, (a.y + b.y) as f32 / 2.0);
        let (cx, cy) = (mx.round() as i32, my.round() as i32);
        let near: String = (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
            .map(|(dx, dy)| cell_char(&mid, cx + dx, cy + dy))
            .collect();
        assert!(
            near.contains(IDENT[s].solid),
            "{}: not found near its midpoint",
            IDENT[s].letter
        );
    }
    // at progress 1 (no transport) the frame is exactly the destination view
    m.from = None;
    m.trans = 1.0;
    let done = compose::compose(&m, 120, 40, false, BRAILLE);
    let plain = compose::compose(&model_at(200.0, View::Sky), 120, 40, false, BRAILLE);
    assert_eq!(lines(&done), lines(&plain));
}

// ───────────────────────── focus, compare and interaction ──────────────────────────

#[test]
fn comparing_pins_an_earlier_uncertain_state_against_the_later_interpretation() {
    let o = obs();
    let mut m = model_at(DURATION, View::Sky);
    m.sel = Some(0);
    m.mark = Some(60.0);
    m.compare = true;
    assert_eq!(m.ref_ep(), Some(epoch_at(60.0)));
    let then = o.timeline.epoch(epoch_at(60.0)).unwrap().slots[0];
    let now = o.timeline.epoch(N_EPOCHS).unwrap().slots[0];
    assert!(then.state != State::Locked && now.state == State::Locked);
    assert!(then.z < now.z);
    assert!(then.sky.unwrap().sigma_deg > now.sky.unwrap().sigma_deg);
    let note = views::compare_note(&ctx(&m), Some(0)).expect("a note");
    assert!(
        note.contains("COMPARE A") && note.contains("t=60s") && note.contains("mirror 50%"),
        "{note}"
    );
    // the frame differs from the same moment without the comparison
    let with = text(&m, 120, 40, false);
    m.compare = false;
    let without = text(&m, 120, 40, false);
    assert_ne!(with, without);
    assert!(with.contains("COMPARE"));
    // an earlier state is only a comparison if it is earlier
    m.compare = true;
    m.seek(40.0);
    assert_eq!(m.ref_ep(), None);
    // pressing c with no mark chooses an uncertain earlier moment itself
    let mut k = model_at(DURATION, View::Fold);
    k.key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::empty()));
    assert!(k.compare && k.ref_ep().is_some());
}

#[test]
fn detuning_the_period_changes_the_representation_but_not_the_analysis() {
    let m0 = model_at(200.0, View::Fold);
    let mut m1 = m0.clone();
    m1.sel = Some(0);
    let mut m2 = m1.clone();
    m2.nudge = 4;
    let (c1, c2) = (ctx(&m1), ctx(&m2));
    let (f1, _) = views::fold::profile_for(&c1, 0, c1.ep, 64, 0).unwrap();
    let (f2, _) = views::fold::profile_for(&c2, 0, c2.ep, 64, 4).unwrap();
    assert!(
        f2.snr() < 0.7 * f1.snr(),
        "detuned fold must be weaker: {} vs {}",
        f2.snr(),
        f1.snr()
    );
    assert_ne!(text(&m1, 120, 40, false), text(&m2, 120, 40, false));
    // the pipeline's published solution is untouched by the user's focus
    let lane = |m: &Model| {
        let f = compose::compose(m, 120, 40, false, BRAILLE);
        lines(&f)[f.layout.lanes_y as usize..(f.layout.lanes_y + f.layout.lanes_h) as usize]
            .to_vec()
    };
    assert_eq!(lane(&m1), lane(&m2));
}

#[test]
fn analysis_is_published_per_epoch_so_time_inside_an_epoch_changes_only_the_trace() {
    let a = model_at(100.0, View::Spectrum);
    let b = model_at(103.9, View::Spectrum);
    assert_eq!(a.ep(), b.ep());
    let ledger = |m: &Model| {
        let f = compose::compose(m, 120, 40, false, BRAILLE);
        lines(&f)[f.layout.lanes_y as usize..(f.layout.lanes_y + f.layout.lanes_h) as usize]
            .to_vec()
    };
    assert_eq!(ledger(&a), ledger(&b));
    // the TRACE view, by contrast, follows the exact time
    let (ta, tb) = (model_at(100.0, View::Trace), model_at(103.9, View::Trace));
    assert_ne!(text(&ta, 120, 40, false), text(&tb, 120, 40, false));
}

#[test]
fn keys_drive_focus_view_and_time_as_documented() {
    let key = |m: &mut Model, c: KeyCode| m.key(KeyEvent::new(c, KeyModifiers::empty()));
    let mut m = Model::new(obs());
    key(&mut m, KeyCode::Char('3'));
    assert_eq!(m.view, View::Fold);
    assert!(
        m.from.is_some() && m.trans == 0.0,
        "changing view starts an identity transport"
    );
    m.advance(1.0);
    assert!(m.from.is_none());
    key(&mut m, KeyCode::Tab);
    assert_eq!(m.view, View::Relate);
    m.key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
    assert_eq!(m.t, 16.0);
    m.key(KeyEvent::new(KeyCode::Left, KeyModifiers::empty()));
    assert_eq!(m.t, 12.0);
    key(&mut m, KeyCode::End);
    assert_eq!(m.t, DURATION);
    key(&mut m, KeyCode::Char('p'));
    assert!(m.t < DURATION);
    let t_event = m.t;
    assert!(m.obs.timeline.events.iter().any(|e| e.t == t_event));
    key(&mut m, KeyCode::Down);
    assert!(m.sel.is_some());
    let first = m.sel;
    key(&mut m, KeyCode::Down);
    assert_ne!(m.sel, first);
    key(&mut m, KeyCode::Char('z'));
    key(&mut m, KeyCode::Char('z'));
    key(&mut m, KeyCode::Char('z'));
    assert_eq!(m.zoom, 0, "zoom cycles through three levels");
    key(&mut m, KeyCode::Char('l'));
    assert!(m.cursor_on);
    key(&mut m, KeyCode::Char('r'));
    assert_eq!(m.t, 0.0);
    assert!(m.sel.is_none() && !m.cursor_on);
    key(&mut m, KeyCode::Char('q'));
    assert!(m.quit);
    // time can never leave the mission
    let mut e = Model::new(obs());
    e.seek(1e12);
    assert_eq!(e.t, DURATION);
    e.seek(f64::NAN);
    assert_eq!(e.t, DURATION);
    e.seek(-5.0);
    assert_eq!(e.t, 0.0);
}

// ───────────────────────── responsive layouts and Mono ─────────────────────────────

#[test]
fn the_layout_classes_follow_the_documented_thresholds() {
    assert_eq!(compose::classify(120, 40), Class::Wide);
    assert_eq!(compose::classify(80, 24), Class::Medium);
    assert_eq!(compose::classify(42, 15), Class::Tiny);
    for (w, h) in [
        (120u16, 40u16),
        (100, 32),
        (80, 24),
        (60, 19),
        (42, 15),
        (30, 10),
        (20, 8),
    ] {
        let l = compose::layout(w, h);
        assert!(l.hero.height >= 1);
        assert!(
            l.hero.y + l.hero.height <= l.lanes_y.max(l.hero.y + 1),
            "{w}x{h}: hero overlaps the ledger"
        );
        assert!(l.hint_y < h);
        // the hero keeps the lion's share on every class that has room for one
        if h >= 15 {
            assert!(
                l.hero.height as f64 >= 0.45 * h as f64,
                "{w}x{h}: hero {} of {}",
                l.hero.height,
                h
            );
        }
    }
}

#[test]
fn responsive_frames_fit_legibly_at_120x40_80x24_and_42x15() {
    for view in View::ALL {
        for (w, h) in [
            (120u16, 40u16),
            (80, 24),
            (42, 15),
            (100, 32),
            (60, 19),
            (30, 10),
            (22, 8),
        ] {
            for mono in [false, true] {
                let m = model_at(200.0, view);
                let f = compose::compose(&m, w, h, mono, BRAILLE);
                let ls = lines(&f);
                assert_eq!(ls.len(), h as usize);
                for (i, l) in ls.iter().enumerate() {
                    assert!(
                        l.chars().count() <= w as usize,
                        "{view:?} {w}x{h} row {i}: {} cols",
                        l.chars().count()
                    );
                }
                let all = ls.join("\n");
                // header names the active representation
                let tag = match f.layout.class {
                    Class::Tiny => format!("[{}]", view.short()),
                    _ => format!("[{} ", view.key()),
                };
                if w >= 30 {
                    assert!(all.contains(&tag), "{view:?} {w}x{h}: header lacks {tag}");
                }
                // the hero is dense: it owns most of the ink
                let hero_ink: usize = (f.layout.hero.y..f.layout.hero.y + f.layout.hero.height)
                    .map(|y| {
                        ls[y as usize]
                            .chars()
                            .filter(|c| !c.is_whitespace())
                            .count()
                    })
                    .sum();
                let total_ink: usize = ls
                    .iter()
                    .map(|l| l.chars().filter(|c| !c.is_whitespace()).count())
                    .sum();
                assert!(
                    hero_ink as f64 > 0.4 * total_ink as f64 && hero_ink > 30,
                    "{view:?} {w}x{h}: hero ink {hero_ink}/{total_ink}"
                );
            }
        }
    }
    // semantic zoom: wide keeps the ledger header and event ticker, tiny drops them but keeps the identities
    let wide = text(&model_at(200.0, View::Sky), 120, 40, false);
    assert!(
        wide.contains("FREQUENCY") && wide.contains("events") && wide.contains("INTERPRETATION")
    );
    let tiny = text(&model_at(200.0, View::Sky), 42, 15, false);
    assert!(!tiny.contains("FREQUENCY") && !tiny.contains("events"));
    for s in 0..3 {
        assert!(
            tiny.contains(&identity::tag(s, State::Locked)),
            "tiny keeps {}",
            IDENT[s].letter
        );
    }
}

#[test]
fn mono_carries_the_meaning_in_shape_and_density_not_colour() {
    for view in View::ALL {
        let m = model_at(200.0, view);
        let f = compose::compose(&m, 120, 40, true, BRAILLE);
        for y in 0..f.surface.height {
            for x in 0..f.surface.width {
                assert!(
                    f.surface.get(x, y).unwrap().style.bg.is_none(),
                    "{view:?}: a Mono frame must not paint backgrounds"
                );
            }
        }
        let t = lines(&f).join("\n");
        for s in 0..3 {
            assert!(
                t.contains(&identity::tag(s, State::Locked)),
                "{view:?}: {} unreadable in Mono",
                IDENT[s].letter
            );
        }
    }
    // the phase-time map and the sky glow use a shade ramp in Mono (4 levels)
    let fold = text(
        &{
            let mut m = model_at(200.0, View::Fold);
            m.sel = Some(2);
            m
        },
        120,
        40,
        true,
    );
    let shades = ['\u{2591}', '\u{2592}', '\u{2593}', '\u{2588}'];
    assert!(
        shades.iter().filter(|c| fold.contains(**c)).count() >= 3,
        "fold map needs several density levels"
    );
    // the same frame through the UI runtime in Mono
    let m = model_at(200.0, View::Sky);
    let via_ui = cli::capture_text(
        &m,
        &Capture {
            w: 80,
            h: 24,
            mono: true,
            color: false,
        },
        BRAILLE,
    )
    .unwrap();
    assert!(via_ui.contains("\u{25CF}A") && via_ui.contains("\u{25C6}B"));
}

#[test]
fn the_help_overlay_floats_without_reflowing_the_world() {
    let mut m = model_at(200.0, View::Sky);
    let base = compose::compose(&m, 120, 40, false, BRAILLE);
    m.help = true;
    let over = compose::compose(&m, 120, 40, false, BRAILLE);
    // editorial locality: rows outside the hero are untouched; the hero changes only inside the box
    let (a, b) = (lines(&base), lines(&over));
    let hero = base.layout.hero;
    for y in 0..40usize {
        if y < hero.y as usize || y >= (hero.y + hero.height) as usize {
            assert_eq!(a[y], b[y], "row {y} outside the hero changed");
        }
    }
    assert!(b
        .join("\n")
        .contains("PROJECT PULSAR \u{2014} three buried signals"));
    assert_eq!(
        base.hero.anchors, over.hero.anchors,
        "the world did not move"
    );
}

// ─────────────────────────────── CLI and the bounded demo ──────────────────────────

#[test]
fn capture_sizes_parse() {
    let c = cli::parse_size("120x40").unwrap();
    assert_eq!((c.w, c.h, c.mono, c.color), (120, 40, false, false));
    let c = cli::parse_size("42x15:mono").unwrap();
    assert!(c.mono && !c.color);
    let c = cli::parse_size("80x24:color").unwrap();
    assert!(c.color && !c.mono);
}

#[test]
fn the_demo_is_bounded_deterministic_and_covers_every_representation() {
    let o = cli::Options::default();
    let cap = Capture {
        w: 100,
        h: 34,
        mono: true,
        color: false,
    };
    let a = cli::demo_text(&o, &cap, obs()).expect("demo");
    let b = cli::demo_text(&o, &cap, obs()).expect("demo");
    assert_eq!(a, b, "the demo is a pure function of its inputs");
    assert_eq!(a.matches("\n=== ").count(), cli::TOUR.len() + 1);
    let digest = a.lines().last().unwrap();
    assert!(digest.starts_with("demo digest: ") && digest.len() == "demo digest: ".len() + 16);
    for v in View::ALL {
        assert!(
            a.contains(&format!("view={}", v.name())),
            "the tour must visit {}",
            v.name()
        );
    }
    assert!(a.contains("identity transport"));
    // the demo's own replay script reproduces its final state
    let script = a
        .lines()
        .rev()
        .nth(1)
        .unwrap()
        .trim_start_matches("replay script: ");
    let mut m = Model::new(obs());
    run_script(&mut m, script).unwrap();
    assert_eq!(m.t, DURATION);
}

#[test]
fn time_and_frequency_constants_are_self_consistent() {
    assert_eq!(sim::N, 16384);
    assert_eq!(DURATION, 256.0);
    assert_eq!(sim::EPOCH_SAMPLES as f64 / FS, EPOCH_S);
    assert_eq!(epoch_at(0.0), 0);
    assert_eq!(epoch_at(3.99), 0);
    assert_eq!(epoch_at(4.0), 1);
    assert_eq!(epoch_at(255.99), 63);
    assert_eq!(epoch_at(256.0), N_EPOCHS);
    assert_eq!(epoch_at(1e9), N_EPOCHS);
}

#[test]
fn no_size_the_terminal_can_take_panics_or_overflows_a_row() {
    let widths = [
        8u16, 9, 10, 12, 16, 20, 24, 30, 36, 42, 50, 60, 64, 80, 99, 100, 101, 120, 160, 220,
    ];
    let heights = [
        4u16, 5, 6, 7, 8, 9, 10, 12, 13, 15, 18, 19, 20, 24, 31, 32, 40, 70,
    ];
    let mut mid = Model::new(obs());
    mid.seek(200.0);
    for &w in &widths {
        for &h in &heights {
            for view in View::ALL {
                for (t, zoom, cursor, compare, help) in [
                    (200.0, 0u8, false, false, false),
                    (200.0, 2, true, true, true),
                    (0.0, 1, false, false, false),
                ] {
                    let mut m = mid.clone();
                    m.seek(t);
                    m.view = view;
                    m.zoom = zoom;
                    m.cursor_on = cursor;
                    m.cursor = (0.97, 0.03);
                    m.help = help;
                    if compare {
                        m.mark = Some(60.0);
                        m.compare = true;
                        m.sel = Some(2);
                    }
                    let f = compose::compose(&m, w, h, false, BRAILLE);
                    for (i, l) in lines(&f).iter().enumerate() {
                        assert!(
                            l.chars().count() <= w as usize,
                            "{view:?} {w}x{h} row {i} overflows"
                        );
                    }
                    assert_eq!(f.surface.width, w);
                    assert_eq!(f.surface.height, h);
                }
            }
        }
    }
}
