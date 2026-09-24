use gibson::ansi::AnsiCompiler;
use gibson::diff::compute_diff;
use gibson::input::{KeyCode, KeyEvent, KeyModifiers};
use gibson::{ColorDepth, Surface};
use gibson_ui_lab::cuebox::{self, Branch, Cuebox, Point, MODAL_FOCUS, PERFORMER_IDS};
use gibson_ui_lab::ui::{self, App, InputKey, InputRecord};

fn key(app: &mut Cuebox, at: u64, k: InputKey) {
    app.key(at, k.event());
}
fn same_frame(a: &Surface, b: &Surface) {
    assert_eq!((a.width, a.height), (b.width, b.height));
    assert_eq!(a.cells, b.cells);
}

#[test]
fn independently_expected_twelve_cue_order_and_endpoints() {
    // Authored expected sequence, not derived from the consumer's cue table.
    let on_time = [
        "cue01", "cue02", "cue03", "cue04", "cue05-on", "cue06", "cue07", "cue08", "cue09",
        "cue10", "cue11", "cue12",
    ];
    let delayed = [
        "cue01",
        "cue02",
        "cue03",
        "cue04",
        "cue05-delay",
        "cue06",
        "cue07",
        "cue08",
        "cue09",
        "cue10",
        "cue11",
        "cue12",
    ];
    for (branch, expected) in [(Branch::OnTime, on_time), (Branch::Delayed, delayed)] {
        cuebox::rehearsal_story(branch).validate().unwrap();
        let end = cuebox::sample(900, branch);
        assert_eq!(end.entered, expected);
        assert_eq!(end.update_count, 900);
        assert_eq!(end.cue, 11);
        assert_eq!(end.title, "Curtain");
        let endpoints: Vec<_> = end.performers.iter().map(|p| p.position).collect();
        assert_eq!(
            endpoints,
            [
                Point(30, 15),
                Point(68, 15),
                Point(30, 44),
                Point(70, 44),
                Point(30, 75),
                Point(70, 75),
                Point(42, 82),
                Point(58, 82)
            ]
        );
        assert_eq!(end.lights, [false; 4]);
        for (i, expected_id) in expected.iter().enumerate() {
            assert_eq!(cuebox::sample((i * 75) as u16, branch).cue_id, *expected_id);
        }
    }
    let start = cuebox::sample(0, Branch::OnTime);
    assert_eq!(
        start
            .performers
            .iter()
            .map(|p| p.position)
            .collect::<Vec<_>>(),
        [
            Point(12, 15),
            Point(85, 15),
            Point(12, 40),
            Point(85, 40),
            Point(12, 65),
            Point(85, 65),
            Point(40, 88),
            Point(94, 88)
        ]
    );
}

#[test]
fn explicit_delayed_entrance_is_a_real_counterfactual_and_reconverges() {
    let normal = Cuebox::default();
    let mut late = normal.clone();
    key(&mut late, 1237, InputKey::Char('d'));
    let a = normal.rehearsal(17_500);
    let b = late.rehearsal(17_500);
    assert_eq!(
        (a.cue_id.as_str(), b.cue_id.as_str()),
        ("cue05-on", "cue05-delay")
    );
    assert_eq!(a.performers[7].position, Point(70, 84));
    assert_eq!(b.performers[7].position, Point(94, 88));
    assert!(!a.performers[7].in_wing);
    assert!(b.performers[7].in_wing);
    assert!(a.lights[3]);
    assert!(!b.lights[3]);
    assert_eq!(&a.performers[..7], &b.performers[..7]);
    assert_eq!(
        normal.rehearsal(30_000).performers,
        late.rehearsal(30_000).performers
    );
    assert_ne!(
        normal.frame(17_500, 80, 24, ColorDepth::Mono).cells,
        late.frame(17_500, 80, 24, ColorDepth::Mono).cells
    );
    key(&mut late, 20_000, InputKey::Char('o'));
    assert_eq!(
        late.branch(),
        Branch::Delayed,
        "an executed entrance is committed"
    );
    let mut undo = Cuebox::default();
    key(&mut undo, 20, InputKey::Char('d'));
    key(&mut undo, 30, InputKey::Char('o'));
    assert_eq!(undo.rehearsal(17_500), normal.rehearsal(17_500));
}

#[test]
fn modal_capture_blocks_commands_and_restores_the_exact_performer() {
    let mut app = Cuebox::default();
    key(&mut app, 77, InputKey::Char('7'));
    key(&mut app, 101, InputKey::Enter);
    assert_eq!(app.focused(), Some(MODAL_FOCUS));
    assert!(app.focus_captured());
    for k in [
        InputKey::Tab,
        InputKey::BackTab,
        InputKey::Char('1'),
        InputKey::Char('d'),
        InputKey::Char('r'),
        InputKey::Char('n'),
        InputKey::Char('p'),
    ] {
        key(&mut app, 151, k);
    }
    assert_eq!(app.selected(), 6);
    assert_eq!(app.branch(), Branch::OnTime);
    assert!(app.playing());
    assert_eq!(app.focused(), Some(MODAL_FOCUS));
    for (w, h) in [(56, 24), (160, 40), (80, 24), (120, 32)] {
        let frozen = app.clone();
        app.frame(1137, w, h, ColorDepth::Mono);
        assert_eq!(app, frozen);
    }
    key(&mut app, 199, InputKey::Enter);
    assert!(!app.modal_open());
    assert!(!app.focus_captured());
    assert_eq!(app.focused(), Some(PERFORMER_IDS[6]));
    key(&mut app, 200, InputKey::Tab);
    assert_eq!(app.focused(), Some(PERFORMER_IDS[7]));
    key(&mut app, 201, InputKey::Tab);
    assert_eq!(
        app.focused(),
        Some(PERFORMER_IDS[0]),
        "modal ID is not in normal traversal"
    );
}

