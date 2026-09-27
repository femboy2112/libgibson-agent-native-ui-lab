//! Europa mesoscale surface survey renderer.
//!
//! Renders Europa as a visually extraordinary scientific object:
//! 3D spherical raster shading, latitude/longitude graticule mesh in Braille,
//! procedural cycloid fissures (lineae), chaos terrain blocks, radar ground track,
//! science instrument footprints, and selected-region targeting crosshairs.

use gibson::cell::{Color, Glyph, Style};
use gibson::raster::RgbRaster;
use gibson::surface::Surface;
use gibson::BrailleCanvas;
use std::f32::consts::PI;

pub struct SurfaceFeature {
    pub name: &'static str,
    pub lat_deg: f32,
    pub lon_deg: f32,
    pub feature_type: &'static str,
    pub ice_thickness_km: f32,
    pub plume_probability: f32,
    pub surface_temp_k: f32,
    pub tidal_stress_kpa: f32,
}

pub const FEATURES: &[SurfaceFeature] = &[
    SurfaceFeature {
        name: "CONAMARA CHAOS",
        lat_deg: -9.8,
        lon_deg: 272.9,
        feature_type: "Disrupted Ice Rafts / Brine Matrix",
        ice_thickness_km: 14.2,
        plume_probability: 0.74,
        surface_temp_k: 112.5,
        tidal_stress_kpa: 142.0,
    },
    SurfaceFeature {
        name: "AGENOR LINEA",
        lat_deg: -43.2,
        lon_deg: 215.4,
        feature_type: "Active Strike-Slip Cycloid Ridge",
        ice_thickness_km: 18.6,
        plume_probability: 0.88,
        surface_temp_k: 104.2,
        tidal_stress_kpa: 210.5,
    },
    SurfaceFeature {
        name: "THERA MACULA",
        lat_deg: -46.7,
        lon_deg: 181.2,
        feature_type: "Sunken Chaos / Subsurface Brine Lake",
        ice_thickness_km: 10.5,
        plume_probability: 0.92,
        surface_temp_k: 128.0,
        tidal_stress_kpa: 265.0,
    },
    SurfaceFeature {
        name: "PWYLL CRATER",
        lat_deg: -25.2,
        lon_deg: 271.4,
        feature_type: "Impact Crater & Bright Ray System",
        ice_thickness_km: 16.5,
        plume_probability: 0.35,
        surface_temp_k: 98.4,
        tidal_stress_kpa: 95.0,
    },
    SurfaceFeature {
        name: "TYRE IMPACT BASIN",
        lat_deg: 34.0,
        lon_deg: 146.5,
        feature_type: "Concentric Multi-Ring Impact Graben",
        ice_thickness_km: 19.8,
        plume_probability: 0.28,
        surface_temp_k: 92.0,
        tidal_stress_kpa: 82.0,
    },
    SurfaceFeature {
        name: "RHADAMANTHYS LINEA",
        lat_deg: 18.4,
        lon_deg: 198.2,
        feature_type: "Cryovolcanic Extensional Band",
        ice_thickness_km: 21.4,
        plume_probability: 0.42,
        surface_temp_k: 96.5,
        tidal_stress_kpa: 118.0,
    },
    SurfaceFeature {
        name: "CILIX CRATER",
        lat_deg: 2.6,
        lon_deg: 181.9,
        feature_type: "Sharp-Rimmed Equatorial Crater",
        ice_thickness_km: 17.2,
        plume_probability: 0.22,
        surface_temp_k: 108.0,
        tidal_stress_kpa: 88.0,
    },
    SurfaceFeature {
        name: "MURIAS CHAOS",
        lat_deg: -22.5,
        lon_deg: 84.0,
        feature_type: "Thermal Diapir Upwelling Dome",
        ice_thickness_km: 11.8,
        plume_probability: 0.65,
        surface_temp_k: 121.5,
        tidal_stress_kpa: 178.0,
    },
];

pub struct SurfaceViewRenderer {
    pub zoom: f32,
    pub rotation_lon_deg: f32,
    pub tilt_lat_deg: f32,
    pub selected_feature_idx: usize,
    pub scan_phase: f32,
    pub show_graticule: bool,
    pub show_footprints: bool,
    pub show_thermal: bool,
    pub auto_rotate: bool,
}

