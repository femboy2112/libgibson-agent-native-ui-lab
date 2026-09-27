//! Command interpreter for the Galileo mission control console.

use crate::render::scale::PrimaryView;
use crate::render::tomography_view::RadarBand;

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    SetView(ViewTarget),
    Zoom(ZoomAction),
    SetTarget(String),
    ExecuteBurn(f32),
    SetRadarBand(RadarBand),
    AcknowledgeAlert,
    SetSkin(String),
    SetWarpRate(f32),
    Pause,
    Resume,
    TogglePause,
    SetGate(f32),
    SetGain(f32),
    Rotate(f32, f32),
    Scrub(f32),
    ToggleThermal,
    ToggleMagnetic,
    ToggleAutoOrbit,
    ToggleAutoRotate,
    SetInstrumentMode(String, String),
    Help,
    Unknown(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewTarget {
    System,
    Trajectory,
    Surface,
    Tomography,
    MissionControl,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ZoomAction {
    In,
    Out,
    Level(PrimaryView),
    Value(f32),
}

pub struct CommandParser;

impl CommandParser {
    pub fn parse(input: &str) -> Option<Command> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        let cmd = parts[0].to_lowercase();
        let arg1 = parts.get(1).map(|s| s.to_lowercase());
        let arg2 = parts.get(2).map(|s| s.to_lowercase());

        match cmd.as_str() {
            "1" | "system" | "sys" => Some(Command::SetView(ViewTarget::System)),
            "2" | "traj" | "trajectory" => Some(Command::SetView(ViewTarget::Trajectory)),
            "3" | "surf" | "surface" => Some(Command::SetView(ViewTarget::Surface)),
            "4" | "tomo" | "tomography" | "radar" => Some(Command::SetView(ViewTarget::Tomography)),
            "5" | "control" | "ops" | "mc" => Some(Command::SetView(ViewTarget::MissionControl)),

            "view" | "v" => match arg1.as_deref() {
                Some("1") | Some("system") | Some("sys") => {
                    Some(Command::SetView(ViewTarget::System))
                }
                Some("2") | Some("traj") | Some("trajectory") => {
                    Some(Command::SetView(ViewTarget::Trajectory))
                }
                Some("3") | Some("surf") | Some("surface") => {
                    Some(Command::SetView(ViewTarget::Surface))
                }
                Some("4") | Some("tomo") | Some("tomography") => {
                    Some(Command::SetView(ViewTarget::Tomography))
                }
                Some("5") | Some("control") | Some("ops") | Some("mc") => {
                    Some(Command::SetView(ViewTarget::MissionControl))
                }
                _ => Some(Command::Unknown(trimmed.to_string())),
            },

            "zoom" | "z" => match arg1.as_deref() {
                Some("in") | Some("+") => Some(Command::Zoom(ZoomAction::In)),
                Some("out") | Some("-") => Some(Command::Zoom(ZoomAction::Out)),
                Some("system") | Some("sys") => {
                    Some(Command::Zoom(ZoomAction::Level(PrimaryView::System)))
                }
                Some("orbit") | Some("traj") => {
                    Some(Command::Zoom(ZoomAction::Level(PrimaryView::Trajectory)))
                }
                Some("surface") | Some("surf") => {
                    Some(Command::Zoom(ZoomAction::Level(PrimaryView::Surface)))
                }
                Some("ice") | Some("tomo") | Some("ocean") => {
                    Some(Command::Zoom(ZoomAction::Level(PrimaryView::Tomography)))
                }
                Some(val_str) => {
                    if let Ok(v) = val_str.parse::<f32>() {
                        Some(Command::Zoom(ZoomAction::Value(v)))
                    } else {
                        Some(Command::Unknown(trimmed.to_string()))
                    }
                }
                None => Some(Command::Zoom(ZoomAction::In)),
            },

            "target" | "tgt" => {
                if let Some(target) = arg1 {
                    Some(Command::SetTarget(target))
                } else {
                    Some(Command::Unknown(
                        "target requires name: io | europa | ganymede | callisto".to_string(),
                    ))
                }
            }

            "burn" | "dv" => {
                let dv = arg1.and_then(|s| s.parse::<f32>().ok()).unwrap_or(25.0);
                Some(Command::ExecuteBurn(dv))
            }

            "band" | "freq" => match arg1.as_deref() {
                Some("hf") | Some("9") | Some("9mhz") => {
                    Some(Command::SetRadarBand(RadarBand::Hf9MHz))
                }
                Some("vhf") | Some("60") | Some("60mhz") => {
                    Some(Command::SetRadarBand(RadarBand::Vhf60MHz))
                }
                Some("split") | Some("dual") => Some(Command::SetRadarBand(RadarBand::SplitBand)),
                _ => Some(Command::Unknown(
                    "band requires 'hf' (9MHz), 'vhf' (60MHz), or 'split'".to_string(),
                )),
            },

            "gate" | "depth" | "cursor" => {
                if let Some(val) = arg1.and_then(|s| s.parse::<f32>().ok()) {
                    Some(Command::SetGate(val))
                } else {
                    Some(Command::Unknown(
                        "gate requires depth in km, e.g. 'gate 18.6'".to_string(),
                    ))
                }
            }

            "gain" => {
                if let Some(val) = arg1.and_then(|s| s.parse::<f32>().ok()) {
                    Some(Command::SetGain(val))
                } else {
                    Some(Command::Unknown(
                        "gain requires dB, e.g. 'gain 42'".to_string(),
                    ))
                }
            }

            "warp" | "speed" | "rate" => {
                if let Some(val) = arg1.and_then(|s| s.parse::<f32>().ok()) {
                    Some(Command::SetWarpRate(val))
                } else {
                    Some(Command::Unknown(
                        "warp requires multiplier: 1, 10, 60, 300".to_string(),
                    ))
                }
            }

            "pause" | "stop" => Some(Command::Pause),
            "resume" | "play" => Some(Command::Resume),

            "scrub" => {
                if let Some(val) = arg1.and_then(|s| s.parse::<f32>().ok()) {
                    Some(Command::Scrub(val))
                } else {
                    Some(Command::Unknown(
                        "scrub requires hours, e.g. 'scrub 66.5'".to_string(),
                    ))
                }
            }

            "rotate" => {
                let lon = arg1.and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
                let lat = arg2.and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
                Some(Command::Rotate(lon, lat))
            }

            "thermal" | "ir" => Some(Command::ToggleThermal),
            "mag" | "magnetic" | "bfield" => Some(Command::ToggleMagnetic),
            "orbit" => Some(Command::ToggleAutoOrbit),
            "spin" => Some(Command::ToggleAutoRotate),

            "instrument" | "inst" | "subsystem" => {
                if let (Some(name), Some(mode)) = (arg1, arg2) {
                    Some(Command::SetInstrumentMode(name, mode))
                } else {
                    Some(Command::Unknown(
                        "instrument requires name and mode: e.g. 'instrument reason active'"
                            .to_string(),
                    ))
                }
            }

            "alert" | "ack" => Some(Command::AcknowledgeAlert),

            "skin" | "theme" => {
                if let Some(skin_name) = arg1 {
                    Some(Command::SetSkin(skin_name))
                } else {
                    Some(Command::Unknown(
                        "skin requires name: black_ice | vapor95 | swiss".to_string(),
                    ))
                }
            }

            "help" | "?" => Some(Command::Help),

            _ => Some(Command::Unknown(trimmed.to_string())),
        }
    }
}
