//! Deterministic demo scripting and time-scrubbed tour for Project Galileo.
//!
//! Provides automated, reproducible demonstration tours across all five mission views:
//! 1. System View (Jovian 3D orbital dynamics & Laplace resonance)
//! 2. Continuous scale zoom transition into Europa Orbit
//! 3. Trajectory Lab (Maneuver burn ΔV, pre/post trajectory comparison)
//! 4. Europa Surface Survey (3D shaded globe, cycloid fissures, SAR footprints)
//! 5. Ice Tomography hero view (Dual-band radar sounding, A-scan power trace)
//! 6. Mission Control Ops matrix (RTG power, DSN comms, thermal loops, alerts)

use crate::render::tomography_view::RadarBand;
use crate::ui::dashboard::{ActiveView, DashboardState};

pub struct DemoScript {
    pub total_duration_ms: u64,
}

impl Default for DemoScript {
    fn default() -> Self {
        Self {
            total_duration_ms: 24_000,
        }
    }
}

impl DemoScript {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply scripted deterministic state for timestamp `t_ms`.
    pub fn apply_at_ms(&self, state: &mut DashboardState, t_ms: u64) {
        let cycle_ms = t_ms % self.total_duration_ms;
        let t_sec = cycle_ms as f32 / 1000.0;

        // Reset and scrub simulation deterministically to t_sec
        state.sim.mission_time_hours = 482.0 * 24.0 + t_sec * 0.1;
        state.telemetry.update(state.sim.mission_time_hours);

        // Sequence through views
        if cycle_ms < 4_000 {
            // Phase 1: Jovian System View (0 - 4s)
            state.active_view = ActiveView::System;
            state.scale_coordinator.zoom_level = 0.0;
            state.scale_coordinator.target_zoom = 0.0;
            state.system_renderer.camera_azimuth = 0.42 + t_sec * 0.05;
            state.system_renderer.camera_elevation = 0.38 + (t_sec * 0.5).sin() * 0.05;
            state.status_feedback = "DEMO [1/5]: JOVIAN SYSTEM ORBITAL ARCHITECTURE".to_string();
        } else if cycle_ms < 8_000 {
            // Phase 2: Trajectory Lab & Maneuver Node (4 - 8s)
            state.active_view = ActiveView::Trajectory;
            let zoom_progress = (cycle_ms - 4_000) as f32 / 4_000.0;
            state.scale_coordinator.zoom_level = 0.0 + zoom_progress * 1.0;
            state.scale_coordinator.target_zoom = 1.0;

            // Script a maneuver burn at t = 6s
            if cycle_ms >= 6_000 {
                let burn_dv = 35.0 + ((cycle_ms - 6_000) as f32 / 2_000.0) * 15.0;
                if let Some(node) = state.trajectory_lab.selected_node_mut() {
                    node.dv_prograde = burn_dv;
                }
                state.trajectory_lab.recompute(&state.sim);
                state.status_feedback = format!(
                    "DEMO [2/5]: TRAJECTORY LAB // IMPULSIVE BURN ΔV = {:.1} m/s",
                    burn_dv
                );
            } else {
                state.status_feedback =
                    "DEMO [2/5]: TRAJECTORY LAB // NOMINAL TRAJECTORY INSERTION".to_string();
            }
        } else if cycle_ms < 12_000 {
            // Phase 3: Europa Surface Survey (8 - 12s)
            state.active_view = ActiveView::Surface;
            let zoom_progress = (cycle_ms - 8_000) as f32 / 4_000.0;
            state.scale_coordinator.zoom_level = 1.0 + zoom_progress * 1.0;
            state.scale_coordinator.target_zoom = 2.0;

            // Rotate globe and scan features
            state.surface_renderer.rotation_lon_deg = 200.0 + (t_sec - 8.0) * 8.0;
            state.surface_renderer.tilt_lat_deg = -20.0 + ((t_sec - 8.0) * 0.8).sin() * 10.0;
            state.surface_renderer.selected_feature_idx = if cycle_ms < 10_000 { 1 } else { 0 }; // Agenor -> Conamara
            let feat = state.surface_renderer.selected_feature();
            state.status_feedback = format!("DEMO [3/5]: EUROPA SURVEY // TARGET: {}", feat.name);
        } else if cycle_ms < 18_000 {
            // Phase 4: Hero View - Ice Shell Tomography (12 - 18s)
            state.active_view = ActiveView::Tomography;
            let zoom_progress = (cycle_ms - 12_000) as f32 / 6_000.0;
            state.scale_coordinator.zoom_level = 2.0 + zoom_progress * 1.0;
            state.scale_coordinator.target_zoom = 3.0;

            state.tomography_renderer.along_track_km = (t_sec - 12.0) * 4.5;
            state.tomography_renderer.pulse_phase = ((t_sec - 12.0) * 1.2) % 1.0;

            let depth_sweep = ((cycle_ms - 12_000) as f32 / 6_000.0).clamp(0.0, 1.0);
            state.tomography_renderer.cursor_depth_km = 3.5 + depth_sweep * 17.5;

            if cycle_ms >= 16_000 {
                state.tomography_renderer.band = RadarBand::SplitBand;
                state.status_feedback =
                    "DEMO [4/5]: ICE TOMOGRAPHY // REASON SPLIT-BAND DUAL SOUNDER ACTIVE"
                        .to_string();
            } else if cycle_ms >= 14_000 {
                state.tomography_renderer.band = RadarBand::Vhf60MHz;
                state.status_feedback =
                    "DEMO [4/5]: ICE TOMOGRAPHY // REASON VHF 60MHz SHALLOW HIGH-RES".to_string();
            } else {
                state.tomography_renderer.band = RadarBand::Hf9MHz;
                state.status_feedback =
                    "DEMO [4/5]: ICE TOMOGRAPHY // REASON HF 9MHz DEEP OCEAN SOUNDING".to_string();
            }
        } else {
            // Phase 5: Mission Control Operations Matrix (18 - 24s)
            state.active_view = ActiveView::MissionControl;
            state.status_feedback =
                "DEMO [5/5]: MISSION OPERATIONS TELEMETRY MATRIX & DSN LINK".to_string();
        }
    }
}
