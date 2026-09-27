//! Trajectory planning lab and maneuver node simulation.
//!
//! Provides hypothetical delta-V node editing, pre/post-burn orbital path
//! prediction, closest approach computation to Europa, radiation exposure
//! estimation, and mission event timelines.

use super::jovian::{JovianModel, Vector3, JUPITER_RADIUS_KM};

/// A planned impulsive or finite maneuver burn.
#[derive(Debug, Clone, PartialEq)]
pub struct ManeuverNode {
    pub id: usize,
    pub epoch_hours: f32,
    pub dv_prograde: f32, // m/s
    pub dv_normal: f32,   // m/s
    pub dv_radial: f32,   // m/s
    pub is_enabled: bool,
}

impl ManeuverNode {
    pub fn delta_v_total(&self) -> f32 {
        (self.dv_prograde * self.dv_prograde
            + self.dv_normal * self.dv_normal
            + self.dv_radial * self.dv_radial)
            .sqrt()
    }

    /// Burn duration in seconds assuming 400 N bipropellant engine and 2,200 kg wet mass.
    pub fn burn_duration_seconds(&self) -> f32 {
        let dv = self.delta_v_total();
        // F = m * a  =>  t = (m * dv) / F
        (2200.0 * dv) / 400.0
    }

    /// Propellant mass consumed in kg (Isp ~ 318 s).
    pub fn propellant_consumed_kg(&self) -> f32 {
        let dv = self.delta_v_total();
        const G0: f32 = 9.80665;
        const ISP: f32 = 318.0;
        let mass_initial = 2200.0;
        mass_initial * (1.0 - (-dv / (ISP * G0)).exp())
    }
}

/// A discrete point on a projected orbital trajectory.
#[derive(Debug, Clone, Copy)]
pub struct TrajectoryPoint {
    pub time_hours: f32,
    pub position: Vector3,
    pub velocity: Vector3,
    pub distance_to_europa_km: f32,
}

/// Closest approach calculation results.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClosestApproach {
    pub time_hours: f32,
    pub distance_km: f32,
    pub altitude_km: f32,
    pub relative_velocity_kms: f32,
    pub b_dot_t_km: f32,
    pub b_dot_r_km: f32,
}

/// Mission event in the timeline.
#[derive(Debug, Clone)]
pub struct MissionEvent {
    pub time_hours: f32,
    pub title: &'static str,
    pub description: &'static str,
    pub delta_v_ms: Option<f32>,
    pub status: &'static str,
}

/// Trajectory planning and analysis laboratory.
#[derive(Debug, Clone)]
pub struct TrajectoryLab {
    pub nodes: Vec<ManeuverNode>,
    pub selected_node_idx: usize,
    pub scrub_time_hours: f32,
    pub nominal_path: Vec<TrajectoryPoint>,
    pub planned_path: Vec<TrajectoryPoint>,
    pub closest_approach_nominal: ClosestApproach,
    pub closest_approach_planned: ClosestApproach,
    pub events: Vec<MissionEvent>,
}

impl TrajectoryLab {
    pub fn new(model: &JovianModel) -> Self {
        let mut lab = Self {
            nodes: vec![ManeuverNode {
                id: 1,
                epoch_hours: model.mission_time_hours + 18.5,
                dv_prograde: 32.4, // m/s
                dv_normal: 5.2,
                dv_radial: -8.1,
                is_enabled: true,
            }],
            selected_node_idx: 0,
            scrub_time_hours: model.mission_time_hours,
            nominal_path: Vec::new(),
            planned_path: Vec::new(),
            closest_approach_nominal: ClosestApproach::default(),
            closest_approach_planned: ClosestApproach::default(),
            events: vec![
                MissionEvent {
                    time_hours: 12.0,
                    title: "PERIJOVE 14",
                    description: "Jovian closest approach (R = 742,000 km)",
                    delta_v_ms: None,
                    status: "EXECUTED",
                },
                MissionEvent {
                    time_hours: 36.5,
                    title: "DSM-2 MANEUVER",
                    description: "Trajectory shaping for Europa resonance",
                    delta_v_ms: Some(18.2),
                    status: "EXECUTED",
                },
                MissionEvent {
                    time_hours: 66.5,
                    title: "PLANNED BURN (NODE-1)",
                    description: "Targeted flyby trimming burn",
                    delta_v_ms: Some(34.2),
                    status: "ARMED",
                },
                MissionEvent {
                    time_hours: 88.2,
                    title: "EUROPA FLYBY E14",
                    description: "Closest approach alt 102.4 km / REASON sounding",
                    delta_v_ms: None,
                    status: "PLANNED",
                },
                MissionEvent {
                    time_hours: 142.0,
                    title: "APOJOVE 14",
                    description: "Orbit apogee near Callisto / Telemetry dump",
                    delta_v_ms: None,
                    status: "PLANNED",
                },
            ],
        };
        lab.recompute(model);
        lab
    }

