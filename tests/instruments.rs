use gibson::{
    input::{KeyCode, KeyEvent, KeyModifiers},
    ColorDepth,
};
use gibson_ui_lab::{
    instruments::*,
    semantic_fixture as sem,
    ui::{self, App},
};

fn spec() -> InstrumentSpec {
    plan(&sem::snapshot(3000, &[]), 3000).remove(0)
}
fn patch(s: &InstrumentSpec) -> InstrumentPatch {
    InstrumentPatch {
        root: s.root.clone(),
        bindings: s.bindings.clone(),
        title: s.title.clone(),
    }
}
fn port() -> SessionPort {
    let mut p = SessionPort::default();
    p.advance(3000).unwrap();
    p.mount(spec()).unwrap();
    p
}
fn column(spec: &mut InstrumentSpec) -> &mut Vec<Element> {
    match &mut spec.root {
        Element::Column { children, .. } => children,
        _ => panic!(),
    }
}
fn reject(mutant: impl FnOnce(&mut InstrumentSpec)) {
    let mut s = spec();
    mutant(&mut s);
    let mut p = port();
    let old = p.clone();
    assert!(p.update("workspace", patch(&s)).is_err() || validate(&s, &p.read_snapshot()).is_err()); // metadata tested on mount below
    assert_eq!(p, old, "rejected update must be atomic");
    let mut fresh = SessionPort::default();
    fresh.advance(3000).unwrap();
    let old = fresh.clone();
    assert!(fresh.mount(s).is_err());
    assert_eq!(fresh, old);
}
#[test]
fn schema_and_allocation_bounds_precede_mount() {
    let state = sem::snapshot(3000, &[]);
    let good = serde_json::to_vec(&spec()).unwrap();
    assert_eq!(decode(&good, &state).unwrap(), spec());
    assert!(decode(&vec![b' '; MAX_JSON_BYTES + 1], &state)
        .unwrap_err()
        .contains("byte budget"));
    for input in [b"{".as_slice(), b"null", b"[]", b"{\"root\": 1}"] {
        assert!(decode(input, &state).is_err());
    }
    let mut v = serde_json::to_value(spec()).unwrap();
    v["native_code"] = serde_json::json!("not allowed");
    assert!(decode(&serde_json::to_vec(&v).unwrap(), &state).is_err());
    let mut v = serde_json::to_value(spec()).unwrap();
    v["bindings"][0]["action"]["Select"]["secret_authority"] = true.into();
    assert!(decode(&serde_json::to_vec(&v).unwrap(), &state).is_err());
    let mut v = serde_json::to_value(spec()).unwrap();
    v["budget"]["nodes"] = "64".into();
    assert!(decode(&serde_json::to_vec(&v).unwrap(), &state).is_err());
}
#[test]
fn identity_tree_depth_and_text_controls_are_rejected_atomically() {
    reject(|s| {
        column(s).push(Element::Text {
            id: "description".into(),
            text: "duplicate".into(),
        })
    });
    reject(|s| {
        let mut e = Element::Text {
            id: "leaf".into(),
            text: "x".into(),
        };
        for n in 0..MAX_DEPTH {
            e = Element::Column {
                id: format!("depth{n}"),
                children: vec![e],
            };
        }
        column(s).push(e)
    });
    reject(|s| {
        for n in 0..MAX_NODES {
            column(s).push(Element::Text {
                id: format!("extra{n}"),
                text: "x".into(),
            })
        }
    });
    reject(|s| {
        column(s).push(Element::Text {
            id: "escape".into(),
            text: "\u{1b}[2J".into(),
        })
    });
    reject(|s| {
        column(s).push(Element::Text {
            id: "carriage".into(),
            text: "hello\rworld".into(),
        })
    });
    reject(|s| {
        column(s).push(Element::Text {
            id: "large".into(),
            text: "x".repeat(2049),
        })
    });
}
#[test]
fn every_budget_fallback_and_replay_seed_is_enforced() {
    let mutate: [fn(&mut InstrumentSpec); 11] = [
        |s| s.budget.nodes = MAX_NODES + 1,
        |s| s.budget.depth = MAX_DEPTH + 1,
        |s| s.budget.raster_width = MAX_RASTER + 1,
        |s| s.budget.raster_height = MAX_RASTER + 1,
        |s| s.budget.effects = 1,
        |s| s.budget.animation_hz = MAX_RATE + 1,
        |s| s.budget.animation_hz = 1,
        |s| s.budget.retained_history = MAX_HISTORY + 1,
        |s| s.budget.retained_history = 0,
        |s| s.mono_fallback.clear(),
        |s| s.seed = None,
    ];
    for f in mutate {
        let mut s = spec();
        f(&mut s);
        let mut p = SessionPort::default();
        p.advance(3000).unwrap();
        let old = p.clone();
        assert!(p.mount(s).is_err());
        assert_eq!(p, old);
    }
    reject(|s| {
        column(s).push(Element::Lines {
            id: "too_wide".into(),
            width: 81,
            height: 8,
            segments: vec![],
            fallback: "x".into(),
        })
    });
    reject(|s| {
        column(s).push(Element::Lines {
            id: "out_of_range".into(),
            width: 8,
            height: 8,
            segments: vec![[0, 0, 8, 8]],
            fallback: "x".into(),
        })
    });
    reject(|s| {
        column(s).push(Element::Lines {
            id: "no_fallback".into(),
            width: 8,
            height: 8,
            segments: vec![],
            fallback: String::new(),
        })
    });
    reject(|s| {
        column(s).push(Element::Lines {
            id: "too_many_lines".into(),
            width: 8,
            height: 8,
            segments: vec![[0, 0, 7, 7]; 129],
            fallback: "x".into(),
        })
    });
}
#[test]
fn subscriptions_and_semantic_authority_cannot_be_forged() {
    reject(|s| s.bindings[0].subscription = "filesystem.write".into());
    reject(|s| s.bindings[0].node = "description".into());
    reject(|s| {
        s.bindings[0].action = InstrumentAction::Select {
            target: "absent".into(),
        }
    });
    reject(|s| s.bindings[0].action = InstrumentAction::ResolvePermission { approved: true });
    reject(|s| s.bindings[0].action = InstrumentAction::ChooseScope { expedition: true });
    reject(|s| {
        s.bindings.pop();
    });
    reject(|s| s.bindings.push(s.bindings[0].clone()));
    let mut p = port();
    let old = p.clone();
    assert!(p.activate("workspace/description").is_err());
    assert!(p.activate("unmounted/architect").is_err());
    assert_eq!(p, old);
    p.advance(10000).unwrap();
    p.sync_plan().unwrap();
    p.activate("workspace/deny").unwrap();
    assert_eq!(p.read_snapshot().permission, Some(false));
    let old = p.clone();
    assert!(p.activate("workspace/allow").is_err());
    assert_eq!(p, old);
}
#[test]
fn focus_identity_survives_relayout_patch_and_modal_unmount() {
    let mut p = port();
    p.request_focus("workspace/builder").unwrap();
    p.advance(5000).unwrap();
    p.sync_plan().unwrap();
    assert_eq!(p.focused(), Some("workspace/builder"));
    let mut modal = spec();
    modal.id = "temporary".into();
    p.mount(modal).unwrap();
    p.request_focus("temporary/scout").unwrap();
    for (w, h) in [(56, 24), (120, 32)] {
        let rendered = ui::plain(&p.frame(w, h, ColorDepth::Mono));
        assert!(rendered.contains("active temporary"));
        assert!(rendered.contains("> SCOUT"));
    }
    p.unmount("temporary").unwrap();
    assert_eq!(p.focused(), Some("workspace/builder"));
    let old = p.clone();
    assert!(p.request_focus("temporary/scout").is_err());
    assert_eq!(p, old);
    for (w, h) in [(56, 24), (80, 24), (120, 32), (160, 40)] {
        let f = p.frame(w, h, ColorDepth::Mono);
        assert!(ui::plain(&f).contains("> BUILDER"));
        assert_eq!(p, old);
    }
    p.activate("workspace/builder").unwrap();
    p.sync_plan().unwrap();
    assert_eq!(p.read_snapshot().selected.as_deref(), Some("builder"));
    assert!(ui::plain(&p.frame(120, 32, ColorDepth::Mono)).contains("Selected: builder"));
}
#[test]
fn planner_mounts_updates_and_unmounts_without_semantic_invention() {
    let app = Instruments::default();
    for (t, title) in [
        (3000, "DEPENDENCY"),
        (5000, "TIMELINE"),
        (9500, "PROVENANCE"),
        (11000, "DECISION"),
    ] {
        let p = app.replay(t).unwrap();
        assert!(p.mounted()["workspace"].title.contains(title));
        assert_eq!(p.read_snapshot(), sem::snapshot(t, &[]));
        assert!(p
            .history()
            .iter()
            .any(|op| matches!(op, Lifecycle::Update { .. })));
    }
    assert!(app
        .replay(11000)
        .unwrap()
        .mounted()
        .contains_key("permission"));
    let p = app.replay(14000).unwrap();
    assert!(!p.mounted().contains_key("permission"));
    assert!(p
        .history()
        .contains(&Lifecycle::Unmount("permission".into())));
}
#[test]
fn full_replay_preserves_specs_patches_focus_actions_and_every_frame() {
    let app = Instruments {
        inputs: vec![
            Input {
                at_ms: 2371,
                intent: Intent::Next,
            },
            Input {
                at_ms: 3123,
                intent: Intent::Activate,
            },
            Input {
                at_ms: 8231,
                intent: Intent::Next,
            },
            Input {
                at_ms: 8339,
                intent: Intent::Activate,
            },
            Input {
                at_ms: 11137,
                intent: Intent::Deny,
            },
        ],
        ..Default::default()
    };
    let recovered: Instruments =
        serde_json::from_slice(&serde_json::to_vec(&app).unwrap()).unwrap();
    for t in [0, 2500, 5001, 8511, 9991, 12001, 18000, 6001, 18000] {
        let a = app.replay(t).unwrap();
        let b = recovered.replay(t).unwrap();
        assert_eq!(a, b);
        assert_eq!(
            a.read_snapshot().event_order,
            sem::snapshot(t, &[]).event_order
        );
        for (w, h) in [(56, 24), (80, 24), (120, 32), (160, 40)] {
            for d in [ColorDepth::TrueColor, ColorDepth::Ansi16, ColorDepth::Mono] {
                let frame = a.frame(w, h, d);
                assert_eq!(frame, b.frame(w, h, d));
                let diff = gibson::compute_diff(Some(&frame), &frame);
                assert_eq!(diff.exact_changed_cell_count(), 0);
                assert_eq!(diff.affected_cell_count(), 0);
                assert!(gibson::ansi::AnsiCompiler::new().compile(&diff).is_empty());
                assert!(ui::plain(&frame).contains("Enter inspect"));
            }
        }
    }
    let end = app.replay(18000).unwrap();
    assert_eq!(end.read_snapshot().selected.as_deref(), Some("builder"));
    assert_eq!(end.read_snapshot().permission, Some(false));
    assert!(end.read_snapshot().artifacts.is_empty());
}
#[test]
fn bounded_output_mounts_actions_and_invalid_input_are_visible() {
    let mut p = port();
    for n in 0..20 {
        p.commit_output("workspace", &format!("receipt {n}"))
            .unwrap();
    }
    assert_eq!(p.output().len(), 16);
    let mut tight = spec();
    tight.id = "tight".into();
    tight.budget.retained_history = 1;
    p.mount(tight).unwrap();
    p.commit_output("tight", "one bounded receipt").unwrap();
    assert_eq!(p.output(), ["one bounded receipt"]);
    let old = p.clone();
    assert!(p.commit_output("tight", "\u{1b}[2J").is_err());
    assert_eq!(p, old);
    for id in ["third", "fourth"] {
        let mut s = spec();
        s.id = id.into();
        p.mount(s).unwrap();
    }
    let mut fifth = spec();
    fifth.id = "fifth".into();
    let old = p.clone();
    assert!(p.mount(fifth).is_err());
    assert_eq!(p, old);
    for _ in 0..sem::MAX_ACTIONS {
        p.activate("workspace/scout").unwrap();
    }
    let old = p.clone();
    assert!(p.activate("workspace/scout").is_err());
    assert_eq!(p, old);
    let mut app = Instruments::default();
    app.key(3000, KeyEvent::char('n'));
    assert!(ui::plain(&app.frame(3000, 120, 32, ColorDepth::Mono)).contains("REJECTED"));
    for _ in 1..MAX_HISTORY {
        app.key(4000, KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()));
    }
    app.key(10000, KeyEvent::char('n'));
    assert!(
        ui::plain(&app.frame(10000, 120, 32, ColorDepth::Mono)).contains("input history budget")
    );
    for (w, h) in [(0, 0), (1, 1), (u16::MAX, u16::MAX)] {
        let f = app.frame(18000, w, h, ColorDepth::Mono);
        assert!(f.width <= ui::MAX_WIDTH && f.height <= ui::MAX_HEIGHT)
    }
}

