//! Trajectory planning lab visualization renderer.
//!
//! Renders dual orbital trajectories (pre-burn nominal vs post-burn planned),
//! maneuver node vectors, Europa closest approach geometry, radiation hazard
//! profiles, and scrubbed vehicle locations.

use crate::sim::jovian::JovianModel;
use crate::sim::trajectory::TrajectoryLab;
use gibson::cell::{Color, Glyph, Style};
use gibson::raster::RgbRaster;
use gibson::surface::Surface;
use gibson::BrailleCanvas;

pub struct TrajectoryViewRenderer {
    pub zoom: f32,
    pub center_on_europa: bool,
}

impl Default for TrajectoryViewRenderer {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            center_on_europa: true,
        }
    }
}

impl TrajectoryViewRenderer {
    pub fn update(&mut self, _dt_seconds: f32) {}

    pub fn adjust_zoom(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(0.25, 8.0);
    }

    pub fn render(
        &self,
        model: &JovianModel,
        lab: &TrajectoryLab,
        width: u16,
        height: u16,
        mono: bool,
    ) -> Surface {
        if width == 0 || height == 0 {
            return Surface::new(width.max(1), height.max(1));
        }

        let pixel_w = width;
        let pixel_h = height * 2;
        let mut raster = RgbRaster::new(pixel_w, pixel_h);
        raster.clear((3, 5, 12)); // Deep orbital navy

        let mut braille = BrailleCanvas::new(width, height);

        // Center on Europa encounter or Jovian system center
        let (origin_x, origin_z) = if self.center_on_europa {
            let ep = model
                .europa
                .position_at(lab.closest_approach_nominal.time_hours);
            (ep.x, ep.z)
        } else {
            (0.0, 0.0)
        };

        let view_range_km = if self.center_on_europa {
            250_000.0 / self.zoom
        } else {
            1_200_000.0 / self.zoom
        };

        let cx = pixel_w as f32 * 0.5;
        let cy = pixel_h as f32 * 0.5;
        let scale_factor = (cx.min(cy) * 0.88) / view_range_km;

        let to_screen = |x: f32, z: f32| -> (i32, i32) {
            let sx = cx + (x - origin_x) * scale_factor;
            let sy = cy - (z - origin_z) * scale_factor;
            (sx.round() as i32, sy.round() as i32)
        };

        // 1. Draw Grid lines in Braille
        let grid_step_km = if self.center_on_europa {
            50_000.0
        } else {
            250_000.0
        };
        for gx in -5..=5 {
            let x = origin_x + gx as f32 * grid_step_km;
            let (p1x, p1y) = to_screen(x, origin_z - view_range_km);
            let (_p2x, p2y) = to_screen(x, origin_z + view_range_km);
            // Dotted grid on braille
            let step = 12;
            let dy = p2y - p1y;
            for i in 0..=(dy.abs() / step).max(1) {
                let y = p1y + i * step;
                braille.set(p1x, y);
            }
        }

        // 2. Draw Europa Body & Sphere of Influence (SOI)
        let t_ca = lab.closest_approach_nominal.time_hours;
        let europa_pos = model.europa.position_at(t_ca);
        let (ex, ey) = to_screen(europa_pos.x, europa_pos.z);

        // Europa SOI radius is ~9,700 km
        let soi_r_pixels = (9700.0 * scale_factor).max(6.0);
        let body_r_pixels = (model.europa.radius_km * scale_factor).max(2.5);

        // SOI dashed halo
        raster.disc(ex as f32, ey as f32, soi_r_pixels, (18, 32, 54));
        // Europa physical disc
        raster.disc(ex as f32, ey as f32, body_r_pixels, (195, 230, 255));
        raster.disc(ex as f32, ey as f32, body_r_pixels + 1.5, (90, 160, 220));

        // 3. Draw Pre-Burn Nominal Trajectory (Green / Cyan)
        let mut nom_braille_pts = Vec::with_capacity(lab.nominal_path.len());
        for pt in &lab.nominal_path {
            let (sx, sy) = to_screen(pt.position.x, pt.position.z);
            nom_braille_pts.push((sx, sy));
        }
        braille.polyline(&nom_braille_pts);

        // 4. Draw Post-Burn Planned Trajectory (Magenta / Amber)
        let mut plan_braille_pts = Vec::with_capacity(lab.planned_path.len());
        for pt in &lab.planned_path {
            let (sx, sy) = to_screen(pt.position.x, pt.position.z);
            plan_braille_pts.push((sx, sy));
            if !mono {
                // Colored glow for planned trajectory on raster
                raster.disc(sx as f32, sy as f32, 1.2, (255, 140, 50));
            }
        }
        braille.polyline(&plan_braille_pts);

        // 5. Draw Active Maneuver Node
        if let Some(node) = lab.selected_node() {
            let (node_pos, _, _) = model.spacecraft_position_at(node.epoch_hours);
            let (nx, ny) = to_screen(node_pos.x, node_pos.z);

            // Node marker: diamond / star
            raster.disc(nx as f32, ny as f32, 3.5, (255, 220, 40));
            raster.disc(nx as f32, ny as f32, 1.8, (255, 255, 255));

            // Burn delta-V vector
            let dv_mag = node.delta_v_total();
            if dv_mag > 0.1 {
                let vec_scale = 1.8;
                let vx = nx + (node.dv_prograde * vec_scale) as i32;
                let vy = ny - (node.dv_radial * vec_scale) as i32;
                raster.line(nx, ny, vx, vy, (255, 60, 120));
            }
        }

        // 6. Draw Closest Approach Markers
        let (ca_x, ca_y) = to_screen(lab.closest_approach_planned.distance_km, 0.0);
        let _ = (ca_x, ca_y);

        // 7. Draw Current Spacecraft Scrub Position
        let (sc_pos, _, _) = model.spacecraft_position_at(lab.scrub_time_hours);
        let (sc_x, sc_y) = to_screen(sc_pos.x, sc_pos.z);
        raster.disc(sc_x as f32, sc_y as f32, 3.0, (70, 255, 210));
        raster.disc(sc_x as f32, sc_y as f32, 5.0, (20, 100, 85));

        // Realize base surface
        let mut surface = if mono {
            raster.to_mono_surface()
        } else {
            raster.to_surface()
        };

        // Overlay Braille curves with custom colors
        let nom_style = if mono {
            Style::new()
        } else {
            Style::new().fg(Color::Rgb(60, 220, 160))
        };
        for cy in 0..braille.height.min(surface.height) {
            for cx in 0..braille.width.min(surface.width) {
                if let Some(ch) = braille.glyph_at(cx, cy) {
                    if let Some(cell) = surface.get_mut(cx, cy) {
                        let bg = cell.style.bg.unwrap_or(Color::Reset);
                        cell.glyph = Glyph::new(&ch.to_string());
                        cell.style = nom_style.bg(bg);
                    }
                }
            }
        }

        // 8. Flight Dynamics HUD Overlay (Top-Left and Top-Right)
        if width >= 50 && height >= 14 {
            let hud_style = Style::new().fg(Color::Rgb(215, 235, 255)).bold();
            let val_style = Style::new().fg(Color::Rgb(255, 215, 60)).bold();
            let nom_hud = Style::new().fg(Color::Rgb(60, 230, 160)).bold();
            let plan_hud = Style::new().fg(Color::Rgb(255, 130, 45)).bold();

            // Flight dynamics header
            surface.print_str(2, 1, "TRAJECTORY LAB // FLYBY DYNAMICS", hud_style, None);

            // Nominal vs Planned Closest Approach comparison
            let nom_alt = format!(
                "NOMINAL CA:  {:>7.1} km  [Vrel {:>4.1} km/s]",
                lab.closest_approach_nominal.altitude_km,
                lab.closest_approach_nominal.relative_velocity_kms
            );
            surface.print_str(2, 2, &nom_alt, nom_hud, None);

            let plan_alt = format!(
                "PLANNED CA:  {:>7.1} km  [Vrel {:>4.1} km/s]",
                lab.closest_approach_planned.altitude_km,
                lab.closest_approach_planned.relative_velocity_kms
            );
            surface.print_str(2, 3, &plan_alt, plan_hud, None);

            // Active maneuver node readout
            if let Some(node) = lab.selected_node() {
                let node_line = format!("NODE #{}: dV={:.1} m/s [PRO:{:+.1} RAD:{:+.1} NORM:{:+.1}] BURN:{:.1}s FUEL:{:.1}kg",
                    node.id,
                    node.delta_v_total(),
                    node.dv_prograde,
                    node.dv_radial,
                    node.dv_normal,
                    node.burn_duration_seconds(),
                    node.propellant_consumed_kg(),
                );
                if node_line.len() as u16 + 2 < width {
                    surface.print_str(2, 4, &node_line, val_style, None);
                }
            }

            // Scrubbed time readout
            let scrub_str = format!(
                "SCRUB MET: T+{:>5.1}h  |  ENC: E-{}h",
                lab.scrub_time_hours,
                ((t_ca - lab.scrub_time_hours).max(0.0) as u32)
            );
            if scrub_str.len() as u16 + 2 < width && height >= 6 {
                surface.print_str(
                    2,
                    5,
                    &scrub_str,
                    Style::new().fg(Color::Rgb(140, 180, 220)),
                    None,
                );
            }

            // B-plane target coordinates
            let b_mag = (lab.closest_approach_planned.b_dot_t_km.powi(2)
                + lab.closest_approach_planned.b_dot_r_km.powi(2))
            .sqrt();
            let b_plane_str = format!(
                "B-PLANE: B.T = {:>+7.1} km  B.R = {:>+7.1} km  [|B| = {:>7.1} km]",
                lab.closest_approach_planned.b_dot_t_km,
                lab.closest_approach_planned.b_dot_r_km,
                b_mag
            );
            if b_plane_str.len() as u16 + 2 < width && height >= 7 {
                surface.print_str(
                    2,
                    6,
                    &b_plane_str,
                    Style::new().fg(Color::Rgb(100, 240, 220)).bold(),
                    None,
                );
            }
        }

        // 9. Interactive Mission Timeline Scrubber Bar
        if width >= 40 && height >= 10 {
            let timeline_y = height.saturating_sub(2);
            let bar_w = (width.saturating_sub(6)) as usize;
            let start_t = model.mission_time_hours;
            let end_t = start_t + model.sc_period_hours * 1.25;
            let span = (end_t - start_t).max(1.0);
            let scrub_frac = ((lab.scrub_time_hours - start_t) / span).clamp(0.0, 1.0);
            let scrub_idx = (scrub_frac * bar_w.saturating_sub(1) as f32).round() as usize;

            let mut bar_chars: Vec<char> = vec!['─'; bar_w];
            // Mark Europa encounter
            let enc_frac = ((t_ca - start_t) / span).clamp(0.0, 1.0);
            let enc_idx = (enc_frac * bar_w.saturating_sub(1) as f32).round() as usize;
            if enc_idx < bar_w {
                bar_chars[enc_idx] = 'E';
            }
            if scrub_idx < bar_w {
                bar_chars[scrub_idx] = '▲';
            }

            let bar_str: String = bar_chars.into_iter().collect();
            surface.print_str(
                2,
                timeline_y,
                &format!("[{}]", bar_str),
                Style::new().fg(Color::Rgb(120, 180, 240)),
                None,
            );
            surface.print_str(
                2,
                timeline_y + 1,
                "TIMELINE: [[] / []] Scrub MET | [+/-/B] Burn ΔV | [Tab] Node",
                Style::new().dim(),
                None,
            );
        }

        surface
    }
}
