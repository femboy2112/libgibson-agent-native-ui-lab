//! Vehicle engineering subsystems, instruments, and alert management.
//!
//! Maintains bounded history for telemetry sparklines, subsystem health monitors,
//! scientific payload modes, and alert logs.

use std::collections::VecDeque;

pub const MAX_HISTORY_SAMPLES: usize = 64;
pub const MAX_ALERT_LOGS: usize = 16;
pub const MAX_COMMAND_HISTORY: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
pub struct Alert {
    pub id: u64,
    pub timestamp_hours: f32,
    pub severity: AlertSeverity,
    pub subsystem: &'static str,
    pub message: String,
    pub acknowledged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstrumentMode {
    Off,
    Standby,
    Active,
    Calibrating,
}

impl InstrumentMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "OFF",
            Self::Standby => "STANDBY",
            Self::Active => "ACTIVE",
            Self::Calibrating => "CALIBRATING",
        }
    }
}

/// Comprehensive vehicle telemetry state.
#[derive(Debug, Clone)]
pub struct TelemetryState {
    // Power subsystem
    pub rtg_power_w: f32,
    pub battery_soc: f32, // 0.0 to 1.0
    pub bus_voltage_v: f32,
    pub bus_current_a: f32,
    pub battery_temp_c: f32,
    pub rtg_history: VecDeque<f32>,

    // Telecommunications
    pub comm_carrier_snr_db: f32,
    pub downlink_rate_kbps: f32,
    pub hga_pointing_error_deg: f32,
    pub dsn_station: &'static str,
    pub snr_history: VecDeque<f32>,

    // Thermal
    pub prop_tank_temp_c: f32,
    pub cryo_sensor_temp_k: f32,
    pub avionics_temp_c: f32,
    pub thermal_history: VecDeque<f32>,

    // Propulsion
    pub rcs_propellant_kg: f32,
    pub main_engine_propellant_kg: f32,
    pub tank_pressure_bar: f32,

    // Attitude determination & control
    pub rwa_speeds_rpm: [f32; 4],
    pub star_tracker_locked: bool,
    pub gyro_drift_deg_hr: f32,

    // Science Payload
    pub radar_reason_mode: InstrumentMode,
    pub spectro_mise_mode: InstrumentMode,
    pub mass_spec_maspex_mode: InstrumentMode,
    pub plasma_pims_mode: InstrumentMode,
    pub dust_suda_mode: InstrumentMode,

    // Alerts & Command logs
    pub alerts: VecDeque<Alert>,
    pub next_alert_id: u64,
    pub command_history: VecDeque<String>,

    // Simulation control
    pub warp_rate: f32,
    pub is_paused: bool,
    pub total_power_demand_w: f32,
    pub net_power_w: f32,
    pub is_in_eclipse: bool,
}

impl Default for TelemetryState {
    fn default() -> Self {
        let mut rtg_history = VecDeque::with_capacity(MAX_HISTORY_SAMPLES);
        let mut snr_history = VecDeque::with_capacity(MAX_HISTORY_SAMPLES);
        let mut thermal_history = VecDeque::with_capacity(MAX_HISTORY_SAMPLES);

        for i in 0..24 {
            let t = i as f32 * 0.1;
            rtg_history.push_back(324.5 + t.sin() * 2.1);
            snr_history.push_back(42.8 + (t * 1.5).cos() * 1.4);
            thermal_history.push_back(14.2 + (t * 0.7).sin() * 0.8);
        }

        let mut alerts = VecDeque::with_capacity(MAX_ALERT_LOGS);
        alerts.push_back(Alert {
            id: 1,
            timestamp_hours: 44.2,
            severity: AlertSeverity::Info,
            subsystem: "COMMS",
            message: "DSN DSS-14 Goldstone carrier lock acquired".into(),
            acknowledged: true,
        });
        alerts.push_back(Alert {
            id: 2,
            timestamp_hours: 47.5,
            severity: AlertSeverity::Warning,
            subsystem: "RAD",
            message: "Jovian magnetospheric electron flux exceeding 1.2e7 cm^-2 s^-1".into(),
            acknowledged: false,
        });

        Self {
            rtg_power_w: 324.8,
            battery_soc: 0.894,
            bus_voltage_v: 28.38,
            bus_current_a: 11.45,
            battery_temp_c: 18.2,
            rtg_history,

            comm_carrier_snr_db: 43.2,
            downlink_rate_kbps: 142.0,
            hga_pointing_error_deg: 0.018,
            dsn_station: "DSS-14 (Goldstone)",
            snr_history,

            prop_tank_temp_c: 19.4,
            cryo_sensor_temp_k: 78.2,
            avionics_temp_c: 14.6,
            thermal_history,

            rcs_propellant_kg: 84.2,
            main_engine_propellant_kg: 1420.5,
            tank_pressure_bar: 22.8,

            rwa_speeds_rpm: [1420.0, -1180.0, 940.0, 1650.0],
            star_tracker_locked: true,
            gyro_drift_deg_hr: 0.002,

            radar_reason_mode: InstrumentMode::Active,
            spectro_mise_mode: InstrumentMode::Active,
            mass_spec_maspex_mode: InstrumentMode::Standby,
            plasma_pims_mode: InstrumentMode::Active,
            dust_suda_mode: InstrumentMode::Standby,

            alerts,
            next_alert_id: 3,
            command_history: VecDeque::from(vec![
                "INIT COMM_LINK --DSN=DSS-14".into(),
                "RADAR ARM --PULSE=HF_CHIRP".into(),
                "ATT_HOLD --MODE=EARTH_POINTING".into(),
            ]),

            warp_rate: 1.0,
            is_paused: false,
            total_power_demand_w: 310.0,
            net_power_w: 14.8,
            is_in_eclipse: false,
        }
    }
}