#[test]
fn play_step_and_back_use_explicit_transport() {
    let mut app = Cuebox::default();
    key(&mut app, 321, InputKey::Char('p'));
    assert_eq!(app.tick_at(1000), 6);
    key(&mut app, 1003, InputKey::Right);
    assert_eq!(app.rehearsal(15_000).cue_id, "cue02");
    assert_eq!(app.tick_at(15_000), 75);
    key(&mut app, 2007, InputKey::Right);
    assert_eq!(app.rehearsal(2007).cue_id, "cue03");
    key(&mut app, 2999, InputKey::Left);
    assert_eq!(app.rehearsal(2999).cue_id, "cue02");
    key(&mut app, 4001, InputKey::Char('p'));
    assert_eq!(app.tick_at(4051), 76);
    for _ in 0..128 {
        key(&mut app, 41_000, InputKey::Char('n'));
    }
    assert_eq!(app.tick_at(u64::MAX), 900);
}

#[test]
fn stable_focus_and_scene_ids_survive_reflow() {
    let mut app = Cuebox::default();
    key(&mut app, 0, InputKey::Char('8'));
    let state = app.rehearsal(17_500);
    let expected = app.scene_ids(&state, 56, 13);
    for (w, h) in [(56, 24), (80, 24), (120, 32), (160, 40)] {
        assert_eq!(app.scene_ids(&state, w, h), expected);
        for depth in [ColorDepth::TrueColor, ColorDepth::Mono] {
            app.frame(17_500, w, h, depth);
            assert_eq!(app.focused(), Some(PERFORMER_IDS[7]));
        }
    }
    let unique: std::collections::BTreeSet<_> = expected.into_iter().collect();
    assert_eq!(unique.len(), 8);
}

#[test]
fn exact_key_replay_is_independent_of_irregular_render_schedule() {
    let inputs = vec![
        InputRecord {
            at_ms: 117,
            key: InputKey::Char('d'),
        },
        InputRecord {
            at_ms: 983,
            key: InputKey::Tab,
        },
        InputRecord {
            at_ms: 1357,
            key: InputKey::Enter,
        },
        InputRecord {
            at_ms: 1739,
            key: InputKey::Char('n'),
        }, // captured
        InputRecord {
            at_ms: 2099,
            key: InputKey::Enter,
        },
        InputRecord {
            at_ms: 3331,
            key: InputKey::Char('p'),
        },
        InputRecord {
            at_ms: 4513,
            key: InputKey::Char('n'),
        },
        InputRecord {
            at_ms: 4699,
            key: InputKey::Char('n'),
        },
        InputRecord {
            at_ms: 5107,
            key: InputKey::Char('p'),
        },
        InputRecord {
            at_ms: 8819,
            key: InputKey::Char('8'),
        },
        InputRecord {
            at_ms: 22_731,
            key: InputKey::Enter,
        },
        InputRecord {
            at_ms: 24_999,
            key: InputKey::Char('i'),
        },
    ];
    let decoded: Vec<InputRecord> =
        serde_json::from_slice(&serde_json::to_vec(&inputs).unwrap()).unwrap();
    let mut live = Cuebox::default();
    let mut cursor = 0;
    for at in [
        0, 13, 117, 501, 1291, 1499, 2111, 3501, 4503, 4789, 7551, 17003, 23001, 39017, 45000,
    ] {
        while cursor < inputs.len() && inputs[cursor].at_ms <= at {
            let event = &inputs[cursor];
            live.key(event.at_ms, event.key.event());
            cursor += 1;
        }
        let replayed = ui::replay(Cuebox::default(), &decoded, at).unwrap();
        assert_eq!(
            live, replayed,
            "exact presentation and transport state at {at}"
        );
        assert_eq!(live.rehearsal(at), replayed.rehearsal(at));
        for (w, h) in [(56, 24), (80, 24), (120, 32), (160, 40)] {
            for depth in [ColorDepth::TrueColor, ColorDepth::Mono] {
                let before = live.clone();
                same_frame(
                    &live.frame(at, w, h, depth),
                    &replayed.frame(at, w, h, depth),
                );
                assert_eq!(live, before, "frame must be pure");
            }
        }
    }
}