#[test]
fn lifetime_identity_admission_and_hostile_replay_fail_closed() {
    let mut p = SessionPort::default();
    p.advance(3000).unwrap();
    let one = |n: usize| {
        let mut s = spec();
        s.root = Element::Choice {
            id: format!("choice{n}"),
            label: "Inspect SCOUT".into(),
        };
        s.bindings = vec![Binding {
            node: format!("choice{n}"),
            subscription: "activate".into(),
            action: InstrumentAction::Select {
                target: "scout".into(),
            },
        }];
        s
    };
    p.mount(one(0)).unwrap();
    for n in 1..MAX_NODES * MAX_MOUNTS {
        p.update("workspace", patch(&one(n))).unwrap();
    }
    let before = p.clone();
    assert!(p
        .update("workspace", patch(&one(MAX_NODES * MAX_MOUNTS)))
        .is_err());
    assert_eq!(p, before);
    assert!(p.history().len() <= MAX_OPERATIONS);
    for inputs in [
        vec![Input {
            at_ms: u64::MAX,
            intent: Intent::Activate,
        }],
        vec![
            Input {
                at_ms: 3000,
                intent: Intent::Next
            };
            MAX_HISTORY + 1
        ],
        vec![
            Input {
                at_ms: 3000,
                intent: Intent::Next,
            },
            Input {
                at_ms: 2000,
                intent: Intent::Next,
            },
        ],
    ] {
        let bad = Instruments {
            inputs,
            ..Default::default()
        };
        assert!(bad.replay(18000).is_err());
        let frame = bad.frame(18000, 56, 24, ColorDepth::Mono);
        assert!(ui::plain(&frame).contains("REJECTED"));
        assert!(frame
            .cells
            .iter()
            .all(|c| c.style.fg.is_none() && c.style.bg.is_none()));
    }
}

