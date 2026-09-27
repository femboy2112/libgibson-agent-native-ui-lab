//! Semantic dashboard and Mission Control layer using `gibson::ui`.
//!
//! Provides the primary mission operations interface:
//! - Multi-scale Jovian visualization stage (System, Trajectory, Surface, Tomography, Ops)
//! - Real-time telemetry rails (RTG power, battery SoC, RF link budget, thermal states)
//! - Payload instruments status (REASON, MISE, MASPEX, PIMS, SUDA)
//! - Dynamic mission alert logs and event timeline
//! - Interactive command console with text input and shortcut dispatcher
//! - Authored responsive layouts for 160x50, 120x40, 100x30, 80x24, 60x20

use std::collections::VecDeque;
use std::sync::Arc;

use gibson::cell::{Color, Style};
use gibson::input::TextInputState;
use gibson::surface::Surface;
use gibson::ui::prelude::*;

use crate::render::scale::{PrimaryView, ScaleCoordinator};
use crate::render::surface_view::SurfaceViewRenderer;
use crate::render::system_view::SystemViewRenderer;
use crate::render::tomography_view::{RadarBand, TomographyViewRenderer};
use crate::render::trajectory_view::TrajectoryViewRenderer;
use crate::sim::jovian::{JovianModel, TargetBody};
use crate::sim::telemetry::{AlertSeverity, TelemetryState};
use crate::sim::trajectory::TrajectoryLab;
use crate::ui::command::{Command, CommandParser, ViewTarget, ZoomAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveView {
    System = 1,
    Trajectory = 2,
    Surface = 3,
    Tomography = 4,
    MissionControl = 5,
}

const SCALE_REVEAL_SECONDS: f32 = 0.65;

/// A bounded visual handoff. The next acquisition expands from the same
/// physical focus on the previous surface; navigation state changes at once.
struct ScaleReveal {
    from: ActiveView,
    to: ActiveView,
    elapsed_secs: f32,
}

/// Discover occupied text runs from the two composed scientific surfaces.
/// Keep the complete row span around their labels intact during a scale
/// handoff, independent of where a responsive inspector is positioned.
fn text_span(a: &Surface, b: &Surface, y: u16, width: u16) -> Option<(u16, u16)> {
    let mut first = width;
    let mut last = 0;
    for x in 0..width {
        let has_text = [a, b].iter().any(|surface| {
            surface.get(x, y).is_some_and(|cell| {
                cell.glyph
                    .grapheme
                    .chars()
                    .any(|ch| ch.is_ascii_alphanumeric())
            })
        });
        if has_text {
            first = first.min(x);
            last = x;
        }
    }
    (first < width).then_some((
        first.saturating_sub(1),
        last.saturating_add(1).min(width.saturating_sub(1)),
    ))
}

#[derive(Debug, Clone)]
pub enum DashboardAction {
    SwitchView(ActiveView),
    InputChanged(TextInputState),
    SubmitCommand,
    Zoom(ZoomAction),
    NextTarget,
    PrevTarget,
    ToggleRadarBand,
    ExecuteBurn(f32),
    AcknowledgeAlert,
    CycleSkin,
    Rotate(f32, f32),
    ScrubTimeline(f32),
    MoveGate(f32),
    AdjustGain(f32),
    TogglePause,
    CycleWarpRate,
    NoOp,
}

pub struct DashboardState {
    pub active_view: ActiveView,
    pub sim: JovianModel,
    pub trajectory_lab: TrajectoryLab,
    pub telemetry: TelemetryState,
    pub scale_coordinator: ScaleCoordinator,

    pub system_renderer: SystemViewRenderer,
    pub trajectory_renderer: TrajectoryViewRenderer,
    pub surface_renderer: SurfaceViewRenderer,
    pub tomography_renderer: TomographyViewRenderer,

    pub command_input: TextInputState,
    pub console_history: VecDeque<String>,
    pub status_feedback: String,
    pub current_skin_name: &'static str,
    pub mono_mode: bool,
    scale_reveal: Option<ScaleReveal>,
}

impl DashboardState {
    pub fn new(mono: bool) -> Self {
        let sim = JovianModel::default();
        let trajectory_lab = TrajectoryLab::new(&sim);
        let telemetry = TelemetryState::default();
        let scale_coordinator = ScaleCoordinator::default();

        let system_renderer = SystemViewRenderer::default();
        let trajectory_renderer = TrajectoryViewRenderer::default();
        let surface_renderer = SurfaceViewRenderer::default();
        let tomography_renderer = TomographyViewRenderer::default();

        let mut console_history = VecDeque::with_capacity(16);
        console_history.push_back("GALILEO MISSION OPS KERNEL INITIALIZED".to_string());
        console_history
            .push_back("DSN CANBERRA 70M LINK ESTABLISHED (CARRIER +43.2 dB)".to_string());
        console_history.push_back("TYPE 'help' OR '1'-'5' FOR COMMAND NAVIGATION".to_string());

        Self {
            active_view: ActiveView::System,
            sim,
            trajectory_lab,
            telemetry,
            scale_coordinator,
            system_renderer,
            trajectory_renderer,
            surface_renderer,
            tomography_renderer,
            command_input: TextInputState::new(),
            console_history,
            status_feedback: "SYS: NOMINAL // ALL SYSTEMS GO".to_string(),
            current_skin_name: "BLACK_ICE",
            mono_mode: mono,
            scale_reveal: None,
        }
    }

    pub fn update(&mut self, dt_seconds: f32) {
        let is_paused = self.telemetry.is_paused;
        let warp = self.telemetry.warp_rate;

        if !is_paused {
            let eff_dt = dt_seconds * warp;
            self.sim.update(eff_dt);
            let sc_pos = self
                .sim
                .spacecraft_position_at(self.sim.mission_time_hours)
                .0;
            let in_eclipse = self.sim.is_in_jupiter_shadow(sc_pos);
            self.telemetry
                .update_coupled(self.sim.mission_time_hours, eff_dt, in_eclipse);

            // Synchronize renderers
            self.system_renderer.update(dt_seconds);
            self.trajectory_renderer.update(dt_seconds);
            self.surface_renderer.update(dt_seconds);
            self.tomography_renderer.update(dt_seconds);
        }

        self.scale_coordinator.update(dt_seconds);
        if let Some(reveal) = &mut self.scale_reveal {
            reveal.elapsed_secs += dt_seconds.max(0.0);
            if reveal.elapsed_secs >= SCALE_REVEAL_SECONDS {
                self.scale_reveal = None;
            }
        }
        if self.scale_coordinator.is_transitioning
            && self.scale_coordinator.transition_progress >= 1.0
        {
            self.active_view = match self.scale_coordinator.current_view {
                PrimaryView::System => ActiveView::System,
                PrimaryView::Trajectory => ActiveView::Trajectory,
                PrimaryView::Surface => ActiveView::Surface,
                PrimaryView::Tomography => ActiveView::Tomography,
                PrimaryView::MissionControl => ActiveView::MissionControl,
            };
        }

        self.sync_surface_target();
    }

    /// Preserve the identity and depth of the selected survey site when
    /// descending into the radargram, including deterministic demo captures.
    pub fn sync_surface_target(&mut self) {
        let feat = self.surface_renderer.selected_feature();
        self.tomography_renderer
            .set_feature(feat.name, feat.ice_thickness_km);
    }

    pub(crate) fn set_scale_reveal(&mut self, from: ActiveView, to: ActiveView, elapsed_secs: f32) {
        self.scale_reveal = Some(ScaleReveal {
            from,
            to,
            elapsed_secs,
        });
    }

    pub(crate) fn clear_scale_reveal(&mut self) {
        self.scale_reveal = None;
    }

    pub fn execute_command(&mut self, cmd: Command) {
        match cmd {
            Command::SetView(target) => {
                self.scale_reveal = None;
                self.active_view = match target {
                    ViewTarget::System => ActiveView::System,
                    ViewTarget::Trajectory => ActiveView::Trajectory,
                    ViewTarget::Surface => ActiveView::Surface,
                    ViewTarget::Tomography => ActiveView::Tomography,
                    ViewTarget::MissionControl => ActiveView::MissionControl,
                };
                let primary = match self.active_view {
                    ActiveView::System => PrimaryView::System,
                    ActiveView::Trajectory => PrimaryView::Trajectory,
                    ActiveView::Surface => PrimaryView::Surface,
                    ActiveView::Tomography => PrimaryView::Tomography,
                    ActiveView::MissionControl => PrimaryView::System,
                };
                self.scale_coordinator.snap_to_view(primary);
                self.log_console(format!("VIEW SWITCH: {:?}", self.active_view));
            }
            Command::Zoom(action) => {
                let from_view = self.active_view;
                match action {
                    ZoomAction::In => match self.active_view {
                        ActiveView::System => {
                            if self.system_renderer.camera_distance_scale >= 2.6 {
                                if self.system_renderer.selected_target_idx == 2 {
                                    self.active_view = ActiveView::Trajectory;
                                    self.scale_coordinator.snap_to_view(PrimaryView::Trajectory);
                                    self.trajectory_renderer.zoom = 0.8;
                                    self.log_console("DESCENDING SCALE: JOVIAN SYSTEM -> EUROPA ORBIT [10,000 km]".to_string());
                                } else {
                                    self.log_console("SELECT EUROPA [N/P] TO ENTER THE EUROPA-ONLY ENCOUNTER LAB".to_string());
                                }
                            } else {
                                self.system_renderer.adjust_zoom(1.3);
                                self.scale_coordinator.zoom_level =
                                    (self.scale_coordinator.zoom_level + 0.2).min(0.9);
                                self.log_console(format!(
                                    "SYSTEM ZOOM: {:.2}x",
                                    self.system_renderer.camera_distance_scale
                                ));
                            }
                        }
                        ActiveView::Trajectory => {
                            if self.trajectory_renderer.zoom >= 2.6 {
                                self.active_view = ActiveView::Surface;
                                self.scale_coordinator.snap_to_view(PrimaryView::Surface);
                                self.surface_renderer.zoom = 0.8;
                                self.log_console("DESCENDING SCALE: EUROPA ORBIT -> REGIONAL SURFACE SURVEY [500 km]".to_string());
                            } else {
                                self.trajectory_renderer.adjust_zoom(1.3);
                                self.scale_coordinator.zoom_level =
                                    (self.scale_coordinator.zoom_level + 0.2).min(1.9);
                                self.log_console(format!(
                                    "TRAJECTORY ZOOM: {:.2}x",
                                    self.trajectory_renderer.zoom
                                ));
                            }
                        }
                        ActiveView::Surface => {
                            if self.surface_renderer.zoom >= 2.2 {
                                self.active_view = ActiveView::Tomography;
                                self.scale_coordinator.snap_to_view(PrimaryView::Tomography);
                                self.tomography_renderer.zoom = 0.8;
                                self.log_console("DESCENDING SCALE: SURFACE SURVEY -> ICE SHELL TOMOGRAPHY [30 km]".to_string());
                            } else {
                                self.surface_renderer.adjust_zoom(1.3);
                                self.scale_coordinator.zoom_level =
                                    (self.scale_coordinator.zoom_level + 0.2).min(2.9);
                                self.log_console(format!(
                                    "SURFACE ZOOM: {:.2}x",
                                    self.surface_renderer.zoom
                                ));
                            }
                        }
                        ActiveView::Tomography => {
                            if self.tomography_renderer.zoom >= 2.5
                                && self.tomography_renderer.band == RadarBand::Hf9MHz
                            {
                                self.tomography_renderer.set_band(RadarBand::Vhf60MHz);
                                self.tomography_renderer.zoom = 1.0;
                                self.scale_coordinator.zoom_level = 3.8;
                                self.log_console(
                                    "SWITCHING RADAR: 9 MHz HF -> HIGH-RES 60 MHz VHF SOUNDER"
                                        .to_string(),
                                );
                            } else {
                                self.tomography_renderer.adjust_zoom(1.3);
                                self.scale_coordinator.zoom_level =
                                    (self.scale_coordinator.zoom_level + 0.2).min(4.0);
                                self.log_console(format!(
                                    "TOMOGRAPHY ZOOM: {:.2}x",
                                    self.tomography_renderer.zoom
                                ));
                            }
                        }
                        ActiveView::MissionControl => {
                            self.active_view = ActiveView::System;
                            self.scale_coordinator.snap_to_view(PrimaryView::System);
                            self.log_console(
                                "VIEW SWITCH: MISSION CONTROL -> JOVIAN SYSTEM".to_string(),
                            );
                        }
                    },
                    ZoomAction::Out => {
                        match self.active_view {
                            ActiveView::System => {
                                self.system_renderer.adjust_zoom(0.77);
                                self.scale_coordinator.zoom_level =
                                    (self.scale_coordinator.zoom_level - 0.2).max(0.0);
                                self.log_console(format!(
                                    "SYSTEM ZOOM: {:.2}x",
                                    self.system_renderer.camera_distance_scale
                                ));
                            }
                            ActiveView::Trajectory => {
                                if self.trajectory_renderer.zoom <= 0.65 {
                                    self.active_view = ActiveView::System;
                                    self.scale_coordinator.snap_to_view(PrimaryView::System);
                                    self.system_renderer.camera_distance_scale = 2.2;
                                    self.log_console("ASCENDING SCALE: EUROPA ORBIT -> MACRO JOVIAN SYSTEM [2,000,000 km]".to_string());
                                } else {
                                    self.trajectory_renderer.adjust_zoom(0.77);
                                    self.scale_coordinator.zoom_level =
                                        (self.scale_coordinator.zoom_level - 0.2).max(1.0);
                                    self.log_console(format!(
                                        "TRAJECTORY ZOOM: {:.2}x",
                                        self.trajectory_renderer.zoom
                                    ));
                                }
                            }
                            ActiveView::Surface => {
                                if self.surface_renderer.zoom <= 0.65 {
                                    self.active_view = ActiveView::Trajectory;
                                    self.scale_coordinator.snap_to_view(PrimaryView::Trajectory);
                                    self.trajectory_renderer.zoom = 2.2;
                                    self.log_console("ASCENDING SCALE: REGIONAL SURFACE -> EUROPA ORBIT [10,000 km]".to_string());
                                } else {
                                    self.surface_renderer.adjust_zoom(0.77);
                                    self.scale_coordinator.zoom_level =
                                        (self.scale_coordinator.zoom_level - 0.2).max(2.0);
                                    self.log_console(format!(
                                        "SURFACE ZOOM: {:.2}x",
                                        self.surface_renderer.zoom
                                    ));
                                }
                            }
                            ActiveView::Tomography => {
                                if self.tomography_renderer.band == RadarBand::Vhf60MHz {
                                    self.tomography_renderer.set_band(RadarBand::Hf9MHz);
                                    self.tomography_renderer.zoom = 2.0;
                                    self.scale_coordinator.zoom_level = 3.2;
                                    self.log_console(
                                        "SWITCHING RADAR: 60 MHz VHF -> DEEP PENETRATION 9 MHz HF"
                                            .to_string(),
                                    );
                                } else if self.tomography_renderer.zoom <= 0.65 {
                                    self.active_view = ActiveView::Surface;
                                    self.scale_coordinator.snap_to_view(PrimaryView::Surface);
                                    self.surface_renderer.zoom = 2.0;
                                    self.log_console("ASCENDING SCALE: ICE TOMOGRAPHY -> REGIONAL SURFACE SURVEY [500 km]".to_string());
                                } else {
                                    self.tomography_renderer.adjust_zoom(0.77);
                                    self.scale_coordinator.zoom_level =
                                        (self.scale_coordinator.zoom_level - 0.2).max(3.0);
                                    self.log_console(format!(
                                        "TOMOGRAPHY ZOOM: {:.2}x",
                                        self.tomography_renderer.zoom
                                    ));
                                }
                            }
                            ActiveView::MissionControl => {
                                self.active_view = ActiveView::System;
                                self.scale_coordinator.snap_to_view(PrimaryView::System);
                                self.log_console(
                                    "VIEW SWITCH: MISSION CONTROL -> JOVIAN SYSTEM".to_string(),
                                );
                            }
                        }
                    }
                    ZoomAction::Level(level) => {
                        self.active_view = match level {
                            PrimaryView::System => ActiveView::System,
                            PrimaryView::Trajectory => ActiveView::Trajectory,
                            PrimaryView::Surface => ActiveView::Surface,
                            PrimaryView::Tomography => ActiveView::Tomography,
                            PrimaryView::MissionControl => ActiveView::MissionControl,
                        };
                        self.scale_coordinator.snap_to_view(level);
                        self.log_console(format!("ZOOM TO SCALE: {:?}", level));
                    }
                    ZoomAction::Value(val) => {
                        self.scale_coordinator.zoom_level = val;
                        self.scale_coordinator.target_zoom = val;
                        match self.active_view {
                            ActiveView::System => {
                                self.system_renderer.camera_distance_scale = val.clamp(0.2, 5.0)
                            }
                            ActiveView::Trajectory => {
                                self.trajectory_renderer.zoom = val.clamp(0.25, 8.0)
                            }
                            ActiveView::Surface => self.surface_renderer.zoom = val.clamp(0.4, 4.0),
                            ActiveView::Tomography => {
                                self.tomography_renderer.zoom = val.clamp(0.4, 4.0)
                            }
                            _ => {}
                        }
                        self.log_console(format!("ZOOM SET: {:.2}", val));
                    }
                }
                if matches!(
                    (from_view, self.active_view),
                    (ActiveView::System, ActiveView::Trajectory)
                        | (ActiveView::Trajectory, ActiveView::System)
                        | (ActiveView::Trajectory, ActiveView::Surface)
                        | (ActiveView::Surface, ActiveView::Trajectory)
                        | (ActiveView::Surface, ActiveView::Tomography)
                        | (ActiveView::Tomography, ActiveView::Surface)
                ) {
                    self.set_scale_reveal(from_view, self.active_view, 0.0);
                }
            }
            Command::SetTarget(target_name) => {
                self.system_renderer.selected_target_idx = match target_name.to_lowercase().as_str()
                {
                    "io" => 1,
                    "europa" => 2,
                    "ganymede" => 3,
                    "callisto" => 4,
                    _ => 0,
                };
                self.sync_orbital_target();
                self.log_console(format!("TRACKING TARGET: {}", target_name.to_uppercase()));
            }
            Command::ExecuteBurn(dv) => {
                let fuel_kg = if let Some(node) = self.trajectory_lab.selected_node_mut() {
                    node.dv_prograde += dv;
                    node.propellant_consumed_kg()
                } else {
                    self.trajectory_lab
                        .add_node(self.sim.mission_time_hours + 1.0);
                    if let Some(node) = self.trajectory_lab.selected_node_mut() {
                        node.dv_prograde = dv;
                        node.propellant_consumed_kg()
                    } else {
                        0.0
                    }
                };
                self.telemetry.consume_propellant(fuel_kg.min(15.0));
                self.trajectory_lab.recompute(&self.sim);
                self.log_console(format!("MANEUVER BURN EXECUTED: ΔV = {:.1} m/s", dv));
            }
            Command::SetRadarBand(band) => {
                self.tomography_renderer.set_band(band);
                self.log_console(format!("REASON RADAR BAND: {:?}", band));
            }
            Command::AcknowledgeAlert => {
                self.telemetry.acknowledge_all_alerts();
                self.log_console("ALL ACTIVE ALERTS ACKNOWLEDGED".to_string());
            }
            Command::SetSkin(skin) => {
                self.current_skin_name = match skin.to_lowercase().as_str() {
                    "vapor95" | "vapor" => "VAPOR95",
                    "swiss" | "swiss_signal" => "SWISS_SIGNAL",
                    _ => "BLACK_ICE",
                };
                self.log_console(format!("UI SKIN CHANGED TO: {}", self.current_skin_name));
            }
            Command::SetWarpRate(rate) => {
                self.telemetry.warp_rate = rate.max(0.1);
                self.log_console(format!("TIME WARP RATE: {}x", self.telemetry.warp_rate));
            }
            Command::Pause => {
                self.telemetry.is_paused = true;
                self.log_console("SIMULATION PAUSED".to_string());
            }
            Command::Resume => {
                self.telemetry.is_paused = false;
                self.log_console("SIMULATION RESUMED".to_string());
            }
            Command::TogglePause => {
                self.telemetry.is_paused = !self.telemetry.is_paused;
                self.log_console(
                    if self.telemetry.is_paused {
                        "SIMULATION PAUSED [SPACE]"
                    } else {
                        "SIMULATION RESUMED [SPACE]"
                    }
                    .to_string(),
                );
            }
            Command::SetGate(depth_km) => {
                self.tomography_renderer.set_depth_cursor(depth_km);
                self.log_console(format!("RADAR DEPTH GATE: {:.1} km", depth_km));
            }
            Command::SetGain(gain_db) => {
                self.tomography_renderer.gain_db = gain_db.clamp(10.0, 75.0);
                self.log_console(format!(
                    "RADAR RECEIVER GAIN: {:.0} dB",
                    self.tomography_renderer.gain_db
                ));
            }
            Command::Rotate(lon, lat) => match self.active_view {
                ActiveView::Surface => self.surface_renderer.rotate_globe(lon, lat),
                ActiveView::System => self
                    .system_renderer
                    .rotate_camera(lon.to_radians(), lat.to_radians()),
                _ => {}
            },
            Command::Scrub(hours) => {
                self.trajectory_lab.set_scrub_time(hours, &self.sim);
                self.log_console(format!("TRAJECTORY LAB SCRUB: T+{:>5.1}h", hours));
            }
            Command::ToggleThermal => {
                self.surface_renderer.toggle_thermal();
                self.log_console(format!(
                    "THERMAL IR LAYER: {}",
                    if self.surface_renderer.show_thermal {
                        "ON"
                    } else {
                        "OFF"
                    }
                ));
            }
            Command::ToggleMagnetic => {
                self.system_renderer.toggle_magnetic_field();
                self.log_console(format!(
                    "MAGNETIC FIELD CURVES: {}",
                    if self.system_renderer.show_magnetic_field {
                        "ON"
                    } else {
                        "OFF"
                    }
                ));
            }
            Command::ToggleAutoOrbit => {
                self.system_renderer.toggle_auto_orbit();
                self.log_console(format!(
                    "CAMERA AUTO-ORBIT: {}",
                    if self.system_renderer.auto_orbit {
                        "ON"
                    } else {
                        "OFF"
                    }
                ));
            }
            Command::ToggleAutoRotate => {
                self.surface_renderer.toggle_auto_rotate();
                self.log_console(format!(
                    "GLOBE AUTO-SPIN: {}",
                    if self.surface_renderer.auto_rotate {
                        "ON"
                    } else {
                        "OFF"
                    }
                ));
            }
            Command::SetInstrumentMode(name, mode_str) => {
                let mode = match mode_str.to_lowercase().as_str() {
                    "active" | "on" => crate::sim::telemetry::InstrumentMode::Active,
                    "standby" => crate::sim::telemetry::InstrumentMode::Standby,
                    "cal" | "calibrating" => crate::sim::telemetry::InstrumentMode::Calibrating,
                    _ => crate::sim::telemetry::InstrumentMode::Off,
                };
                match name.to_lowercase().as_str() {
                    "reason" | "radar" => self.telemetry.radar_reason_mode = mode,
                    "mise" | "spectro" => self.telemetry.spectro_mise_mode = mode,
                    "maspex" | "mass" => self.telemetry.mass_spec_maspex_mode = mode,
                    "pims" | "plasma" => self.telemetry.plasma_pims_mode = mode,
                    "suda" | "dust" => self.telemetry.dust_suda_mode = mode,
                    _ => {}
                }
                self.log_console(format!(
                    "INSTRUMENT {} -> {}",
                    name.to_uppercase(),
                    mode.name()
                ));
            }
            Command::Help => {
                self.log_console("CMDS: view <1-5>, zoom, burn <dv>, radar <hf|vhf|split>, gate <km>, gain <db>, warp <rate>, pause, scrub <hr>, thermal, mag, skin <name>".to_string());
            }
            Command::Unknown(u) => {
                self.log_console(format!("UNKNOWN COMMAND: '{}' (type 'help')", u));
            }
        }
    }

    pub fn log_console(&mut self, msg: String) {
        if self.console_history.len() >= 16 {
            self.console_history.pop_front();
        }
        self.status_feedback = msg.clone();
        self.console_history.push_back(msg);
    }

    fn sync_orbital_target(&mut self) {
        self.sim.target = match self.system_renderer.selected_target_idx {
            1 => TargetBody::Io,
            2 => TargetBody::Europa,
            3 => TargetBody::Ganymede,
            4 => TargetBody::Callisto,
            _ => TargetBody::Jupiter,
        };
    }

    pub fn handle_action(&mut self, action: DashboardAction) {
        match action {
            DashboardAction::SwitchView(view) => {
                self.execute_command(Command::SetView(match view {
                    ActiveView::System => ViewTarget::System,
                    ActiveView::Trajectory => ViewTarget::Trajectory,
                    ActiveView::Surface => ViewTarget::Surface,
                    ActiveView::Tomography => ViewTarget::Tomography,
                    ActiveView::MissionControl => ViewTarget::MissionControl,
                }));
            }
            DashboardAction::InputChanged(new_state) => {
                self.command_input = new_state;
            }
            DashboardAction::SubmitCommand => {
                let text = self.command_input.text.clone();
                self.command_input = TextInputState::new();
                if let Some(cmd) = CommandParser::parse(&text) {
                    self.execute_command(cmd);
                } else if !text.trim().is_empty() {
                    self.execute_command(Command::Unknown(text));
                }
            }
            DashboardAction::Zoom(zoom_action) => {
                self.execute_command(Command::Zoom(zoom_action));
            }
            DashboardAction::NextTarget => {
                if self.active_view == ActiveView::Surface {
                    self.surface_renderer.next_feature();
                    let f = self.surface_renderer.selected_feature();
                    self.log_console(format!("SURFACE TARGET: {}", f.name));
                } else {
                    self.system_renderer.next_target();
                    self.sync_orbital_target();
                    let t = self.system_renderer.selected_target_name();
                    self.log_console(format!("SYSTEM TARGET: {}", t));
                }
            }
            DashboardAction::PrevTarget => {
                if self.active_view == ActiveView::Surface {
                    self.surface_renderer.prev_feature();
                    let f = self.surface_renderer.selected_feature();
                    self.log_console(format!("SURFACE TARGET: {}", f.name));
                } else {
                    self.system_renderer.prev_target();
                    self.sync_orbital_target();
                    let t = self.system_renderer.selected_target_name();
                    self.log_console(format!("SYSTEM TARGET: {}", t));
                }
            }
            DashboardAction::ToggleRadarBand => {
                self.tomography_renderer.toggle_band();
                self.log_console(format!(
                    "REASON BAND TOGGLED: {:?}",
                    self.tomography_renderer.band
                ));
            }
            DashboardAction::ExecuteBurn(dv) => {
                self.execute_command(Command::ExecuteBurn(dv));
            }
            DashboardAction::AcknowledgeAlert => {
                self.execute_command(Command::AcknowledgeAlert);
            }
            DashboardAction::CycleSkin => {
                self.current_skin_name = match self.current_skin_name {
                    "BLACK_ICE" => "SWISS_SIGNAL",
                    "SWISS_SIGNAL" => "VAPOR95",
                    _ => "BLACK_ICE",
                };
                self.log_console(format!("UI SKIN: {}", self.current_skin_name));
            }
            DashboardAction::Rotate(dx, dy) => match self.active_view {
                ActiveView::Surface => self.surface_renderer.rotate_globe(dx * 8.0, dy * 5.0),
                ActiveView::System => self.system_renderer.rotate_camera(dx * 0.08, dy * 0.06),
                ActiveView::Trajectory => self.trajectory_lab.scrub_time(dx * 2.0, &self.sim),
                ActiveView::Tomography => self.tomography_renderer.move_depth_cursor(dy * 1.0),
                _ => {}
            },
            DashboardAction::ScrubTimeline(dh) => {
                self.trajectory_lab.scrub_time(dh, &self.sim);
            }
            DashboardAction::MoveGate(dz) => {
                self.tomography_renderer.move_depth_cursor(dz);
            }
            DashboardAction::AdjustGain(dg) => {
                self.tomography_renderer.adjust_gain(dg);
            }
            DashboardAction::TogglePause => {
                self.execute_command(Command::TogglePause);
            }
            DashboardAction::CycleWarpRate => {
                let next_rate = match self.telemetry.warp_rate as u32 {
                    1 => 10.0,
                    10 => 60.0,
                    60 => 300.0,
                    300 => 1000.0,
                    _ => 1.0,
                };
                self.execute_command(Command::SetWarpRate(next_rate));
            }
            DashboardAction::NoOp => {}
        }
    }

    /// Render the active visualization surface at the requested dimensions.
    pub fn render_visualization_surface(&self, width: u16, height: u16) -> Arc<Surface> {
        let mut surface = self.render_view_surface(self.active_view, width, height);
        if let Some(reveal) = &self.scale_reveal {
            if reveal.to == self.active_view {
                let previous = self.render_view_surface(reveal.from, width, height);
                let progress = (reveal.elapsed_secs / SCALE_REVEAL_SECONDS).clamp(0.0, 1.0);
                let ease = progress * progress * (3.0 - 2.0 * progress);
                let (ax, ay) = match (reveal.from, reveal.to) {
                    (ActiveView::Surface, ActiveView::Tomography)
                    | (ActiveView::Tomography, ActiveView::Surface) => {
                        self.surface_renderer.selected_feature_cell(width, height)
                    }
                    (ActiveView::System, ActiveView::Trajectory)
                    | (ActiveView::Trajectory, ActiveView::System) => {
                        let pos = self.sim.europa.position_at(self.sim.mission_time_hours);
                        let (x, y, _) = self
                            .system_renderer
                            .project_3d(pos, width, height.saturating_mul(2))
                            .unwrap_or((width as i32 / 2, height as i32, 0.0));
                        (
                            x.clamp(0, width.saturating_sub(1) as i32) as u16,
                            (y / 2).clamp(0, height.saturating_sub(1) as i32) as u16,
                        )
                    }
                    _ => (width / 2, height / 2),
                };
                // Terminal cells are roughly twice as tall as wide. Measure
                // distance in raster pixels to reveal a circular world-space
                // aperture around the selected moon or geological target.
                let radius =
                    ease * (((width as f32).powi(2) + (height as f32 * 2.0).powi(2)).sqrt());
                if progress == 0.0 || (height < 26 && progress < 0.5) {
                    surface = previous;
                } else if height >= 26 && progress < 1.0 {
                    let text_spans: Vec<_> = (0..height)
                        .map(|y| text_span(&previous, &surface, y, width))
                        .collect();
                    for y in 0..height {
                        for x in 0..width {
                            let dx = x as f32 - ax as f32;
                            let dy = (y as f32 - ay as f32) * 2.0;
                            let textual = text_spans[y as usize]
                                .is_some_and(|(start, end)| (start..=end).contains(&x));
                            if (textual && progress < 0.5)
                                || (!textual && dx * dx + dy * dy > radius * radius)
                            {
                                if let Some(cell) = previous.get(x, y) {
                                    surface.set_cell(x, y, cell.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        Arc::new(surface)
    }

    fn render_view_surface(&self, view: ActiveView, width: u16, height: u16) -> Surface {
        match view {
            ActiveView::System => {
                self.system_renderer
                    .render(&self.sim, width, height, self.mono_mode)
            }
            ActiveView::Trajectory => self.trajectory_renderer.render(
                &self.sim,
                &self.trajectory_lab,
                width,
                height,
                self.mono_mode,
            ),
            ActiveView::Surface => self.surface_renderer.render(width, height, self.mono_mode),
            ActiveView::Tomography => {
                self.tomography_renderer
                    .render(width, height, self.mono_mode)
            }
            ActiveView::MissionControl => self.render_mission_control_overview(width, height),
        }
    }

    /// Renders the Ops Overview scientific telemetry matrix.
    fn render_mission_control_overview(&self, width: u16, height: u16) -> Surface {
        let mut surface = Surface::new(width.max(1), height.max(1));
        surface.clear();

        // Header Banner
        let header = " PROJECT GALILEO // JOVIAN MISSION OPERATIONS TELEMETRY MATRIX ";
        surface.print_str(
            1,
            0,
            header,
            Style::new()
                .fg(Color::rgb(220, 240, 255))
                .bg(Color::rgb(20, 40, 80))
                .bold(),
            None,
        );

        let col_w = (width / 3).max(20);

        // Column 1: Flight Dynamics & Spacecraft State
        let sc_state = self.sim.spacecraft_state();
        let r_km = sc_state.position.length();
        let v_kms = sc_state.velocity.length();
        let rad_dose = TrajectoryLab::radiation_dose_rate_at(r_km);

        let c1_x = 1;
        let mut row = 2;
        surface.print_str(
            c1_x,
            row,
            "── FLIGHT DYNAMICS ─────────",
            Style::new().fg(Color::rgb(100, 180, 240)).bold(),
            None,
        );
        row += 1;
        surface.print_str(
            c1_x,
            row,
            &format!("RADIUS (J):   {:>10.1} km", r_km),
            Style::new().fg(Color::rgb(200, 220, 240)),
            None,
        );
        row += 1;
        surface.print_str(
            c1_x,
            row,
            &format!("VELOCITY:     {:>10.2} km/s", v_kms),
            Style::new().fg(Color::rgb(200, 220, 240)),
            None,
        );
        row += 1;
        surface.print_str(
            c1_x,
            row,
            &format!("SEMI-MAJOR a: {:>10.0} km", self.sim.sc_semi_major_axis),
            Style::new().fg(Color::rgb(200, 220, 240)),
            None,
        );
        row += 1;
        surface.print_str(
            c1_x,
            row,
            &format!("ECCENTRICITY: {:>10.4}", self.sim.sc_eccentricity),
            Style::new().fg(Color::rgb(200, 220, 240)),
            None,
        );
        row += 1;
        surface.print_str(
            c1_x,
            row,
            &format!(
                "EUROPA CA:    {:>10.1} km",
                self.trajectory_lab.closest_approach_nominal.altitude_km
            ),
            Style::new().fg(Color::rgb(100, 240, 200)).bold(),
            None,
        );
        row += 1;
        surface.print_str(
            c1_x,
            row,
            &format!("RAD DOSE:     {:>10.1} rad/h", rad_dose),
            Style::new().fg(Color::rgb(255, 180, 80)),
            None,
        );
        row += 2;

        // Column 1 Part 2: Propulsion & Maneuver State
        surface.print_str(
            c1_x,
            row,
            "── PROPULSION & MANEUVER ───",
            Style::new().fg(Color::rgb(100, 180, 240)).bold(),
            None,
        );
        row += 1;
        let node_dv = self
            .trajectory_lab
            .selected_node()
            .map(|n| n.delta_v_total())
            .unwrap_or(0.0);
        let node_dur = self
            .trajectory_lab
            .selected_node()
            .map(|n| n.burn_duration_seconds())
            .unwrap_or(0.0);
        let node_prop = self
            .trajectory_lab
            .selected_node()
            .map(|n| n.propellant_consumed_kg())
            .unwrap_or(0.0);
        surface.print_str(
            c1_x,
            row,
            &format!("PLANNED BURN: {:>10.1} m/s", node_dv),
            Style::new().fg(Color::rgb(255, 220, 100)),
            None,
        );
        row += 1;
        surface.print_str(
            c1_x,
            row,
            &format!("BURN DURATION:{:>10.1} s", node_dur),
            Style::new().fg(Color::rgb(200, 220, 240)),
            None,
        );
        row += 1;
        surface.print_str(
            c1_x,
            row,
            &format!("PROP CONSUMED:{:>10.1} kg", node_prop),
            Style::new().fg(Color::rgb(200, 220, 240)),
            None,
        );

        // Column 2: Electrical Power & Communications Link
        let c2_x = (col_w + 2).min(width.saturating_sub(25));
        let mut row2 = 2;
        surface.print_str(
            c2_x,
            row2,
            "── POWER SUBSYSTEM ─────────",
            Style::new().fg(Color::rgb(120, 220, 140)).bold(),
            None,
        );
        row2 += 1;
        surface.print_str(
            c2_x,
            row2,
            &format!("RTG OUTPUT:   {:>10.1} W", self.telemetry.rtg_power_w),
            Style::new().fg(Color::rgb(200, 240, 220)),
            None,
        );
        row2 += 1;
        surface.print_str(
            c2_x,
            row2,
            &format!(
                "BATTERY SOC:  {:>9.1} %",
                self.telemetry.battery_soc * 100.0
            ),
            Style::new().fg(Color::rgb(120, 240, 140)).bold(),
            None,
        );
        row2 += 1;
        surface.print_str(
            c2_x,
            row2,
            &format!("BUS VOLTAGE:  {:>10.2} V", self.telemetry.bus_voltage_v),
            Style::new().fg(Color::rgb(200, 240, 220)),
            None,
        );
        row2 += 1;
        surface.print_str(
            c2_x,
            row2,
            &format!("BUS CURRENT:  {:>10.2} A", self.telemetry.bus_current_a),
            Style::new().fg(Color::rgb(200, 240, 220)),
            None,
        );
        row2 += 2;

        let (light_time_sec, _, _) = self.sim.earth_communication();
        let lt_min = light_time_sec / 60.0;

        surface.print_str(
            c2_x,
            row2,
            "── RF COMMUNICATIONS ───────",
            Style::new().fg(Color::rgb(120, 220, 140)).bold(),
            None,
        );
        row2 += 1;
        surface.print_str(
            c2_x,
            row2,
            &format!(
                "CARRIER SNR:  {:>10.1} dB",
                self.telemetry.comm_carrier_snr_db
            ),
            Style::new().fg(Color::rgb(120, 240, 160)).bold(),
            None,
        );
        row2 += 1;
        surface.print_str(
            c2_x,
            row2,
            &format!(
                "DOWNLINK:     {:>10.1} kbps",
                self.telemetry.downlink_rate_kbps
            ),
            Style::new().fg(Color::rgb(200, 240, 220)),
            None,
        );
        row2 += 1;
        surface.print_str(
            c2_x,
            row2,
            &format!("DSN STATION:  {}", self.telemetry.dsn_station),
            Style::new().fg(Color::rgb(200, 240, 220)),
            None,
        );
        row2 += 1;
        surface.print_str(
            c2_x,
            row2,
            &format!("1-WAY LIGHT:  {:>10.1} min", lt_min),
            Style::new().fg(Color::rgb(255, 220, 120)),
            None,
        );

        // Column 3: Science Payload Instruments & Alerts
        let c3_x = (col_w * 2 + 3).min(width.saturating_sub(25));
        if c3_x + 20 <= width {
            let mut row3 = 2;
            surface.print_str(
                c3_x,
                row3,
                "── INSTRUMENT PAYLOAD ──────",
                Style::new().fg(Color::rgb(220, 160, 240)).bold(),
                None,
            );
            row3 += 1;
            surface.print_str(
                c3_x,
                row3,
                &format!(
                    "REASON (RADAR): {}",
                    self.telemetry.radar_reason_mode.name()
                ),
                Style::new().fg(Color::rgb(100, 240, 220)),
                None,
            );
            row3 += 1;
            surface.print_str(
                c3_x,
                row3,
                &format!(
                    "MISE (IR SPEC): {}",
                    self.telemetry.spectro_mise_mode.name()
                ),
                Style::new().fg(Color::rgb(200, 220, 240)),
                None,
            );
            row3 += 1;
            surface.print_str(
                c3_x,
                row3,
                &format!(
                    "MASPEX (SPECT): {}",
                    self.telemetry.mass_spec_maspex_mode.name()
                ),
                Style::new().fg(Color::rgb(200, 220, 240)),
                None,
            );
            row3 += 1;
            surface.print_str(
                c3_x,
                row3,
                &format!("PIMS (PLASMA):  {}", self.telemetry.plasma_pims_mode.name()),
                Style::new().fg(Color::rgb(200, 220, 240)),
                None,
            );
            row3 += 1;
            surface.print_str(
                c3_x,
                row3,
                &format!("SUDA (DUST):    {}", self.telemetry.dust_suda_mode.name()),
                Style::new().fg(Color::rgb(200, 220, 240)),
                None,
            );
            row3 += 2;

            surface.print_str(
                c3_x,
                row3,
                "── MISSION ALERTS LOG ──────",
                Style::new().fg(Color::rgb(255, 140, 100)).bold(),
                None,
            );
            row3 += 1;
            for alert in self
                .telemetry
                .alerts
                .iter()
                .rev()
                .take(height.saturating_sub(row3 + 2) as usize)
            {
                let sev_tag = match alert.severity {
                    AlertSeverity::Info => "[INFO]",
                    AlertSeverity::Warning => "[WARN]",
                    AlertSeverity::Critical => "[CRIT]",
                };
                let col = match alert.severity {
                    AlertSeverity::Info => Color::rgb(140, 180, 220),
                    AlertSeverity::Warning => Color::rgb(255, 160, 60),
                    AlertSeverity::Critical => Color::rgb(255, 80, 80),
                };
                let line = format!("{} {}", sev_tag, alert.subsystem);
                surface.print_str(c3_x, row3, &line, Style::new().fg(col).bold(), None);
                row3 += 1;
                if row3 >= height.saturating_sub(1) {
                    break;
                }
            }
        }

        surface
    }

    /// Build the full semantic UI tree using `gibson::ui`.
    pub fn view(&self, cx: &BuildCx) -> Element<DashboardAction> {
        let env = cx.environment;
        let width = env.width;
        let height = env.height;

        // Compute available space for the primary visualization stage
        let is_spacious = width >= 120 && height >= 35;
        let is_compact = width < 80 || height < 24;

        let right_rail_w = if is_spacious { 32 } else { 0 };
        let header_h: u16 = if is_compact { 1 } else { 2 };
        let footer_h: u16 = if is_compact { 1 } else { 3 };

        let vis_w = width
            .saturating_sub(right_rail_w + if is_spacious { 2 } else { 0 })
            .max(10);
        let vis_h = height.saturating_sub(header_h + footer_h + 1).max(5);

        // 1. Header Bar: Title, View Switcher Tabs, Clock, Status Badge
        let mut header_row = row().density(Density::Compact).padding(0);

        header_row = header_row.child(
            heading(if width < 110 {
                "GALILEO"
            } else {
                "PROJECT GALILEO"
            })
            .tone(Tone::Accent)
            .emphasis(Emphasis::Strong),
        );
        if width >= 110 {
            header_row =
                header_row.child(label(" // ").tone(Tone::Neutral).emphasis(Emphasis::Faint));
        }

        let tabs = if width < 110 {
            ["1:S", "2:T", "3:E", "4:I", "5:O"]
        } else {
            ["1:SYS", "2:TRAJ", "3:SURF", "4:TOMO", "5:OPS"]
        };

        // View Selection Buttons
        header_row = header_row
            .child(
                button(tabs[0])
                    .selected(self.active_view == ActiveView::System)
                    .on_press(DashboardAction::SwitchView(ActiveView::System)),
            )
            .child(
                button(tabs[1])
                    .selected(self.active_view == ActiveView::Trajectory)
                    .on_press(DashboardAction::SwitchView(ActiveView::Trajectory)),
            )
            .child(
                button(tabs[2])
                    .selected(self.active_view == ActiveView::Surface)
                    .on_press(DashboardAction::SwitchView(ActiveView::Surface)),
            )
            .child(
                button(tabs[3])
                    .selected(self.active_view == ActiveView::Tomography)
                    .on_press(DashboardAction::SwitchView(ActiveView::Tomography)),
            )
            .child(
                button(tabs[4])
                    .selected(self.active_view == ActiveView::MissionControl)
                    .on_press(DashboardAction::SwitchView(ActiveView::MissionControl)),
            );

        header_row = header_row.child(spacer());

        // Approximate view span/depth indicator, not an exact map ratio.
        let scale_tag = self.scale_coordinator.physical_scale_label();
        let short_tag = self.scale_coordinator.short_scale_label();
        if width >= 180 {
            header_row = header_row.child(
                badge(scale_tag)
                    .tone(Tone::Accent)
                    .emphasis(Emphasis::Strong),
            );
            header_row = header_row.child(label(" "));
        } else if width >= 100 {
            header_row = header_row.child(
                badge(format!("SCALE: {}", short_tag))
                    .tone(Tone::Accent)
                    .emphasis(Emphasis::Strong),
            );
            header_row = header_row.child(label(" "));
        }

        // Clock & Status Badge
        let (light_time_sec, _, _) = self.sim.earth_communication();
        if width >= 160 {
            let clock_str = format!(
                "MET: +{:03}d {:02}h | DSN LT: {:.1}m",
                (self.sim.mission_time_hours.max(0.0) / 24.0) as u32,
                (self.sim.mission_time_hours.max(0.0) as u32) % 24,
                light_time_sec / 60.0
            );
            header_row =
                header_row.child(label(clock_str).tone(Tone::Info).emphasis(Emphasis::Muted));
            header_row = header_row.child(label(" "));
        }

        let alert_cnt = self
            .telemetry
            .alerts
            .iter()
            .filter(|a| !a.acknowledged)
            .count();
        if alert_cnt > 0 && width >= 90 {
            header_row = header_row.child(
                badge(format!("ALERTS: {}", alert_cnt))
                    .tone(Tone::Warning)
                    .emphasis(Emphasis::Strong),
            );
        } else if width >= 120 {
            header_row = header_row.child(
                badge("NOMINAL")
                    .tone(Tone::Success)
                    .emphasis(Emphasis::Normal),
            );
        }

        // 2. Visualization Surface Render
        let vis_surface = self.render_visualization_surface(vis_w, vis_h);
        let vis_element = surface(vis_surface).width(vis_w).height(vis_h);

        // 3. Right Rail (when spacious): Live Telemetry Cards
        let center_content = if is_spacious {
            let mut rail = column()
                .width(right_rail_w)
                .density(Density::Compact)
                .gap(0);

            // Power Subsystem Card
            let p_frac = self.telemetry.battery_soc.clamp(0.0, 1.0);
            rail = rail.child(
                card("EPS TELEMETRY")
                    .density(Density::Compact)
                    .child(progress("BATT", p_frac))
                    .child(text(format!(
                        "RTG: {:.1}W | BUS: {:.1}V",
                        self.telemetry.rtg_power_w, self.telemetry.bus_voltage_v
                    ))),
            );

            // RF Comms Link Card
            rail = rail.child(
                card("COMMS LINK")
                    .density(Density::Compact)
                    .child(text(format!(
                        "SNR: +{:.1}dB | RATE: {:.0}k",
                        self.telemetry.comm_carrier_snr_db, self.telemetry.downlink_rate_kbps
                    )))
                    .child(text(format!("STATION: {}", self.telemetry.dsn_station))),
            );

            // Navigation State Card
            let rad_dose = TrajectoryLab::radiation_dose_rate_at(
                self.sim.spacecraft_state().position.length(),
            );
            rail = rail.child(
                card("FLIGHT DYNAMICS")
                    .density(Density::Compact)
                    .child(text(format!(
                        "EUROPA CA: {:.1}km",
                        self.trajectory_lab.closest_approach_nominal.altitude_km
                    )))
                    .child(text(format!("RAD DOSE: {:.1}rad/h", rad_dose))),
            );

            // Console History Snippet
            let mut history_col = column().density(Density::Compact);
            for log in self.console_history.iter().rev().take(3) {
                history_col =
                    history_col.child(text(format!("> {}", log)).emphasis(Emphasis::Faint));
            }
            rail = rail.child(card("OPS LOG").density(Density::Compact).child(history_col));

            row().width(width).gap(1).child(vis_element).child(rail)
        } else {
            row().width(width).child(vis_element)
        };

        // 4. Bottom Command Console Bar
        let mut console_row = row().density(Density::Compact).padding(0);
        console_row =
            console_row.child(text("CMD> ").tone(Tone::Accent).emphasis(Emphasis::Strong));
        console_row = console_row.child(
            text_input(&self.command_input)
                .key("cmd_input")
                .placeholder(if width < 110 {
                    "type command or help"
                } else {
                    "view <1-5>, zoom, burn, radar, target, skin, help..."
                })
                .on_edit(DashboardAction::InputChanged)
                .on_press(DashboardAction::SubmitCommand)
                .grow(1.0),
        );
        console_row = console_row.child(
            button("EXEC")
                .key("exec_btn")
                .tone(Tone::Accent)
                .on_press(DashboardAction::SubmitCommand),
        );

        // Status Line / Key Hints
        let pause_tag = if self.telemetry.is_paused {
            "PAUSED"
        } else {
            "RUN"
        };
        let view_hints = match self.active_view {
            ActiveView::Trajectory => "H/L scrub · [/] timeline · B burn",
            ActiveView::Surface => "HJKL spin · N/P feature · G thermal",
            ActiveView::Tomography => "↑↓ gate · R band · +/- gain",
            ActiveView::System => "HJKL camera · N/P target · M field",
            ActiveView::MissionControl => "A acknowledge alerts",
        };
        let hint = if is_compact {
            format!("[{pause_tag}] 1-5 views · Z/X zoom · {view_hints}")
        } else if width < 110 {
            format!("[{pause_tag}] 1-5 views · Z/X zoom · {view_hints} · help in CMD")
        } else {
            let feedback: String = self.status_feedback.chars().take(49).collect();
            format!("[{pause_tag}] {feedback} · {view_hints} · Space pause · W warp")
        };
        let hint_line = text(hint).tone(Tone::Info).emphasis(Emphasis::Muted);

        // Root Screen Assembly
        screen()
            .density(Density::Compact)
            .padding(0)
            .gap(0)
            .child(header_row)
            .child(center_content)
            .child(console_row)
            .child(hint_line)
    }

    /// Map current skin name to `gibson::ui::Skin`
    pub fn resolved_skin(&self) -> Skin {
        match self.current_skin_name {
            "VAPOR95" => skins::VAPOR95,
            "SWISS_SIGNAL" => skins::SWISS_SIGNAL,
            _ => skins::BLACK_ICE,
        }
    }
}
