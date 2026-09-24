//! FOLDROOM: a finite rigid-panel folding workbench using only the frozen public API.
use crate::ui::{self, App};
use gibson::input::{KeyCode, KeyEvent};
use gibson::raster3d::{Camera, Material, Rasterizer};
use gibson::{Color, ColorDepth, Rect, Style, Surface, Vec3};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

pub const PANEL_COUNT: usize = 10;
pub const TRIANGLE_COUNT: usize = PANEL_COUNT * 2;
pub const MAX_RASTER: (u16, u16) = (160, 80);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sequence {
    Around,
    Paired,
}
impl Sequence {
    pub fn name(self) -> &'static str {
        match self {
            Self::Around => "AROUND",
            Self::Paired => "PAIRED",
        }
    }
    pub fn stages(self) -> u16 {
        match self {
            Self::Around => 6,
            Self::Paired => 4,
        }
    }
    fn stage(self, panel: usize) -> u16 {
        match self {
            Self::Around => match panel {
                1 => 0,
                4 => 1,
                3 => 2,
                2 => 3,
                5 => 5,
                _ => 4,
            },
            Self::Paired => match panel {
                1 | 3 => 1,
                2 | 4 => 2,
                5 => 3,
                _ => 0,
            },
        }
    }
    fn stage_name(self, stage: u16) -> &'static str {
        match (self, stage) {
            (Self::Around, 0) => "front wall",
            (Self::Around, 1) => "left wall",
            (Self::Around, 2) => "back wall",
            (Self::Around, 3) => "right wall",
            (Self::Around, 4) | (Self::Paired, 0) => "four tabs",
            (Self::Paired, 1) => "front + back",
            (Self::Paired, 2) => "left + right",
            _ => "lid",
        }
    }
}

