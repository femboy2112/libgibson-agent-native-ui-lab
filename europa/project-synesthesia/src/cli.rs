use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    Auto,
    TrueColor,
    Ansi256,
    Ansi16,
    Mono,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub demo: bool,
    pub silent: bool,
    pub seed: u64,
    pub at_ms: Option<u64>,
    pub pattern: String,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub color: ColorChoice,
    pub profile: bool,
    pub dump: bool,
    pub performance: bool,
    pub fps: u32,
    pub frames: Option<u64>,
    pub glyphs: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            demo: false,
            silent: false,
            seed: 2112,
            at_ms: None,
            pattern: "pulse".into(),
            width: None,
            height: None,
            color: ColorChoice::Auto,
            profile: false,
            dump: false,
            performance: false,
            fps: 30,
            frames: None,
            glyphs: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, ParseError> {
        let mut out = Self::default();
        for arg in args {
            match arg.as_str() {
                "--demo" => out.demo = true,
                "--silent" => out.silent = true,
                "--truecolor" => out.color = ColorChoice::TrueColor,
                "--ansi256" => out.color = ColorChoice::Ansi256,
                "--ansi16" => out.color = ColorChoice::Ansi16,
                "--mono" => out.color = ColorChoice::Mono,
                "--profile" => out.profile = true,
                "--dump" => out.dump = true,
                "--performance" => out.performance = true,
                "--help" | "-h" => return Err(ParseError("help".into())),
                _ => {
                    if let Some(value) = arg.strip_prefix("--seed=") {
                        out.seed = parse(value, "seed")?;
                    } else if let Some(value) = arg.strip_prefix("--at-ms=") {
                        out.at_ms = Some(parse(value, "at-ms")?);
                    } else if let Some(value) = arg.strip_prefix("--pattern=") {
                        if value.is_empty() {
                            return Err(ParseError("pattern must not be empty".into()));
                        }
                        out.pattern = value.to_ascii_lowercase();
                    } else if let Some(value) = arg.strip_prefix("--width=") {
                        out.width = Some(parse_dimension(value, "width")?);
                    } else if let Some(value) = arg.strip_prefix("--height=") {
                        out.height = Some(parse_dimension(value, "height")?);
                    } else if let Some(value) = arg.strip_prefix("--fps=") {
                        let fps: u32 = parse(value, "fps")?;
                        if !(1..=120).contains(&fps) {
                            return Err(ParseError("fps must be between 1 and 120".into()));
                        }
                        out.fps = fps;
                    } else if let Some(value) = arg.strip_prefix("--frames=") {
                        out.frames = Some(parse(value, "frames")?);
                    } else if let Some(value) = arg.strip_prefix("--glyphs=") {
                        out.glyphs = Some(value.to_ascii_lowercase());
                    } else {
                        return Err(ParseError(format!("unknown option: {arg}")));
                    }
                }
            }
        }
        if out.at_ms.is_some() && !out.silent {
            out.silent = true;
        }
        Ok(out)
    }

    pub fn headless(&self) -> bool {
        self.dump || self.at_ms.is_some() || self.frames.is_some()
    }
}

fn parse<T: std::str::FromStr>(value: &str, name: &str) -> Result<T, ParseError> {
    value
        .parse()
        .map_err(|_| ParseError(format!("invalid {name}: {value}")))
}

fn parse_dimension(value: &str, name: &str) -> Result<u16, ParseError> {
    let n: u16 = parse(value, name)?;
    if n == 0 {
        return Err(ParseError(format!("{name} must be greater than zero")));
    }
    Ok(n)
}

pub const HELP: &str = "SYNESTHESIA — a terminal music instrument\n\
\n\
Usage: synesthesia [OPTIONS]\n\
\n\
  --demo                 Load the deterministic four-track pattern\n\
  --silent               Run the same instrument without audio output\n\
  --seed=N               Deterministic simulation seed (default: 2112)\n\
  --at-ms=N              Render the exact state at N milliseconds, then exit\n\
  --pattern=NAME          pulse, glass, offbeat, or empty\n\
  --width=N --height=N   Virtual terminal dimensions for capture mode\n\
  --truecolor|--ansi256|--ansi16|--mono\n\
  --glyphs=auto|braille|halfblock|block|ascii\n\
  --fps=N                Interactive cadence, 1..120 (default: 30)\n\
  --dump                 Render one headless frame to stdout\n\
  --profile              Print frame, surface, cell-diff, and byte timings\n\
  --performance          Enter the concert visualization\n\
  --frames=N             Render N headless frames and report bounded metrics\n\
\n\
Keys: Space play/stop · arrows move/edit · Enter toggle note · Tab focus\n\
      [ ] BPM · F filter · W waveform · M mute · P performance · Q quit\n";
