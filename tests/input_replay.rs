use gibson_ui_lab::{
    manga::Manga,
    ui::{self, InputKey, InputRecord},
};

#[test]
fn input_trace_rejects_bad_order_control_text_and_budget_before_dispatch() {
    let key = InputRecord {
        at_ms: 10,
        key: InputKey::Tab,
    };
    for keys in [
        vec![key.clone(); 129],
        vec![
            key.clone(),
            InputRecord {
                at_ms: 9,
                ..key.clone()
            },
        ],
        vec![InputRecord {
            at_ms: 0,
            key: InputKey::Char('\u{1b}'),
        }],
    ] {
        assert!(ui::replay(Manga::default(), &keys, 18000).is_err());
    }
}
