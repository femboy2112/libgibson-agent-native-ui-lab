use gibson::{input::KeyEvent, ColorDepth};
use gibson_ui_lab::{
    instruments::Instruments, manga::Manga, reactions::Reactions, semantic_fixture as sem, ui::App,
};

#[test]
fn three_realizations_preserve_the_same_source_and_explicit_permission_decision() {
    for deny in [false, true] {
        let mut manga = Manga::default();
        let mut reactions = Reactions::default();
        let mut instruments = Instruments::default();
        if deny {
            for app in [&mut manga as &mut dyn App, &mut reactions, &mut instruments] {
                app.key(11017, KeyEvent::char('n'));
            }
        }
        for at in [0, 2017, 4709, 8997, 11016, 11018, 14003, 18000] {
            let expected = sem::snapshot(at, &manga.actions);
            assert_eq!(expected, reactions.snapshot(at));
            assert_eq!(expected, instruments.replay(at).unwrap().read_snapshot());
            assert_eq!(expected.event_order, sem::snapshot(at, &[]).event_order);
            for app in [&manga as &dyn App, &reactions, &instruments] {
                let a = app.frame(at, 56, 24, ColorDepth::Mono);
                app.frame(at, 160, 40, ColorDepth::TrueColor);
                assert_eq!(a, app.frame(at, 56, 24, ColorDepth::Mono));
            }
        }
        assert_eq!(sem::snapshot(18000, &manga.actions).permission, Some(!deny));
    }
}
