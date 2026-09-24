use gibson::{
    input::{KeyCode, KeyEvent, KeyModifiers},
    ColorDepth, FocusId,
};
use gibson_ui_lab::{
    reactions::{cues, Metaphor, Mode, ReactionConfig, Reactions, Role, TARGETS},
    semantic_fixture::{self as sem, SemanticEvent, Step},
    ui::{self, App},
};

const MODES: [Mode; 3] = [Mode::None, Mode::Restrained, Mode::Heavy];
const METAPHORS: [Metaphor; 2] = [Metaphor::Constellation, Metaphor::Paperwork];

#[test]
fn mode_and_metaphor_cannot_mutate_the_fixture_or_explicit_decisions() {
    let mut baseline = None;
    for mode in MODES {
        for metaphor in METAPHORS {
            let mut app = Reactions::new(mode, metaphor, ReactionConfig::default()).unwrap();
            let mut witnesses = Vec::new();
            for t in [0, 2500, 4700, 6000, 10000, 11000, 15000, 18000] {
                if t == 6000 {
                    app.key(t, KeyEvent::char('e'));
                }
                if t == 11000 {
                    app.key(t, KeyEvent::char('n'));
                }
                let before = app.snapshot(t);
                for (w, h) in [(56, 24), (80, 24), (120, 32), (160, 40)] {
                    for depth in [ColorDepth::TrueColor, ColorDepth::Ansi16, ColorDepth::Mono] {
                        let a = app.frame(t, w, h, depth);
                        assert_eq!(a, app.frame(t, w, h, depth));
                        assert_eq!((a.width, a.height), (w, h));
                        let diff = gibson::compute_diff(Some(&a), &a);
                        assert_eq!(diff.exact_changed_cell_count(), 0);
                        assert_eq!(diff.affected_cell_count(), 0);
                        assert!(gibson::ansi::AnsiCompiler::new().compile(&diff).is_empty());
                        let text = ui::plain(&a);
                        assert!(text.contains("L local repair"));
                        assert!(text.contains("N deny local artifact"));
                        assert!(text.contains("Tab/"));
                        if t >= 6000 {
                            assert!(text.contains("DECISION: architecture expedition"));
                        }
                    }
                }
                assert_eq!(before, app.snapshot(t), "paint must be pure");
                assert_eq!(before.event_order, sem::snapshot(t, &[]).event_order);
                witnesses.push(before);
            }
            let end = app.snapshot(18000);
            assert!(end.scope_expedition);
            assert_eq!(end.permission, Some(false));
            assert!(end.artifacts.is_empty());
            assert_ne!(
                end,
                sem::snapshot(18000, &[]),
                "counterfactual control detects explicit decisions"
            );
            if let Some(ref baseline) = baseline {
                assert_eq!(&witnesses, baseline);
            } else {
                baseline = Some(witnesses);
            }
        }
    }
}

#[test]
fn original_reaction_slot_can_change_metaphor_without_changing_policy_or_action() {
    let mut cosmic = Reactions::default();
    let mut paperwork = Reactions::default();
    paperwork.metaphor = Metaphor::Paperwork;
    for (w, h) in [(56, 24), (80, 24), (120, 32), (160, 40)] {
        let a = cosmic.frame(5000, w, h, ColorDepth::Mono);
        let b = paperwork.frame(5000, w, h, ColorDepth::Mono);
        assert!(ui::plain(&a).contains("UNIVERSE?"));
        assert!(ui::plain(&b).contains("REQUEST TO REQUEST"));
        assert_ne!(a, b);
    }
    cosmic.activate(5000, "scope.local").unwrap();
    paperwork.activate(5000, "scope.local").unwrap();
    assert_eq!(cosmic.actions(), paperwork.actions());
    assert_eq!(cosmic.snapshot(18000), paperwork.snapshot(18000));
    cosmic.mode = Mode::None;
    paperwork.mode = Mode::None;
    assert_eq!(
        cosmic.frame(5000, 120, 32, ColorDepth::Mono),
        paperwork.frame(5000, 120, 32, ColorDepth::Mono)
    );
}

