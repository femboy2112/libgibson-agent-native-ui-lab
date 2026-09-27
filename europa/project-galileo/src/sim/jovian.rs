//! Deterministic Jovian system orbital simulation.
//!
//! Models Jupiter and the Galilean moons (Io, Europa, Ganymede, Callisto)
//! with approximate 1:2:4 periods for Io, Europa, and Ganymede. The spacecraft
//! uses an illustrative Keplerian ellipse rather than a validated ephemeris.

use std::f32::consts::PI;

pub const JUPITER_RADIUS_KM: f32 = 71492.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetBody {
    Jupiter,
    Io,
    Europa,
    Ganymede,
    Callisto,
    Spacecraft,
}

impl TargetBody {
    pub const ALL: [Self; 6] = [
        Self::Jupiter,
        Self::Io,
        Self::Europa,
        Self::Ganymede,
        Self::Callisto,
        Self::Spacecraft,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Jupiter => "JUPITER",
            Self::Io => "IO",
            Self::Europa => "EUROPA",
            Self::Ganymede => "GANYMEDE",
            Self::Callisto => "CALLISTO",
            Self::Spacecraft => "GALILEO-ORBITER",
        }
    }

    pub fn color_rgb(self) -> (u8, u8, u8) {
        match self {
            Self::Jupiter => (235, 175, 115),   // Warm ochre / amber
            Self::Io => (245, 205, 55),         // Volcanic sulfur yellow
            Self::Europa => (195, 230, 255),    // Ice white / pale cyan
            Self::Ganymede => (170, 160, 145),  // Silicate gray / brown
            Self::Callisto => (110, 105, 100),  // Heavily cratered dark gray
            Self::Spacecraft => (70, 245, 200), // Bright cyan telemetry
        }
    }
}

/// A 3D vector in kilometers.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vector3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn distance_to(self, other: Self) -> f32 {
        (self - other).length()
    }

    pub fn normalized(self) -> Self {
        let len = self.length();
        if len > 1e-6 {
            Self {
                x: self.x / len,
                y: self.y / len,
                z: self.z / len,
            }
        } else {
            Self::ZERO
        }
    }

    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }
}

impl std::ops::Add for Vector3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl std::ops::Sub for Vector3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
        }
    }
}

impl std::ops::Mul<f32> for Vector3 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

/// Physical and orbital parameters for a Jovian moon.
#[derive(Debug, Clone)]
pub struct Moon {
    pub body: TargetBody,
    pub radius_km: f32,
    pub semi_major_axis_km: f32,
    pub period_hours: f32,
    pub inclination_deg: f32,
    pub initial_phase_rad: f32,
}

impl Moon {
    pub fn io() -> Self {
        Self {
            body: TargetBody::Io,
            radius_km: 1821.6,
            semi_major_axis_km: 421700.0,
            period_hours: 42.46, // ~1.769 days
            inclination_deg: 0.05,
            initial_phase_rad: 0.85,
        }
    }

    pub fn europa() -> Self {
        Self {
            body: TargetBody::Europa,
            radius_km: 1560.8,
            semi_major_axis_km: 670900.0,
            period_hours: 84.92, // ~3.551 days (2x Io)
            inclination_deg: 0.47,
            // Illustrative epoch chosen so the resonant spacecraft tour has
            // its first sampled close encounter near MET +255 h.
            initial_phase_rad: 1.25,
        }
    }

    pub fn ganymede() -> Self {
        Self {
            body: TargetBody::Ganymede,
            radius_km: 2634.1,
            semi_major_axis_km: 1070400.0,
            period_hours: 171.71, // ~7.155 days (4x Io)
            inclination_deg: 0.20,
            initial_phase_rad: 4.12,
        }
    }

    pub fn callisto() -> Self {
        Self {
            body: TargetBody::Callisto,
            radius_km: 2410.3,
            semi_major_axis_km: 1882700.0,
            period_hours: 400.54, // ~16.689 days
            inclination_deg: 0.28,
            initial_phase_rad: 5.70,
        }
    }

    /// Compute orbital position at simulated time (hours).
    pub fn position_at(&self, time_hours: f32) -> Vector3 {
        let mean_motion = 2.0 * PI / self.period_hours;
        let theta = self.initial_phase_rad + mean_motion * time_hours;
        let inc_rad = self.inclination_deg * PI / 180.0;

        let r = self.semi_major_axis_km;
        let x = r * theta.cos();
        let y = r * theta.sin() * inc_rad.sin(); // Small vertical tilt
        let z = r * theta.sin() * inc_rad.cos();
        Vector3::new(x, y, z)
    }

