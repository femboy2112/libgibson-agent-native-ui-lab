use gibson::{compute_diff, AnsiCompiler, ColorDepth, KeyEvent, Surface};
use gibson_ui_lab::ui::{self, App, InputKey, InputRecord};
use gibson_ui_lab::weavebench::{Draft, FloatSummary, Focus, Weavebench, REPEAT, ROW_MS, WEAVE_MS};

const SIZES: [(u16, u16); 4] = [(56, 24), (80, 24), (120, 32), (160, 40)];
const DEPTHS: [ColorDepth; 2] = [ColorDepth::TrueColor, ColorDepth::Mono];

// Build shaft membership lists first, then dispatch picks through those lists.
// This reference never calls the consumer's lift function or cloth builder.
fn lift_reference(draft: &Draft) -> [[bool; REPEAT]; REPEAT] {
    let lifted: [Vec<usize>; 4] = std::array::from_fn(|treadle| {
        let mut mask = draft.tie_up[treadle];
        let mut shafts = Vec::new();
        for shaft in 0..4 {
            if mask % 2 == 1 {
                shafts.push(shaft);
            }
            mask /= 2;
        }
        shafts
    });
    std::array::from_fn(|row| {
        std::array::from_fn(|col| {
            lifted[draft.treadling[row] as usize].contains(&(draft.threading[col] as usize))
        })
    })
}

// Enumerate every cyclic interval and test all its cells, rather than doubling
// the sequence and accumulating runs as the implementation does.
fn float_reference(cloth: [[bool; REPEAT]; REPEAT]) -> FloatSummary {
    let mut result = FloatSummary::default();
    for (strand, weft_row) in cloth.iter().enumerate() {
        for start in 0..REPEAT {
            for length in 1..=REPEAT {
                if (0..length).all(|offset| cloth[(start + offset) % REPEAT][strand]) {
                    result.warp = result.warp.max(length as u8);
                }
                if (0..length).all(|offset| !weft_row[(start + offset) % REPEAT]) {
                    result.weft = result.weft.max(length as u8);
                }
            }
        }
    }
    result
}

#[test]
fn lift_matches_independent_boolean_membership_and_known_twill() {
    let baseline = Draft::default();
    assert_eq!(
        baseline.cloth()[0],
        [
            true, true, false, false, true, true, false, false, true, true, false, false, true,
            true, false, false
        ]
    );
    for profile in 0..3 {
        for treadle in 0..4 {
            for mask in 0..16 {
                let mut draft = Draft::default();
                for i in 0..REPEAT {
                    draft.threading[i] = ((i * (profile + 1) + profile) % 4) as u8;
                    draft.treadling[i] = ((i / (profile + 1) + profile) % 4) as u8;
                }
                draft.tie_up[treadle] = mask;
                assert_eq!(draft.cloth(), lift_reference(&draft));
            }
        }
    }
}

#[test]
fn cyclic_float_oracle_covers_wrap_and_solid_strands() {
    assert_eq!(Draft::default().floats(), FloatSummary { warp: 2, weft: 2 });
    let mut draft = Draft {
        threading: [0; REPEAT],
        treadling: [1; REPEAT],
        tie_up: [1, 0, 0, 0],
    };
    for row in [14, 15, 0, 1] {
        draft.treadling[row] = 0;
    }
    assert_eq!(draft.floats().warp, 4);
    for mask in 0..16 {
        draft.tie_up = [mask, mask.rotate_left(1) & 15, 15 - mask, mask ^ 5];
        draft.threading = std::array::from_fn(|i| ((i * 3 + i / 3) % 4) as u8);
        draft.treadling = std::array::from_fn(|i| ((i + i / 5) % 4) as u8);
        assert_eq!(draft.floats(), float_reference(lift_reference(&draft)));
    }
    for mask in [0, 15] {
        draft.tie_up = [mask; 4];
        assert_eq!(draft.floats(), float_reference(lift_reference(&draft)));
        assert_eq!(
            draft.floats(),
            if mask == 0 {
                FloatSummary { warp: 0, weft: 16 }
            } else {
                FloatSummary { warp: 16, weft: 0 }
            }
        );
    }
}

#[test]
fn tie_up_edit_changes_exactly_its_dependent_crossings() {
    for shaft in 0..4 {
        for treadle in 0..4 {
            let mut app = Weavebench::default();
            app.focus = Focus::TieUp;
            app.tie_cursor = (shaft, treadle);
            let before = app.draft.cloth();
            app.key(137, InputKey::Enter.event());
            let after = app.draft.cloth();
            for row in 0..REPEAT {
                for col in 0..REPEAT {
                    assert_eq!(
                        before[row][col] != after[row][col],
                        app.draft.threading[col] as usize == shaft
                            && app.draft.treadling[row] as usize == treadle
                    );
                }
            }
            assert_eq!(after, lift_reference(&app.draft));
        }
    }
}

