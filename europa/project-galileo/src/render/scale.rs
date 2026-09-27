//! Scale coordinator and continuous zoom interpolator.
//!
//! Controls the continuous transition between radically different physical scales:
//! - Scale 0: Jovian System (Macro orbital view, ~2,000,000 km)
//! - Scale 1: Europa Orbit (~10,000 km)
//! - Scale 2: Europa Surface Survey (~500 km swath)
//! - Scale 3: Ice Shell Cross-Section (~30 km depth section)
//! - Scale 4: Subsurface Ocean Tomography (~100 km deep sounding)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryView {
    System,
    Trajectory,
    Surface,
    Tomography,
    MissionControl,
}

impl PrimaryView {
    pub const ALL: [Self; 5] = [
        Self::System,
        Self::Trajectory,
        Self::Surface,
        Self::Tomography,
        Self::MissionControl,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::System => "1: JOVIAN SYSTEM",
            Self::Trajectory => "2: TRAJECTORY LAB",
            Self::Surface => "3: EUROPA SURVEY",
            Self::Tomography => "4: ICE TOMOGRAPHY",
            Self::MissionControl => "5: MISSION CONTROL",
        }
    }

    pub fn target_scale(self) -> f32 {
        match self {
            Self::System => 0.0,
            Self::Trajectory => 1.0,
            Self::Surface => 2.0,
            Self::Tomography => 3.0,
            Self::MissionControl => 1.0,
        }
    }
}

/// Continuous scale and camera transition controller.
#[derive(Debug, Clone)]
pub struct ScaleCoordinator {
    pub current_view: PrimaryView,
    pub target_view: PrimaryView,
    pub zoom_level: f32, // 0.0 (System) to 4.0 (Ocean)
    pub target_zoom: f32,
    pub transition_progress: f32, // 0.0 to 1.0
    pub is_transitioning: bool,
    pub transition_duration_secs: f32,
}

impl Default for ScaleCoordinator {
    fn default() -> Self {
        Self {
            current_view: PrimaryView::System,
            target_view: PrimaryView::System,
            zoom_level: 0.0,
            target_zoom: 0.0,
            transition_progress: 1.0,
            is_transitioning: false,
            transition_duration_secs: 1.2,
        }
    }
}

impl ScaleCoordinator {
    pub fn snap_to_view(&mut self, view: PrimaryView) {
        self.current_view = view;
        self.target_view = view;
        self.zoom_level = view.target_scale();
        self.target_zoom = view.target_scale();
        self.is_transitioning = false;
        self.transition_progress = 1.0;
    }

    pub fn set_view(&mut self, view: PrimaryView) {
        if self.current_view != view {
            self.target_view = view;
            self.target_zoom = view.target_scale();
            self.is_transitioning = true;
            self.transition_progress = 0.0;
        }
    }

    pub fn zoom_in(&mut self) {
        self.target_zoom = (self.target_zoom + 0.5).min(4.0);
        self.is_transitioning = true;
        self.transition_progress = 0.0;
        self.update_view_from_zoom();
    }

    pub fn zoom_out(&mut self) {
        self.target_zoom = (self.target_zoom - 0.5).max(0.0);
        self.is_transitioning = true;
        self.transition_progress = 0.0;
        self.update_view_from_zoom();
    }

    fn update_view_from_zoom(&mut self) {
        self.target_view = if self.target_zoom < 0.75 {
            PrimaryView::System
        } else if self.target_zoom < 1.75 {
            PrimaryView::Trajectory
        } else if self.target_zoom < 2.75 {
            PrimaryView::Surface
        } else {
            PrimaryView::Tomography
        };
    }

    pub fn update(&mut self, dt_seconds: f32) {
        if self.is_transitioning {
            let step = dt_seconds / self.transition_duration_secs;
            self.transition_progress = (self.transition_progress + step).min(1.0);

            // Smooth cubic ease-in-out
            let t = self.transition_progress;
            let ease = if t < 0.5 {
                4.0 * t * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
            };

            self.zoom_level =
                self.zoom_level + (self.target_zoom - self.zoom_level) * ease.min(1.0);

            if self.transition_progress >= 1.0 {
                self.is_transitioning = false;
                self.current_view = self.target_view;
                self.zoom_level = self.target_zoom;
            }
        }
    }

    pub fn physical_scale_label(&self) -> &'static str {
        if self.zoom_level < 0.5 {
            "SCALE: 1:2,500,000 km [MACRO JOVIAN]"
        } else if self.zoom_level < 1.5 {
            "SCALE: 1:50,000 km [ORBITAL REGIME]"
        } else if self.zoom_level < 2.5 {
            "SCALE: 1:500 km [EUROPA SURFACE]"
        } else if self.zoom_level < 3.5 {
            "SCALE: 1:25 km [ICE SHELL CROSS-SECTION]"
        } else {
            "SCALE: 1:5 km [SUB-ICE OCEAN TOMOGRAPHY]"
        }
    }

    pub fn short_scale_label(&self) -> &'static str {
        if self.zoom_level < 0.5 {
            "2.5M km"
        } else if self.zoom_level < 1.5 {
            "50,000 km"
        } else if self.zoom_level < 2.5 {
            "500 km"
        } else if self.zoom_level < 3.5 {
            "25 km"
        } else {
            "5 km"
        }
    }
}
