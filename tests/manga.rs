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
