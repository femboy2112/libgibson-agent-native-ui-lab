use gibson_ui_lab::reactions::{Metaphor, Mode, ReactionConfig, Reactions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|s| s == "--help") {
        println!("Semantic reaction experiment: --mode=none|restrained|heavy --metaphor=constellation|paperwork\n--config-json='{{...}}' admits only the documented bounded policy fields.");
    }
    let value = |prefix: &str| args.iter().find_map(|a| a.strip_prefix(prefix));
    let config = value("--config-json=")
        .map(ReactionConfig::from_json)
        .transpose()?
        .unwrap_or_default();
    let app = Reactions::new(
        Mode::parse(value("--mode=").unwrap_or("restrained"))?,
        Metaphor::parse(value("--metaphor=").unwrap_or("constellation"))?,
        config,
    )?;
    gibson_ui_lab::ui::run(app)
}