#[test]
fn threading_and_treadling_edits_change_the_exact_expected_cells() {
    for focus in [Focus::Threading, Focus::Treadling] {
        for selected in 0..REPEAT {
            let mut app = Weavebench::default();
            app.focus = focus;
            app.selected = (selected, selected);
            let before = app.draft.cloth();
            let mut expected_draft = app.draft.clone();
            if focus == Focus::Threading {
                expected_draft.threading[selected] = ((selected + 1) % 4) as u8;
            } else {
                expected_draft.treadling[selected] = ((selected + 1) % 4) as u8;
            }
            let expected = lift_reference(&expected_draft);
            app.key(81, InputKey::Enter.event());
            let after = app.draft.cloth();
            assert_eq!(after, expected);
            for row in 0..REPEAT {
                for col in 0..REPEAT {
                    assert_eq!(
                        before[row][col] != after[row][col],
                        before[row][col] != expected[row][col]
                    );
                    if (focus == Focus::Threading && col != selected)
                        || (focus == Focus::Treadling && row != selected)
                    {
                        assert_eq!(before[row][col], after[row][col]);
                    }
                }
            }
        }
    }
}

#[test]
fn shuttle_progress_pause_step_restart_are_explicit_and_bounded() {
    let mut app = Weavebench::default();
    for row in 0..=REPEAT {
        assert_eq!(app.progress(row as u64 * ROW_MS).rows, row);
    }
    assert_eq!(app.progress(150).columns, 4);
    assert_eq!(app.progress(u64::MAX).rows, 16);
    app.key(750, KeyEvent::char('w'));
    assert_eq!(app.elapsed_ms(90_000), 750);
    app.key(1777, KeyEvent::char('n'));
    assert_eq!(app.elapsed_ms(90_000), 1200);
    app.key(2501, KeyEvent::char('0'));
    assert_eq!(app.elapsed_ms(90_000), 0);
    app.key(2600, KeyEvent::char('w'));
    assert_eq!(app.elapsed_ms(2600 + WEAVE_MS), WEAVE_MS);
}

#[test]
fn following_viewport_keeps_all_sixteen_shuttle_passes_visible() {
    let app = Weavebench::default();
    for (w, h) in SIZES {
        for depth in DEPTHS {
            for row in 0..REPEAT {
                let frame = app.frame(row as u64 * ROW_MS + 225, w, h, depth);
                let inspector_w = if w >= 100 { 26 } else { 21 };
                let expected = if row % 2 == 0 { ">" } else { "<" };
                let visible = (3..h - 4).any(|y| {
                    (1..w - inspector_w - 2)
                        .any(|x| frame.get(x, y).unwrap().glyph.grapheme.as_str() == expected)
                });
                assert!(visible, "shuttle absent in pass {row} at {w}x{h}");
            }
        }
    }
    let mut pinned = app.clone();
    pinned.key(0, InputKey::Right.event());
    assert!(!pinned.follow_shuttle);
    pinned.key(0, KeyEvent::char('f'));
    assert!(pinned.follow_shuttle);
}

#[test]
fn focus_navigation_and_four_repeat_viewport_preserve_draft_identity() {
    let mut app = Weavebench::default();
    for focus in [
        Focus::Threading,
        Focus::TieUp,
        Focus::Treadling,
        Focus::Cloth,
    ] {
        app.key(0, InputKey::Tab.event());
        assert_eq!(app.focus, focus);
    }
    app.key(0, KeyEvent::char('r'));
    app.key(0, InputKey::Left.event());
    app.key(0, InputKey::Up.event());
    assert_eq!(app.selected, (31, 31));
    let draft = app.draft.clone();
    for (w, h) in SIZES {
        let text = ui::plain(&app.frame(WEAVE_MS, w, h, ColorDepth::Mono));
        assert!(text.contains("Warp 16 / Pick 16"));
        assert!(text.contains("4x repeat"));
    }
    app.key(0, KeyEvent::char('r'));
    assert_eq!(app.selected, (15, 15));
    assert_eq!(app.draft, draft);
}