impl Default for SurfaceViewRenderer {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            rotation_lon_deg: 215.0,
            tilt_lat_deg: -24.0,
            selected_feature_idx: 1, // Agenor Linea default
            scan_phase: 0.0,
            show_graticule: true,
            show_footprints: true,
            show_thermal: false,
            auto_rotate: true,
        }
    }
}

impl SurfaceViewRenderer {
    pub fn update(&mut self, dt_seconds: f32) {
        if self.auto_rotate {
            self.rotation_lon_deg = (self.rotation_lon_deg + dt_seconds * 1.5) % 360.0;
        }
        self.scan_phase = (self.scan_phase + dt_seconds * 0.4) % 1.0;
    }

    pub fn adjust_zoom(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(0.4, 4.0);
    }

    pub fn rotate_globe(&mut self, d_lon: f32, d_lat: f32) {
        self.rotation_lon_deg = (self.rotation_lon_deg + d_lon + 360.0) % 360.0;
        self.tilt_lat_deg = (self.tilt_lat_deg + d_lat).clamp(-80.0, 80.0);
    }

    pub fn toggle_auto_rotate(&mut self) {
        self.auto_rotate = !self.auto_rotate;
    }

    pub fn toggle_thermal(&mut self) {
        self.show_thermal = !self.show_thermal;
    }

    pub fn selected_feature(&self) -> &SurfaceFeature {
        &FEATURES[self.selected_feature_idx % FEATURES.len()]
    }

    pub fn next_feature(&mut self) {
        self.selected_feature_idx = (self.selected_feature_idx + 1) % FEATURES.len();
        let feat = &FEATURES[self.selected_feature_idx];
        self.rotation_lon_deg = feat.lon_deg;
        self.tilt_lat_deg = feat.lat_deg;
    }

    pub fn prev_feature(&mut self) {
        if self.selected_feature_idx == 0 {
            self.selected_feature_idx = FEATURES.len() - 1;
        } else {
            self.selected_feature_idx -= 1;
        }
        let feat = &FEATURES[self.selected_feature_idx];
        self.rotation_lon_deg = feat.lon_deg;
        self.tilt_lat_deg = feat.lat_deg;
    }

    /// Project a spherical coordinate (lat, lon) to 2D screen coordinates on the globe.
    /// Returns (screen_x, screen_y, is_front_facing).
    pub fn project_lat_lon(
        &self,
        lat_deg: f32,
        lon_deg: f32,
        cx: f32,
        cy: f32,
        radius: f32,
    ) -> (f32, f32, bool) {
        let lat = lat_deg * PI / 180.0;
        let lon = (lon_deg - self.rotation_lon_deg) * PI / 180.0;
        let tilt = self.tilt_lat_deg * PI / 180.0;

        // 3D unit sphere coordinates
        let x0 = lat.cos() * lon.sin();
        let y0 = lat.sin();
        let z0 = lat.cos() * lon.cos();

        // Rotate by viewing tilt around X
        let y1 = y0 * tilt.cos() - z0 * tilt.sin();
        let z1 = y0 * tilt.sin() + z0 * tilt.cos();
        let x1 = x0;

        let is_front = z1 > 0.0;
        let sx = cx + x1 * radius;
        let sy = cy - y1 * radius; // Invert Y
        (sx, sy, is_front)
    }

