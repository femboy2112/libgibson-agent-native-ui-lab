//! 3D Jovian orbital system visualization renderer.
//!
//! Renders Jupiter with atmospheric belts and Great Red Spot, the four Galilean
//! moons in resonant orbits, spacecraft trajectory curves, maneuver vectors, and
//! Earth communication lines of sight using LibGibson's RgbRaster and BrailleCanvas.

use crate::sim::jovian::{JovianModel, TargetBody, Vector3, JUPITER_RADIUS_KM};
use gibson::cell::{Color, Glyph, Style};
use gibson::raster::RgbRaster;
use gibson::surface::Surface;
use gibson::BrailleCanvas;
use std::f32::consts::PI;

pub struct SystemViewRenderer {
    pub camera_azimuth: f32,        // radians
    pub camera_elevation: f32,      // radians (~0.35 is nice top-down perspective)
    pub camera_distance_scale: f32, // zoom factor
    pub show_radiation_belt: bool,
    pub show_orbit_labels: bool,
    pub show_magnetic_field: bool,
    pub show_shadow_cone: bool,
    pub auto_orbit: bool,
    pub selected_target_idx: usize,
}

impl Default for SystemViewRenderer {
    fn default() -> Self {
        Self {
            camera_azimuth: 0.42,
            camera_elevation: 0.38,
            camera_distance_scale: 1.0,
            show_radiation_belt: true,
            show_orbit_labels: true,
            show_magnetic_field: true,
            show_shadow_cone: true,
            auto_orbit: true,
            selected_target_idx: 2, // Europa default
        }
    }
}

impl SystemViewRenderer {
    pub fn update(&mut self, dt_seconds: f32) {
        if self.auto_orbit {
            self.camera_azimuth = (self.camera_azimuth + dt_seconds * 0.05) % (2.0 * PI);
        }
    }

    pub fn rotate_camera(&mut self, d_az: f32, d_el: f32) {
        self.camera_azimuth = (self.camera_azimuth + d_az) % (2.0 * PI);
        self.camera_elevation = (self.camera_elevation + d_el).clamp(0.05, 1.45);
    }

    pub fn adjust_zoom(&mut self, factor: f32) {
        self.camera_distance_scale = (self.camera_distance_scale * factor).clamp(0.2, 5.0);
    }

    pub fn toggle_auto_orbit(&mut self) {
        self.auto_orbit = !self.auto_orbit;
    }

    pub fn toggle_magnetic_field(&mut self) {
        self.show_magnetic_field = !self.show_magnetic_field;
    }