#[test]
fn arbitrary_valid_mounts_are_realized_not_reserved_name_only() {
    let mut p = SessionPort::default();
    p.advance(3000).unwrap();
    let mut s = spec();
    s.id = "custom".into();
    s.title = "CUSTOM INTERACTIVE".into();
    p.mount(s).unwrap();
    let frame = ui::plain(&p.frame(56, 24, ColorDepth::Mono));
    assert!(frame.contains("CUSTOM INTERACTIVE"));
    assert!(frame.contains("> ARCHITECT"));
    let mut readonly = spec();
    readonly.id = "annotation".into();
    readonly.title = "READ ONLY".into();
    readonly.root = Element::Text {
        id: "note".into(),
        text: "A visible bounded note".into(),
    };
    readonly.bindings.clear();
    p.mount(readonly).unwrap();
    let frame = ui::plain(&p.frame(56, 24, ColorDepth::Mono));
    assert!(frame.contains("READ ONLY"));
    assert!(frame.contains("visible bounded note"));
}

#[test]
fn accepted_composition_nodes_compile_and_aggregate_rasters_are_bounded() {
    let mut p = SessionPort::default();
    p.advance(3000).unwrap();
    let mut s = spec();
    s.root = Element::Row {
        id: "row".into(),
        children: vec![
            Element::Panel {
                id: "left".into(),
                title: "LEFT".into(),
                children: vec![Element::Choice {
                    id: "architect".into(),
                    label: "Select architect".into(),
                }],
            },
            Element::Panel {
                id: "right".into(),
                title: "RIGHT".into(),
                children: vec![Element::Text {
                    id: "readout".into(),
                    text: "Bounded content".into(),
                }],
            },
        ],
    };
    s.bindings.truncate(1);
    p.mount(s).unwrap();
    let rendered = ui::plain(&p.frame(120, 32, ColorDepth::Mono));
    assert!(rendered.contains("LEFT"));
    assert!(rendered.contains("RIGHT"));
    assert!(rendered.contains("Bounded content"));
    let mut too_many = spec();
    too_many.budget.raster_width = MAX_RASTER;
    too_many.budget.raster_height = MAX_RASTER;
    for n in 0..2 {
        column(&mut too_many).push(Element::Lines {
            id: format!("area{n}"),
            width: MAX_RASTER,
            height: MAX_RASTER,
            segments: vec![],
            fallback: "bounded".into(),
        });
    }
    assert!(validate(&too_many, &sem::snapshot(3000, &[]))
        .unwrap_err()
        .contains("aggregate raster"));
}
