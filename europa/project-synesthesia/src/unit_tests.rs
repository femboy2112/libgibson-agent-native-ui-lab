use std::time::Instant;

use gibson::input::{Event, KeyCode, KeyEvent};

use crate::app::AppModel;
use crate::cli::Options;
use crate::engine::{Engine, STEPS_PER_TRACK};
use crate::visual::{SpectrumHistory, HISTORY_CAPACITY};

#[test]
fn deterministic_pcm_and_fft_for_fixed_seed_and_time() {
    let engine = Engine::default();
    let first = engine.render_at_ms(1_337, 0x5eed);
    let repeat = engine.render_at_ms(1_337, 0x5eed);
    assert_eq!(first.samples, repeat.samples);
    assert_eq!(first.spectrum, repeat.spectrum);
    assert_eq!(first.active_note_count, repeat.active_note_count);
    assert_eq!(first.step_index, repeat.step_index);
}

#[test]
fn filter_cutoff_changes_the_measured_high_frequency_field() {
    let mut open = Engine::default();
    open.patch.cutoff_hz = 12_000.0;
    let mut closed = open.clone();
    closed.patch.cutoff_hz = 280.0;
    let high = open.render_at_ms(91, 19);
    let low = closed.render_at_ms(91, 19);
    let high_band = 80..FFT_BINS_FOR_TEST;
    let open_energy: f32 = high_band.clone().map(|i| high.spectrum[i]).sum();
    let closed_energy: f32 = high_band.map(|i| low.spectrum[i]).sum();
    assert!(
        closed_energy < open_energy * 0.4,
        "open={open_energy} closed={closed_energy}"
    );
    assert_ne!(high.samples, low.samples);
}

#[test]
fn mute_and_solo_change_the_output_energy() {
    let mut engine = Engine::default();
    engine.tracks[0].muted = true;
    engine.tracks[1].muted = true;
    engine.tracks[2].muted = true;
    let bass_only = engine.render_at_ms(640, 12);
    engine.tracks[3].solo = true;
    let solo = engine.render_at_ms(640, 12);
    assert_eq!(bass_only.samples, solo.samples);
    engine.tracks[3].muted = true;
    let silent = engine.render_at_ms(640, 12);
    assert!(silent.routing[6] < 1.0e-6);
}

#[test]
fn bounded_spectrogram_history_keeps_only_the_newest_rows() {
    let mut history = SpectrumHistory::default();
    for value in 0..(HISTORY_CAPACITY + 11) {
        let mut row = [0.0; crate::engine::FFT_BINS];
        row[0] = value as f32;
        history.push(&row);
    }
    assert_eq!(history.len(), HISTORY_CAPACITY);
    assert_eq!(history.age(0).unwrap()[0], (HISTORY_CAPACITY + 10) as f32);
    assert_eq!(history.age(HISTORY_CAPACITY - 1).unwrap()[0], 11.0);
}

#[test]
fn arrows_tabs_and_modal_text_are_consumed_by_the_application_model() {
    let mut model = AppModel::from_options(&Options::default()).unwrap();
    let now = Instant::now();
    let key = |code| Event::Key(KeyEvent::new(code, gibson::input::KeyModifiers::empty()));

    model.handle_event(&key(KeyCode::Right), now);
    assert_eq!(model.selected_step, 1);
    model.handle_event(&key(KeyCode::Enter), now);
    assert_eq!(model.engine.tracks[0].steps[1].note, Some(36));
    model.handle_event(&key(KeyCode::Up), now);
    assert_eq!(model.engine.tracks[0].steps[1].note, Some(37));

    model.handle_event(&key(KeyCode::Tab), now);
    assert_eq!(model.focus, crate::visual::Focus::Patch);
    model.handle_event(&key(KeyCode::Char('f')), now);
    for digit in ['5', '5', '0', '0'] {
        model.handle_event(&key(KeyCode::Char(digit)), now);
    }
    model.handle_event(&key(KeyCode::Enter), now);
    assert!((model.engine.patch.cutoff_hz - 5_500.0).abs() < 0.01);

    for _ in 0..128 {
        model.handle_event(&key(KeyCode::Left), now);
        model.handle_event(&key(KeyCode::Right), now);
        model.handle_event(&key(KeyCode::Tab), now);
        model.handle_event(&key(KeyCode::BackTab), now);
    }
    assert_eq!(model.selected_step, 1);
    assert_eq!(model.events_seen, 5 + 4 + 1 + 128 * 4);
    assert_eq!(model.key_events_seen, model.events_seen);
    assert_eq!(model.resize_events_seen, 0);
    model.handle_event(&Event::Resize(120, 40), now);
    assert_eq!(model.events_seen, 5 + 4 + 1 + 128 * 4 + 1);
    assert_eq!(model.key_events_seen, 5 + 4 + 1 + 128 * 4);
    assert_eq!(model.resize_events_seen, 1);
    assert!(model.selected_track < crate::engine::TRACK_COUNT);
    assert!(model
        .engine
        .tracks
        .iter()
        .all(|t| (1..=STEPS_PER_TRACK as u8).contains(&t.pattern_length)));
}

const FFT_BINS_FOR_TEST: usize = crate::engine::FFT_BINS;
