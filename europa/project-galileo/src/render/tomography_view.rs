//! Europa subsurface ice shell tomography & radar sounding renderer.
//!
//! HERO SCIENTIFIC VIEW:
//! Renders a live synthetic radargram (B-scan) and power profile (A-scan)
//! representing dual-frequency radar sounding (REASON replica: 9 MHz HF / 60 MHz VHF)
//! through Europa's 0-35 km ice shell and subsurface ocean.
//!
//! Visual features:
//! - Dual-layer subcell RgbRaster radargram (dielectric reflectivity & thermal field)
//! - Double ridge surface relief and vacuum interface
//! - Brittle conductive lid (0-4 km) with hyper-saline brine pockets & fault conduits
//! - Ductile convective layer (4-22 km) with rising thermal diapir plumes
//! - Basal melting/freezing ice-ocean boundary with acoustic/radar impedance mismatch
//! - Turbulent subsurface ocean (>22 km) with buoyant hydrothermal plumes
//! - Real-time propagating radar pulse wavefront
//! - Braille overlays for fracture networks, isotherms, and echo peaks
//! - Live A-scan logarithmic power trace (dB vs depth)

use gibson::cell::{Cell, Color, Glyph, Style};
use gibson::raster::RgbRaster;
use gibson::surface::Surface;
use gibson::BrailleCanvas;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadarBand {
    Hf9MHz,    // High-penetration deep sounding (0-35 km)
    Vhf60MHz,  // High-resolution shallow sounding (0-7 km)
    SplitBand, // Dual-frequency sounding (0-35 km combined)
}

pub struct TomographyViewRenderer {
    pub zoom: f32,
    pub band: RadarBand,
    pub along_track_km: f32,
    pub pulse_phase: f32, // 0.0 to 1.0 sweeping pulse
    pub feature_name: &'static str,
    pub nominal_ice_thickness_km: f32,
    pub gain_db: f32,
    pub cursor_depth_km: f32,
    pub show_isotherms: bool,
    pub show_ascan: bool,
}

impl Default for TomographyViewRenderer {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            band: RadarBand::Hf9MHz,
            along_track_km: 0.0,
            pulse_phase: 0.25,
            feature_name: "AGENOR LINEA - STRIKE-SLIP RIDGE",
            nominal_ice_thickness_km: 18.6,
            gain_db: 42.0,
            cursor_depth_km: 18.6,
            show_isotherms: true,
            show_ascan: true,
        }
    }
}

impl TomographyViewRenderer {
    pub fn update(&mut self, dt_seconds: f32) {
        self.along_track_km += dt_seconds * 3.5; // Spacecraft ground track motion
        self.pulse_phase = (self.pulse_phase + dt_seconds * 0.75) % 1.0;
    }

    pub fn adjust_zoom(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(0.4, 4.0);
    }

    pub fn toggle_band(&mut self) {
        self.band = match self.band {
            RadarBand::Hf9MHz => RadarBand::Vhf60MHz,
            RadarBand::Vhf60MHz => RadarBand::SplitBand,
            RadarBand::SplitBand => RadarBand::Hf9MHz,
        };
    }

    pub fn move_depth_cursor(&mut self, delta_km: f32) {
        let max_depth = match self.band {
            RadarBand::Hf9MHz | RadarBand::SplitBand => 35.0,
            RadarBand::Vhf60MHz => 7.0,
        };
        self.cursor_depth_km = (self.cursor_depth_km + delta_km).clamp(0.0, max_depth);
    }

    pub fn set_depth_cursor(&mut self, depth_km: f32) {
        let max_depth = match self.band {
            RadarBand::Hf9MHz | RadarBand::SplitBand => 35.0,
            RadarBand::Vhf60MHz => 7.0,
        };
        self.cursor_depth_km = depth_km.clamp(0.0, max_depth);
    }

    pub fn adjust_gain(&mut self, delta_db: f32) {
        self.gain_db = (self.gain_db + delta_db).clamp(10.0, 75.0);
    }

