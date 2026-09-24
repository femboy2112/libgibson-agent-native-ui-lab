use gibson::{
    input::{KeyCode, KeyEvent, KeyModifiers},
    ColorDepth, FocusId,
};
use gibson_ui_lab::{
    manga::Manga,
    semantic_fixture as sem,
    ui::{self, App},
};

#[test]
fn same_trace_replays_through_reflow_and_every_capability() {
    let mut manga = Manga::default();
    manga.focus.set(FocusId(3));
    let before = sem::snapshot(9000, &manga.actions);
    for (w, h) in [(56, 24), (80, 24), (120, 32), (160, 40)] {
        for d in [ColorDepth::TrueColor, ColorDepth::Ansi16, ColorDepth::Mono] {
            for t in [0, 2500, 6500, 9000, 10000, 18000] {
                let a = manga.frame(t, w, h, d);
                assert_eq!(a, manga.frame(t, w, h, d));
                assert_eq!((a.width, a.height), (w, h));
                let diff = gibson::compute_diff(Some(&a), &a);
                assert_eq!(diff.exact_changed_cell_count(), 0);
                assert_eq!(diff.affected_cell_count(), 0);
                assert!(gibson::ansi::AnsiCompiler::new().compile(&diff).is_empty());
                assert!(ui::plain(&a).contains("Tab/"));
                if (2500..18000).contains(&t) {
                    assert!(ui::plain(&a).contains("> 3 BUILDER"));
                }
            }
        }
    }
    assert_eq!(manga.focus.current(), Some(FocusId(3)));
    assert_eq!(before, sem::snapshot(9000, &manga.actions));
}

#[test]
fn selection_permission_and_repair_have_semantic_witnesses() {
    let mut manga = Manga::default();
    manga.key(10000, KeyEvent::new(KeyCode::Right, KeyModifiers::empty()));
    manga.key(10000, KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));
    manga.key(10000, KeyEvent::char('n'));
    let s = sem::snapshot(18000, &manga.actions);
    assert_eq!(s.selected.as_deref(), Some("scout"));
    assert_eq!(s.permission, Some(false));
    assert_eq!(s.event_order, sem::snapshot(18000, &[]).event_order);
    let early = ui::plain(&manga.frame(7000, 120, 32, ColorDepth::Mono));
    let repaired = ui::plain(&manga.frame(9000, 120, 32, ColorDepth::Mono));
    assert!(early.contains("gutter torn"));
    assert!(repaired.contains("REPAIR SEAM"));
}

#[test]
fn bounded_small_and_hostile_terminal_dimensions() {
    for (w, h) in [(0, 0), (1, 1), (24, 12), (u16::MAX, u16::MAX)] {
        let s = Manga::default().frame(u64::MAX, w, h, ColorDepth::Mono);
        assert!(s.width <= ui::MAX_WIDTH && s.height <= ui::MAX_HEIGHT);
    }
}

#[test]
fn fresh_consumer_replays_focus_actions_and_screen_not_only_semantics() {
    use ui::{InputKey, InputRecord};
    let keys = vec![
        InputRecord {
            at_ms: 9000,
            key: InputKey::Right,
        },
        InputRecord {
            at_ms: 10000,
            key: InputKey::Enter,
        },
        InputRecord {
            at_ms: 10123,
            key: InputKey::Char('n'),
        },
    ];
    let mut live = Manga::default();
    for key in &keys {
        live.key(key.at_ms, key.key.event());
    }
    let encoded = serde_json::to_vec(&keys).unwrap();
    let restored: Vec<InputRecord> = serde_json::from_slice(&encoded).unwrap();
    let replay = ui::replay(Manga::default(), &restored, 11000).unwrap();
    assert_eq!(live.actions, replay.actions);
    assert_eq!(live.focus, replay.focus);
    for (w, h) in [(56, 24), (80, 24), (120, 32), (160, 40)] {
        assert_eq!(
            live.frame(11000, w, h, ColorDepth::Mono),
            replay.frame(11000, w, h, ColorDepth::Mono)
        );
    }
    // Negative control: semantic actions alone omit the focus-only input.
    let incomplete = Manga {
        actions: live.actions.clone(),
        ..Manga::default()
    };
    assert_ne!(
        live.frame(11000, 80, 24, ColorDepth::Mono),
        incomplete.frame(11000, 80, 24, ColorDepth::Mono)
    );
}

#[test]
fn selection_spam_cannot_silently_consume_the_permission_slot() {
    let mut app = Manga::default();
    for _ in 0..64 {
        app.key(10000, KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()));
    }
    assert!(ui::plain(&app.frame(10000, 80, 24, ColorDepth::Mono)).contains("INPUT REJECTED"));
    app.key(10001, KeyEvent::char('n'));
    assert_eq!(sem::snapshot(18000, &app.actions).permission, Some(false));
    assert!(sem::snapshot(18000, &app.actions).artifacts.is_empty());
}
