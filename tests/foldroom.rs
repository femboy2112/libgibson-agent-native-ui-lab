//! Independent geometric constraints and input-to-frame evidence for FOLDROOM.
use gibson::input::KeyEvent;
use gibson::{compute_diff, AnsiCompiler, ColorDepth, Surface, Vec3};
use gibson_ui_lab::foldroom::{Foldroom, Sequence, MAX_RASTER, PANEL_COUNT, TRIANGLE_COUNT};
use gibson_ui_lab::ui::{self, App, InputKey, InputRecord};

fn press(app: &mut Foldroom, key: InputKey) {
    app.key(0, key.event());
}
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.00002, "{a} != {b}");
}
fn point(a: Vec3, expected: [f32; 3]) {
    close(a.x, expected[0]);
    close(a.y, expected[1]);
    close(a.z, expected[2]);
}
fn distance(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}
fn on_edge(point: Vec3, a: Vec3, b: Vec3) -> bool {
    (distance(a, point) + distance(point, b) - distance(a, b)).abs() < 0.00002
}
fn same_surface(a: &Surface, b: &Surface) {
    assert_eq!((a.width, a.height), (b.width, b.height));
    assert_eq!(a.cells, b.cells);
}

#[test]
fn authored_flat_coordinates_and_known_closed_box() {
    let app = Foldroom::default();
    let flat = app.panels();
    let bounds = [
        [-1., 1., -0.75, 0.75],
        [-1., 1., -1.95, -0.75],
        [1., 2.2, -0.75, 0.75],
        [-1., 1., 0.75, 1.95],
        [-2.2, -1., -0.75, 0.75],
        [-1., 1., 1.95, 3.45],
        [-1.3, -1., -1.95, -0.75],
        [1., 1.3, -1.95, -0.75],
        [-1.3, -1., 0.75, 1.95],
        [1., 1.3, 0.75, 1.95],
    ];
    for (panel, [x0, x1, z0, z1]) in flat.iter().zip(bounds) {
        for (actual, expected) in
            panel
                .vertices
                .iter()
                .zip([[x0, 0., z0], [x1, 0., z0], [x1, 0., z1], [x0, 0., z1]])
        {
            point(*actual, expected);
        }
    }
    let expected = [
        [
            [-1., 0., -0.75],
            [1., 0., -0.75],
            [1., 0., 0.75],
            [-1., 0., 0.75],
        ],
        [
            [-1., 1.2, -0.75],
            [1., 1.2, -0.75],
            [1., 0., -0.75],
            [-1., 0., -0.75],
        ],
        [
            [1., 0., -0.75],
            [1., 1.2, -0.75],
            [1., 1.2, 0.75],
            [1., 0., 0.75],
        ],
        [
            [-1., 0., 0.75],
            [1., 0., 0.75],
            [1., 1.2, 0.75],
            [-1., 1.2, 0.75],
        ],
        [
            [-1., 1.2, -0.75],
            [-1., 0., -0.75],
            [-1., 0., 0.75],
            [-1., 1.2, 0.75],
        ],
        [
            [-1., 1.2, 0.75],
            [1., 1.2, 0.75],
            [1., 1.2, -0.75],
            [-1., 1.2, -0.75],
        ],
        [
            [-1., 1.2, -0.45],
            [-1., 1.2, -0.75],
            [-1., 0., -0.75],
            [-1., 0., -0.45],
        ],
        [
            [1., 1.2, -0.75],
            [1., 1.2, -0.45],
            [1., 0., -0.45],
            [1., 0., -0.75],
        ],
        [
            [-1., 0., 0.45],
            [-1., 0., 0.75],
            [-1., 1.2, 0.75],
            [-1., 1.2, 0.45],
        ],
        [
            [1., 0., 0.75],
            [1., 0., 0.45],
            [1., 1.2, 0.45],
            [1., 1.2, 0.75],
        ],
    ];
    for paired in [false, true] {
        let mut folded = app.clone();
        if paired {
            press(&mut folded, InputKey::Char('s'));
        }
        press(&mut folded, InputKey::Char('c'));
        for (panel, vertices) in folded.panels().iter().zip(expected) {
            for (actual, expected) in panel.vertices.iter().zip(vertices) {
                point(*actual, expected);
            }
        }
    }
}

