use clap::Parser;
use gibson::audio::human_music::{
    cover::{CoverMap, CoverTarget, cover, CoverAdmission, CoverError, CoverFidelityPreset, CoverFidelityProfile, LineRelation, HarmonyRelation, GrooveRelation, FormRelation, OrchestrationRelation},
    reference_song::ReferenceSong,
    world::MusicWorld,
    functor::Composition,
    policy::PerformanceProfile,
    contract::CompositionGrammar,
};
use gibson::ui::{
    App, AppEvent, BuildCx, Element, Control, skins,
    element::*,
    style::{Tone, Emphasis}
};
use gibson::input::{Event, KeyCode};
use std::io;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "fixtures/ode_to_joy.tsv")]
    fixture: String,
    #[arg(long, default_value = "12345")]
    seed: u64,
    #[arg(long, default_value = "VAPOR95")]
    world: String,
    #[arg(long, default_value = "Interpretive")]
    fidelity: String,
    #[arg(long)]
    headless: Option<String>,
}

struct Model {
    source: ReferenceSong,
    world: MusicWorld,
    seed: u64,
    preset: CoverFidelityPreset,
    axes_on: [bool; 8],
    quotient: Option<CoverMap>,
    admission: Option<CoverAdmission>,
    cover_comp: Option<Composition>,
    error_msg: Option<String>,
}

fn regenerate(model: &mut Model) {
    let mut profile = CoverFidelityProfile::preset(model.preset);
    if !model.axes_on[0] { profile.motif = LineRelation::Free; }
    if !model.axes_on[1] { profile.riff = LineRelation::Free; }
    if !model.axes_on[2] { profile.groove = GrooveRelation::Free; }
    if !model.axes_on[3] && !model.axes_on[4] { profile.harmony = HarmonyRelation::Free; }
    if !model.axes_on[5] { profile.form = FormRelation::Free; }
    if !model.axes_on[6] { profile.orchestration = OrchestrationRelation::Free; }
    if !model.axes_on[7] { profile.bass = LineRelation::Free; }

    match model.source.extract_fidelity(&profile, Some(model.preset), None) {
        Ok((map, _report)) => {
            model.quotient = Some(map.clone());
            let target = CoverTarget {
                world: &model.world,
                seed: model.seed,
                grammar: CompositionGrammar::DeflectedLift,
                options: Default::default(),
                profile: PerformanceProfile::BAND,
            };
            match cover(&map, target) {
                Ok(comp) => {
                    model.cover_comp = Some(comp);
                    model.admission = None;
                    model.error_msg = None;
                }
                Err(CoverError::Rejected(admission)) => {
                    model.cover_comp = None;
                    model.admission = Some(*admission);
                    model.error_msg = Some("Refused by World".into());
                }
                Err(e) => {
                    model.cover_comp = None;
                    model.admission = None;
                    model.error_msg = Some(format!("{:?}", e));
                }
            }
        }
        Err(e) => {
            model.quotient = None;
            model.cover_comp = None;
            model.admission = None;
            model.error_msg = Some(format!("Extraction error: {:?}", e));
        }
    }
}