    pub fn set_feature(&mut self, name: &'static str, thickness_km: f32) {
        self.feature_name = name;
        self.nominal_ice_thickness_km = thickness_km;
        if self.cursor_depth_km > 30.0 || self.cursor_depth_km < 5.0 {
            self.cursor_depth_km = thickness_km;
        }
    }

    /// Compute procedural radar reflectivity / dielectric loss at (along_track_x, depth_z).
    /// depth_z is in km (0.0 = surface, 35.0 = deep ocean).
    /// along_track_x is in km (-20.0 to +20.0).
    fn evaluate_medium(&self, x: f32, z: f32) -> MediumSample {
        let ice_depth = self.nominal_ice_thickness_km
            + 1.8 * (x * 0.18 + self.along_track_km * 0.05).sin()
            + 0.9 * (x * 0.42).cos();

        // Surface elevation relief (ridges and troughs)
        let surface_relief = 0.25 * (x * 0.8).sin() + 0.12 * (x * 2.1).cos();
        let effective_z = z - surface_relief;

        if effective_z < 0.0 {
            // Vacuum above Europa
            return MediumSample {
                reflectivity: 0.0,
                temperature_k: 100.0,
                dielectric_constant: 1.0,
                medium_type: MediumType::Vacuum,
            };
        }

        if effective_z < 0.2 {
            // Surface reflection: intense radar specular echo
            let roughness = ((x * 5.0).sin() * 0.5 + 0.5).powi(2);
            return MediumSample {
                reflectivity: 0.92 + roughness * 0.08,
                temperature_k: 105.0,
                dielectric_constant: 3.15,
                medium_type: MediumType::Surface,
            };
        }

        if effective_z < ice_depth {
            // Inside Ice Shell
            let depth_ratio = effective_z / ice_depth;
            let temp_k = 105.0 + depth_ratio * (270.0 - 105.0);

            // Cold brittle lid (0 to ~4 km)
            if effective_z < 4.5 {
                // Fracture conduits / cycloid crack reflections
                let crack1 = ((x - 2.5).abs() * 3.5).min(1.0);
                let crack2 = ((x + 6.0 + effective_z * 0.3).abs() * 2.8).min(1.0);
                let fracture_intensity = (1.0 - crack1).max(1.0 - crack2).powi(3);

                // Brine sill anomaly near 3.2 km depth
                let brine_dist = ((x - 4.0).powi(2) + (effective_z - 3.2).powi(2) * 9.0).sqrt();
                let brine_echo = if brine_dist < 1.4 {
                    (1.0 - brine_dist / 1.4).powi(2) * 0.85
                } else {
                    0.0
                };

                let speckle =
                    (((x * 12.3 + effective_z * 19.7).sin() * 43_758.55).fract() - 0.5) * 0.08;
                let refl =
                    (0.12 + fracture_intensity * 0.65 + brine_echo + speckle).clamp(0.0, 1.0);

                MediumSample {
                    reflectivity: refl,
                    temperature_k: temp_k,
                    dielectric_constant: 3.1 + brine_echo * 4.0,
                    medium_type: if brine_echo > 0.3 {
                        MediumType::BrinePocket
                    } else if fracture_intensity > 0.4 {
                        MediumType::FractureZone
                    } else {
                        MediumType::BrittleIce
                    },
                }
            } else {
                // Ductile convective ice (4.5 km to basal depth)
                // Thermal diapir upwelling plumes
                let diapir_x = -3.0 + (effective_z * 0.1).sin() * 2.0;
                let diapir_dist =
                    ((x - diapir_x).powi(2) + (effective_z - 12.0).powi(2) * 0.4).sqrt();
                let diapir_warmth = if diapir_dist < 5.0 {
                    (1.0 - diapir_dist / 5.0).powi(2) * 25.0
                } else {
                    0.0
                };

                // Attenuation increases exponentially with temperature
                let attenuation_loss = (-((effective_z - 4.5) * 0.12)).exp();
                let convective_rolls = (x * 0.35 + (effective_z * 0.25).sin()).sin() * 0.15;
                let speckle =
                    (((x * 7.1 + effective_z * 11.3).sin() * 28461.2).fract() - 0.5) * 0.05;

                let refl =
                    ((0.08 + convective_rolls + speckle) * attenuation_loss).clamp(0.01, 0.7);

                MediumSample {
                    reflectivity: refl,
                    temperature_k: temp_k + diapir_warmth,
                    dielectric_constant: 3.2,
                    medium_type: if diapir_warmth > 8.0 {
                        MediumType::ThermalDiapir
                    } else {
                        MediumType::DuctileIce
                    },
                }
            }
        } else if (effective_z - ice_depth).abs() < 0.6 {
            // Basal Ice-Ocean Boundary (Giant dielectric jump from eps=3.2 to eps=80)
            let interface_roughness = ((x * 4.2).sin() * 0.4 + (x * 8.7).cos() * 0.2).abs();
            let refl = 0.88 - interface_roughness * 0.25;
            MediumSample {
                reflectivity: refl.clamp(0.4, 0.95),
                temperature_k: 270.5,
                dielectric_constant: 45.0,
                medium_type: MediumType::BasalInterface,
            }
        } else {
            // Subsurface Saline Ocean (> ice_depth)
            let ocean_depth = effective_z - ice_depth;
            // Hydrothermal plume upwellings from seafloor
            let plume_x = 2.0 + (ocean_depth * 0.4).sin() * 1.5;
            let plume_dist = (x - plume_x).abs();
            let plume_activity = if plume_dist < 3.0 {
                (1.0 - plume_dist / 3.0).powi(2) * 0.45
            } else {
                0.0
            };

            // Ocean is radar opaque (severe volumetric attenuation), but thermal tomography / acoustic mapping shows plume structure
            let refl = (0.04 + plume_activity * 0.35).clamp(0.01, 0.6);

            MediumSample {
                reflectivity: refl,
                temperature_k: 273.15 + plume_activity * 4.0,
                dielectric_constant: 80.0,
                medium_type: if plume_activity > 0.15 {
                    MediumType::HydrothermalPlume
                } else {
                    MediumType::SalineOcean
                },
            }
        }
    }