/// State has bounded integer controls; no wall clock, rendering cache, or viewport state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Foldroom {
    sequence: Sequence,
    progress: u16,
    selected: usize,
    yaw: i16,
    elevation: i16,
    separated: bool,
}
impl Default for Foldroom {
    fn default() -> Self {
        Self {
            sequence: Sequence::Around,
            progress: 0,
            selected: 0,
            yaw: 32,
            elevation: 48,
            separated: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub id: usize,
    pub name: &'static str,
    pub parent: Option<usize>,
    pub vertices: [Vec3; 4],
    /// World-space attachment segment, absent on the fixed base.
    pub hinge: Option<[Vec3; 2]>,
    pub angle_degrees: u16,
}
impl Panel {
    pub fn center(&self) -> Vec3 {
        self.vertices
            .iter()
            .fold(Vec3::default(), |sum, &v| sum.plus(v))
            .scale(0.25)
    }
}

#[derive(Clone, Copy)]
struct Spec {
    name: &'static str,
    bounds: [f32; 4],
    parent: Option<usize>,
    hinge: [Vec3; 2],
}
fn specs() -> [Spec; PANEL_COUNT] {
    let x_hinge = |z| [Vec3::new(-1., 0., z), Vec3::new(1., 0., z)];
    let z_hinge = |x, a, b| [Vec3::new(x, 0., a), Vec3::new(x, 0., b)];
    [
        Spec {
            name: "BASE",
            bounds: [-1., 1., -0.75, 0.75],
            parent: None,
            hinge: x_hinge(0.),
        },
        Spec {
            name: "FRONT",
            bounds: [-1., 1., -1.95, -0.75],
            parent: Some(0),
            hinge: x_hinge(-0.75),
        },
        Spec {
            name: "RIGHT",
            bounds: [1., 2.2, -0.75, 0.75],
            parent: Some(0),
            hinge: z_hinge(1., -0.75, 0.75),
        },
        Spec {
            name: "BACK",
            bounds: [-1., 1., 0.75, 1.95],
            parent: Some(0),
            hinge: x_hinge(0.75),
        },
        Spec {
            name: "LEFT",
            bounds: [-2.2, -1., -0.75, 0.75],
            parent: Some(0),
            hinge: z_hinge(-1., -0.75, 0.75),
        },
        Spec {
            name: "LID",
            bounds: [-1., 1., 1.95, 3.45],
            parent: Some(3),
            hinge: x_hinge(1.95),
        },
        Spec {
            name: "FRONT-L TAB",
            bounds: [-1.3, -1., -1.95, -0.75],
            parent: Some(1),
            hinge: z_hinge(-1., -1.95, -0.75),
        },
        Spec {
            name: "FRONT-R TAB",
            bounds: [1., 1.3, -1.95, -0.75],
            parent: Some(1),
            hinge: z_hinge(1., -1.95, -0.75),
        },
        Spec {
            name: "BACK-L TAB",
            bounds: [-1.3, -1., 0.75, 1.95],
            parent: Some(3),
            hinge: z_hinge(-1., 0.75, 1.95),
        },
        Spec {
            name: "BACK-R TAB",
            bounds: [1., 1.3, 0.75, 1.95],
            parent: Some(3),
            hinge: z_hinge(1., 0.75, 1.95),
        },
    ]
}
fn turn(point: Vec3, hinge: [Vec3; 2], angle: f32) -> Vec3 {
    let axis = hinge[1].minus(hinge[0]).normalize();
    let v = point.minus(hinge[0]);
    let (s, c) = angle.sin_cos();
    hinge[0]
        .plus(v.scale(c))
        .plus(axis.cross(v).scale(s))
        .plus(axis.scale(axis.dot(v) * (1. - c)))
}
fn world_point(
    point: Vec3,
    id: usize,
    all: &[Spec; PANEL_COUNT],
    angles: &[u16; PANEL_COUNT],
) -> Vec3 {
    let spec = all[id];
    let Some(parent) = spec.parent else {
        return point;
    };
    let sign = if matches!(id, 3 | 4 | 5 | 6 | 8) {
        -1.
    } else {
        1.
    };
    let moved = turn(point, spec.hinge, sign * f32::from(angles[id]) * PI / 180.);
    world_point(moved, parent, all, angles)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub panel: usize,
    pub x: u16,
    pub y: u16,
}
pub struct ModelView {
    pub raster: Rasterizer,
    pub labels: Vec<Label>,
}

impl Foldroom {
    pub fn sequence(&self) -> Sequence {
        self.sequence
    }
    pub fn progress(&self) -> u16 {
        self.progress
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn separated(&self) -> bool {
        self.separated
    }
    pub fn orbit(&self) -> (i16, i16) {
        (self.yaw, self.elevation)
    }
    pub fn angles(&self) -> [u16; PANEL_COUNT] {
        std::array::from_fn(|id| {
            if id == 0 {
                0
            } else {
                self.progress
                    .saturating_sub(self.sequence.stage(id) * 90)
                    .min(90)
            }
        })
    }
    pub fn panels(&self) -> Vec<Panel> {
        let all = specs();
        let angles = self.angles();
        all.iter()
            .enumerate()
            .map(|(id, spec)| {
                let [x0, x1, z0, z1] = spec.bounds;
                Panel {
                    id,
                    name: spec.name,
                    parent: spec.parent,
                    vertices: [
                        Vec3::new(x0, 0., z0),
                        Vec3::new(x1, 0., z0),
                        Vec3::new(x1, 0., z1),
                        Vec3::new(x0, 0., z1),
                    ]
                    .map(|v| world_point(v, id, &all, &angles)),
                    hinge: spec
                        .parent
                        .map(|parent| spec.hinge.map(|v| world_point(v, parent, &all, &angles))),
                    angle_degrees: angles[id],
                }
            })
            .collect()
    }
    /// Separation is an inspection-only translation, never fed into hinge geometry.
    pub fn display_panels(&self) -> Vec<Panel> {
        let mut panels = self.panels();
        if self.separated {
            let panel = &mut panels[self.selected];
            let offset = panel
                .center()
                .minus(Vec3::new(0., 0.6, 0.))
                .normalize()
                .scale(0.8);
            for v in &mut panel.vertices {
                *v = v.plus(offset);
            }
            panel.hinge = panel.hinge.map(|h| h.map(|v| v.plus(offset)));
        }
        panels
    }
    pub fn camera(&self) -> Camera {
        let (yaw, pitch) = (
            f32::from(self.yaw).to_radians(),
            f32::from(self.elevation).to_radians(),
        );
        let target = Vec3::new(0., 0.4, 0.55);
        let radius = 7.8;
        Camera {
            position: target.plus(Vec3::new(
                radius * pitch.cos() * yaw.sin(),
                radius * pitch.sin(),
                -radius * pitch.cos() * yaw.cos(),
            )),
            target,
            up: Vec3::new(0., 1., 0.),
            fov_y: 0.88,
            near: 0.1,
            far: 40.,
        }
    }
    /// Filled triangles, edge lines and label visibility share one camera/depth raster.
    pub fn model_view(&self, width: u16, height: u16, mono: bool) -> ModelView {
        let (width, height) = (width.min(MAX_RASTER.0), height.min(MAX_RASTER.1));
        let mut raster = Rasterizer::new(width, height);
        raster.clear((0, 0, 0));
        let panels = self.display_panels();
        let camera = self.camera();
        let colors = [
            (75, 136, 156),
            (218, 133, 78),
            (92, 176, 155),
            (98, 126, 211),
            (187, 121, 180),
            (209, 183, 103),
        ];
        for panel in &panels {
            let selected = panel.id == self.selected;
            let color = if mono {
                let level = if selected {
                    235
                } else {
                    90 + (panel.id % 5) as u8 * 23
                };
                (level, level, level)
            } else if selected {
                (255, 223, 137)
            } else if panel.id >= 6 {
                (117, 127, 151)
            } else {
                colors[panel.id]
            };
            let material = Material {
                color,
                ambient: 0.85,
                diffuse: 0.15,
                emissive: 0.,
            };
            for [a, b, c] in [[0, 1, 2], [0, 2, 3]] {
                raster.draw_triangle(
                    [panel.vertices[a], panel.vertices[b], panel.vertices[c]],
                    &camera,
                    material,
                );
            }
        }
        for panel in &panels {
            let edge_color = if panel.id == self.selected {
                (255, 255, 255)
            } else {
                (175, 187, 205)
            };
            // Subpixel-scale bias keeps the wire coherent with its filled panel.
            let toward_eye = |v: Vec3| v.plus(camera.position.minus(v).normalize().scale(0.012));
            for i in 0..4 {
                raster.line(
                    toward_eye(panel.vertices[i]),
                    toward_eye(panel.vertices[(i + 1) % 4]),
                    &camera,
                    edge_color,
                );
            }
        }
        let mut labels = Vec::new();
        for panel in &panels {
            if let Some((x, y, depth)) = camera.project(panel.center(), width, height) {
                let (x, y) = (x.floor() as u16, y.floor() as u16);
                if x < width
                    && y < height
                    && raster
                        .depth(i32::from(x), i32::from(y))
                        .is_some_and(|z| z.is_finite() && depth <= z + 0.12)
                {
                    labels.push(Label {
                        panel: panel.id,
                        x,
                        y: y / 2,
                    });
                }
            }
        }
        ModelView { raster, labels }
    }
    fn change_progress(&mut self, delta: i32) {
        self.progress = (i32::from(self.progress) + delta)
            .clamp(0, i32::from(self.sequence.stages()) * 90) as u16;
    }
    fn inspector(&self, visible: bool) -> Vec<String> {
        let panels = self.panels();
        let p = &panels[self.selected];
        let center = p.center();
        let edge_a = p.vertices[1].minus(p.vertices[0]).length();
        let edge_b = p.vertices[2].minus(p.vertices[1]).length();
        vec![
            format!("SELECTED {}  {}", (self.selected + 1) % 10, p.name),
            format!(
                "angle {:>2} deg | parent {}",
                p.angle_degrees,
                p.parent.map_or("fixed", |id| panels[id].name)
            ),
            format!("size {:.2} x {:.2} units", edge_a, edge_b),
            format!("center {:+.2} {:+.2} {:+.2}", center.x, center.y, center.z),
            format!(
                "{} | separation {}",
                if visible { "visible" } else { "hidden/edge-on" },
                if self.separated { "ON" } else { "off" }
            ),
            format!("camera yaw {} pitch {}", self.yaw, self.elevation),
        ]
    }
}

impl App for Foldroom {
    fn frame(&self, _at_ms: u64, w: u16, h: u16, depth: ColorDepth) -> Surface {
        let (w, h) = ui::dimensions(w, h);
        let mut out = Surface::new(w, h);
        let ink = Style::new().fg(Color::White);
        let accent = Style::new().fg(Color::Yellow).bold();
        ui::put(
            &mut out,
            0,
            0,
            "FOLDROOM  /  rigid-panel carton workbench",
            accent,
        );
        let stage = (self.progress / 90).min(self.sequence.stages() - 1);
        ui::put(
            &mut out,
            0,
            1,
            &format!(
                "{}  step {}/{}: {}  [{:>3}/{} deg]",
                self.sequence.name(),
                stage + 1,
                self.sequence.stages(),
                self.sequence.stage_name(stage),
                self.progress,
                self.sequence.stages() * 90
            ),
            ink,
        );
        let wide = w >= 100;
        let inspector_width = if wide { 33 } else { 0 };
        let model_w = w.saturating_sub(inspector_width).min(MAX_RASTER.0);
        let model_h = h
            .saturating_sub(if wide { 7 } else { 13 })
            .min(MAX_RASTER.1 / 2);
        let view = self.model_view(model_w, model_h * 2, depth == ColorDepth::Mono);
        let picture = if depth == ColorDepth::Mono {
            view.raster.raster.to_mono_surface()
        } else {
            view.raster.raster.to_surface()
        };
        out.blit_transparent_clipped(&picture, 0, 3, Rect::new(0, 3, model_w, model_h));
        for label in &view.labels {
            let s = ((label.panel + 1) % 10).to_string();
            let style = if label.panel == self.selected {
                accent.reverse()
            } else {
                ink.bg(Color::Black)
            };
            ui::put(&mut out, label.x, label.y + 3, &s, style);
        }
        let visible = view.labels.iter().any(|l| l.panel == self.selected);
        let lines = self.inspector(visible);
        let (ix, iy) = if wide {
            (model_w + 1, 3)
        } else {
            (0, 3 + model_h)
        };
        for (i, line) in lines.iter().enumerate() {
            ui::put(
                &mut out,
                ix,
                iy + i as u16,
                line,
                if i == 0 { accent } else { ink },
            );
        }
        if wide {
            for (i, p) in self.panels().iter().enumerate() {
                ui::put(
                    &mut out,
                    ix,
                    iy + 8 + i as u16,
                    &format!("{} {} {:>2}deg", (i + 1) % 10, p.name, p.angle_degrees),
                    if i == self.selected {
                        accent.reverse()
                    } else {
                        ink
                    },
                );
            }
        }
        let footer = [
            "Left/Right angle  [/] step  f flat  c closed",
            "Tab/Up/Down face  0-9 pick  Enter/e separate",
            "s sequence  a/d orbit  w/x tilt  r reset",
            "Finite geometry only; no material or collision proof.",
        ];
        for (i, line) in footer.iter().enumerate() {
            ui::put(&mut out, 0, h.saturating_sub(4) + i as u16, line, ink);
        }
        ui::finish(out, depth)
    }
    fn key(&mut self, _at_ms: u64, key: KeyEvent) {
        match key.code {
            KeyCode::Left => self.change_progress(-5),
            KeyCode::Right => self.change_progress(5),
            KeyCode::Char('[') => self.change_progress(-90),
            KeyCode::Char(']') => self.change_progress(90),
            KeyCode::Tab | KeyCode::Down => self.selected = (self.selected + 1) % PANEL_COUNT,
            KeyCode::BackTab | KeyCode::Up => {
                self.selected = (self.selected + PANEL_COUNT - 1) % PANEL_COUNT
            }
            KeyCode::Char(c @ '0'..='9') => {
                self.selected = (c as usize - '0' as usize + 9) % PANEL_COUNT
            }
            KeyCode::Enter | KeyCode::Char('e') => self.separated = !self.separated,
            KeyCode::Char('f') => self.progress = 0,
            KeyCode::Char('c') => self.progress = self.sequence.stages() * 90,
            KeyCode::Char('s') => {
                let previous = self.sequence.stages();
                self.sequence = match self.sequence {
                    Sequence::Around => Sequence::Paired,
                    Sequence::Paired => Sequence::Around,
                };
                self.progress = (u32::from(self.progress) * u32::from(self.sequence.stages())
                    / u32::from(previous)) as u16;
            }
            KeyCode::Char('a') => self.yaw = (self.yaw - 10).rem_euclid(360),
            KeyCode::Char('d') => self.yaw = (self.yaw + 10).rem_euclid(360),
            KeyCode::Char('w') => self.elevation = (self.elevation + 5).min(85),
            KeyCode::Char('x') => self.elevation = (self.elevation - 5).max(-75),
            KeyCode::Char('r') => *self = Self::default(),
            _ => {}
        }
    }
}