fn view(model: &Model, _cx: &BuildCx) -> Element<()> {
    let top_bar = text(" PROJECT THESEUS - HUMAN_MUSIC QUOTIENT MACHINE ")
        .tone(Tone::Accent)
        .emphasis(Emphasis::Strong);

    let left = card("REFERENCE SOURCE")
        .child(text(" [ Ode to Joy ] "))
        .child(text(format!(" Seed: {}", model.seed)));

    let right_status = if model.cover_comp.is_some() {
        "FRESH COVER (GENERATED)"
    } else {
        "FRESH COVER (FAILED)"
    };
    let right = card(right_status)
        .child(text(format!(" Target: {}", model.world.name)));

    let top = row().child(left).child(spacer()).child(right);

    let mut q_col = panel("THE COVERMAP (IDENTITY QUOTIENT)");
    if let Some(_map) = &model.quotient {
        let motif_view = if model.axes_on[0] { text("MOTIF: [======]").tone(Tone::Success) } else { text("MOTIF: [      ]").tone(Tone::Neutral).emphasis(Emphasis::Faint) };
        let riff_view = if model.axes_on[1] { text("RIFF : [======]").tone(Tone::Success) } else { text("RIFF : [      ]").tone(Tone::Neutral).emphasis(Emphasis::Faint) };
        let groove_view = if model.axes_on[2] { text("GROOV: [======]").tone(Tone::Success) } else { text("GROOV: [      ]").tone(Tone::Neutral).emphasis(Emphasis::Faint) };
        let harm_view = if model.axes_on[3] { text("HARMO: [======]").tone(Tone::Success) } else { text("HARMO: [      ]").tone(Tone::Neutral).emphasis(Emphasis::Faint) };
        let form_view = if model.axes_on[5] { text("FORM : [======]").tone(Tone::Success) } else { text("FORM : [      ]").tone(Tone::Neutral).emphasis(Emphasis::Faint) };
        
        q_col = q_col.child(motif_view).child(riff_view).child(groove_view).child(harm_view).child(form_view);
    } else {
        q_col = q_col.child(text("NO QUOTIENT").tone(Tone::Warning));
    }

    if let Some(msg) = &model.error_msg {
        q_col = q_col.child(text(format!("ERROR: {}", msg)).tone(Tone::Danger));
    }
    if let Some(admission) = &model.admission {
        for check in &admission.conformance.checks {
            if !check.passed {
                q_col = q_col.child(text(format!("FAILED: {:?} - {}", check.axis, check.detail)).tone(Tone::Danger));
            }
        }
    }

    let controls = panel("CONTROL PANEL")
        .child(text(format!("Fidelity Preset (f): {}", model.preset.label())).tone(Tone::Accent))
        .child(text(format!("World (w): {}", model.world.name)).tone(Tone::Accent))
        .child(text("Toggle axes: 1:Motif 2:Riff 3:Groove 4:HarmContour 5:HarmLoop 6:Form 7:Orchestration 8:BassFigure"));

    screen().child(
        column()
            .child(top_bar)
            .child(top)
            .child(q_col)
            .child(controls)
    )

}

fn update(model: &mut Model, event: AppEvent<()>) -> Control {
    if let AppEvent::Input(Event::Key(k)) = event {
        match k.code {
            KeyCode::Char('q') | KeyCode::Esc => return Control::Quit,
            KeyCode::Char('f') => {
                model.preset = match model.preset {
                    CoverFidelityPreset::Loose => CoverFidelityPreset::Interpretive,
                    CoverFidelityPreset::Interpretive => CoverFidelityPreset::Faithful,
                    CoverFidelityPreset::Faithful => CoverFidelityPreset::Strict,
                    CoverFidelityPreset::Strict => CoverFidelityPreset::Loose,
                };
                regenerate(model);
            }
            KeyCode::Char('1') => { model.axes_on[0] = !model.axes_on[0]; regenerate(model); }
            KeyCode::Char('2') => { model.axes_on[1] = !model.axes_on[1]; regenerate(model); }
            KeyCode::Char('3') => { model.axes_on[2] = !model.axes_on[2]; regenerate(model); }
            KeyCode::Char('4') => { model.axes_on[3] = !model.axes_on[3]; regenerate(model); }
            KeyCode::Char('5') => { model.axes_on[4] = !model.axes_on[4]; regenerate(model); }
            KeyCode::Char('6') => { model.axes_on[5] = !model.axes_on[5]; regenerate(model); }
            KeyCode::Char('7') => { model.axes_on[6] = !model.axes_on[6]; regenerate(model); }
            KeyCode::Char('8') => { model.axes_on[7] = !model.axes_on[7]; regenerate(model); }
            KeyCode::Char('w') => {
                if model.world.name == "VAPOR95" {
                    model.world = MusicWorld::black_ice();
                } else if model.world.name == "BLACK_ICE" {
                    model.world = MusicWorld::swiss_signal();
                } else {
                    model.world = MusicWorld::vapor95();
                }
                regenerate(model);
            }
            KeyCode::Char('r') => {
                regenerate(model);
            }
            _ => {}
        }
    }
    Control::Continue
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    
    let tsv = std::fs::read_to_string(&args.fixture).unwrap_or_else(|_| "".into());
    let ref_song = ReferenceSong::from_tsv(&tsv, "lead").unwrap();

    let mut model = Model {
        source: ref_song,
        world: MusicWorld::vapor95(),
        seed: args.seed,
        preset: CoverFidelityPreset::Interpretive,
        axes_on: [true; 8],
        quotient: None,
        admission: None,
        cover_comp: None,
        error_msg: None,
    };
    regenerate(&mut model);

    App::fullscreen().skin(skins::VAPOR95).run(model, update, view)?;
    Ok(())
}