    pub fn render(&self, width: u16, height: u16, mono: bool) -> Surface {
        if width == 0 || height == 0 {
            return Surface::new(width.max(1), height.max(1));
        }

        // Layout partitioning:
        // Left: Radargram 2D B-Scan
        // Right: A-Scan Logarithmic Power dB Trace & Geophysical Diagnostics
        let ascan_width = if self.show_ascan && width >= 70 {
            (width / 3).clamp(24, 40)
        } else {
            0
        };
        let bscan_width = width.saturating_sub(ascan_width);

        let pixel_w = bscan_width;
        let pixel_h = height * 2; // Subcell half-block vertical resolution
        let mut raster = RgbRaster::new(pixel_w.max(1), pixel_h.max(1));
        raster.clear((3, 6, 12));

        let mut braille = BrailleCanvas::new(bscan_width.max(1), height);

        let max_depth_km = match self.band {
            RadarBand::Hf9MHz | RadarBand::SplitBand => 35.0,
            RadarBand::Vhf60MHz => 7.0, // High-resolution shallow sounding
        };
        let swath_extent_km = 30.0 / self.zoom; // Scaled along-track swath

        // 1. Render B-Scan Radargram Raster
        let pulse_y = (self.pulse_phase * pixel_h as f32) as i32;

        for py in 0..pixel_h {
            let z_km = (py as f32 / pixel_h as f32) * max_depth_km;

            // Sweeping pulse illumination multiplier
            let dy = (py as i32 - pulse_y).abs();
            let pulse_boost = if dy < 4 {
                1.0 + (4 - dy) as f32 * 0.4
            } else if (py as i32) < pulse_y && (pulse_y - py as i32) < 20 {
                // Phosphor decay trail
                1.0 + (20 - (pulse_y - py as i32)) as f32 * 0.02
            } else {
                1.0
            };

            for px in 0..pixel_w {
                let x_km = ((px as f32 / pixel_w as f32) - 0.5) * swath_extent_km;
                let sample = self.evaluate_medium(x_km, z_km);

                let color = self.map_sample_to_color(&sample, pulse_boost);
                raster.set(px as i32, py as i32, color);
            }
        }

        // Draw interactive depth gate cursor on raster (dotted gold line)
        let cursor_py = ((self.cursor_depth_km / max_depth_km) * pixel_h as f32).round() as i32;
        if cursor_py >= 0 && cursor_py < pixel_h as i32 {
            for px in 0..pixel_w {
                if px % 4 != 0 {
                    raster.set(px as i32, cursor_py, (255, 215, 60));
                }
            }
        }

        // 2. Render Braille Structural Overlays (Cracks, Basal Line, Isotherms, Pulse Wavefront)
        let braille_px_w = (bscan_width * 2) as f32;
        let braille_px_h = (height * 4) as f32;

        // Isotherms: 150K, 200K, 250K
        if self.show_isotherms && self.band == RadarBand::Hf9MHz {
            for target_k in [150.0, 200.0, 250.0] {
                for bx in 0..(bscan_width * 2) {
                    let x_km = ((bx as f32 / braille_px_w) - 0.5) * swath_extent_km;
                    // Find depth where temperature matches target_k
                    let ice_thick = self.nominal_ice_thickness_km + 1.8 * (x_km * 0.18).sin();
                    let ratio = (target_k - 105.0) / (270.0 - 105.0);
                    let target_z = ice_thick * ratio;
                    let by = ((target_z / max_depth_km) * braille_px_h) as i32;
                    if by < height as i32 * 4 && bx % 3 != 0 {
                        braille.set(bx as i32, by);
                    }
                }
            }
        }

        // Basal Ice-Ocean Boundary Sharp Braille Profile
        for bx in 0..(bscan_width * 2) {
            let x_km = ((bx as f32 / braille_px_w) - 0.5) * swath_extent_km;
            let ice_thick = self.nominal_ice_thickness_km
                + 1.8 * (x_km * 0.18 + self.along_track_km * 0.05).sin()
                + 0.9 * (x_km * 0.42).cos();
            let by = ((ice_thick / max_depth_km) * braille_px_h) as i32;
            if by < height as i32 * 4 {
                braille.set(bx as i32, by);
                if by + 1 < height as i32 * 4 {
                    braille.set(bx as i32, by + 1);
                }
            }
        }

        // Propagating Radar Wavefront line in Braille
        let pulse_braille_y = (self.pulse_phase * braille_px_h) as i32;
        if pulse_braille_y < height as i32 * 4 {
            for bx in 0..(bscan_width * 2) {
                if ((bx as i32) + (self.pulse_phase * 20.0) as i32) % 4 != 0 {
                    braille.set(bx as i32, pulse_braille_y);
                }
            }
        }

        // Convert raster to surface
        let mut bscan_surface = if mono {
            raster.to_mono_surface()
        } else {
            raster.to_surface()
        };

        // Overlay Braille onto B-Scan surface
        let braille_surface = braille.to_surface(Style::default());
        for y in 0..height {
            for x in 0..bscan_width {
                if let Some(bc) = braille_surface.get(x, y) {
                    if bc.glyph != Glyph::space() {
                        let existing = bscan_surface.get(x, y).cloned().unwrap_or_default();
                        let fg = if mono {
                            Color::rgb(255, 255, 255)
                        } else {
                            Color::rgb(140, 230, 255)
                        };
                        let mut style = Style::new().fg(fg);
                        if let Some(bg) = existing.style.bg {
                            style = style.bg(bg);
                        }
                        bscan_surface.set_cell(x, y, Cell::new(bc.glyph.clone(), style));
                    }
                }
            }
        }

        // Annotate B-Scan with Depth Scale Marks on left edge
        let scale_step = match self.band {
            RadarBand::Hf9MHz | RadarBand::SplitBand => 5, // 0k, 5k, 10k, 15k, 20k, 25k, 30k
            RadarBand::Vhf60MHz => 1,
        };
        for d in (0..=(max_depth_km as i32)).step_by(scale_step) {
            let row = ((d as f32 / max_depth_km) * (height.saturating_sub(1) as f32)) as u16;
            let tag = format!("{:2}k-", d);
            let style = Style::new().fg(Color::rgb(120, 160, 200)).bold();
            bscan_surface.print_str(0, row, &tag, style, None);
        }

        // Header telemetry bar inside B-Scan pane
        let header_str = format!(
            " REASON/{} [Z: 0-{:.0}km] SWATH: {:.0}km | GAIN: +{:.0}dB | {}",
            match self.band {
                RadarBand::Hf9MHz => "HF 9MHz",
                RadarBand::Vhf60MHz => "VHF 60MHz",
                RadarBand::SplitBand => "SPLIT DUAL-BAND",
            },
            max_depth_km,
            swath_extent_km,
            self.gain_db,
            self.feature_name
        );
        bscan_surface.print_str(
            1,
            0,
            &header_str,
            Style::new()
                .fg(Color::rgb(240, 250, 255))
                .bg(Color::rgb(15, 30, 60))
                .bold(),
            None,
        );

        // Print cursor depth gate label on B-Scan surface
        let cursor_row =
            ((self.cursor_depth_km / max_depth_km) * (height.saturating_sub(1) as f32)) as u16;
        let gate_tag = format!("▶ GATE {:>4.1}k", self.cursor_depth_km);
        if bscan_width >= 16 && cursor_row < height {
            bscan_surface.print_str(
                bscan_width.saturating_sub(15),
                cursor_row,
                &gate_tag,
                Style::new().fg(Color::rgb(255, 225, 50)).bold(),
                None,
            );
        }

        // If no A-scan pane requested, return B-scan directly
        if ascan_width == 0 {
            return bscan_surface;
        }

        // 3. Assemble Full Surface with A-Scan Diagnostic Pane
        let mut final_surface = Surface::new(width, height);
        // Blit B-scan onto left
        for y in 0..height {
            for x in 0..bscan_width {
                if let Some(c) = bscan_surface.get(x, y) {
                    final_surface.set_cell(x, y, c.clone());
                }
            }
        }

        // Divider column
        let divider_x = bscan_width;
        let div_style = Style::new().fg(Color::rgb(60, 90, 140));
        for y in 0..height {
            final_surface.set_cell(divider_x, y, Cell::new(Glyph::new("│"), div_style));
        }

        // Render A-Scan Trace on right pane
        let a_x0 = divider_x + 1;
        let a_w = width.saturating_sub(a_x0);
        let bar_max_w = (a_w.saturating_sub(11)) as usize;

        // A-scan Header
        final_surface.print_str(
            a_x0,
            0,
            " RADAR A-SCAN ECHO (dB)",
            Style::new()
                .fg(Color::rgb(220, 240, 255))
                .bg(Color::rgb(20, 35, 70))
                .bold(),
            None,
        );

        // Compute synthetic A-scan along the center track (x = 0)
        let row_count = height.saturating_sub(7);
        for i in 0..row_count {
            let row_y = 2 + i;
            let z_km = (i as f32 / row_count as f32) * max_depth_km;
            let sample = self.evaluate_medium(0.0, z_km);

            // Synthetic dB power return:
            // 0 dB at surface, exponential loss through ice, spike at basal interface
            let db_val = match sample.medium_type {
                MediumType::Vacuum => -95.0,
                MediumType::Surface => 0.0,
                MediumType::BrittleIce => -28.0 - z_km * 1.5,
                MediumType::FractureZone => -18.0 - z_km * 1.2,
                MediumType::BrinePocket => -8.0,
                MediumType::DuctileIce => -42.0 - (z_km - 4.5) * 3.2,
                MediumType::ThermalDiapir => -34.0 - (z_km - 4.5) * 2.8,
                MediumType::BasalInterface => -14.0, // High reflectivity contrast
                MediumType::SalineOcean => -78.0,
                MediumType::HydrothermalPlume => -58.0,
            };

            // Normalize dB: range -90 dB to 0 dB
            let norm = ((db_val + 90.0) / 90.0).clamp(0.0, 1.0);
            let bar_len = ((norm * bar_max_w as f32).round() as usize).min(bar_max_w);

            let bar_chars: String = "█".repeat(bar_len);
            let depth_lbl = format!("{:4.1}k", z_km);

            let (bar_color, tag) = match sample.medium_type {
                MediumType::Vacuum => (Color::rgb(40, 50, 70), "VACUUM"),
                MediumType::Surface => (Color::rgb(255, 255, 255), "SURFACE"),
                MediumType::BrittleIce => (Color::rgb(80, 180, 240), "BRITTLE ICE"),
                MediumType::FractureZone => (Color::rgb(140, 220, 255), "FRACTURE"),
                MediumType::BrinePocket => (Color::rgb(255, 180, 80), "BRINE SILL"),
                MediumType::DuctileIce => (Color::rgb(60, 120, 180), "DUCTILE ICE"),
                MediumType::ThermalDiapir => (Color::rgb(220, 160, 90), "DIAPIR"),
                MediumType::BasalInterface => (Color::rgb(255, 240, 160), "OCEAN INTERFACE"),
                MediumType::SalineOcean => (Color::rgb(30, 80, 150), "OCEAN"),
                MediumType::HydrothermalPlume => (Color::rgb(200, 100, 70), "PLUME"),
            };

            let row_style = if mono {
                Style::new().fg(Color::rgb(255, 255, 255))
            } else {
                Style::new().fg(bar_color)
            };

            final_surface.print_str(
                a_x0,
                row_y,
                &depth_lbl,
                Style::new().fg(Color::rgb(140, 170, 210)),
                None,
            );
            final_surface.print_str(a_x0 + 6, row_y, &bar_chars, row_style, None);
            if bar_len < bar_max_w && a_w >= 30 {
                final_surface.print_str(
                    a_x0 + 7 + bar_len as u16,
                    row_y,
                    tag,
                    Style::new().fg(Color::rgb(100, 130, 160)),
                    None,
                );
            }
        }

        // Diagnostics & Live Gate Telemetry Card at bottom right
        let diag_y = height.saturating_sub(6);
        if diag_y > 2 + row_count && a_w >= 24 {
            let gate_sample = self.evaluate_medium(0.0, self.cursor_depth_km);
            let c_km_s = 299_792.47_f32;
            let eps = gate_sample.dielectric_constant;
            let v_phase = c_km_s / eps.sqrt();
            let two_way_tau_us = (2.0 * self.cursor_depth_km / v_phase) * 1_000_000.0;
            let alpha_db_km = match gate_sample.medium_type {
                MediumType::Vacuum | MediumType::Surface => 0.0,
                MediumType::BrittleIce => 1.8,
                MediumType::FractureZone => 3.4,
                MediumType::BrinePocket => 18.5,
                MediumType::DuctileIce => {
                    12.0 + (gate_sample.temperature_k - 180.0).max(0.0) * 0.15
                }
                MediumType::ThermalDiapir => 16.5,
                MediumType::BasalInterface => 28.0,
                MediumType::SalineOcean | MediumType::HydrothermalPlume => 92.0,
            };
            let ocean_conf = if (self.cursor_depth_km - self.nominal_ice_thickness_km).abs() < 1.0 {
                98.8
            } else if self.cursor_depth_km > self.nominal_ice_thickness_km {
                99.9
            } else if self.cursor_depth_km > self.nominal_ice_thickness_km - 3.0 {
                68.0
            } else {
                3.5
            };

            let title_gate = format!(
                " GATE DEPTH: {:>4.1}km | TAU: {:>5.1}µs",
                self.cursor_depth_km, two_way_tau_us
            );
            final_surface.print_str(
                a_x0,
                diag_y,
                &title_gate,
                Style::new().fg(Color::rgb(255, 215, 60)).bold(),
                None,
            );

            let eps_line = format!(" EPS: εr={:<4.2} | ATTEN: {:>4.1}dB/km", eps, alpha_db_km);
            final_surface.print_str(
                a_x0,
                diag_y + 1,
                &eps_line,
                Style::new().fg(Color::rgb(160, 210, 240)),
                None,
            );

            let temp_line = format!(
                " TEMP: {:>5.1}K | OCEAN CONF: {:>4.1}%",
                gate_sample.temperature_k, ocean_conf
            );
            final_surface.print_str(
                a_x0,
                diag_y + 2,
                &temp_line,
                Style::new().fg(Color::rgb(100, 240, 180)).bold(),
                None,
            );

            let hint_line = format!(" [↑/↓] Gate | [R] Band | GAIN: {:>+.0}dB", self.gain_db);
            final_surface.print_str(
                a_x0,
                diag_y + 3,
                &hint_line,
                Style::new().fg(Color::rgb(120, 160, 200)),
                None,
            );
        }

        final_surface
    }

