//! The visual grammar: a cathedral/megacity whose physical architecture *is* the
//! dependency graph.
//!
//! Semantics → space:
//! * **service** → a tower/chamber whose height is stress and whose color is integrity;
//! * **dependency edge** → a bridge/buttress between tower crowns;
//! * **request flow** → a moving energy glyph travelling the bridge;
//! * **queue pressure** → a compressed amber band at the tower base;
//! * **latency** → vertical stretch of the tower;
//! * **retry storm** → recursive echo copies of the travelling glyph;
//! * **cascading failure** → bridges fading, crowns collapsing to rubble;
//! * **availability** → structural integrity (color + solidity);
//! * **isolated service** → a severed wing, displaced across a gap;
//! * **recovery** → the same geometry easing back into coherent form.
//!
//! The four scale transitions (whole system / cluster / causal chain / service)
//! are one continuous camera over **one** fixed scene, so a transition reads as
//! travelling through the same structure, never as swapping dashboards.

use gibson::capability::ColorDepth;
use gibson::cell::{Cell, Color, Glyph, Style};
use gibson::surface::{Rect, Surface};

use crate::incident::Phase;
use crate::sim::{Engine, ServiceId};

/// The camera's semantic level of detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    Whole,
    Cluster(u8),
    Chain(ServiceId),
    Service(ServiceId),
}

impl Scale {
    pub fn label(self) -> String {
        match self {
            Scale::Whole => "WHOLE SYSTEM".into(),
            Scale::Cluster(c) => format!("CLUSTER {c}"),
            Scale::Chain(id) => format!("CAUSAL CHAIN #{id}"),
            Scale::Service(id) => format!("SERVICE #{id}"),
        }
    }
    pub fn next(self, engine: &Engine, selected: ServiceId) -> Scale {
        let clusters = engine.fixture.clusters.len() as u8;
        match self {
            Scale::Whole => Scale::Cluster(engine.fixture.services[selected as usize].cluster % clusters),
            Scale::Cluster(_) => Scale::Chain(selected),
            Scale::Chain(_) => Scale::Service(selected),
            Scale::Service(_) => Scale::Whole,
        }
    }
}

// ── scene layout ────────────────────────────────────────────────────────────

const DX: f32 = 34.0;
const DY: f32 = 30.0;
const TIER_DY: f32 = 4.6;
const COL_DX: f32 = 3.4;
const ISLAND_OFFSET: f32 = 78.0;

/// The fixed scene: tower base positions in "world cells". Independent of the
/// terminal size, so the camera zooms rather than the structure moving.
pub struct Layout {
    pub base: Vec<(f32, f32)>,
    pub cluster_center: Vec<(f32, f32)>,
    pub isolated: Vec<bool>,
    pub bounds: (f32, f32, f32, f32),
}

impl Layout {
    pub fn build(engine: &Engine) -> Layout {
        let n = engine.len();
        let n_clusters = engine.fixture.clusters.len();
        let mut base = vec![(0.0f32, 0.0f32); n];
        let mut per_cluster: Vec<Vec<ServiceId>> = vec![Vec::new(); n_clusters];
        for s in &engine.fixture.services {
            per_cluster[s.cluster as usize].push(s.id);
        }
        let mut cluster_center = vec![(0.0f32, 0.0f32); n_clusters];
        for (c, ids) in per_cluster.iter().enumerate() {
            let col = (c % 3) as f32;
            let row = (c / 3) as f32;
            let ox = col * DX;
            let oy = row * DY;
            cluster_center[c] = (ox, oy - 2.0 * TIER_DY);
            // Group by tier, then spread along x within each tier.
            let mut tiers: Vec<Vec<ServiceId>> = vec![Vec::new(); 6];
            for &id in ids {
                tiers[engine.fixture.services[id as usize].tier.min(5) as usize].push(id);
            }
            for (t, tier_ids) in tiers.iter().enumerate() {
                let k = tier_ids.len();
                for (j, &id) in tier_ids.iter().enumerate() {
                    let jitter = crate::rng::Rng::hash01(0xC47_4ED, id as u64) - 0.5;
                    let x = ox + (j as f32 - (k.saturating_sub(1) as f32) / 2.0) * COL_DX
                        + jitter * 1.2;
                    let y = oy - t as f32 * TIER_DY + jitter * 0.8;
                    base[id as usize] = (x, y);
                }
            }
        }
        let isolated: Vec<bool> = engine.states.iter().map(|s| s.isolated).collect();
        for id in 0..n {
            if isolated[id] {
                base[id].0 += ISLAND_OFFSET;
            }
        }
        let (mut min_x, mut min_y, mut max_x, mut max_y) =
            (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        for &(x, y) in &base {
            min_x = min_x.min(x - 3.0);
            max_x = max_x.max(x + 3.0);
            min_y = min_y.min(y - 16.0);
            max_y = max_y.max(y + 3.0);
        }
        Layout {
            base,
            cluster_center,
            isolated,
            bounds: (min_x, min_y, max_x, max_y),
        }
    }

    pub fn pos(&self, id: ServiceId) -> (f32, f32) {
        self.base[id as usize]
    }

    /// A rough scene-space tower height (used by the camera to frame a service).
    pub fn tower_height(&self, engine: &Engine, id: ServiceId) -> f32 {
        let def = &engine.fixture.services[id as usize];
        let s = &engine.states[id as usize];
        let load = s.load_ratio(def);
        3.0 + 10.0 * s.distress(def) + 5.0 * load + 2.0 * (s.latency / def.base_latency - 1.0).max(0.0)
    }
}

// ── camera ──────────────────────────────────────────────────────────────────

pub struct Camera {
    pub cx: f32,
    pub cy: f32,
    pub zoom: f32,
    tcx: f32,
    tcy: f32,
    tzoom: f32,
}

impl Camera {
    pub fn new() -> Self {
        Camera {
            cx: 0.0,
            cy: 0.0,
            zoom: 1.0,
            tcx: 0.0,
            tcy: 0.0,
            tzoom: 1.0,
        }
    }