impl TelemetryState {
    pub fn update(&mut self, time_hours: f32) {
        self.update_coupled(time_hours, 0.033, false);
    }

    pub fn update_coupled(&mut self, time_hours: f32, dt_seconds: f32, in_eclipse: bool) {
        let t = time_hours;
        self.is_in_eclipse = in_eclipse;

        // 1. RTG thermal radioactive decay & fluctuation
        self.rtg_power_w = 324.0 + (t * 0.5).sin() * 2.5 + (t * 2.1).cos() * 0.8;

        // 2. Instrument power loads
        let reason_load = match self.radar_reason_mode {
            InstrumentMode::Active => 125.0,
            InstrumentMode::Standby => 15.0,
            InstrumentMode::Calibrating => 85.0,
            InstrumentMode::Off => 0.0,
        };
        let mise_load = match self.spectro_mise_mode {
            InstrumentMode::Active => 45.0,
            InstrumentMode::Standby => 8.0,
            InstrumentMode::Calibrating => 30.0,
            InstrumentMode::Off => 0.0,
        };
        let maspex_load = match self.mass_spec_maspex_mode {
            InstrumentMode::Active => 65.0,
            InstrumentMode::Standby => 10.0,
            InstrumentMode::Calibrating => 40.0,
            InstrumentMode::Off => 0.0,
        };
        let fields_load = match self.plasma_pims_mode {
            InstrumentMode::Active => 22.0,
            _ => 5.0,
        };
        let base_avionics = 145.0; // OBC, Star trackers, transponder, heaters

        self.total_power_demand_w =
            base_avionics + reason_load + mise_load + maspex_load + fields_load;
        self.net_power_w = self.rtg_power_w - self.total_power_demand_w;

        // Battery capacity ~ 1200 Watt-hours (4.32 MJ)
        const BATTERY_CAPACITY_WH: f32 = 1200.0;
        let delta_soc = (self.net_power_w * (dt_seconds / 3600.0)) / BATTERY_CAPACITY_WH;
        self.battery_soc = (self.battery_soc + delta_soc).clamp(0.05, 1.0);

        self.bus_voltage_v = 24.0 + self.battery_soc * 4.4;
        self.bus_current_a = self.total_power_demand_w / self.bus_voltage_v;

        // RF Comm link
        self.comm_carrier_snr_db = 42.5 + (t * 0.3).sin() * 1.8;
        if in_eclipse {
            self.comm_carrier_snr_db = (self.comm_carrier_snr_db - 3.5).max(0.0);
        }

        // Thermal states
        let eclipse_cooling = if in_eclipse { -8.5 } else { 0.0 };
        self.cryo_sensor_temp_k =
            77.0 + (t * 0.2).cos().abs() * 2.2 + if in_eclipse { -2.0 } else { 0.0 };
        self.avionics_temp_c = 14.0 + (t * 0.15).sin() * 1.2 + eclipse_cooling * 0.4;
        self.prop_tank_temp_c = 19.4 + (t * 0.1).sin() * 0.9 + eclipse_cooling * 0.6;

        // Push history samples (bounded FIFO)
        if self.rtg_history.len() >= MAX_HISTORY_SAMPLES {
            self.rtg_history.pop_front();
        }
        self.rtg_history.push_back(self.rtg_power_w);

        if self.snr_history.len() >= MAX_HISTORY_SAMPLES {
            self.snr_history.pop_front();
        }
        self.snr_history.push_back(self.comm_carrier_snr_db);

        if self.thermal_history.len() >= MAX_HISTORY_SAMPLES {
            self.thermal_history.pop_front();
        }
        self.thermal_history.push_back(self.avionics_temp_c);
    }

    pub fn add_alert(
        &mut self,
        time_hours: f32,
        severity: AlertSeverity,
        subsystem: &'static str,
        message: impl Into<String>,
    ) {
        if self.alerts.len() >= MAX_ALERT_LOGS {
            self.alerts.pop_front();
        }
        let id = self.next_alert_id;
        self.next_alert_id += 1;
        self.alerts.push_back(Alert {
            id,
            timestamp_hours: time_hours,
            severity,
            subsystem,
            message: message.into(),
            acknowledged: false,
        });
    }

    pub fn record_command(&mut self, cmd: impl Into<String>) {
        if self.command_history.len() >= MAX_COMMAND_HISTORY {
            self.command_history.pop_front();
        }
        self.command_history.push_back(cmd.into());
    }

    pub fn has_unacknowledged_alerts(&self) -> bool {
        self.alerts.iter().any(|a| !a.acknowledged)
    }

    pub fn acknowledge_all_alerts(&mut self) {
        for a in &mut self.alerts {
            a.acknowledged = true;
        }
    }

    pub fn consume_propellant(&mut self, mass_kg: f32) {
        self.main_engine_propellant_kg = (self.main_engine_propellant_kg - mass_kg).max(0.0);
        self.tank_pressure_bar = (24.0 * (self.main_engine_propellant_kg / 1420.5)).max(1.0);
    }
}