    fn map_sample_to_color(&self, sample: &MediumSample, boost: f32) -> (u8, u8, u8) {
        let gain_factor = (self.gain_db / 40.0).clamp(0.25, 2.5);
        let boost = boost * gain_factor;
        match sample.medium_type {
            MediumType::Vacuum => (2, 4, 10),
            MediumType::Surface => {
                let v = ((230.0 * sample.reflectivity * boost).min(255.0)) as u8;
                (v, v, 255)
            }
            MediumType::BrittleIce => {
                let r = ((20.0 + sample.reflectivity * 60.0) * boost).min(255.0) as u8;
                let g = ((50.0 + sample.reflectivity * 120.0) * boost).min(255.0) as u8;
                let b = ((90.0 + sample.reflectivity * 140.0) * boost).min(255.0) as u8;
                (r, g, b)
            }
            MediumType::FractureZone => {
                let r = ((40.0 + sample.reflectivity * 140.0) * boost).min(255.0) as u8;
                let g = ((120.0 + sample.reflectivity * 120.0) * boost).min(255.0) as u8;
                let b = (255.0 * boost).min(255.0) as u8;
                (r, g, b)
            }
            MediumType::BrinePocket => {
                // Warm amber anomalous radar reflection
                let r = (240.0 * boost).min(255.0) as u8;
                let g = (160.0 * boost).min(255.0) as u8;
                let b = (50.0 * boost).min(255.0) as u8;
                (r, g, b)
            }
            MediumType::DuctileIce => {
                // Deeper navy with thermal gradient
                let r = ((25.0 + (sample.temperature_k - 150.0).max(0.0) * 0.4) * boost).min(255.0)
                    as u8;
                let g = ((35.0 + sample.reflectivity * 70.0) * boost).min(255.0) as u8;
                let b = ((70.0 + sample.reflectivity * 90.0) * boost).min(255.0) as u8;
                (r, g, b)
            }
            MediumType::ThermalDiapir => {
                // Warm glowing upwelling dome
                let r = (180.0 * boost).min(255.0) as u8;
                let g = (140.0 * boost).min(255.0) as u8;
                let b = (80.0 * boost).min(255.0) as u8;
                (r, g, b)
            }
            MediumType::BasalInterface => {
                // Blazing bright radar reflection at ice-ocean boundary
                let r = (210.0 * boost).min(255.0) as u8;
                let g = (245.0 * boost).min(255.0) as u8;
                let b = (255.0 * boost).min(255.0) as u8;
                (r, g, b)
            }
            MediumType::SalineOcean => {
                // Abyssal deep navy
                let r = ((8.0 + sample.reflectivity * 20.0) * boost).min(255.0) as u8;
                let g = ((16.0 + sample.reflectivity * 40.0) * boost).min(255.0) as u8;
                let b = ((38.0 + sample.reflectivity * 60.0) * boost).min(255.0) as u8;
                (r, g, b)
            }
            MediumType::HydrothermalPlume => {
                // Active buoyant hydrothermal vent plume
                let r = ((90.0 + sample.reflectivity * 120.0) * boost).min(255.0) as u8;
                let g = ((50.0 + sample.reflectivity * 80.0) * boost).min(255.0) as u8;
                let b = ((120.0 + sample.reflectivity * 100.0) * boost).min(255.0) as u8;
                (r, g, b)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum MediumType {
    Vacuum,
    Surface,
    BrittleIce,
    FractureZone,
    BrinePocket,
    DuctileIce,
    ThermalDiapir,
    BasalInterface,
    SalineOcean,
    HydrothermalPlume,
}

struct MediumSample {
    reflectivity: f32,
    temperature_k: f32,
    #[allow(dead_code)]
    dielectric_constant: f32,
    medium_type: MediumType,
}
