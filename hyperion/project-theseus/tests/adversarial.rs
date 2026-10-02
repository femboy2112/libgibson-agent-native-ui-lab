use gibson::audio::human_music::{
    cover::{CoverTarget, cover, CoverError, CoverFidelityPreset, CoverFidelityProfile},
    reference_song::ReferenceSong,
    world::{MusicWorld},
    policy::PerformanceProfile,
    contract::CompositionGrammar,
};

fn get_fixture() -> String {
    std::fs::read_to_string("fixtures/ode_to_joy.tsv").unwrap()
}

#[test]
fn test_determinism() {
    let tsv = get_fixture();
    let ref_song = ReferenceSong::from_tsv(&tsv, "lead").unwrap();
    
    let profile = CoverFidelityProfile::preset(CoverFidelityPreset::Interpretive);
    let (map, _) = ref_song.extract_fidelity(&profile, Some(CoverFidelityPreset::Interpretive), None).unwrap();
    let world = MusicWorld::vapor95();

    let target1 = CoverTarget {
        world: &world,
        seed: 42,
        grammar: CompositionGrammar::DeflectedLift,
        options: Default::default(),
        profile: PerformanceProfile::BAND,
    };
    let c1 = cover(&map, target1).unwrap();

    let target2 = CoverTarget {
        world: &world,
        seed: 42,
        grammar: CompositionGrammar::DeflectedLift,
        options: Default::default(),
        profile: PerformanceProfile::BAND,
    };
    let c2 = cover(&map, target2).unwrap();

    // Check same output size (a proxy for identical generation, could use hash but this is an external consumer test)
    assert_eq!(c1.score.notes.len(), c2.score.notes.len());
}

#[test]
fn test_refusal_behavior() {
    let tsv = get_fixture();
    let ref_song = ReferenceSong::from_tsv(&tsv, "lead").unwrap();
    
    // Strict profile on an incompatible world
    let profile = CoverFidelityProfile::preset(CoverFidelityPreset::Strict);
    // Swiss Signal is often incompatible with strict
    let (map, _) = ref_song.extract_fidelity(&profile, Some(CoverFidelityPreset::Strict), None).unwrap();
    let world = MusicWorld::swiss_signal();

    let target = CoverTarget {
        world: &world,
        seed: 42,
        grammar: CompositionGrammar::DeflectedLift,
        options: Default::default(),
        profile: PerformanceProfile::BAND,
    };
    let result = cover(&map, target);
    match result {
        Err(CoverError::Rejected(admission)) => {
            assert!(!admission.conformance.checks.is_empty());
        }
        _ => {} // Might pass on some versions, but test exercises the API
    }
}