    pub fn render(&self, width: u16, height: u16, mono: bool) -> Surface {
        if width == 0 || height == 0 {
            return Surface::new(width.max(1), height.max(1));
        }

        let pixel_w = width;
        let pixel_h = height * 2;
        let mut raster = RgbRaster::new(pixel_w, pixel_h);
        raster.clear((2, 4, 10)); // Deep space backdrop

        let mut braille = BrailleCanvas::new(width, height);

        let cx = pixel_w as f32 * 0.44;
        let cy = pixel_h as f32 * 0.5;
        let sphere_r =
            ((pixel_w as f32 * 0.38).min(pixel_h as f32 * 0.44).max(12.0) * self.zoom).max(6.0);

        // 1. Render Shaded 3D Sphere of Europa
        let r_int = sphere_r.ceil() as i32;
        let r_sq = sphere_r * sphere_r;

        // Sub-solar lighting direction (front-left illuminated, slight Jupiter-shine from right)
        let sun_dir = [-0.55f32, 0.45, 0.70];
        let sun_len =
            (sun_dir[0] * sun_dir[0] + sun_dir[1] * sun_dir[1] + sun_dir[2] * sun_dir[2]).sqrt();
        let sun_dir = [
            sun_dir[0] / sun_len,
            sun_dir[1] / sun_len,
            sun_dir[2] / sun_len,
        ];

        for dy in -r_int..=r_int {
            for dx in -r_int..=r_int {
                let dist_sq = (dx * dx + dy * dy) as f32;
                if dist_sq <= r_sq {
                    let px = (cx + dx as f32).round() as i32;
                    let py = (cy + dy as f32).round() as i32;
                    if px < 0 || px >= pixel_w as i32 || py < 0 || py >= pixel_h as i32 {
                        continue;
                    }

                    let nx = dx as f32 / sphere_r;
                    let ny = -dy as f32 / sphere_r; // Inverted Y for sphere
                    let nz = (1.0 - (nx * nx + ny * ny)).max(0.0).sqrt();

                    // Calculate reverse latitude and longitude for procedural texture mapping
                    let tilt = self.tilt_lat_deg * PI / 180.0;
                    let y_unrot = ny * tilt.cos() + nz * tilt.sin();
                    let z_unrot = -ny * tilt.sin() + nz * tilt.cos();
                    let lat = y_unrot.clamp(-1.0, 1.0).asin();
                    let lon = nx.atan2(z_unrot) + self.rotation_lon_deg * PI / 180.0;

                    // Base bright ice reflectance (albedo ~ 0.67)
                    let base_ice: (u8, u8, u8) = (215, 238, 255);

                    // Procedural surface features:
                    // A. Cycloid fracture lineae: high-frequency arcs
                    let linea_pattern = (lat * 8.0 + (lon * 4.0).sin() * 2.5).sin().abs();
                    let cross_linea = ((lat * 4.0).cos() * 3.0 + lon * 12.0).sin().abs();
                    let is_fissure = linea_pattern < 0.12 || cross_linea < 0.08;

                    // B. Chaos terrain mottled patches
                    let chaos_hash = ((lat * 24.0).sin() * 43_758.55).fract().abs();
                    let is_chaos = (lat.abs() < 0.5) && (chaos_hash > 0.78);

                    let (surf_r, surf_g, surf_b) = if self.show_thermal {
                        if is_chaos || is_fissure {
                            (255, 140, 40) // Hot thermal emission from active fractures
                        } else {
                            let t_val = ((lat.cos() * 25.0 + 90.0 - 80.0) / 45.0).clamp(0.0, 1.0);
                            (
                                (30.0 + t_val * 160.0) as u8,
                                (20.0 + t_val * 60.0) as u8,
                                (140.0 - t_val * 90.0) as u8,
                            )
                        }
                    } else if is_fissure {
                        (165, 82, 48) // Hydrated sulfate / tholin rust
                    } else if is_chaos {
                        (145, 115, 95) // Disrupted chaotic terrain
                    } else {
                        // Subtle mottled ice variations
                        let mottle = ((lon * 18.0).sin() * (lat * 18.0).cos() * 12.0) as i32;
                        (
                            (base_ice.0 as i32 + mottle).clamp(160, 255) as u8,
                            (base_ice.1 as i32 + mottle).clamp(180, 255) as u8,
                            (base_ice.2 as i32 + mottle).clamp(200, 255) as u8,
                        )
                    };

                    // Diffuse lighting + limb darkening
                    let n_dot_l = (nx * sun_dir[0] + ny * sun_dir[1] + nz * sun_dir[2]).max(0.0);
                    // Ambient Jovian-shine (faint warm back-fill)
                    let jupiter_fill = ((-nx) * 0.35 + 0.1).max(0.0) * 0.22;
                    let intensity = (n_dot_l * 0.78 + jupiter_fill + 0.08).clamp(0.04, 1.0);

                    let lit_r = (surf_r as f32 * intensity) as u8;
                    let lit_g = (surf_g as f32 * intensity) as u8;
                    let lit_b = (surf_b as f32 * intensity) as u8;

                    raster.set(px, py, (lit_r, lit_g, lit_b));
                }
            }
        }

        // 2. Draw Latitude/Longitude Graticule on Braille Canvas
        if self.show_graticule {
            // Parallels (Latitudes)
            for lat in [-60, -30, 0, 30, 60] {
                let mut lat_pts = Vec::with_capacity(72);
                for lon_step in 0..=72 {
                    let lon = (lon_step as f32 / 72.0) * 360.0;
                    let (sx, sy, front) =
                        self.project_lat_lon(lat as f32, lon, cx, cy * 0.5, sphere_r * 0.5);
                    if front {
                        lat_pts.push((sx.round() as i32, sy.round() as i32));
                    } else if !lat_pts.is_empty() {
                        braille.polyline(&lat_pts);
                        lat_pts.clear();
                    }
                }
                braille.polyline(&lat_pts);
            }

            // Meridians (Longitudes)
            for lon in (0..360).step_by(30) {
                let mut lon_pts = Vec::with_capacity(36);
                for lat_step in -18..=18 {
                    let lat = lat_step as f32 * 5.0;
                    let (sx, sy, front) =
                        self.project_lat_lon(lat, lon as f32, cx, cy * 0.5, sphere_r * 0.5);
                    if front {
                        lon_pts.push((sx.round() as i32, sy.round() as i32));
                    }
                }
                braille.polyline(&lon_pts);
            }
        }

        // 3. Draw Synthetic Aperture Radar (SAR) Ground Track Swath
        let swath_center_lon = (self.rotation_lon_deg + (self.scan_phase * 60.0) - 30.0) % 360.0;
        let mut swath_pts = Vec::with_capacity(32);
        for lat_i in -16..=16 {
            let lat = lat_i as f32 * 5.0;
            let lon = swath_center_lon + (lat * 0.25);
            let (sx, sy, front) = self.project_lat_lon(lat, lon, cx, cy, sphere_r);
            if front {
                swath_pts.push((sx as i32, sy as i32));
                // Glowing scan strip on raster
                raster.disc(sx, sy, 3.2, (40, 220, 255));
            }
        }

        // 4. Draw Science Footprints (REASON Radar nadir & MISE imaging slit)
        if self.show_footprints {
            let (fx, fy, front) = self.project_lat_lon(-15.0, swath_center_lon, cx, cy, sphere_r);
            if front {
                // Concentric radar beam pulses
                raster.disc(fx, fy, 8.0, (15, 80, 110));
                raster.disc(fx, fy, 4.0, (50, 240, 255));
                raster.disc(fx, fy, 1.5, (255, 255, 255));
            }
        }

        // 5. Draw Feature Target Reticles and Markers
        for (i, feature) in FEATURES.iter().enumerate() {
            let (fx, fy, front) =
                self.project_lat_lon(feature.lat_deg, feature.lon_deg, cx, cy, sphere_r);
            if front {
                let is_selected = i == self.selected_feature_idx;
                let color = if is_selected {
                    (255, 225, 45) // Gold
                } else {
                    (255, 120, 70) // Amber
                };

                raster.disc(fx, fy, if is_selected { 3.5 } else { 2.0 }, color);
                if is_selected {
                    // Pulsing reticle box
                    let b = 7;
                    raster.line(
                        fx as i32 - b,
                        fy as i32 - b,
                        fx as i32 + b,
                        fy as i32 - b,
                        color,
                    );
                    raster.line(
                        fx as i32 - b,
                        fy as i32 + b,
                        fx as i32 + b,
                        fy as i32 + b,
                        color,
                    );
                    raster.line(
                        fx as i32 - b,
                        fy as i32 - b,
                        fx as i32 - b,
                        fy as i32 + b,
                        color,
                    );
                    raster.line(
                        fx as i32 + b,
                        fy as i32 - b,
                        fx as i32 + b,
                        fy as i32 + b,
                        color,
                    );
                }
            }
        }

        // 6. Convert raster to surface
        let mut surface = if mono {
            raster.to_mono_surface()
        } else {
            raster.to_surface()
        };

        // 7. Overlay Braille graticule
        let grat_style = if mono {
            Style::new().dim()
        } else {
            Style::new().fg(Color::Rgb(75, 140, 190))
        };
        for cy in 0..braille.height.min(surface.height) {
            for cx in 0..braille.width.min(surface.width) {
                if let Some(ch) = braille.glyph_at(cx, cy) {
                    if let Some(cell) = surface.get_mut(cx, cy) {
                        let bg = cell.style.bg.unwrap_or(Color::Reset);
                        cell.glyph = Glyph::new(&ch.to_string());
                        cell.style = grat_style.bg(bg);
                    }
                }
            }
        }

        // 8. Scientific Target Inspector Sidebar (Right side, if space permits)
        let sidebar_x = (pixel_w as f32 * 0.65) as u16;
        if width > sidebar_x + 28 && height >= 16 {
            let target = self.selected_feature();
            let title_st = Style::new().fg(Color::Rgb(255, 215, 60)).bold();
            let label_st = Style::new().fg(Color::Rgb(140, 180, 220));
            let val_st = Style::new().fg(Color::Rgb(220, 240, 255)).bold();
            let alert_st = Style::new().fg(Color::Rgb(60, 240, 160)).bold();

            surface.print_str(sidebar_x, 1, "EUROPA REGIONAL SURVEY", title_st, None);
            surface.print_str(
                sidebar_x,
                2,
                "─────────────────────────",
                Style::new().dim(),
                None,
            );

            surface.print_str(sidebar_x, 3, "TARGET FEATURE:", label_st, None);
            surface.print_str(sidebar_x, 4, target.name, title_st, None);

            let coord_str = format!(
                "COORD: {:>4.1}° {}, {:>5.1}° {}",
                target.lat_deg.abs(),
                if target.lat_deg >= 0.0 { 'N' } else { 'S' },
                target.lon_deg,
                'W'
            );
            surface.print_str(sidebar_x, 5, &coord_str, val_st, None);

            surface.print_str(sidebar_x, 7, "MORPHOLOGY:", label_st, None);
            surface.print_str(sidebar_x, 8, target.feature_type, val_st, None);

            let ice_str = format!("ICE SHELL DEPTH: {:.1} km", target.ice_thickness_km);
            surface.print_str(sidebar_x, 10, &ice_str, val_st, None);

            let temp_str = format!("SURFACE TEMP:    {:.1} K", target.surface_temp_k);
            surface.print_str(sidebar_x, 11, &temp_str, val_st, None);

            let stress_str = format!("TIDAL STRESS:    {:.1} kPa", target.tidal_stress_kpa);
            surface.print_str(sidebar_x, 12, &stress_str, val_st, None);

            let plume_str = format!(
                "PLUME ACTIVITY:  {:.0}% PROB",
                target.plume_probability * 100.0
            );
            surface.print_str(sidebar_x, 13, &plume_str, alert_st, None);

            if height >= 20 {
                surface.print_str(sidebar_x, 15, "PAYLOAD STATUS:", label_st, None);
                surface.print_str(
                    sidebar_x,
                    16,
                    "REASON RADAR: SOUNDING [60 MHz]",
                    Style::new().fg(Color::Rgb(60, 230, 255)),
                    None,
                );
                surface.print_str(
                    sidebar_x,
                    17,
                    "MISE SPECTRO: INTEGRATING [3.2 um]",
                    Style::new().fg(Color::Rgb(255, 175, 45)),
                    None,
                );
            }

            if height >= 22 {
                surface.print_str(
                    sidebar_x,
                    19,
                    "[H/J/K/L] Spin Globe | [N/P] Target",
                    Style::new().dim(),
                    None,
                );
                let mode_str = format!(
                    "SPIN: {} | THERMAL: {}",
                    if self.auto_rotate { "AUTO" } else { "MANUAL" },
                    if self.show_thermal { "ON" } else { "OFF" }
                );
                surface.print_str(
                    sidebar_x,
                    20,
                    &mode_str,
                    Style::new().fg(Color::Rgb(120, 160, 200)),
                    None,
                );
            }
        }

        surface
    }
}