#[test]
fn all_requested_sizes_are_legible_in_color_and_mono() {
    let mut app = Cuebox::default();
    key(&mut app, 100, InputKey::Char('d'));
    for (w, h) in [(56, 24), (80, 24), (120, 32), (160, 40)] {
        for depth in [ColorDepth::TrueColor, ColorDepth::Mono] {
            let frame = app.frame(17_500, w, h, depth);
            assert_eq!((frame.width, frame.height), (w, h));
            assert_eq!(frame.cells.len(), usize::from(w) * usize::from(h));
            let text = ui::plain(&frame);
            for required in [
                "CUEBOX",
                "STAGE / Q05",
                "CUES / Q05",
                "Hold Hana",
                "DELAYED",
                "L1/",
                "L2\\",
                "L3.",
                "L4:",
                "###",
                "===",
                "<A>",
                "[B]",
                "[C]",
                "[D]",
                "[E]",
                "[F]",
                "[G]",
                "[H]",
            ] {
                assert!(
                    text.contains(required),
                    "missing {required} at {w}x{h} {depth:?}\n{text}"
                );
            }
            if depth == ColorDepth::Mono {
                assert!(frame
                    .cells
                    .iter()
                    .all(|c| c.style.fg.is_none() && c.style.bg.is_none()));
            }
        }
    }
}

#[test]
fn paused_frames_have_exact_zero_change_footprint_and_bytes() {
    let mut app = Cuebox::default();
    key(&mut app, 1733, InputKey::Char('p'));
    for (w, h) in [(56, 24), (80, 24), (120, 32), (160, 40)] {
        for depth in [ColorDepth::TrueColor, ColorDepth::Mono] {
            let first = app.frame(1733, w, h, depth);
            let frozen = app.frame(44_999, w, h, depth);
            same_frame(&first, &frozen);
            let diff = compute_diff(Some(&first), &frozen);
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

#[test]
fn resource_and_input_bounds_are_explicit() {
    let app = Cuebox::default();
    assert_eq!(app.end_ms(), 45_000);
    assert_eq!(app.rehearsal(u64::MAX).tick, 900);
    assert_eq!(cuebox::sample(u16::MAX, Branch::Delayed).update_count, 900);
    for (w, h) in [
        (0, 0),
        (0, 24),
        (56, 0),
        (1, 1),
        (29, 13),
        (u16::MAX, u16::MAX),
    ] {
        let frame = app.frame(u64::MAX, w, h, ColorDepth::Mono);
        assert!(frame.width <= 240 && frame.height <= 80);
        assert!(frame.cells.len() <= 19_200);
    }
    let bad = vec![
        InputRecord {
            at_ms: 0,
            key: InputKey::Tab
        };
        129
    ];
    assert!(ui::replay(Cuebox::default(), &bad, u64::MAX).is_err());
    let reversed = [
        InputRecord {
            at_ms: 2,
            key: InputKey::Tab,
        },
        InputRecord {
            at_ms: 1,
            key: InputKey::Tab,
        },
    ];
    assert!(ui::replay(Cuebox::default(), &reversed, u64::MAX).is_err());
    let control = [InputRecord {
        at_ms: 0,
        key: InputKey::Char('\x1b'),
    }];
    assert!(ui::replay(Cuebox::default(), &control, u64::MAX).is_err());
    let mut protected = app.clone();
    protected.key(50, KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
    assert_eq!(protected, app);
}

#[test]
fn actual_stage_cells_show_branch_and_lighting_not_only_labels() {
    let normal = Cuebox::default();
    let mut delayed = normal.clone();
    key(&mut delayed, 117, InputKey::Char('d'));
    for depth in [ColorDepth::TrueColor, ColorDepth::Mono] {
        let on = normal.frame(17_500, 56, 24, depth);
        let late = delayed.frame(17_500, 56, 24, depth);
        let glyph = |s: &Surface, x, y| s.get(x, y).unwrap().glyph.grapheme.to_string();
        // Independently calculated coordinates for the compact stage geometry:
        // 56x13 stage begins at y=2; Hana has moved on the normal route only.
        assert_eq!(glyph(&on, 37, 10), "H");
        assert_eq!(glyph(&late, 49, 11), "H");
        assert_ne!(glyph(&late, 37, 10), "H");
        assert_ne!(glyph(&on, 49, 11), "H");
        // Actual floor field, away from headers, actors, and selected path.
        assert_eq!(glyph(&late, 1, 4), "/");
        assert_eq!(glyph(&late, 31, 4), "\\");
        assert_eq!(glyph(&on, 31, 10), ":");
        assert_eq!(glyph(&late, 31, 10), " ");
    }
    key(&mut delayed, 17_501, InputKey::Char('8'));
    key(&mut delayed, 17_503, InputKey::Enter);
    for (w, h) in [(56, 24), (80, 24), (120, 32), (160, 40)] {
        for depth in [ColorDepth::TrueColor, ColorDepth::Mono] {
            let text = ui::plain(&delayed.frame(17_505, w, h, depth));
            for expected in [
                "CUE CARD / Q05",
                "Hold Hana in wing",
                "Branch: DELAYED",
                "Performer [H] Hana",
                "Position 94,88",
                "restores actor focus",
            ] {
                assert!(
                    text.contains(expected),
                    "{expected} missing at {w}x{h}:\n{text}"
                );
            }
        }
    }
}
