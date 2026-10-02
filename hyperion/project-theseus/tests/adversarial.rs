//! Adversarial integration tests for Project Theseus.
//!
//! These are *external* tests: they drive the released LibGibson v0.4.0 API from
//! outside the crate and the app's own RGB renderer through its library surface.
//! No test asserts `x.is_ok() || x.is_err()`; every one pins a concrete claim
//! with a positive control where the claim could otherwise pass vacuously.

use gibson::audio::human_music::{
    contract::CompositionGrammar,
    cover::{
        cover, CoverAxis, CoverError, CoverFidelityPreset, CoverFidelityProfile, CoverKnowledge,
        CoverMap, CoverSpec, CoverTarget,
    },
    fingerprint::CanonicalFingerprint,
    functor::perform_pocketed,
    performance::PerformanceOptions,
    policy::PerformanceProfile,
    reference_song::ReferenceSong,
    semantic::demo_trace,
    song::SongMap,
    synth::HumanMusicSynth,
    world::MusicWorld,
};
use gibson::audio::render::OfflineRenderer;
use gibson::audio::time::SampleRate;
use gibson::capability::ColorDepth;

use project_theseus::model::hash_audio;
use project_theseus::visual::{
    build_frame, identity_panel, palette_for, GlyphMode, VisualParams,
};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("fixtures/{name}"))
        .unwrap_or_else(|e| panic!("read fixtures/{name}: {e}"))
}

fn target(world: &MusicWorld, seed: u64) -> CoverTarget<'_> {
    CoverTarget {
        world,
        seed,
        grammar: CompositionGrammar::DeflectedLift,
        options: PerformanceOptions::default(),
        profile: PerformanceProfile::BAND,
    }
}

fn render_hash(comp: &gibson::audio::human_music::functor::Composition, world: &MusicWorld) -> u64 {
    let mut synth = HumanMusicSynth::new(&comp.score, world, SampleRate::STUDIO);
    let renderer = OfflineRenderer::new(SampleRate::STUDIO, 1024);
    let result = renderer.render_seconds(&mut synth, 2.0);
    hash_audio(&result.audio)
}

// A short reference whose *pinned* (theme) material lives in beats 0..8 and
// whose free tail lives at beat 12.
const BASE: &str = "meter 4 4\nkey 0 major\ntempo 120\nlength 16/1\n\
note lead 0/1 1/1 60\nnote lead 1/1 1/1 62\nnote lead 2/1 1/1 64\nnote lead 3/1 1/1 65\n\
note lead 4/1 1/1 67\nnote lead 12/1 1/1 69\n";
// Same theme; different free tail.
const FREE_VARIANT: &str = "meter 4 4\nkey 0 major\ntempo 120\nlength 16/1\n\
note lead 0/1 1/1 60\nnote lead 1/1 1/1 62\nnote lead 2/1 1/1 64\nnote lead 3/1 1/1 65\n\
note lead 4/1 1/1 67\nnote lead 12/1 1/1 72\n";
// Same tail; one note inside the theme changed.
const PINNED_VARIANT: &str = "meter 4 4\nkey 0 major\ntempo 120\nlength 16/1\n\
note lead 0/1 1/1 60\nnote lead 1/1 1/1 62\nnote lead 2/1 1/1 63\nnote lead 3/1 1/1 65\n\
note lead 4/1 1/1 67\nnote lead 12/1 1/1 69\n";

fn loose_map(tsv: &str) -> CoverMap {
    let song = ReferenceSong::from_tsv(tsv, "lead").expect("parse reference");
    let profile = CoverFidelityProfile::preset(CoverFidelityPreset::Loose);
    song.extract_fidelity(&profile, Some(CoverFidelityPreset::Loose), None)
        .expect("extract loose quotient")
        .0
}

// 1. Material the quotient frees must not reach the generator: two references
//    agreeing on the pinned theme and differing only in the free tail produce
//    the SAME quotient AND the SAME cover output.
#[test]
fn adversarial_01_freed_material_does_not_leak_into_the_quotient_or_the_cover() {
    let a = loose_map(BASE);
    let b = loose_map(FREE_VARIANT);
    assert_eq!(
        a.canonical_fingerprint(),
        b.canonical_fingerprint(),
        "a change outside the pinned theme must not change the quotient"
    );

    let world = MusicWorld::vapor95();
    let ca = cover(&a, target(&world, 42)).expect("cover a");
    let cb = cover(&b, target(&world, 42)).expect("cover b");
    assert_eq!(
        ca.perf.canonical_fingerprint(),
        cb.perf.canonical_fingerprint(),
        "same quotient + same target must yield the same performance plan"
    );
    assert_eq!(
        render_hash(&ca, &world),
        render_hash(&cb, &world),
        "same quotient + same target must render identical audio"
    );
}

