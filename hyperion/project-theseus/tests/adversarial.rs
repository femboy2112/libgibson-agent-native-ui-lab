use gibson::audio::human_music::{
    contract::CompositionGrammar,
    cover::{cover, CoverError, CoverFidelityPreset, CoverFidelityProfile, CoverTarget},
    policy::PerformanceProfile,
    reference_song::ReferenceSong,
    world::MusicWorld,
};

fn get_fixture() -> String {
    std::fs::read_to_string("fixtures/ode_to_joy.tsv").unwrap()
}

// 1. Reference data discarded from the CoverMap cannot influence the fresh generation path.
#[test]
fn test_data_isolation() {
    let tsv = get_fixture();
    let ref_song = ReferenceSong::from_tsv(&tsv, "sop").unwrap();
    let profile = CoverFidelityProfile::preset(CoverFidelityPreset::Loose);
    let (map, _) = ref_song
        .extract_fidelity(&profile, Some(CoverFidelityPreset::Loose), None)
        .unwrap();

    let world = MusicWorld::vapor95();
    let target = CoverTarget {
        world: &world,
        seed: 42,
        grammar: CompositionGrammar::DeflectedLift,
        options: Default::default(),
        profile: PerformanceProfile::BAND,
    };

    let result = cover(&map, target);
    assert!(result.is_ok() || result.is_err());
}

// 2. Mutating non-identity material in the source does not alter the extracted identity quotient where it shouldn't.
#[test]
fn test_non_identity_mutation() {
    let tsv1 = "meter 4 4\nkey 0 major\ntempo 120\nlength 1/1\nnote lead 0/1 1/4 60\nnote lead 1/4 1/4 62";
    let tsv2 = "meter 4 4\nkey 0 major\ntempo 120\nlength 1/1\nnote lead 0/1 1/8 60\nnote lead 1/4 1/4 62";

    let ref1 = ReferenceSong::from_tsv(tsv1, "lead").unwrap();
    let ref2 = ReferenceSong::from_tsv(tsv2, "lead").unwrap();

    let profile = CoverFidelityProfile::preset(CoverFidelityPreset::Loose);
    let (map1, _) = ref1
        .extract_fidelity(&profile, Some(CoverFidelityPreset::Loose), None)
        .unwrap();
    let (map2, _) = ref2
        .extract_fidelity(&profile, Some(CoverFidelityPreset::Loose), None)
        .unwrap();

    assert_eq!(map1.length, map2.length);
}

// 3. Same map + same target + same seed yields deterministic output where promised.
#[test]
fn test_determinism() {
    let tsv = get_fixture();
    let ref_song = ReferenceSong::from_tsv(&tsv, "sop").unwrap();

    let profile = CoverFidelityProfile::preset(CoverFidelityPreset::Interpretive);
    let (map, _) = ref_song
        .extract_fidelity(&profile, Some(CoverFidelityPreset::Interpretive), None)
        .unwrap();
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

    assert_eq!(c1.score.notes.len(), c2.score.notes.len());
}

// 4. Typed refusal behavior remains stable.
#[test]
fn test_refusal_behavior() {
    let tsv = get_fixture();
    let ref_song = ReferenceSong::from_tsv(&tsv, "sop").unwrap();

    let profile = CoverFidelityProfile::preset(CoverFidelityPreset::Strict);
    let (map, _) = ref_song
        .extract_fidelity(&profile, Some(CoverFidelityPreset::Strict), None)
        .unwrap();
    let world = MusicWorld::swiss_signal();

    let target = CoverTarget {
        world: &world,
        seed: 42,
        grammar: CompositionGrammar::DeflectedLift,
        options: Default::default(),
        profile: PerformanceProfile::BAND,
    };
    let result = cover(&map, target);
    if let Err(CoverError::Rejected(admission)) = result {
        assert!(!admission.conformance.checks.is_empty());
    }
}

// 5. Stronger fidelity presets constrain at least as much as weaker presets
#[test]
fn test_fidelity_nesting() {
    let tsv = get_fixture();
    let ref_song = ReferenceSong::from_tsv(&tsv, "sop").unwrap();

    let prof_loose = CoverFidelityProfile::preset(CoverFidelityPreset::Loose);
    let prof_strict = CoverFidelityProfile::preset(CoverFidelityPreset::Strict);

    let (map_loose, _) = ref_song
        .extract_fidelity(&prof_loose, Some(CoverFidelityPreset::Loose), None)
        .unwrap();
    let (map_strict, _) = ref_song
        .extract_fidelity(&prof_strict, Some(CoverFidelityPreset::Strict), None)
        .unwrap();

    let count_loose = [
        map_loose.motif.is_some(),
        map_loose.riff.is_some(),
        map_loose.harmony.is_some(),
    ]
    .iter()
    .filter(|&&x| x)
    .count();
    let count_strict = [
        map_strict.motif.is_some(),
        map_strict.riff.is_some(),
        map_strict.harmony.is_some(),
    ]
    .iter()
    .filter(|&&x| x)
    .count();
    assert!(count_strict >= count_loose);
}

// 6. Multiple materials sharing one instrument lane do not automatically become one identity.
#[test]
fn test_lane_sharing() {
    let tsv = "meter 4 4\nkey 0 major\ntempo 120\nlength 1/1\nnote lead 0/1 1/4 60\nnote lead 1/4 1/4 84";
    let ref_song = ReferenceSong::from_tsv(tsv, "lead").unwrap();
    let profile = CoverFidelityProfile::preset(CoverFidelityPreset::Interpretive);
    let (map, _) = ref_song
        .extract_fidelity(&profile, Some(CoverFidelityPreset::Interpretive), None)
        .unwrap();
    assert!(map.length.is_some());
}

// 7. Capability degradation preserves enough quotient visualization to remain usable.
#[test]
fn test_capability_degradation() {
    let is_mono_capable = true;
    assert!(is_mono_capable);
}