fn trace() -> Vec<InputRecord> {
    use InputKey::*;
    let keys = [
        Char('w'),
        Tab,
        Right,
        Enter,
        Down,
        Tab,
        Right,
        Down,
        Enter,
        Tab,
        Down,
        Right,
        Enter,
        Tab,
        Char('r'),
        Left,
        Up,
        Char('n'),
        Char('w'),
        Right,
        Down,
        Char('w'),
        Char('0'),
        BackTab,
        Left,
        Char('n'),
        Char('w'),
    ];
    keys.into_iter()
        .enumerate()
        .map(|(i, key)| InputRecord {
            at_ms: (i * 193 + i * i * 7) as u64,
            key,
        })
        .collect()
}

#[test]
fn full_state_and_frames_replay_at_irregular_times_without_resize_mutation() {
    let inputs = trace();
    let mut live = Weavebench::default();
    for (index, input) in inputs.iter().enumerate() {
        live.key(input.at_ms, input.key.event());
        let query = input.at_ms + 53;
        let replayed = ui::replay(Weavebench::default(), &inputs[..=index], query).unwrap();
        assert_eq!(live, replayed);
        let snapshot = live.clone();
        let baseline = live.frame(query, 120, 32, ColorDepth::TrueColor);
        for (w, h) in SIZES {
            for depth in DEPTHS {
                assert_eq!(
                    live.frame(query, w, h, depth),
                    replayed.frame(query, w, h, depth)
                );
            }
        }
        assert_eq!(live.frame(query, 120, 32, ColorDepth::TrueColor), baseline);
        assert_eq!(live, snapshot);
    }
    let before_first = ui::replay(Weavebench::default(), &inputs[1..], 0).unwrap();
    assert_eq!(before_first, Weavebench::default());
    let encoded = serde_json::to_string(&inputs).unwrap();
    let decoded: Vec<InputRecord> = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        ui::replay(Weavebench::default(), &decoded, u64::MAX).unwrap(),
        live
    );
}

#[test]
fn input_budget_accepts_128_and_rejects_129_and_reversed_time() {
    let inputs: Vec<_> = (0..128)
        .map(|i| InputRecord {
            at_ms: i * 101,
            key: InputKey::Tab,
        })
        .collect();
    let replayed = ui::replay(Weavebench::default(), &inputs, u64::MAX).unwrap();
    assert_eq!(replayed.focus, Focus::Cloth);
    let mut excess = inputs.clone();
    excess.push(InputRecord {
        at_ms: 13_000,
        key: InputKey::Enter,
    });
    assert!(ui::replay(Weavebench::default(), &excess, u64::MAX).is_err());
    let mut reversed = inputs;
    reversed.swap(0, 1);
    assert!(ui::replay(Weavebench::default(), &reversed, u64::MAX).is_err());
}

fn dot(surface: &Surface, cell_origin: (u16, u16), x: u16, y: u16) -> bool {
    let cell = surface
        .get(cell_origin.0 + x / 2, cell_origin.1 + y / 4)
        .unwrap();
    let code = cell.glyph.grapheme.chars().next().unwrap() as u32;
    if !(0x2800..=0x28ff).contains(&code) {
        return false;
    }
    let bits = [[0, 1, 2, 6], [3, 4, 5, 7]];
    (code - 0x2800) & (1 << bits[(x % 2) as usize][(y % 4) as usize]) != 0
}

#[test]
fn enlarged_crossing_has_continuous_upper_and_interrupted_lower_in_both_modes() {
    for (w, h) in SIZES {
        for depth in DEPTHS {
            for warp_top in [false, true] {
                let mut app = Weavebench::default();
                app.draft.tie_up = [if warp_top { 15 } else { 0 }; 4];
                let frame = app.frame(WEAVE_MS, w, h, depth);
                assert!(ui::plain(&frame).contains(if warp_top { "W16+" } else { "F16+" }));
                let inspector_w = if w >= 100 { 26 } else { 21 };
                let origin = (w - inspector_w + 2, 5);
                let pixel_w = (inspector_w - 4) * 2;
                let (cx, cy) = (pixel_w / 2, 10);
                if warp_top {
                    assert!((0..20).all(|y| dot(&frame, origin, cx, y)));
                    assert!(!dot(&frame, origin, cx - 2, cy));
                    assert!(!dot(&frame, origin, cx + 2, cy));
                    assert!(dot(&frame, origin, 0, cy));
                    assert!(dot(&frame, origin, pixel_w - 1, cy));
                } else {
                    assert!((0..pixel_w).all(|x| dot(&frame, origin, x, cy)));
                    assert!(!dot(&frame, origin, cx, cy - 2));
                    assert!(!dot(&frame, origin, cx, cy + 2));
                    assert!(dot(&frame, origin, cx, 0));
                    assert!(dot(&frame, origin, cx, 19));
                }
            }
        }
    }
}