    /// Compute orbital velocity vector at simulated time (hours).
    pub fn velocity_at(&self, time_hours: f32) -> Vector3 {
        let mean_motion = 2.0 * PI / self.period_hours;
        let theta = self.initial_phase_rad + mean_motion * time_hours;
        let inc_rad = self.inclination_deg * PI / 180.0;

        let v_mag = (2.0 * PI * self.semi_major_axis_km) / (self.period_hours * 3600.0); // km/s
        let vx = -v_mag * theta.sin();
        let vy = v_mag * theta.cos() * inc_rad.sin();
        let vz = v_mag * theta.cos() * inc_rad.cos();
        Vector3::new(vx, vy, vz)
    }
}

/// Spacecraft state in the Jovian system.
#[derive(Debug, Clone)]
pub struct SpacecraftState {
    pub position: Vector3,
    pub velocity: Vector3,
    pub periapsis_km: f32,
    pub apoapsis_km: f32,
    pub period_hours: f32,
    pub true_anomaly_rad: f32,
    pub target: TargetBody,
    pub distance_to_target_km: f32,
    pub relative_velocity_kms: f32,
}

/// Complete Jovian orbital model.
#[derive(Debug, Clone)]
pub struct JovianModel {
    pub io: Moon,
    pub europa: Moon,
    pub ganymede: Moon,
    pub callisto: Moon,
    pub mission_time_hours: f32,
    pub time_warp: f32,
    pub paused: bool,
    pub target: TargetBody,
    // Spacecraft nominal orbital parameters (resonant petal orbit)
    pub sc_semi_major_axis: f32,
    pub sc_eccentricity: f32,
    pub sc_inclination_rad: f32,
    pub sc_arg_periapsis_rad: f32,
    pub sc_period_hours: f32,
}

impl Default for JovianModel {
    fn default() -> Self {
        let sc_rp = 670900.0 + 5000.0; // Illustrative Europa encounter corridor
        let sc_ra = 1882700.0 * 0.95; // Apoapsis inside Callisto's orbit (1,788,565 km)
        let sc_a = (sc_rp + sc_ra) / 2.0;
        let sc_e = (sc_ra - sc_rp) / (sc_ra + sc_rp);
        let sc_period = 254.76; // Resonant tour orbit period ~10.6 days

        Self {
            io: Moon::io(),
            europa: Moon::europa(),
            ganymede: Moon::ganymede(),
            callisto: Moon::callisto(),
            mission_time_hours: 48.0, // Initial epoch
            time_warp: 1.0,
            paused: false,
            target: TargetBody::Europa,
            sc_semi_major_axis: sc_a,
            sc_eccentricity: sc_e,
            sc_inclination_rad: 0.47_f32.to_radians(), // aligned with Europa's orbital plane
            sc_arg_periapsis_rad: 1.25,
            sc_period_hours: sc_period,
        }
    }
}

impl JovianModel {
    pub fn update(&mut self, dt_seconds: f32) {
        if !self.paused {
            let dt_hours = (dt_seconds / 3600.0) * self.time_warp;
            self.mission_time_hours += dt_hours;
        }
    }

    /// Compute nominal spacecraft position at time T (hours).
    pub fn spacecraft_position_at(&self, time_hours: f32) -> (Vector3, Vector3, f32) {
        let mean_anomaly = (2.0 * PI / self.sc_period_hours) * time_hours;
        // Solve Kepler's equation: M = E - e*sin(E)
        let mut ecc_anomaly = mean_anomaly;
        for _ in 0..5 {
            let f = ecc_anomaly - self.sc_eccentricity * ecc_anomaly.sin() - mean_anomaly;
            let f_prime = 1.0 - self.sc_eccentricity * ecc_anomaly.cos();
            ecc_anomaly -= f / f_prime;
        }

        // True anomaly nu
        let sin_half_nu = ((1.0 + self.sc_eccentricity) / (1.0 - self.sc_eccentricity)).sqrt()
            * (ecc_anomaly * 0.5).tan();
        let nu = 2.0 * sin_half_nu.atan();

        // Orbital radius
        let r = self.sc_semi_major_axis * (1.0 - self.sc_eccentricity * ecc_anomaly.cos());

        // Position in orbital plane
        let u = nu + self.sc_arg_periapsis_rad;
        let x = r * u.cos();
        let y = r * u.sin() * self.sc_inclination_rad.sin();
        let z = r * u.sin() * self.sc_inclination_rad.cos();
        let pos = Vector3::new(x, y, z);

        // Vis-viva velocity magnitude (standard Jovian GM = 1.26686534e8 km^3/s^2)
        const GM_JUPITER: f32 = 126686534.0;
        let v_mag = (GM_JUPITER * (2.0 / r - 1.0 / self.sc_semi_major_axis))
            .max(0.0)
            .sqrt();

        // Velocity unit vector roughly tangential to orbit
        let u_dot = u + PI * 0.5;
        let vx = v_mag * u_dot.cos();
        let vy = v_mag * u_dot.sin() * self.sc_inclination_rad.sin();
        let vz = v_mag * u_dot.sin() * self.sc_inclination_rad.cos();
        let vel = Vector3::new(vx, vy, vz);

        (pos, vel, nu)
    }