    pub fn aim(&mut self, engine: &Engine, layout: &Layout, scale: Scale, _selected: ServiceId, vw: f32, vh: f32) {
        let (min_x, min_y, max_x, max_y) = layout.bounds;
        let (mut cx, mut cy, mut zoom) = match scale {
            Scale::Whole => {
                let bw = (max_x - min_x).max(1.0);
                let bh = (max_y - min_y).max(1.0);
                (
                    (min_x + max_x) / 2.0,
                    (min_y + max_y) / 2.0,
                    (vw / (bw + 8.0)).min(vh / (bh + 12.0)).clamp(0.12, 1.4),
                )
            }
            Scale::Cluster(c) => {
                let (x, y) = layout.cluster_center[c as usize];
                (x, y, (vw / 46.0).min(vh / 42.0).clamp(0.6, 4.5))
            }
            Scale::Chain(id) => {
                let (x, y) = layout.pos(id);
                (x, y, (vw / 70.0).clamp(1.0, 4.0).min(vh / 46.0))
            }
            Scale::Service(id) => {
                let (x, y) = layout.pos(id);
                let h = layout.tower_height(engine, id);
                (x, y - h / 2.0, (vh / (h + 10.0)).clamp(2.0, 14.0))
            }
        };
        // Keep the camera inside the world so a scale change never shows void.
        cx = cx.clamp(min_x - 10.0, max_x + 10.0);
        cy = cy.clamp(min_y - 10.0, max_y + 10.0);
        zoom = zoom.max(0.05);
        self.tcx = cx;
        self.tcy = cy;
        self.tzoom = zoom;
    }

    /// Ease toward the aim — the transition is the journey, not a cut.
    pub fn update(&mut self) {
        let k = 0.22;
        self.cx += (self.tcx - self.cx) * k;
        self.cy += (self.tcy - self.cy) * k;
        // Zoom interpolates in log space so 1→10 and 10→1 feel symmetric.
        let lz = self.zoom.ln();
        let tlz = self.tzoom.ln();
        self.zoom = (lz + (tlz - lz) * k).exp();
    }

    pub fn settled(&self) -> bool {
        (self.cx - self.tcx).abs() < 0.01
            && (self.cy - self.tcy).abs() < 0.01
            && (self.zoom.ln() - self.tzoom.ln()).abs() < 0.001
    }