    pub fn selected_node(&self) -> Option<&ManeuverNode> {
        self.nodes.get(self.selected_node_idx)
    }

    pub fn selected_node_mut(&mut self) -> Option<&mut ManeuverNode> {
        self.nodes.get_mut(self.selected_node_idx)
    }

    pub fn add_node(&mut self, epoch_hours: f32) {
        let next_id = self.nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1;
        self.nodes.push(ManeuverNode {
            id: next_id,
            epoch_hours,
            dv_prograde: 10.0,
            dv_normal: 0.0,
            dv_radial: 0.0,
            is_enabled: true,
        });
        self.selected_node_idx = self.nodes.len().saturating_sub(1);
    }

    pub fn remove_selected_node(&mut self) {
        if !self.nodes.is_empty() {
            self.nodes.remove(self.selected_node_idx);
            if self.selected_node_idx >= self.nodes.len() && !self.nodes.is_empty() {
                self.selected_node_idx = self.nodes.len() - 1;
            }
        }
    }

    pub fn scrub_time(&mut self, delta_hours: f32, model: &JovianModel) {
        let max_t = model.mission_time_hours + model.sc_period_hours * 1.5;
        self.scrub_time_hours =
            (self.scrub_time_hours + delta_hours).clamp(model.mission_time_hours, max_t);
    }

    pub fn set_scrub_time(&mut self, time_hours: f32, model: &JovianModel) {
        let max_t = model.mission_time_hours + model.sc_period_hours * 1.5;
        self.scrub_time_hours = time_hours.clamp(model.mission_time_hours, max_t);
    }

    pub fn adjust_selected_node(
        &mut self,
        d_prograde: f32,
        d_normal: f32,
        d_radial: f32,
        model: &JovianModel,
    ) {
        if let Some(node) = self.selected_node_mut() {
            node.dv_prograde += d_prograde;
            node.dv_normal += d_normal;
            node.dv_radial += d_radial;
        }
        self.recompute(model);
    }