#[test]
fn recorded_keys_replay_focus_actions_and_complete_frames_at_irregular_times() {
    use ui::{InputKey as K, InputRecord as R};
    let inputs = [
        R {
            at_ms: 4701,
            key: K::Right,
        },
        R {
            at_ms: 4933,
            key: K::Enter,
        },
        R {
            at_ms: 6707,
            key: K::Left,
        },
        R {
            at_ms: 10003,
            key: K::Right,
        },
        R {
            at_ms: 10021,
            key: K::Right,
        },
        R {
            at_ms: 10817,
            key: K::Char('n'),
        },
    ];
    for mode in MODES {
        for metaphor in METAPHORS {
            let fresh = || Reactions::new(mode, metaphor, ReactionConfig::default()).unwrap();
            let mut live = fresh();
            let mut next = 0;
            for at in [
                101, 4700, 4809, 5001, 7103, 10005, 10022, 10901, 15001, 18000,
            ] {
                while next < inputs.len() && inputs[next].at_ms <= at {
                    live.key(inputs[next].at_ms, inputs[next].key.event());
                    next += 1;
                }
                let replay = ui::replay(fresh(), &inputs, at).unwrap();
                assert_eq!(live.focus, replay.focus);
                assert_eq!(live.input_notice, replay.input_notice);
                assert_eq!(live.actions(), replay.actions());
                assert_eq!(live.snapshot(at), replay.snapshot(at));
                for (w, h) in [(56, 24), (80, 24), (120, 32), (160, 40)] {
                    for color in [ColorDepth::TrueColor, ColorDepth::Ansi16, ColorDepth::Mono] {
                        assert_eq!(live.frame(at, w, h, color), replay.frame(at, w, h, color));
                    }
                }
            }
        }
    }
}

#[test]
fn policy_has_evidenced_roles_exclusions_cooldowns_and_a_hard_active_budget() {
    let trace = sem::fixture();
    let config = ReactionConfig::default();
    let mut seen = Vec::new();
    for t in (0..=18000).step_by(100) {
        let active = cues(&trace, t, Mode::Heavy, &config).unwrap();
        assert!(active.len() <= config.max_active);
        for cue in active {
            if !seen.contains(&cue.role) {
                seen.push(cue.role);
            }
        }
        assert!(cues(&trace, t, Mode::Restrained, &config).unwrap().len() <= 1);
        assert!(cues(&trace, t, Mode::None, &config).unwrap().is_empty());
    }
    assert!(seen.len() >= 8, "actually observed roles: {seen:?}");
    let excluded = ReactionConfig {
        exclusions: vec![Role::ScopeExplosion],
        ..config.clone()
    };
    assert!(!cues(&trace, 4700, Mode::Heavy, &excluded)
        .unwrap()
        .iter()
        .any(|c| c.role == Role::ScopeExplosion));
    let progress = |at_ms| Step {
        at_ms,
        event: SemanticEvent::ToolProgress {
            agent: "scout".into(),
            percent: 50,
            summary: "witness".into(),
        },
    };
    let active = cues(
        &[progress(1000), progress(1100), progress(1700)],
        1800,
        Mode::Heavy,
        &config,
    )
    .unwrap();
    assert_eq!(
        active.iter().map(|c| c.at_ms).collect::<Vec<_>>(),
        vec![1700, 1000]
    );
    assert!(cues(&[progress(1000)], 8000, Mode::Heavy, &config)
        .unwrap()
        .is_empty());
    let fail = |at_ms| Step {
        at_ms,
        event: SemanticEvent::ToolFinished {
            agent: "builder".into(),
            tool: "fixture.test".into(),
            success: false,
            summary: "known-negative fixture".into(),
        },
    };
    assert_eq!(
        cues(&[fail(1000), fail(2000)], 2000, Mode::Heavy, &config).unwrap()[0].role,
        Role::RepeatedFailure
    );
}