    #[inline]
    pub fn project(&self, x: f32, y: f32, vp: Rect) -> (f32, f32) {
        let sx = (x - self.cx) * self.zoom + vp.x as f32 + vp.width as f32 / 2.0;
        let sy = (y - self.cy) * self.zoom + vp.y as f32 + vp.height as f32 / 2.0;
        (sx, sy)
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

// ── palette ─────────────────────────────────────────────────────────────────

fn health_color(health: f32, distress: f32) -> Color {
    let bad = (1.0 - health).clamp(0.0, 1.0);
    let base = Color::Rgb(74, 196, 140); // sound masonry
    let mid = Color::Rgb(232, 190, 70); // stressed
    let hot = Color::Rgb(228, 66, 52); // collapsing
    let c = if bad < 0.5 {
        base.lerp(mid, bad * 2.0)
    } else {
        mid.lerp(hot, (bad - 0.5) * 2.0)
    };
    // Distress makes even a healthy tower smolder.
    c.lerp(hot, (distress * 0.25).clamp(0.0, 1.0))
}

fn phase_color(phase: Phase) -> Color {
    match phase {
        Phase::Normal => Color::Rgb(90, 190, 150),
        Phase::Rising => Color::Rgb(110, 190, 230),
        Phase::Overload => Color::Rgb(240, 180, 70),
        Phase::LocalFault => Color::Rgb(240, 140, 80),
        Phase::Cascade => Color::Rgb(240, 60, 60),
        Phase::Diagnosis => Color::Rgb(170, 210, 255),
        Phase::Intervention => Color::Rgb(220, 150, 240),
        Phase::PartialRecovery => Color::Rgb(150, 220, 180),
        Phase::Restored => Color::Rgb(120, 230, 200),
    }
}

fn bridge_color(error: f32, flow_norm: f32) -> Color {
    let calm = Color::Rgb(70, 120, 150);
    let hot = Color::Rgb(240, 90, 60);
    let c = calm.lerp(hot, error.clamp(0.0, 1.0));
    // Low-flow bridges dim toward the masonry.
    c.lerp(Color::Rgb(60, 70, 90), (1.0 - flow_norm.clamp(0.0, 1.0)) * 0.5)
}

// ── painter ─────────────────────────────────────────────────────────────────

struct Painter<'a> {
    s: &'a mut Surface,
    w: i32,
    h: i32,
}

impl<'a> Painter<'a> {
    fn new(s: &'a mut Surface) -> Self {
        let (w, h) = (s.width as i32, s.height as i32);
        Painter { s, w, h }
    }

    #[inline]
    fn put(&mut self, x: i32, y: i32, ch: char, style: Style) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.s
                .set_cell(x as u16, y as u16, Cell::new(Glyph::from_char(ch), style));
        }
    }

    #[inline]
    fn is_blank(&self, x: i32, y: i32) -> bool {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            match self.s.get(x as u16, y as u16) {
                Some(c) => c.glyph.grapheme.chars().all(|ch| ch == ' '),
                None => true,
            }
        } else {
            false
        }
    }

    /// Draw only into empty sky, so masonry wins over background structure.
    #[inline]
    fn put_soft(&mut self, x: i32, y: i32, ch: char, style: Style) {
        if self.is_blank(x, y) {
            self.put(x, y, ch, style);
        }
    }

    fn text(&mut self, x: i32, y: i32, text: &str, style: Style) -> i32 {
        if y < 0 || y >= self.h {
            return x;
        }
        if x >= self.w {
            return x;
        }
        let mut cx = x;
        for ch in text.chars() {
            if cx >= 0 {
                self.put(cx, y, ch, style);
            }
            cx += 1;
        }
        cx.max(0)
    }

    fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, style: Style, thick: bool) {
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx - dy;
        let (mut x, mut y) = (x0, y0);
        loop {
            let ch = if dy * 2 <= dx {
                '─'
            } else if dx * 2 <= dy {
                '│'
            } else if (sx * sy) > 0 {
                '╲'
            } else {
                '╱'
            };
            self.put_soft(x, y, ch, style);
            if thick {
                self.put_soft(x, y + 1, ch, style);
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
            // A runaway guard: degenerate projection must not spin.
            if (x - x0).abs() > 4000 || (y - y0).abs() > 4000 {
                break;
            }
        }
    }
}

// ── view state ──────────────────────────────────────────────────────────────

pub struct ViewState<'a> {
    pub engine: &'a Engine,
    pub layout: &'a Layout,
    pub camera: &'a Camera,
    pub phase: Phase,
    pub scale: Scale,
    pub selected: ServiceId,
    pub frame: u32,
    pub paused: bool,
    pub journal_len: usize,
    pub incident_len: usize,
    pub now_playing: &'a str,
    pub music_state: &'a str,
    pub message: &'a str,
    pub color_depth: ColorDepth,
    pub show_help: bool,
}

const HEADER_H: i32 = 1;
const FOOTER_H: i32 = 2;
const PANEL_W: i32 = 30;