// 2. Positive control for (1): changing the pinned theme MUST change the
//    quotient. Without this, test 1 could pass because extraction ignores
//    everything.
#[test]
fn adversarial_02_pinned_material_does_change_the_quotient() {
    let a = loose_map(BASE);
    let c = loose_map(PINNED_VARIANT);
    assert_ne!(
        a.canonical_fingerprint(),
        c.canonical_fingerprint(),
        "a change inside the pinned theme must change the quotient"
    );
}

// 3. Same quotient + same target + same seed is byte-deterministic on the real
//    shipping path: identical plan fingerprint AND identical rendered audio.
#[test]
fn adversarial_03_same_map_same_target_same_seed_is_deterministic() {
    let song = ReferenceSong::from_tsv(&fixture("ode_to_joy.tsv"), "sop").expect("ode");
    let profile = CoverFidelityProfile::preset(CoverFidelityPreset::Interpretive);
    let (map, _) = song
        .extract_fidelity(&profile, Some(CoverFidelityPreset::Interpretive), None)
        .expect("extract");

    let world = MusicWorld::black_ice();
    let c1 = cover(&map, target(&world, 42)).expect("cover 1");
    let c2 = cover(&map, target(&world, 42)).expect("cover 2");
    assert_eq!(c1.perf.canonical_fingerprint(), c2.perf.canonical_fingerprint());
    assert_eq!(c1.score.notes.len(), c2.score.notes.len());
    assert_eq!(
        render_hash(&c1, &world),
        render_hash(&c2, &world),
        "identical inputs must render identical audio"
    );
    assert_ne!(
        render_hash(&c1, &world),
        0,
        "the audio hash must be a real reading, not the FNV seed"
    );
}

// 4. Typed refusal is stable and axis-specific, with a positive control: the
//    same pinned quotient is lawfully covered in another world.
#[test]
fn adversarial_04_typed_refusal_is_stable_and_world_specific() {
    let song = ReferenceSong::from_tsv(&fixture("ode_to_joy.tsv"), "sop").expect("ode");
    let profile = CoverFidelityProfile::preset(CoverFidelityPreset::Strict);
    let (map, _) = song
        .extract_fidelity(&profile, Some(CoverFidelityPreset::Strict), None)
        .expect("extract");

    // SWISS_SIGNAL must refuse the pinned simultaneous attacks...
    let swiss = MusicWorld::swiss_signal();
    match cover(&map, target(&swiss, 42)) {
        Err(CoverError::Invalid(msg)) => assert!(
            msg.to_lowercase().contains("harmony"),
            "expected a harmony refusal, got {msg:?}"
        ),
        Ok(_) => panic!("expected a typed Invalid refusal in SWISS_SIGNAL, but it covered"),
        Err(e) => panic!("expected a typed Invalid refusal in SWISS_SIGNAL, got {e:?}"),
    }

    // ...while BLACK_ICE lawfully covers the very same quotient.
    let ice = MusicWorld::black_ice();
    let covered = cover(&map, target(&ice, 42));
    assert!(
        covered.is_ok(),
        "positive control: the same quotient must cover in BLACK_ICE, got {:?}",
        covered.err()
    );

    // The refusal must be stable across repeated attempts.
    let again = cover(&map, target(&swiss, 42));
    assert!(matches!(again, Err(CoverError::Invalid(_))), "refusal must be stable");
}

// 5. Stronger fidelity presets never observe less than weaker ones.
#[test]
fn adversarial_05_fidelity_invariant_count_is_monotone() {
    let song = ReferenceSong::from_tsv(&fixture("ode_to_joy.tsv"), "sop").expect("ode");
    let invariants = |preset: CoverFidelityPreset| -> usize {
        let profile = CoverFidelityProfile::preset(preset);
        let (map, _) = song.extract_fidelity(&profile, Some(preset), None).expect("extract");
        CoverAxis::ALL
            .into_iter()
            .filter(|a| map.knowledge(*a) == CoverKnowledge::Invariant)
            .count()
    };
    let loose = invariants(CoverFidelityPreset::Loose);
    let interp = invariants(CoverFidelityPreset::Interpretive);
    let faithful = invariants(CoverFidelityPreset::Faithful);
    let strict = invariants(CoverFidelityPreset::Strict);
    assert!(interp >= loose, "interpretive {interp} < loose {loose}");
    assert!(faithful >= interp, "faithful {faithful} < interpretive {interp}");
    assert!(strict >= faithful, "strict {strict} < faithful {faithful}");
    // Positive control: strict genuinely pins more than loose.
    assert!(strict > loose, "strict {strict} should pin more axes than loose {loose}");
}