    /// Current full spacecraft state.
    pub fn spacecraft_state(&self) -> SpacecraftState {
        let (pos, vel, nu) = self.spacecraft_position_at(self.mission_time_hours);

        let target_pos = match self.target {
            TargetBody::Jupiter => Vector3::ZERO,
            TargetBody::Io => self.io.position_at(self.mission_time_hours),
            TargetBody::Europa => self.europa.position_at(self.mission_time_hours),
            TargetBody::Ganymede => self.ganymede.position_at(self.mission_time_hours),
            TargetBody::Callisto => self.callisto.position_at(self.mission_time_hours),
            TargetBody::Spacecraft => pos,
        };

        let target_vel = match self.target {
            TargetBody::Jupiter => Vector3::ZERO,
            TargetBody::Io => self.io.velocity_at(self.mission_time_hours),
            TargetBody::Europa => self.europa.velocity_at(self.mission_time_hours),
            TargetBody::Ganymede => self.ganymede.velocity_at(self.mission_time_hours),
            TargetBody::Callisto => self.callisto.velocity_at(self.mission_time_hours),
            TargetBody::Spacecraft => vel,
        };

        let dist = pos.distance_to(target_pos);
        let rel_vel = (vel - target_vel).length();
        let rp = self.sc_semi_major_axis * (1.0 - self.sc_eccentricity);
        let ra = self.sc_semi_major_axis * (1.0 + self.sc_eccentricity);

        SpacecraftState {
            position: pos,
            velocity: vel,
            periapsis_km: rp,
            apoapsis_km: ra,
            period_hours: self.sc_period_hours,
            true_anomaly_rad: nu,
            target: self.target,
            distance_to_target_km: dist,
            relative_velocity_kms: rel_vel,
        }
    }

    /// Communication line-of-sight metrics to Earth.
    pub fn earth_communication(&self) -> (f32, f32, bool) {
        // Earth distance varies between 588 million km (closest) and 968 million km (conjunction)
        // Approximate orbital epoch variation
        let earth_phase = (self.mission_time_hours / (365.25 * 24.0)) * 2.0 * PI;
        let dist_au = 5.2 - earth_phase.cos(); // 4.2 to 6.2 AU
        let dist_km = dist_au * 149597870.7;

        // One-way light time in seconds (c ~ 299792 km/s)
        let light_time_seconds = dist_km / 299_792.47;

        // Sun-Earth-Probe (SEP) angle in degrees
        let sep_angle = (earth_phase.sin().abs() * 90.0).max(4.2);

        // Illustrative Jovian shadow flag; this does not model the separate
        // Earth-probe line-of-sight occultation or solar-conjunction blackout.
        let sc_pos = self.spacecraft_position_at(self.mission_time_hours).0;
        let occulted = self.is_in_jupiter_shadow(sc_pos);
        (light_time_seconds, sep_angle, occulted)
    }

    /// Check whether a 3D position (km) is inside Jupiter's umbral shadow cone.
    /// Sun is towards +X. Shadow cone extends along -X with radius R_J = 71,492 km.
    pub fn is_in_jupiter_shadow(&self, pos: Vector3) -> bool {
        if pos.x >= 0.0 {
            return false;
        }
        let perp_dist_sq = pos.y * pos.y + pos.z * pos.z;
        perp_dist_sq < (JUPITER_RADIUS_KM * JUPITER_RADIUS_KM)
    }

    /// Computes the Jovian magnetic dipole field vector (in Gauss) at position `pos`.
    /// Jupiter's magnetic dipole is tilted ~9.6° from the rotation axis.
    pub fn magnetic_field_at(&self, pos: Vector3) -> (Vector3, f32) {
        let r = pos.length();
        if r < 100.0 {
            return (Vector3::new(0.0, 1.0, 0.0), 0.0);
        }
        let r_norm = pos.normalized();
        let tilt = 9.6_f32.to_radians();
        let m = Vector3::new(tilt.sin(), tilt.cos(), 0.0);
        let m_dot_r = m.dot(r_norm);
        let b_vec = r_norm * (3.0 * m_dot_r) - m;
        let r_j = r / JUPITER_RADIUS_KM;
        let b_mag_gauss = 4.28 / (r_j * r_j * r_j);
        (b_vec.normalized(), b_mag_gauss)
    }
}