/// Build one full-frame surface (architecture + chrome) at `width`×`height`.
pub fn build_frame(view: &ViewState, width: u16, height: u16) -> Surface {
    let mut surface = Surface::new(width, height);
    let (w, h) = (width as i32, height as i32);
    let panel = if w >= 108 { PANEL_W } else { 0 };
    let vp = Rect::new(0, HEADER_H as u16, (w - panel).max(1) as u16, (h - HEADER_H - FOOTER_H).max(1) as u16);

    {
        let mut p = Painter::new(&mut surface);
        sky(&mut p, vp);
        ground(&mut p, view, vp);
    }

    draw_bridges(&mut surface, view, vp);
    draw_towers(&mut surface, view, vp);
    draw_energy(&mut surface, view, vp);

    {
        let mut p = Painter::new(&mut surface);
        draw_selection(&mut p, view, vp);
    }

    if panel > 0 {
        draw_panel(&mut surface, view, Rect::new((w - panel) as u16, HEADER_H as u16, panel as u16, (h - HEADER_H - FOOTER_H).max(1) as u16));
    }
    draw_header(&mut surface, view, width);
    draw_footer(&mut surface, view, width, height);
    if view.show_help {
        draw_help(&mut surface, width, height);
    }
    surface
}

fn sky(p: &mut Painter, vp: Rect) {
    let top = Color::Rgb(8, 10, 22);
    let bottom = Color::Rgb(28, 24, 48);
    for y in vp.y as i32..(vp.y + vp.height) as i32 {
        let t = (y as f32 / (vp.height.max(1)) as f32).clamp(0.0, 1.0);
        let c = top.lerp(bottom, t);
        let style = Style::new().bg(c);
        let row: String = std::iter::repeat_n(' ', vp.width as usize).collect();
        p.text(vp.x as i32, y, &row, style);
    }
}

fn ground(p: &mut Painter, view: &ViewState, vp: Rect) {
    // A plinth under each district grounds the architecture in a shared datum.
    for &(x, y) in &view.layout.cluster_center {
        let (sx, sy) = view.camera.project(x - 20.0, y + 10.0, vp);
        let (ex, _) = view.camera.project(x + 20.0, y + 10.0, vp);
        if ex < vp.x as f32 || sx > (vp.x + vp.width) as f32 {
            continue;
        }
        let style = Style::new().fg(Color::Rgb(40, 44, 66));
        let mut x0 = sx.round() as i32;
        let mut x1 = ex.round() as i32;
        if x1 < x0 {
            std::mem::swap(&mut x0, &mut x1);
        }
        let wlim = p.w - 1;
        let yy = sy.round() as i32;
        for gx in x0.max(0)..=x1.min(wlim) {
            p.put(gx, yy, '▁', style);
        }
    }
}

fn draw_bridges(surface: &mut Surface, view: &ViewState, vp: Rect) {
    let engine = view.engine;
    let mut p = Painter::new(surface);
    let frame = view.frame as f32;
    for link in &engine.links {
        if link.flow <= 0.01 && link.error < 0.1 {
            continue;
        }
        let (fx, fy) = view.layout.pos(link.from);
        let (tx, ty) = view.layout.pos(link.to);
        let fh = view.layout.tower_height(engine, link.from);
        let th = view.layout.tower_height(engine, link.to);
        // Anchor at the tower crowns; a collapsed crown drags the bridge down.
        let (ax, ay) = view.camera.project(fx, fy - fh, vp);
        let (bx, by) = view.camera.project(tx, ty - th, vp);
        // Cull bridges fully outside the viewport.
        if (ax < vp.x as f32 - 5.0 && bx < vp.x as f32 - 5.0)
            || (ax > (vp.x + vp.width) as f32 + 5.0 && bx > (vp.x + vp.width) as f32 + 5.0)
        {
            continue;
        }
        let flow_norm = (link.flow / 60.0).clamp(0.0, 1.0);
        let style = Style::new().fg(bridge_color(link.error, flow_norm));
        let thick = link.flow > 45.0;
        p.line(ax.round() as i32, ay.round() as i32, bx.round() as i32, by.round() as i32, style, thick);

        // Retry storm: echoing ghost spans, offset along the bridge, dimmer each.
        if link.error > 0.5 && link.flow > 1.0 {
            for e in 1..=2 {
                let off = e as f32 * 0.07;
                let ghost = style.fg(bridge_color(link.error, flow_norm).lerp(Color::Rgb(255, 170, 60), 0.6));
                let dim = Style {
                    dim: true,
                    ..ghost
                };
                let _ = off;
                let mx = (ax + bx) / 2.0;
                let my = (ay + by) / 2.0 + e as f32;
                p.put(mx.round() as i32, my.round() as i32, '╳', dim);
            }
        }
        let _ = frame;
    }
}