    pub fn selected_target_name(&self) -> &'static str {
        match self.selected_target_idx % 5 {
            0 => "JUPITER",
            1 => "IO",
            2 => "EUROPA",
            3 => "GANYMEDE",
            _ => "CALLISTO",
        }
    }

    pub fn next_target(&mut self) {
        self.selected_target_idx = (self.selected_target_idx + 1) % 5;
    }

    pub fn prev_target(&mut self) {
        self.selected_target_idx = if self.selected_target_idx == 0 {
            4
        } else {
            self.selected_target_idx - 1
        };
    }
    /// Projects a 3D Jovian coordinate (km) to 2D pixel coordinates on an RgbRaster.
    pub fn project_3d(&self, pos: Vector3, width: u16, height: u16) -> Option<(i32, i32, f32)> {
        // Center of raster
        let cx = width as f32 * 0.5;
        let cy = height as f32 * 0.5;

        // View scale: Callisto orbit (~1.88e6 km) maps to ~45% of half-width
        let max_orbit_km = 2_200_000.0 / self.camera_distance_scale;
        let scale = (cx.min(cy) * 0.92) / max_orbit_km;

        // Rotate by camera azimuth around Y (Jovian polar axis)
        let cos_az = self.camera_azimuth.cos();
        let sin_az = self.camera_azimuth.sin();
        let x1 = pos.x * cos_az - pos.z * sin_az;
        let z1 = pos.x * sin_az + pos.z * cos_az;
        let y1 = pos.y;

        // Rotate by camera elevation around X
        let cos_el = self.camera_elevation.cos();
        let sin_el = self.camera_elevation.sin();
        let y2 = y1 * cos_el - z1 * sin_el;
        let z2 = y1 * sin_el + z1 * cos_el;

        // Screen coordinates
        let screen_x = cx + x1 * scale;
        let screen_y = cy - y2 * scale; // Inverted Y for screen coordinates

        Some((screen_x.round() as i32, screen_y.round() as i32, z2))
    }

    /// Render the complete 3D Jovian system view onto a LibGibson Surface.
    pub fn render(&self, model: &JovianModel, width: u16, height: u16, mono: bool) -> Surface {
        if width == 0 || height == 0 {
            return Surface::new(width.max(1), height.max(1));
        }

        // Allocate high-resolution RGB raster (addressable half-block pixels: width x (height * 2))
        let pixel_w = width;
        let pixel_h = height * 2;
        let mut raster = RgbRaster::new(pixel_w, pixel_h);
        raster.clear((2, 3, 10)); // Deep space dark ink background

        // Also create a Braille canvas for ultra-crisp subcell orbital lines (2x4 dots per cell)
        let mut braille = BrailleCanvas::new(width, height);

        // 1. Draw Starfield (deterministic background stars)
        for i in 0..72 {
            let sx = ((i * 127 + 43) % pixel_w as usize) as i32;
            let sy = ((i * 251 + 89) % pixel_h as usize) as i32;
            let brightness = if i % 7 == 0 {
                210
            } else if i % 3 == 0 {
                140
            } else {
                75
            };
            raster.set(
                sx,
                sy,
                (
                    brightness,
                    brightness,
                    (brightness as f32 * 1.15).min(255.0) as u8,
                ),
            );
        }

        // 2. Draw Jovian Radiation Belt / Io Plasma Torus (faint amber/orange glow)
        if self.show_radiation_belt && !mono {
            let torus_r = model.io.semi_major_axis_km;
            for i in 0..64 {
                let th = (i as f32 / 64.0) * 2.0 * PI;
                let tx = torus_r * th.cos();
                let tz = torus_r * th.sin();
                let ty = (th * 2.0).sin() * 12000.0; // Tilted plasma sheet
                if let Some((px, py, _)) =
                    self.project_3d(Vector3::new(tx, ty, tz), pixel_w, pixel_h)
                {
                    raster.disc(px as f32, py as f32, 2.0, (65, 38, 12));
                }
            }
        }

        // 3. Draw Moon Orbital Tracks on Braille Canvas
        let moons = [&model.io, &model.europa, &model.ganymede, &model.callisto];
        for moon in moons {
            let mut track_points = Vec::with_capacity(96);
            for i in 0..=96 {
                let th = (i as f32 / 96.0) * 2.0 * PI;
                let inc = moon.inclination_deg * PI / 180.0;
                let x = moon.semi_major_axis_km * th.cos();
                let y = moon.semi_major_axis_km * th.sin() * inc.sin();
                let z = moon.semi_major_axis_km * th.sin() * inc.cos();
                if let Some((px, py, _)) = self.project_3d(
                    Vector3::new(x, y, z),
                    braille.pixel_width(),
                    braille.pixel_height(),
                ) {
                    track_points.push((px, py));
                }
            }
            braille.polyline(&track_points);
        }

        // 4. Draw Spacecraft Orbit (Tour Petal Ellipse) on Braille Canvas
        {
            let mut sc_points = Vec::with_capacity(128);
            let period = model.sc_period_hours;
            for i in 0..=128 {
                let t = (i as f32 / 128.0) * period;
                let (p, _, _) = model.spacecraft_position_at(t);
                if let Some((px, py, _)) =
                    self.project_3d(p, braille.pixel_width(), braille.pixel_height())
                {
                    sc_points.push((px, py));
                }
            }
            braille.polyline(&sc_points);
        }

        // 4b. Draw Jovian Dipole Magnetic Field Lines (tilted 9.6 deg)
        if self.show_magnetic_field {
            let tilt = 9.6_f32.to_radians();
            for &l_shell_rj in &[3.2_f32, 5.9, 9.4] {
                let l_km = l_shell_rj * JUPITER_RADIUS_KM;
                for &phi in &[0.0_f32, PI * 0.5, PI, PI * 1.5] {
                    let mut b_line = Vec::with_capacity(32);
                    for step in 0..=32 {
                        let theta = 0.22 * PI + (step as f32 / 32.0) * (0.56 * PI);
                        let r = l_km * theta.sin().powi(2);
                        let xm = r * theta.sin() * phi.cos();
                        let ym = r * theta.cos();
                        let zm = r * theta.sin() * phi.sin();
                        let x = xm * tilt.cos() + ym * tilt.sin();
                        let y = -xm * tilt.sin() + ym * tilt.cos();
                        let z = zm;
                        if let Some((px, py, _)) = self.project_3d(
                            Vector3::new(x, y, z),
                            braille.pixel_width(),
                            braille.pixel_height(),
                        ) {
                            b_line.push((px, py));
                        }
                    }
                    braille.polyline(&b_line);
                }
            }
        }

        // 4c. Draw Jupiter's Umbral Shadow Cone (-X direction)
        if self.show_shadow_cone {
            let shadow_len = 1_800_000.0;
            let top_edge = Vector3::new(-shadow_len, JUPITER_RADIUS_KM, 0.0);
            let bot_edge = Vector3::new(-shadow_len, -JUPITER_RADIUS_KM, 0.0);
            let top_limb = Vector3::new(0.0, JUPITER_RADIUS_KM, 0.0);
            let bot_limb = Vector3::new(0.0, -JUPITER_RADIUS_KM, 0.0);

            if let (Some((p1x, p1y, _)), Some((p2x, p2y, _))) = (
                self.project_3d(top_limb, braille.pixel_width(), braille.pixel_height()),
                self.project_3d(top_edge, braille.pixel_width(), braille.pixel_height()),
            ) {
                braille.line(p1x, p1y, p2x, p2y);
            }
            if let (Some((p1x, p1y, _)), Some((p2x, p2y, _))) = (
                self.project_3d(bot_limb, braille.pixel_width(), braille.pixel_height()),
                self.project_3d(bot_edge, braille.pixel_width(), braille.pixel_height()),
            ) {
                braille.line(p1x, p1y, p2x, p2y);
            }
        }

        // 5. Draw Jupiter at center
        let jupiter_center = self.project_3d(Vector3::ZERO, pixel_w, pixel_h).unwrap_or((
            pixel_w as i32 / 2,
            pixel_h as i32 / 2,
            0.0,
        ));
        let jupiter_screen_r = {
            let edge = self
                .project_3d(Vector3::new(JUPITER_RADIUS_KM, 0.0, 0.0), pixel_w, pixel_h)
                .unwrap_or((jupiter_center.0 + 8, jupiter_center.1, 0.0));
            (edge.0 - jupiter_center.0).abs().max(4) as f32
        };

        // Render Jupiter's spherical body with atmospheric bands & Great Red Spot
        let jx = jupiter_center.0;
        let jy = jupiter_center.1;
        let r_int = jupiter_screen_r.round() as i32;

        for dy in -r_int..=r_int {
            for dx in -r_int..=r_int {
                let dist_sq = dx * dx + dy * dy;
                let r_sq = (jupiter_screen_r * jupiter_screen_r) as i32;
                if dist_sq <= r_sq {
                    let px = jx + dx;
                    let py = jy + dy;

                    // Spherical normal Z for 3D sphere lighting
                    let norm_z = (1.0 - (dist_sq as f32 / (jupiter_screen_r * jupiter_screen_r)))
                        .max(0.0)
                        .sqrt();

                    // Atmospheric latitude on Jupiter [-1.0 to 1.0]
                    let lat = dy as f32 / jupiter_screen_r;

                    // Procedural Jovian band coloring:
                    // North/South polar hoods, Temperate zones, Equatorial belts, Equatorial zone
                    let base_color = if lat.abs() > 0.72 {
                        (145, 120, 95) // Polar hoods
                    } else if (0.42..=0.68).contains(&lat) {
                        (195, 160, 125) // North/South Temperate zones
                    } else if (0.16..=0.42).contains(&lat) {
                        (165, 85, 52) // North Equatorial Belt (deep brownish red)
                    } else if (-0.14..=0.16).contains(&lat) {
                        (225, 205, 175) // Equatorial Zone (bright cream)
                    } else if (-0.48..=-0.14).contains(&lat) {
                        // South Equatorial Belt & Great Red Spot!
                        if (-0.38..=-0.22).contains(&lat) && (dx > -r_int / 4 && dx < r_int / 3) {
                            (215, 65, 42) // The Great Red Spot!
                        } else {
                            (175, 92, 58) // SEB brown
                        }
                    } else {
                        (180, 150, 120)
                    };

                    // Limb darkening + solar illumination (Sun to the left-front)
                    let illumination =
                        (norm_z * 0.75 + (-(dx as f32) / jupiter_screen_r * 0.25) + 0.15)
                            .clamp(0.12, 1.0);
                    let final_r = (base_color.0 as f32 * illumination) as u8;
                    let final_g = (base_color.1 as f32 * illumination) as u8;
                    let final_b = (base_color.2 as f32 * illumination) as u8;

                    raster.set(px, py, (final_r, final_g, final_b));
                }
            }
        }

        // 6. Draw Moons (Io, Europa, Ganymede, Callisto)
        for moon in moons {
            let pos = moon.position_at(model.mission_time_hours);
            if let Some((px, py, _)) = self.project_3d(pos, pixel_w, pixel_h) {
                let color = moon.body.color_rgb();
                let dot_r = match moon.body {
                    TargetBody::Ganymede | TargetBody::Callisto => 2.0,
                    _ => 1.5,
                };
                raster.disc(px as f32, py as f32, dot_r, color);

                // Glow around moon
                raster.disc(
                    px as f32,
                    py as f32,
                    dot_r + 1.2,
                    (color.0 / 3, color.1 / 3, color.2 / 3),
                );
            }
        }

        // 7. Draw Spacecraft
        let (sc_pos, sc_vel, _) = model.spacecraft_position_at(model.mission_time_hours);
        if let Some((sc_px, sc_py, _)) = self.project_3d(sc_pos, pixel_w, pixel_h) {
            // Bright teal/cyan orbiter core
            raster.disc(sc_px as f32, sc_py as f32, 2.2, (60, 255, 220));
            raster.disc(sc_px as f32, sc_py as f32, 4.0, (15, 90, 80));

            // Velocity vector projected
            let v_lead = sc_pos + sc_vel.normalized() * 120000.0;
            if let Some((vx, vy, _)) = self.project_3d(v_lead, pixel_w, pixel_h) {
                raster.line(sc_px, sc_py, vx, vy, (40, 230, 180));
            }

            // Earth Communication Line-of-Sight (towards -X, +Z in heliocentric frame)
            let earth_vec = Vector3::new(-450000.0, 80000.0, -320000.0);
            let earth_target = sc_pos + earth_vec;
            if let Some((ex, ey, _)) = self.project_3d(earth_target, pixel_w, pixel_h) {
                // Dashed comms beam
                raster.line(sc_px, sc_py, ex, ey, (120, 160, 240));
            }
        }

        // 8. Convert raster to base surface
        let mut surface = if mono {
            raster.to_mono_surface()
        } else {
            raster.to_surface()
        };

        // 9. Overlay the ultra-fine Braille orbital curves onto the surface
        let orbit_style = if mono {
            Style::new().dim()
        } else {
            Style::new().fg(Color::Rgb(70, 120, 160))
        };
        for cy in 0..braille.height.min(surface.height) {
            for cx in 0..braille.width.min(surface.width) {
                if let Some(ch) = braille.glyph_at(cx, cy) {
                    if let Some(cell) = surface.get_mut(cx, cy) {
                        // Retain the existing cell background so orbital lines blend seamlessly!
                        let bg = cell.style.bg.unwrap_or(Color::Reset);
                        cell.glyph = Glyph::new(&ch.to_string());
                        cell.style = orbit_style.bg(bg);
                    }
                }
            }
        }

        // 10. Draw Body Labels and Reticles
        if self.show_orbit_labels && width >= 40 {
            let label_style = if mono {
                Style::new().bold()
            } else {
                Style::new().fg(Color::Rgb(210, 230, 255)).bold()
            };

            // Jupiter label
            let j_cx = (jupiter_center.0 as u16).saturating_sub(3);
            let j_cy = ((jupiter_center.1 / 2) as u16).saturating_add(2);
            if j_cx + 7 < width && j_cy < height {
                surface.print_str(j_cx, j_cy, "JUPITER", label_style, None);
            }

            // Moon labels
            for moon in moons {
                let pos = moon.position_at(model.mission_time_hours);
                if let Some((px, py, _)) = self.project_3d(pos, pixel_w, pixel_h) {
                    let cell_x = (px as u16).saturating_add(2);
                    let cell_y = (py / 2) as u16;
                    let name = moon.body.name();
                    if cell_x + name.len() as u16 + 1 < width && cell_y < height {
                        let m_style = if mono {
                            Style::new()
                        } else {
                            let (r, g, b) = moon.body.color_rgb();
                            Style::new().fg(Color::Rgb(r, g, b))
                        };
                        surface.print_str(cell_x, cell_y, name, m_style, None);
                    }
                }
            }

            // Target reticle around selected body
            let target_pos = match model.target {
                TargetBody::Jupiter => Vector3::ZERO,
                TargetBody::Io => model.io.position_at(model.mission_time_hours),
                TargetBody::Europa => model.europa.position_at(model.mission_time_hours),
                TargetBody::Ganymede => model.ganymede.position_at(model.mission_time_hours),
                TargetBody::Callisto => model.callisto.position_at(model.mission_time_hours),
                TargetBody::Spacecraft => sc_pos,
            };

            if let Some((tx, ty, _)) = self.project_3d(target_pos, pixel_w, pixel_h) {
                let cell_x = tx as u16;
                let cell_y = (ty / 2) as u16;
                let reticle_style = Style::new().fg(Color::Rgb(255, 215, 60)).bold();
                if cell_x >= 2 && cell_x + 3 < width && cell_y < height {
                    surface.print_str(cell_x.saturating_sub(2), cell_y, "[", reticle_style, None);
                    surface.print_str(cell_x.saturating_add(2), cell_y, "]", reticle_style, None);
                }
            }

            // Spacecraft badge
            if let Some((sc_px, sc_py, _)) = self.project_3d(sc_pos, pixel_w, pixel_h) {
                let cell_x = (sc_px as u16).saturating_add(2);
                let cell_y = (sc_py / 2) as u16;
                if cell_x + 14 < width && cell_y < height {
                    let sc_style = Style::new().fg(Color::Rgb(70, 245, 200)).bold();
                    surface.print_str(cell_x, cell_y, "GALILEO", sc_style, None);
                }
            }
        }

        // 11. Eclipse & Camera HUD
        if model.is_in_jupiter_shadow(sc_pos) && width >= 42 {
            let eclipse_style = Style::new()
                .fg(Color::Rgb(255, 80, 80))
                .bg(Color::Rgb(60, 15, 15))
                .bold();
            surface.print_str(
                2,
                1,
                " ⚠ ECLIPSE: IN JUPITER UMBRA (SOLAR/COMM LOSS) ",
                eclipse_style,
                None,
            );
        }

        if width >= 55 && height >= 12 {
            let az_deg = self.camera_azimuth.to_degrees() % 360.0;
            let el_deg = self.camera_elevation.to_degrees();
            let hud_str = format!(
                "CAM AZ: {:>3.0}° EL: {:>2.0}° ZOOM: {:.1}x | TRACK: {} | [H/J/K/L] Pan [Z/X] Zoom [M] Mag",
                az_deg,
                el_deg,
                self.camera_distance_scale,
                self.selected_target_name()
            );
            surface.print_str(
                2,
                height.saturating_sub(1),
                &hud_str,
                Style::new().fg(Color::Rgb(100, 140, 180)),
                None,
            );
        }

        surface
    }
}