#[test]
fn rigid_edges_diagonals_and_shared_hinges_through_both_sequences() {
    let sizes: [(f32, f32); PANEL_COUNT] = [
        (2., 1.5),
        (2., 1.2),
        (1.2, 1.5),
        (2., 1.2),
        (1.2, 1.5),
        (2., 1.5),
        (0.3, 1.2),
        (0.3, 1.2),
        (0.3, 1.2),
        (0.3, 1.2),
    ];
    for paired in [false, true] {
        let mut app = Foldroom::default();
        if paired {
            press(&mut app, InputKey::Char('s'));
        }
        for _ in 0..=app.sequence().stages() * 18 {
            let panels = app.panels();
            for (panel, (width, height)) in panels.iter().zip(sizes) {
                assert!(panel.vertices.iter().all(|v| v.is_finite()));
                for (i, length) in [width, height, width, height].into_iter().enumerate() {
                    close(
                        distance(panel.vertices[i], panel.vertices[(i + 1) % 4]),
                        length,
                    );
                }
                close(
                    distance(panel.vertices[0], panel.vertices[2]),
                    (width * width + height * height).sqrt(),
                );
                if let (Some(parent), Some(hinge)) = (panel.parent, panel.hinge) {
                    for endpoint in hinge {
                        for polygon in [panel, &panels[parent]] {
                            assert!(
                                (0..4).any(|i| on_edge(
                                    endpoint,
                                    polygon.vertices[i],
                                    polygon.vertices[(i + 1) % 4]
                                )),
                                "lost hinge panel {} parent {}",
                                panel.id,
                                parent
                            );
                        }
                    }
                }
            }
            press(&mut app, InputKey::Right);
        }
    }
}