    /// Recompute pre-burn and post-burn trajectories.
    pub fn recompute(&mut self, model: &JovianModel) {
        let start_time = model.mission_time_hours;
        let horizon_hours = model.sc_period_hours * 1.25;
        let steps = 180;
        let dt = horizon_hours / steps as f32;

        self.nominal_path.clear();
        self.planned_path.clear();

        let mut min_dist_nom = f32::INFINITY;
        let mut min_point_nom = TrajectoryPoint {
            time_hours: start_time,
            position: Vector3::ZERO,
            velocity: Vector3::ZERO,
            distance_to_europa_km: 0.0,
        };

        let mut min_dist_plan = f32::INFINITY;
        let mut min_point_plan = TrajectoryPoint {
            time_hours: start_time,
            position: Vector3::ZERO,
            velocity: Vector3::ZERO,
            distance_to_europa_km: 0.0,
        };

        // Active maneuver node
        let active_node = self.nodes.iter().find(|n| n.is_enabled);

        for step in 0..=steps {
            let t = start_time + step as f32 * dt;
            let (pos_nom, vel_nom, _) = model.spacecraft_position_at(t);
            let europa_pos = model.europa.position_at(t);
            let dist_nom = pos_nom.distance_to(europa_pos);

            let pt_nom = TrajectoryPoint {
                time_hours: t,
                position: pos_nom,
                velocity: vel_nom,
                distance_to_europa_km: dist_nom,
            };
            self.nominal_path.push(pt_nom);

            if dist_nom < min_dist_nom {
                min_dist_nom = dist_nom;
                min_point_nom = pt_nom;
            }

            // Compute post-burn planned path
            let (pos_plan, vel_plan) = if let Some(node) = active_node {
                if t >= node.epoch_hours {
                    // Propagate perturbation after burn epoch
                    let dt_since_burn = t - node.epoch_hours;
                    let dv_kms = Vector3::new(
                        node.dv_prograde * 0.001,
                        node.dv_normal * 0.001,
                        node.dv_radial * 0.001,
                    );
                    // Approximate linearised orbital perturbation
                    let d_pos = dv_kms * (dt_since_burn * 3600.0);
                    let perturbed_pos = pos_nom + d_pos;
                    let perturbed_vel = vel_nom + dv_kms;
                    (perturbed_pos, perturbed_vel)
                } else {
                    (pos_nom, vel_nom)
                }
            } else {
                (pos_nom, vel_nom)
            };

            let dist_plan = pos_plan.distance_to(europa_pos);
            let pt_plan = TrajectoryPoint {
                time_hours: t,
                position: pos_plan,
                velocity: vel_plan,
                distance_to_europa_km: dist_plan,
            };
            self.planned_path.push(pt_plan);

            if dist_plan < min_dist_plan {
                min_dist_plan = dist_plan;
                min_point_plan = pt_plan;
            }
        }

        let europa_radius = model.europa.radius_km;
        let europa_vel_nom = model.europa.velocity_at(min_point_nom.time_hours);
        let rel_v_nom = (min_point_nom.velocity - europa_vel_nom).length();

        let rel_pos_nom =
            min_point_nom.position - model.europa.position_at(min_point_nom.time_hours);
        let b_dot_t_nom = rel_pos_nom.x * 0.866 - rel_pos_nom.z * 0.5;
        let b_dot_r_nom = rel_pos_nom.y;

        self.closest_approach_nominal = ClosestApproach {
            time_hours: min_point_nom.time_hours,
            distance_km: min_dist_nom,
            altitude_km: (min_dist_nom - europa_radius).max(0.0),
            relative_velocity_kms: rel_v_nom,
            b_dot_t_km: b_dot_t_nom,
            b_dot_r_km: b_dot_r_nom,
        };

        let europa_vel_plan = model.europa.velocity_at(min_point_plan.time_hours);
        let rel_v_plan = (min_point_plan.velocity - europa_vel_plan).length();
        let rel_pos_plan =
            min_point_plan.position - model.europa.position_at(min_point_plan.time_hours);
        let b_dot_t_plan = rel_pos_plan.x * 0.866 - rel_pos_plan.z * 0.5;
        let b_dot_r_plan = rel_pos_plan.y;

        self.closest_approach_planned = ClosestApproach {
            time_hours: min_point_plan.time_hours,
            distance_km: min_dist_plan,
            altitude_km: (min_dist_plan - europa_radius).max(0.0),
            relative_velocity_kms: rel_v_plan,
            b_dot_t_km: b_dot_t_plan,
            b_dot_r_km: b_dot_r_plan,
        };
    }

    /// Jovian radiation dosage rate (rad/hr) at a given distance from Jupiter center.
    pub fn radiation_dose_rate_at(radius_km: f32) -> f32 {
        let r_j = radius_km / JUPITER_RADIUS_KM;
        if r_j < 1.2 {
            50000.0 // Inside ring/inner belt: extreme lethal radiation
        } else if r_j < 6.0 {
            // Io torus region (5.9 R_j)
            1200.0 * (6.0 / r_j).powi(3)
        } else if r_j < 9.5 {
            // Europa orbit region (9.4 R_j) ~ 40-70 krad/day => ~2000 rad/hr
            180.0 * (9.5 / r_j).powi(2)
        } else if r_j < 15.0 {
            // Ganymede region (15 R_j)
            35.0 * (15.0 / r_j)
        } else {
            // Callisto region (26 R_j)
            1.2
        }
    }
}