fn draw_towers(surface: &mut Surface, view: &ViewState, vp: Rect) {
    let engine = view.engine;
    let mut p = Painter::new(surface);
    // Painter's algorithm: districts with smaller y (further back) first.
    let mut order: Vec<ServiceId> = (0..engine.len() as ServiceId).collect();
    order.sort_by(|&a, &b| {
        view.layout
            .pos(a)
            .1
            .partial_cmp(&view.layout.pos(b).1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let tower_w = (view.camera.zoom).clamp(1.0, 6.0).round() as i32;
    let show_detail = view.camera.zoom >= 2.2;
    let top_clip = HEADER_H;
    let bot_clip = vp.y as i32 + vp.height as i32;

    for id in order {
        let def = &engine.fixture.services[id as usize];
        let s = &engine.states[id as usize];
        let distress = s.distress(def);
        let (bx, by) = view.layout.pos(id);
        let h = view.layout.tower_height(engine, id);
        let (sx, sy) = view.camera.project(bx, by, vp);
        let (_, stop) = view.camera.project(bx, by - h, vp);
        let base_y = sy.round() as i32;
        let top_y = stop.round() as i32;
        if sx < -tower_w as f32 || sx > (vp.x + vp.width) as f32 + tower_w as f32 {
            continue;
        }
        let cx = sx.round() as i32;
        let color = health_color(s.health, distress);
        let solid = Style::new().fg(color);
        let collapsed = s.health < 0.22 || s.isolated;

        if collapsed {
            // Rubble: a short jagged stub, no chamber above.
            for dx in -(tower_w / 2)..=(tower_w / 2) {
                let jag = (id as i32 + dx).rem_euclid(3);
                let yy = base_y - jag;
                if yy >= top_clip && yy < bot_clip {
                    p.put(cx + dx, yy, "▁▂▃".chars().nth(jag as usize).unwrap_or('▁'), Style::new().fg(Color::Rgb(120, 60, 60)));
                }
            }
            if s.isolated {
                p.put(cx, base_y - 3, '✂', Style::new().fg(Color::Rgb(240, 120, 120)).bold());
            }
            continue;
        }

        // Solid chamber, with a fractional cap block at the crown.
        let mut y = base_y;
        while y >= top_y && y >= top_clip && y < bot_clip {
            let frac = if y == top_y {
                ((stop.fract().abs()) * 8.0) as usize
            } else {
                8
            };
            let ch = match frac {
                0 => '▁',
                1 => '▂',
                2 => '▃',
                3 => '▄',
                4 => '▅',
                5 => '▆',
                6 => '▇',
                _ => '█',
            };
            for dx in -(tower_w / 2)..=(tower_w / 2) {
                p.put(cx + dx, y, ch, solid);
            }
            // Windows glow where replicas are serving.
            if show_detail && (y + id as i32) % 3 == 0 {
                let wc = if s.breaker == crate::sim::Breaker::Open {
                    '✕'
                } else if s.replicas > 1 {
                    '▪'
                } else {
                    '·'
                };
                p.put(cx, y, wc, Style::new().fg(Color::Rgb(255, 244, 190)));
            }
            y -= 1;
        }

        // Queue pressure: a compressed amber band climbing from the base.
        let load = s.load_ratio(def).min(1.0);
        if load > 0.25 && show_detail {
            let band = (load * (h * view.camera.zoom).min(8.0)).round() as i32;
            let amber = Style::new().fg(Color::Rgb(240, 190, 80));
            for k in 0..band {
                let yy = base_y - k;
                if yy >= top_clip && yy < bot_clip {
                    p.put(cx + tower_w / 2 + 1, yy, '▓', amber);
                }
            }
        }
        if !view.layout.isolated[id as usize] && s.bad_deploy {
            p.put(cx, top_clip.max(top_y - 1), '!', Style::new().fg(Color::Rgb(255, 90, 90)).bold());
        }
    }
}

fn draw_energy(surface: &mut Surface, view: &ViewState, vp: Rect) {
    let engine = view.engine;
    let mut p = Painter::new(surface);
    let frame = view.frame as f32;
    for link in &engine.links {
        if link.flow < 0.5 || link.error > 0.9 {
            continue;
        }
        let (fx, fy) = view.layout.pos(link.from);
        let (tx, ty) = view.layout.pos(link.to);
        let fh = view.layout.tower_height(engine, link.from);
        let th = view.layout.tower_height(engine, link.to);
        let (ax, ay) = view.camera.project(fx, fy - fh * 0.7, vp);
        let (bx, by) = view.camera.project(tx, ty - th * 0.7, vp);
        if (ax < vp.x as f32 - 2.0 && bx < vp.x as f32 - 2.0)
            || (ax > (vp.x + vp.width) as f32 + 2.0 && bx > (vp.x + vp.width) as f32 + 2.0)
        {
            continue;
        }
        let speed = 0.008 + (link.flow / 120.0).clamp(0.0, 0.05);
        let phase = crate::rng::Rng::hash01(0xE0E0, (link.from as u64) << 16 | link.to as u64);
        let t = ((frame * speed + phase) % 1.0).abs();
        let x = ax + (bx - ax) * t;
        let y = ay + (by - ay) * t;
        let bright = Color::Rgb(255, 250, 210).lerp(Color::Rgb(255, 120, 80), link.error.clamp(0.0, 1.0));
        p.put(x.round() as i32, y.round() as i32, '●', Style::new().fg(bright));
        // Retry recursion: trailing echoes of the same request.
        if link.error > 0.35 {
            for e in 1..=3 {
                let te = (t - e as f32 * 0.05).rem_euclid(1.0);
                let ex = ax + (bx - ax) * te;
                let ey = ay + (by - ay) * te;
                p.put(ex.round() as i32, ey.round() as i32, '·', Style::new().fg(bright).dim());
            }
        }
    }
}

fn draw_selection(p: &mut Painter, view: &ViewState, vp: Rect) {
    let id = view.selected;
    let (bx, by) = view.layout.pos(id);
    let h = view.layout.tower_height(view.engine, id);
    let (sx, sy) = view.camera.project(bx, by + 1.0, vp);
    let (_, ty) = view.camera.project(bx, by - h - 1.0, vp);
    let style = Style::new().fg(Color::Rgb(255, 220, 120)).bold();
    let x = sx.round() as i32;
    for y in ty.round() as i32..=sy.round() as i32 {
        p.put(x - 2, y, '│', style);
        p.put(x + 2, y, '│', style);
    }
    p.put(x - 2, sy.round() as i32 + 1, '└', style);

    // In causal-chain mode, trace the live path as a golden arcade.
    if let Scale::Chain(sel) = view.scale {
        let path = causal_path(view.engine, sel);
        let gold = Style::new().fg(Color::Rgb(255, 210, 120));
        for w in path.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (ax, ay) = view.layout.pos(a);
            let (bx2, by2) = view.layout.pos(b);
            let ah = view.layout.tower_height(view.engine, a);
            let bh = view.layout.tower_height(view.engine, b);
            let (sx0, sy0) = view.camera.project(ax, ay - ah, vp);
            let (sx1, sy1) = view.camera.project(bx2, by2 - bh, vp);
            p.line(sx0.round() as i32, sy0.round() as i32, sx1.round() as i32, sy1.round() as i32, gold, false);
        }
    }
}

/// The live failure path: from `start`, follow the most-stressed dependency up and
/// down a few hops. Deterministic and cheap.
pub fn causal_path(engine: &Engine, start: ServiceId) -> Vec<ServiceId> {
    let mut path = vec![start];
    // Downstream: the most distressed dependency repeatedly.
    let mut cur = start;
    for _ in 0..4 {
        let def = &engine.fixture.services[cur as usize];
        let mut best: Option<(f32, ServiceId)> = None;
        for &d in &def.deps {
            let ds = &engine.states[d as usize];
            let score = ds.distress(&engine.fixture.services[d as usize]) + 0.3 * (1.0 - ds.health);
            if best.is_none_or(|(b, _)| score > b) {
                best = Some((score, d));
            }
        }
        match best {
            Some((_, d)) => {
                path.push(d);
                cur = d;
            }
            None => break,
        }
    }
    // Upstream: the most distressed dependent.
    let mut cur = start;
    for _ in 0..4 {
        let mut best: Option<(f32, ServiceId)> = None;
        for s in &engine.fixture.services {
            if s.deps.contains(&cur) {
                let ss = &engine.states[s.id as usize];
                let score = ss.distress(&engine.fixture.services[s.id as usize]);
                if best.is_none_or(|(b, _)| score > b) {
                    best = Some((score, s.id));
                }
            }
        }
        match best {
            Some((_, c)) => {
                path.insert(0, c);
                cur = c;
            }
            None => break,
        }
    }
    path.dedup();
    path
}

fn draw_header(surface: &mut Surface, view: &ViewState, width: u16) {
    let mut p = Painter::new(surface);
    let style = Style::new().fg(phase_color(view.phase)).bold();
    let m = view.engine.metrics;
    let mut line = format!(
        " CATHEDRAL  {}  sev {:.2}  services {}  crit {:.1}%  ovl {:.1}%  brk {}  t={}  {} ",
        view.phase.label(),
        view.phase.severity(),
        view.engine.len(),
        m.frac_critical * 100.0,
        m.frac_overloaded * 100.0,
        m.open_breakers,
        view.frame,
        if view.paused { "PAUSED" } else { "LIVE" },
    );
    if line.chars().count() > width as usize {
        line = line.chars().take(width as usize).collect();
    }
    p.text(0, 0, &line, style);
}

fn draw_footer(surface: &mut Surface, view: &ViewState, width: u16, height: u16) {
    let mut p = Painter::new(surface);
    let keys = Style::new().fg(Color::Rgb(150, 165, 200));
    let hot = Style::new().fg(Color::Rgb(255, 220, 120)).bold();
    let y = height as i32 - 2;
    let w = width as i32;
    let compact = w < 100;
    let keyline = if compact {
        "[F]ault [R]oll [I]sol [X]re [S]hed [A]ck [N]ote [Tab]scale [Q]uit"
    } else {
        "[F]inject-fault  [R]ollback  [I]solate  [X]reroute  [S]hed-load  [A]cknowledge  [N]ote  [1-4]scale  [<-/->]select  [Space]pause  [W]AV  [H]elp  [Q]uit"
    };
    p.text(0, y, &format!(" {}", keyline), keys);
    let y2 = height as i32 - 1;
    let left = format!(
        " {} · {} · incident events {} · journal {} · music {}",
        view.scale.label(),
        view.message,
        view.incident_len,
        view.journal_len,
        view.now_playing,
    );
    let mut txt = left;
    if txt.chars().count() > w as usize {
        txt = txt.chars().take(w as usize).collect();
    }
    p.text(0, y2, &txt, Style::new().fg(Color::Rgb(120, 135, 170)));
    // The music semantic axis, tiny but honest.
    let sem = view.music_state;
    if (w as usize) > sem.chars().count() + 2 {
        let x = w - sem.chars().count() as i32 - 1;
        p.text(x, y2, sem, hot);
    }
}

fn draw_panel(surface: &mut Surface, view: &ViewState, rect: Rect) {
    let mut p = Painter::new(surface);
    let x0 = rect.x as i32;
    let y0 = rect.y as i32;
    let w = rect.width as i32;
    let h = rect.height as i32;
    // Panel background.
    let bg = Style::new().bg(Color::Rgb(14, 16, 28)).fg(Color::Rgb(150, 165, 200));
    for y in y0..y0 + h {
        p.text(x0, y, &" ".repeat(w as usize), bg);
    }
    // Divider.
    for y in y0..y0 + h {
        p.put(x0, y, '│', Style::new().fg(Color::Rgb(50, 58, 86)));
    }
    let mut y = y0 + 1;
    p.text(x0 + 2, y, "FOCUS", Style::new().fg(Color::Rgb(255, 220, 120)).bold());
    y += 1;
    let id = view.selected;
    let def = &view.engine.fixture.services[id as usize];
    let s = &view.engine.states[id as usize];
    p.text(x0 + 2, y, &format!("#{id} {}", def.name), Style::new().fg(Color::Rgb(210, 220, 245)));
    y += 1;
    p.text(x0 + 2, y, &format!("district {}", view.engine.fixture.clusters[def.cluster as usize]), Style::new().fg(Color::Rgb(150, 165, 200)));
    y += 2;
    // Integrity / queue / latency gauges.
    gauge(&mut p, x0 + 2, y, w - 4, "integr", s.health, health_color(s.health, s.distress(def)));
    y += 1;
    gauge(&mut p, x0 + 2, y, w - 4, "queue", s.load_ratio(def).min(1.0), Color::Rgb(240, 190, 80));
    y += 1;
    gauge(&mut p, x0 + 2, y, w - 4, "error", s.error_rate.min(1.0), Color::Rgb(240, 90, 60));
    y += 2;
    p.text(x0 + 2, y, &format!("lat {:.1}t  rep {}  ver {}", s.latency, s.replicas, s.version), Style::new().fg(Color::Rgb(150, 165, 200)));
    y += 1;
    let bstate = match s.breaker {
        crate::sim::Breaker::Closed => "CLOSED",
        crate::sim::Breaker::Open => "OPEN",
        crate::sim::Breaker::HalfOpen => "HALF-OPEN",
    };
    p.text(x0 + 2, y, &format!("breaker {bstate}  trips {}", s.trips), Style::new().fg(if s.breaker == crate::sim::Breaker::Open { Color::Rgb(240, 90, 60) } else { Color::Rgb(150, 165, 200) }));
    y += 2;
    p.text(x0 + 2, y, "DEPENDENCIES", Style::new().fg(Color::Rgb(255, 220, 120)).bold());
    y += 1;
    for &d in def.deps.iter().take(4) {
        let ds = &view.engine.states[d as usize];
        p.text(x0 + 2, y, &format!("→ #{d} {:.2}", ds.health), Style::new().fg(health_color(ds.health, ds.distress(&view.engine.fixture.services[d as usize]))));
        y += 1;
    }
    // Major affected cluster.
    y = y.max(rect.y as i32 + h / 2 + 2);
    p.text(x0 + 2, y, "HOTSPOTS", Style::new().fg(Color::Rgb(255, 220, 120)).bold());
    y += 1;
    let mut hot: Vec<(f32, ServiceId)> = (0..view.engine.len() as ServiceId)
        .map(|i| (view.engine.states[i as usize].distress(&view.engine.fixture.services[i as usize]), i))
        .collect();
    hot.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    for &(d, id) in hot.iter().take(8) {
        let def = &view.engine.fixture.services[id as usize];
        p.text(x0 + 2, y, &format!("{:>4.2} #{} {}", d, id, &def.name[..def.name.len().min(12)]), Style::new().fg(health_color(1.0 - d, d)));
        y += 1;
        if y > rect.y as i32 + h - 1 {
            break;
        }
    }
}

fn gauge(p: &mut Painter, x: i32, y: i32, w: i32, label: &str, value: f32, color: Color) {
    let v = value.clamp(0.0, 1.0);
    let bar_w = (w - 9).max(4);
    let filled = (v * bar_w as f32).round() as i32;
    p.text(x, y, &format!("{label:>6} "), Style::new().fg(Color::Rgb(150, 165, 200)));
    for i in 0..bar_w {
        let ch = if i < filled { '█' } else { '·' };
        p.put(x + 7 + i, y, ch, Style::new().fg(color));
    }
}

fn draw_help(surface: &mut Surface, width: u16, height: u16) {
    let mut p = Painter::new(surface);
    let w = width as i32;
    let h = height as i32;
    let bw = 62.min(w - 4);
    let bh = 16.min(h - 4);
    let x0 = (w - bw) / 2;
    let y0 = (h - bh) / 2;
    let bg = Style::new().bg(Color::Rgb(18, 20, 34)).fg(Color::Rgb(215, 225, 250));
    for y in y0..y0 + bh {
        p.text(x0, y, &" ".repeat(bw as usize), bg);
    }
    for x in x0..x0 + bw {
        p.put(x, y0, '─', Style::new().fg(Color::Rgb(120, 135, 170)));
        p.put(x, y0 + bh - 1, '─', Style::new().fg(Color::Rgb(120, 135, 170)));
    }
    let lines = [
        "CATHEDRAL — controls",
        "",
        "F inject a faulty deployment on the focus",
        "R roll the deployment back",
        "I isolate the focus (sever its wing)",
        "X reroute traffic off the focus",
        "S shed a fraction of the focus's load",
        "A acknowledge the incident   N annotate",
        "",
        "1 whole system  2 cluster  3 causal chain  4 service",
        "TAB cycles scale   ←/→ move focus   SPACE pause",
        "W export the current checked performance to WAV",
        "H toggle this help   Q quit (restores the terminal)",
    ];
    for (i, l) in lines.iter().enumerate() {
        let style = if i == 0 {
            Style::new().fg(Color::Rgb(255, 220, 120)).bold()
        } else {
            Style::new().fg(Color::Rgb(200, 210, 235))
        };
        p.text(x0 + 2, y0 + 1 + i as i32, l, style);
    }
}