// 6. The identity projection and the v1 lane projection are genuinely different
//    objects on a generated source (the whole point of the v0.4.0 quotient).
#[test]
fn adversarial_06_identity_projection_differs_from_lane_projection() {
    let world = MusicWorld::black_ice();
    let song = SongMap::build(&demo_trace(64.0), 2112, Some(CompositionGrammar::HookArc));
    let source = perform_pocketed(&song, &world, PerformanceOptions::default());

    let spec = CoverSpec::from_contract(&song.plan.contract);
    let identity = CoverMap::extract(&source, &world, spec.clone()).expect("identity extract");
    let lane = CoverMap::extract_lane(&source, &world, spec).expect("lane extract");

    assert!(identity.validate().is_ok(), "identity map must validate");
    assert!(lane.validate().is_ok(), "lane map must validate");
    assert_ne!(
        identity.projection, lane.projection,
        "identity and lane projections must be distinct projections"
    );
    assert_ne!(
        identity.canonical_fingerprint(),
        lane.canonical_fingerprint(),
        "identity and lane quotients must be distinct objects"
    );
}

// ---------------------------------------------------------------------------
// render-matrix helpers
// ---------------------------------------------------------------------------

fn params(depth: ColorDepth, glyph: GlyphMode, axes_on: [bool; 8], refusal: bool) -> VisualParams {
    let bars = vec![
        (0.0f32, 1.0f32, 60i32),
        (1.0, 1.0, 62),
        (2.0, 1.0, 64),
        (3.0, 1.0, 65),
        (4.0, 1.0, 67),
    ];
    VisualParams {
        axes_on,
        axis_known: [true; 8],
        axis_conformance: [Some(true); 8],
        refusal_axes: std::array::from_fn(|i| refusal && i == 3),
        refusal,
        time: 0.7,
        seed: 42,
        palette: palette_for("VAPOR95"),
        color_depth: depth,
        glyph,
        particles: Vec::new(),
        cover_notes: bars.len(),
        source_notes: bars.len(),
        cover_bars: bars.clone(),
        source_bars: bars,
        cover_beats: 8.0,
        source_beats: 8.0,
        envelope: (0..64).map(|i| ((i as f32) * 0.31).sin().abs()).collect(),
        playhead: 0.3,
        status: "matrix".to_string(),
        world_name: "VAPOR95".to_string(),
        fidelity: "Interpretive".to_string(),
        theseus_pct: 100.0,
    }
}

fn nonblank(surface: &gibson::surface::Surface) -> usize {
    surface
        .to_visible_lines()
        .iter()
        .filter(|l| l.chars().any(|c| !c.is_whitespace()))
        .count()
}

// 7. The renderer must never panic, blank, or lose the quotient marker across
//    the full geometry × capability × glyph matrix — AND a removed axis must
//    visibly disappear from the bridge (PURGED), while a refusal lights the
//    involved axis (REFUSED).
#[test]
fn adversarial_07_render_matrix_never_panics_and_removed_axes_disappear() {
    let geometries = [(42u16, 15u16), (60, 20), (80, 24), (120, 40), (160, 50)];
    let caps = [
        ColorDepth::TrueColor,
        ColorDepth::Ansi256,
        ColorDepth::Ansi16,
        ColorDepth::Mono,
    ];
    let glyphs = [
        GlyphMode::HalfBlock,
        GlyphMode::Block,
        GlyphMode::Braille,
        GlyphMode::Ascii,
    ];

    for (w, h) in geometries {
        for cap in caps {
            for glyph in glyphs {
                let p = params(cap, glyph, [true; 8], false);
                let frame = build_frame(w, h, &p);
                assert_eq!(frame.width, w, "{w}x{h} {cap:?} {glyph:?} width");
                assert_eq!(frame.height, h, "{w}x{h} {cap:?} {glyph:?} height");
                assert_eq!(frame.to_visible_lines().len(), h as usize);
                assert!(
                    nonblank(&frame) > 0,
                    "{w}x{h} {cap:?} {glyph:?} rendered blank"
                );
                assert!(
                    frame.to_visible_lines().iter().any(|l| l.contains("QUOTIENT")),
                    "{w}x{h} {cap:?} {glyph:?} lost the quotient marker"
                );

                // The reactor surface itself must also realize without panicking.
                let reactor = identity_panel(w.saturating_sub(2).max(4), h.saturating_sub(4).max(3), &p);
                assert!(reactor.width > 0 && reactor.height > 0);
            }
        }
    }

    // A removed axis is visibly purged; a preserved one is visibly present.
    let big = (120u16, 40u16);
    let mut all_on = [true; 8];
    all_on[7] = false;
    let frame = build_frame(big.0, big.1, &params(ColorDepth::TrueColor, GlyphMode::HalfBlock, all_on, false));
    let text = frame.to_visible_lines().join("\n");
    assert!(text.contains("PRESERVED"), "preserved axes must be labelled");
    assert!(text.contains("PURGED"), "a removed axis must visibly disappear from the bridge");

    // A refusal is a first-class visual event involving named axes.
    let refused = build_frame(
        big.0,
        big.1,
        &params(
            ColorDepth::TrueColor,
            GlyphMode::HalfBlock,
            [true; 8],
            true,
        ),
    );
    let rtext = refused.to_visible_lines().join("\n");
    assert!(rtext.contains("LAWFUL REFUSAL"), "refusal must be announced");
    assert!(rtext.contains("REFUSED"), "the involved axis must be marked REFUSED");
}