#[test]
fn counterfactual_controls_change_the_model_and_camera_independently() {
    let base = Foldroom::default();
    let mut around = base.clone();
    press(&mut around, InputKey::Char(']'));
    assert_eq!(around.angles(), [0, 90, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut paired = base.clone();
    press(&mut paired, InputKey::Char('s'));
    press(&mut paired, InputKey::Char(']'));
    assert_eq!(paired.sequence(), Sequence::Paired);
    assert_eq!(paired.angles(), [0, 0, 0, 0, 0, 0, 90, 90, 90, 90]);
    assert_ne!(around.panels(), paired.panels());
    press(&mut around, InputKey::Left);
    assert_eq!(around.angles()[1], 85);
    press(&mut around, InputKey::Right);
    assert_eq!(around.angles()[1], 90);
    let model = around.panels();
    let original_camera = around.camera();
    let before_orbit = around.model_view(110, 60, false).raster.raster;
    press(&mut around, InputKey::Char('d'));
    assert_ne!(around.camera(), original_camera);
    assert_ne!(
        around.model_view(110, 60, false).raster.raster.pixels(),
        before_orbit.pixels()
    );
    assert_eq!(around.panels(), model);
    press(&mut around, InputKey::Char('6'));
    let selected_model = around.panels();
    assert_eq!(around.selected(), 5);
    press(&mut around, InputKey::Enter);
    assert_eq!(around.panels(), selected_model);
    let display = around.display_panels();
    for id in 0..PANEL_COUNT {
        assert_eq!(display[id] == model[id], id != 5);
    }
    assert!(
        compute_diff(
            Some(&base.frame(0, 120, 32, ColorDepth::TrueColor)),
            &around.frame(0, 120, 32, ColorDepth::TrueColor)
        )
        .exact_changed_cell_count()
            > 50
    );
}

#[test]
fn selected_face_identity_survives_occlusion_orbit_and_inspection_separation() {
    let mut app = Foldroom::default();
    press(&mut app, InputKey::Char('1'));
    press(&mut app, InputKey::Char('c'));
    assert_eq!(app.selected(), 0);
    let hidden = app.model_view(110, 60, false);
    assert!(
        !hidden.labels.iter().any(|l| l.panel == 0),
        "base should be occluded by closed lid"
    );
    assert!(ui::plain(&app.frame(0, 120, 32, ColorDepth::Mono)).contains("SELECTED 1  BASE"));
    assert!(ui::plain(&app.frame(0, 120, 32, ColorDepth::Mono)).contains("hidden/edge-on"));
    assert!(
        hidden.labels.iter().any(|l| l.panel == 1),
        "front camera must show front wall"
    );
    assert!(
        !hidden.labels.iter().any(|l| l.panel == 3),
        "front camera must hide back wall"
    );
    for _ in 0..18 {
        press(&mut app, InputKey::Char('d'));
    }
    let back_view = app.model_view(110, 60, false);
    assert!(
        !back_view.labels.iter().any(|l| l.panel == 1),
        "back camera must hide front wall"
    );
    assert!(
        back_view.labels.iter().any(|l| l.panel == 3),
        "back camera must show back wall"
    );
    for _ in 0..36 {
        press(&mut app, InputKey::Char('a'));
        assert_eq!(app.selected(), 0);
        assert_eq!(app.panels()[app.selected()].name, "BASE");
    }
    press(&mut app, InputKey::Char('6'));
    press(&mut app, InputKey::Char('e'));
    assert_eq!(app.selected(), 5);
    assert_eq!(app.display_panels()[5].id, 5);
    assert!(app
        .model_view(110, 60, false)
        .labels
        .iter()
        .any(|l| l.panel == 5));
    press(&mut app, InputKey::BackTab);
    assert_eq!(app.selected(), 4);
    press(&mut app, InputKey::Tab);
    assert_eq!(app.selected(), 5);
    press(&mut app, InputKey::Char('e'));
    for _ in 0..26 {
        press(&mut app, InputKey::Char('x'));
    }
    let underside = app.model_view(110, 60, false);
    assert!(underside.labels.iter().any(|l| l.panel == 0));
    assert!(!underside.labels.iter().any(|l| l.panel == 5));
    assert_eq!(app.selected(), 5);
}

fn trace() -> Vec<InputRecord> {
    [
        (13, InputKey::Char('6')),
        (47, InputKey::Char(']')),
        (163, InputKey::Right),
        (809, InputKey::Char('d')),
        (811, InputKey::Char('w')),
        (1229, InputKey::Char('s')),
        (1883, InputKey::Char(']')),
        (1911, InputKey::Tab),
        (2447, InputKey::Enter),
        (3083, InputKey::Char('c')),
        (4001, InputKey::Up),
        (5027, InputKey::Char('a')),
        (7003, InputKey::Char('f')),
        (9007, InputKey::Char('e')),
    ]
    .into_iter()
    .map(|(at_ms, key)| InputRecord { at_ms, key })
    .collect()
}

#[test]
fn full_recorded_input_state_and_frame_replay_at_irregular_times_all_sizes_depths() {
    let recording = serde_json::to_vec(&trace()).unwrap();
    let records: Vec<InputRecord> = serde_json::from_slice(&recording).unwrap();
    let times = [
        0, 12, 13, 46, 48, 164, 811, 1230, 1884, 1912, 2448, 3083, 4002, 5028, 7004, 9008, 17003,
    ];
    let mut live = Foldroom::default();
    let mut consumed = 0;
    for at in times {
        while consumed < records.len() && records[consumed].at_ms <= at {
            live.key(records[consumed].at_ms, records[consumed].key.event());
            consumed += 1;
        }
        let replayed = ui::replay(Foldroom::default(), &records, at).unwrap();
        assert_eq!(
            serde_json::to_value(&live).unwrap(),
            serde_json::to_value(&replayed).unwrap()
        );
        let state = live.clone();
        for (w, h) in [(56, 24), (80, 24), (120, 32), (160, 40)] {
            for depth in [ColorDepth::TrueColor, ColorDepth::Mono] {
                let rendered = live.frame(at, w, h, depth);
                same_surface(&rendered, &replayed.frame(at, w, h, depth));
                assert_eq!((rendered.width, rendered.height), (w, h));
                let text = ui::plain(&rendered);
                assert!(text.contains("FOLDROOM") && text.contains("SELECTED"));
                if depth == ColorDepth::Mono {
                    assert!(rendered
                        .cells
                        .iter()
                        .all(|c| c.style.fg.is_none() && c.style.bg.is_none()));
                    assert!(rendered.cells.iter().any(|c| c
                        .glyph
                        .grapheme
                        .chars()
                        .any(|ch| ('\u{2801}'..='\u{28ff}').contains(&ch))));
                }
                let frozen = live.frame(at + 431, w, h, depth);
                let diff = compute_diff(Some(&rendered), &frozen);
                assert_eq!(diff.exact_changed_cell_count(), 0);
                assert_eq!(diff.affected_cell_count(), 0);
                assert_eq!(AnsiCompiler::new().compile(&diff).len(), 0);
            }
            assert_eq!(live, state, "resize mutated semantic state");
        }
    }
}

#[test]
fn bounded_resources_input_limits_and_extreme_small_viewports() {
    let mut app = Foldroom::default();
    for _ in 0..500 {
        press(&mut app, InputKey::Right);
        press(&mut app, InputKey::Char('w'));
    }
    assert_eq!(app.progress(), 540);
    assert_eq!(app.orbit().1, 85);
    assert_eq!(TRIANGLE_COUNT, 20);
    let view = app.model_view(u16::MAX, u16::MAX, false);
    assert_eq!(
        (view.raster.raster.width(), view.raster.raster.height()),
        MAX_RASTER
    );
    assert_eq!(view.raster.stats.triangles_submitted, 20);
    assert!(view.raster.stats.triangles_drawn > 0);
    for (w, h) in [
        (0, 0),
        (1, 1),
        (3, 2),
        (12, 5),
        (240, 80),
        (u16::MAX, u16::MAX),
    ] {
        let frame = app.frame(0, w, h, ColorDepth::Mono);
        assert_eq!((frame.width, frame.height), ui::dimensions(w, h));
    }
    let entry = InputRecord {
        at_ms: 0,
        key: InputKey::Right,
    };
    assert!(ui::replay(Foldroom::default(), &vec![entry.clone(); 128], 1).is_ok());
    assert!(ui::replay(Foldroom::default(), &vec![entry; 129], 1).is_err());
    let invalid = [
        InputRecord {
            at_ms: 2,
            key: InputKey::Right,
        },
        InputRecord {
            at_ms: 1,
            key: InputKey::Left,
        },
    ];
    assert!(ui::replay(Foldroom::default(), &invalid, 10).is_err());
    let key: KeyEvent = InputKey::Char('r').event();
    app.key(0, key);
    assert_eq!(app, Foldroom::default());
}
