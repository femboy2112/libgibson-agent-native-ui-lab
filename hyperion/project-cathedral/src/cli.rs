//! Command-line surface.

use gibson::capability::ColorDepth;

pub const HELP: &str = "\
PROJECT CATHEDRAL — a deterministic distributed-system incident as architecture and music

USAGE:
  cathedral [OPTIONS]

VIEW:
  (none)                 interactive live cathedral
  --frames=N             run N deterministic frames then exit (headless if not a TTY)
  --dump                 print the final ANSI frame instead of drawing interactively
  --text                 print the final frame as plain readable text (no ANSI)
  --at=N                 jump to frame N and print one frame (deterministic seek)
  --width=N --height=N   force the render size
  --color=auto|truecolor|256|16|mono
  --capability           run the capability matrix (5 sizes x 4 color depths)
  --json                 machine-readable summary where applicable

INCIDENT:
  --wtf                  drive the scripted deterministic catastrophic scenario
  --seed=N               journal + score seed (default 0xCA7EDBA5202604; the
                         topology fixture is fixed and deterministic)
  --fps=N                cadence for headless frame-time accounting (default 20)

MUSIC:
  --music                rebuild checked BAND performances live (default on)
  --no-music             disable the music director
  --world=ice|vapor|swiss
  --music-out=DIR        render the final checked performance to DIR/cathedral.wav + hash
  --play                 stream the score to the default output device through
                         libgibson's audio-cpal backend (ALSA on Linux); interactive
                         mode autoplays the first take. W exports and plays.
  --play-seconds=N       headless: audition only the first N seconds, then stop

REPLAY:
  --record=PATH          write the action journal JSON
  --replay=PATH          replay fixture+seed+journal and verify equivalent semantic state

  --help
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorChoice {
    Auto,
    TrueColor,
    Ansi256,
    Ansi16,
    Mono,
}

impl ColorChoice {
    pub fn depth(self) -> ColorDepth {
        match self {
            ColorChoice::Auto | ColorChoice::TrueColor => ColorDepth::TrueColor,
            ColorChoice::Ansi256 => ColorDepth::Ansi256,
            ColorChoice::Ansi16 => ColorDepth::Ansi16,
            ColorChoice::Mono => ColorDepth::Mono,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    pub seed: u64,
    pub frames: Option<u64>,
    pub at: Option<u64>,
    pub fps: u32,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub color: ColorChoice,
    pub dump: bool,
    pub text: bool,
    pub capability: bool,
    pub json: bool,
    pub wtf: bool,
    pub music: bool,
    pub world: String,
    pub music_out: Option<String>,
    pub play: bool,
    pub play_seconds: Option<u32>,
    pub record: Option<String>,
    pub replay: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            seed: 0x00CA_7EDB_A520_2604,
            frames: None,
            at: None,
            fps: 20,
            width: None,
            height: None,
            color: ColorChoice::Auto,
            dump: false,
            text: false,
            capability: false,
            json: false,
            wtf: false,
            music: true,
            world: "ice".into(),
            music_out: None,
            play: false,
            play_seconds: None,
            record: None,
            replay: None,
        }
    }
}

impl Options {
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Options, String> {
        let mut o = Options::default();
        let it = args.into_iter().peekable();
        for a in it {
            let (key, val) = match a.split_once('=') {
                Some((k, v)) => (k.to_string(), Some(v.to_string())),
                None => (a.clone(), None),
            };
            match key.as_str() {
                "--help" | "-h" => return Err("help".into()),
                "--frames" => o.frames = Some(num(val.as_deref(), "--frames")? as u64),
                "--at" => o.at = Some(num(val.as_deref(), "--at")? as u64),
                "--fps" => o.fps = num(val.as_deref(), "--fps")?.max(1),
                "--width" => o.width = Some(num(val.as_deref(), "--width")? as u16),
                "--height" => o.height = Some(num(val.as_deref(), "--height")? as u16),
                "--seed" => {
                    let v = val.as_deref().ok_or("--seed needs a value")?;
                    o.seed = parse_u64(v)?;
                }
                "--color" => {
                    o.color = match val.as_deref() {
                        Some("truecolor") | Some("true") => ColorChoice::TrueColor,
                        Some("256") | Some("ansi256") => ColorChoice::Ansi256,
                        Some("16") | Some("ansi16") => ColorChoice::Ansi16,
                        Some("mono") | Some("none") => ColorChoice::Mono,
                        Some("auto") | None => ColorChoice::Auto,
                        Some(other) => return Err(format!("unknown color: {other}")),
                    }
                }
                "--world" => {
                    o.world = val.ok_or("--world needs a value")?;
                }
                "--music-out" => o.music_out = Some(val.ok_or("--music-out needs a value")?),
                "--record" => o.record = Some(val.ok_or("--record needs a value")?),
                "--replay" => o.replay = Some(val.ok_or("--replay needs a value")?),
                "--dump" => o.dump = true,
                "--text" => o.text = true,
                "--capability" => o.capability = true,
                "--json" => o.json = true,
                "--wtf" => o.wtf = true,
                "--music" => o.music = true,
                "--no-music" => o.music = false,
                "--play" => o.play = true,
                "--play-seconds" => {
                    o.play_seconds = Some(num(val.as_deref(), "--play-seconds")?);
                }
                other if other.starts_with("--") => {
                    return Err(format!("unknown option: {other}"));
                }
                _ => {}
            }
        }
        Ok(o)
    }
}

fn num(v: Option<&str>, flag: &str) -> Result<u32, String> {
    let s = v.ok_or_else(|| format!("{flag} needs a value"))?;
    s.parse::<u32>().map_err(|e| format!("{flag}: {e}"))
}

fn parse_u64(s: &str) -> Result<u64, String> {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).map_err(|e| format!("seed: {e}"))
    } else {
        s.parse::<u64>().map_err(|e| format!("seed: {e}"))
    }
}