#[test]
fn malformed_config_and_unordered_or_oversized_sources_fail_closed() {
    let good = ReactionConfig::default();
    assert_eq!(
        ReactionConfig::from_json(&serde_json::to_string(&good).unwrap()).unwrap(),
        good
    );
    for bad in [
        ReactionConfig {
            max_active: 0,
            ..good.clone()
        },
        ReactionConfig {
            max_active: 4,
            ..good.clone()
        },
        ReactionConfig {
            cooldown_ms: 0,
            ..good.clone()
        },
        ReactionConfig {
            lifetime_ms: u64::MAX,
            ..good.clone()
        },
        ReactionConfig {
            intensity: 255,
            ..good.clone()
        },
        ReactionConfig {
            exclusions: vec![Role::Progress, Role::Progress],
            ..good.clone()
        },
    ] {
        assert!(bad.validate().is_err());
        assert!(Reactions::new(Mode::None, Metaphor::Paperwork, bad.clone()).is_err());
        assert!(cues(&[], 0, Mode::None, &bad).is_err());
    }
    for json in [
        "null",
        "{}",
        "{\"raw_ansi\":\"injected\"}",
        "{\"intensity\":NaN}",
    ] {
        assert!(ReactionConfig::from_json(json).is_err());
    }
    let unknown = serde_json::to_string(&good)
        .unwrap()
        .replace("}", ",\"keyboard_capture\":true}");
    assert!(ReactionConfig::from_json(&unknown).is_err());
    assert!(ReactionConfig::from_json(&" ".repeat(2049)).is_err());
    assert!(Mode::parse("loud").is_err());
    assert!(Metaphor::parse("arbitrary-asset").is_err());
    let source = Step {
        at_ms: 1,
        event: SemanticEvent::SessionStarted,
    };
    assert!(cues(&vec![source.clone(); 129], 100, Mode::None, &good).is_err());
    assert!(cues(
        &[
            source,
            Step {
                at_ms: 0,
                event: SemanticEvent::SessionFinished
            }
        ],
        100,
        Mode::Heavy,
        &good
    )
    .is_err());
}

#[test]
fn focus_survives_reflow_and_only_registered_available_actions_emit() {
    let mut app = Reactions::default();
    assert!(!app.focus.set(FocusId(999)));
    assert!(app.activate(1000, "scope.expedition").is_err());
    assert!(app.activate(10000, "session.execute_shell").is_err());
    app.key(5000, KeyEvent::new(KeyCode::Right, KeyModifiers::empty()));
    assert_eq!(app.focus.current(), Some(TARGETS[1].0));
    for (w, h) in [(56, 24), (160, 40), (80, 24)] {
        assert!(ui::plain(&app.frame(5000, w, h, ColorDepth::Mono))
            .contains("> E architecture expedition"));
    }
    assert_eq!(app.focus.current(), Some(TARGETS[1].0));
    app.key(5000, KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));
    assert!(app.snapshot(6000).scope_expedition);
    app.key(11000, KeyEvent::new(KeyCode::Right, KeyModifiers::empty()));
    assert_eq!(app.focus.current(), Some(TARGETS[2].0));
    app.key(11000, KeyEvent::new(KeyCode::Right, KeyModifiers::empty()));
    app.key(11000, KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));
    assert_eq!(app.snapshot(18000).permission, Some(false));
    let before = app.actions().to_vec();
    app.key(11000, KeyEvent::char('q'));
    app.key(
        11000,
        KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL),
    );
    assert!(app.activate(10000, "scope.local").is_err());
    assert_eq!(app.actions(), before);
    assert!(!ui::plain(&app.frame(15000, 120, 32, ColorDepth::Mono)).contains("AN ACTUAL RECEIPT"));
}

#[test]
fn bounded_resources_and_changed_frame_damage_have_concrete_witnesses() {
    let mut app = Reactions::default();
    for _ in 0..sem::MAX_ACTIONS - 1 {
        app.activate(6000, "scope.local").unwrap();
    }
    assert!(app.activate(6000, "scope.local").is_err());
    app.key(6000, KeyEvent::char('e'));
    assert!(ui::plain(&app.frame(6000, 120, 32, ColorDepth::Mono)).contains("ACTION REJECTED"));
    app.activate(11000, "permission.deny").unwrap();
    assert_eq!(app.snapshot(18000).permission, Some(false));
    for (w, h) in [(0, 0), (1, 1), (39, 19), (u16::MAX, u16::MAX)] {
        let s = app.frame(u64::MAX, w, h, ColorDepth::Mono);
        assert!(s.width <= ui::MAX_WIDTH && s.height <= ui::MAX_HEIGHT);
    }
    let a = app.frame(6000, 120, 32, ColorDepth::TrueColor);
    let b = app.frame(6800, 120, 32, ColorDepth::TrueColor);
    let diff = gibson::compute_diff(Some(&a), &b);
    assert!(diff.exact_changed_cell_count() > 0);
    assert!(diff.exact_changed_cell_count() < 120 * 32 / 2);
    assert!(diff.affected_cell_count() >= diff.exact_changed_cell_count());
    assert!(!gibson::ansi::AnsiCompiler::new().compile(&diff).is_empty());
}