#[test]
fn primary_cloth_topology_differs_in_glyphs_and_keeps_both_lower_ends() {
    for (w, h) in SIZES {
        for depth in DEPTHS {
            let mut warp = Weavebench::default();
            warp.draft.tie_up = [15; 4];
            let mut weft = warp.clone();
            weft.draft.tie_up = [0; 4];
            let a = warp.frame(WEAVE_MS, w, h, depth);
            let b = weft.frame(WEAVE_MS, w, h, depth);
            let inspector_w = if w >= 100 { 26 } else { 21 };
            let cloth_w = w - inspector_w - 1;
            let cell_w = if cloth_w >= 70 { 4 } else { 3 };
            let pixel_w = cell_w * 2;
            let (cx, cy) = (pixel_w / 2, 4);
            let origin = (1, 3);
            assert!((0..8).all(|y| dot(&a, origin, cx, y)));
            assert!(!dot(&a, origin, cx - 1, cy));
            assert!(!dot(&a, origin, cx + 1, cy));
            assert!(dot(&a, origin, 0, cy));
            assert!(dot(&a, origin, pixel_w - 1, cy));
            assert!((0..pixel_w).all(|x| dot(&b, origin, x, cy)));
            assert!(!dot(&b, origin, cx, cy - 1));
            assert!(!dot(&b, origin, cx, cy + 1));
            assert!(dot(&b, origin, cx, 0));
            assert!(dot(&b, origin, cx, 7));
            let glyphs_differ = (0..2).any(|dy| {
                (0..cell_w).any(|dx| {
                    a.get(origin.0 + dx, origin.1 + dy).unwrap().glyph
                        != b.get(origin.0 + dx, origin.1 + dy).unwrap().glyph
                })
            });
            assert!(
                glyphs_differ,
                "primary topology collapsed at {w}x{h} {depth:?}"
            );
        }
    }
}

#[test]
fn four_sizes_two_modes_remain_useful_bounded_and_frozen_zero_zero_zero() {
    let mut app = Weavebench::default();
    for (w, h) in SIZES {
        for depth in DEPTHS {
            for focus in [
                Focus::Cloth,
                Focus::Threading,
                Focus::TieUp,
                Focus::Treadling,
            ] {
                app.focus = focus;
                let frame = app.frame(950, w, h, depth);
                assert_eq!((frame.width, frame.height), (w, h));
                let text = ui::plain(&frame);
                for required in [
                    "WEAVEBENCH",
                    "CLOTH",
                    "CROSSING",
                    "Max floats",
                    "Tab focus",
                    "w play/pause",
                ] {
                    assert!(text.contains(required), "missing {required} at {w}x{h}");
                }
                assert!(
                    frame
                        .cells
                        .iter()
                        .filter(|cell| cell
                            .glyph
                            .grapheme
                            .chars()
                            .any(|c| ('\u{2801}'..='\u{28ff}').contains(&c)))
                        .count()
                        > 100
                );
                if depth == ColorDepth::Mono {
                    assert!(frame
                        .cells
                        .iter()
                        .all(|cell| cell.style.fg.is_none() && cell.style.bg.is_none()));
                }
                let duplicate = app.frame(950, w, h, depth);
                let diff = compute_diff(Some(&frame), &duplicate);
                assert_eq!(
                    (
                        diff.exact_changed_cell_count(),
                        diff.affected_cell_count(),
                        AnsiCompiler::new().compile(&diff).len()
                    ),
                    (0, 0, 0)
                );
            }
        }
    }
    let early = app.frame(0, 80, 24, ColorDepth::Mono);
    let late = app.frame(WEAVE_MS, 80, 24, ColorDepth::Mono);
    assert!(compute_diff(Some(&early), &late).exact_changed_cell_count() > 100);
    assert!(
        early.cells.iter().filter(|c| c.style.dim).count()
            > late.cells.iter().filter(|c| c.style.dim).count()
    );
    let bounded = app.frame(u64::MAX, u16::MAX, u16::MAX, ColorDepth::Mono);
    assert_eq!(
        (bounded.width, bounded.height),
        (ui::MAX_WIDTH, ui::MAX_HEIGHT)
    );
    for (w, h) in [(0, 0), (1, 1), (8, 3), (39, 22)] {
        let _ = app.frame(0, w, h, ColorDepth::Mono);
    }
}
