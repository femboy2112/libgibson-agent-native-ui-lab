//! The spatial view: execution history as a navigable tunnel.
//!
//! * time is the z axis (1 step = 1 world unit); the camera rides the active branch
//! * each branch is a hexagonal tunnel whose six rails are the six tasks; ring colour is the
//!   semantic epoch and its brightness is the *rendered audio energy* of that step
//! * state variables are persistent threads spiralling inside the tunnel
//! * a fork is a real geometric split: the child tunnel peels away from the parent at the
//!   fork step and lives in its own lane/plane (rolled about the time axis)
//! * the old future of a rewound branch persists as dotted ghost geometry, with a burnt
//!   star where it ended in catastrophe
//! * in compare mode the dimensions become a braid: identical dimensions fuse into one
//!   thread, divergent ones pull apart, and causal roots bloom.
//!
//! Rendering is a pure function of a [`Snapshot`] and a [`Params`]; it owns no clock.

use crate::epoch::Epoch;
use crate::history::{BranchId, RootKind};
use crate::sem::*;
use crate::vm::*;
use gibson::geom::Vec3;
use gibson::raster::{Rgb as Px, RgbRaster};
use gibson::raster3d::{Camera, Fog, Material, Rasterizer};
use gibson::{Cell, Color, ColorDepth, Glyph, Style, SubcellGlyphMode, Surface};

pub type Rgb = [u8; 3];

// --- snapshot --------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct RecView {
    pub step: u32,
    pub epoch: Epoch,
    pub vars: [i32; NVARS],
    pub tcode: [u8; MAX_TASKS],
    pub task: u8,
    pub class: Class,
    pub flags: u16,
    pub obj: u8,
    pub aux: i32,
    pub parents: [u32; 4],
    pub energy: f32,
    pub dead: bool,
    pub locks: [u8; NLOCKS],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Role {
    /// The branch the cursor is on.
    Active,
    /// An ancestor of the active branch; its steps before `live_until` are lived history.
    Ancestor { live_until: u32 },
    /// Everything else: persists as ghost geometry.
    Ghost,
}

#[derive(Clone, Debug)]
pub struct BranchView {
    pub id: BranchId,
    pub label: String,
    pub parent: Option<BranchId>,
    pub fork_at: u32,
    pub end: u32,
    pub terminal: Option<Terminal>,
    pub lane: (f32, f32, f32),
    pub role: Role,
    /// Records for steps `first_step ..`.
    pub first_step: u32,
    pub recs: Vec<RecView>,
}

impl BranchView {
    pub fn rec(&self, step: u32) -> Option<&RecView> {
        step.checked_sub(self.first_step)
            .and_then(|i| self.recs.get(i as usize))
    }
}

#[derive(Clone, Debug)]
pub struct CompareView {
    pub a: BranchId,
    pub b: BranchId,
    /// Per step: relation of every dimension.
    pub rows: Vec<(Verdict, [Rel; NDIM], Option<RootKind>)>,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub branches: Vec<BranchView>,
    pub active: BranchId,
    /// Position: number of steps executed so far in the cursor's view of history.
    pub cursor: u32,
    pub compare: Option<CompareView>,
    /// Presentation clock (seconds): drives *visual* motion only, never history.
    pub time: f32,
    /// A branch born `age` seconds ago (fork bloom).
    pub bloom: Option<(BranchId, f32)>,
    /// Smoothed camera (step units, lateral).
    pub cam: (f32, f32, f32),
    /// Highlighted causal ancestry (step ids) of the inspected event.
    pub ancestry: Vec<(u32, u8)>,
    pub inspect_branch: BranchId,
    /// 1 = lanes fully apart, 0 = every branch folded back into the trunk (collapse/rejoin).
    pub spread: f32,
    /// Camera yaw about the vertical axis: 0 looks into the future, π looks back at the past.
    pub yaw: f32,
    /// Camera rides inside the tunnel (true) or chases it from outside (false).
    pub inside: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub cw: u16,
    pub ch: u16,
    pub depth: ColorDepth,
    pub glyphs: SubcellGlyphMode,
}

// --- LOD ---------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct Lod {
    pub back: u32,
    pub fwd: u32,
    pub ring_every: u32,
    pub threads: usize,
    pub links: usize,
    pub ghosts_max: usize,
    pub labels: bool,
}

pub fn lod_for(cw: u16, ch: u16) -> Lod {
    let area = cw as u32 * ch as u32;
    if area < 700 {
        Lod {
            back: 4,
            fwd: 30,
            ring_every: 4,
            threads: 2,
            links: 10,
            ghosts_max: 2,
            labels: false,
        }
    } else if area < 1800 {
        Lod {
            back: 6,
            fwd: 40,
            ring_every: 3,
            threads: 4,
            links: 40,
            ghosts_max: 3,
            labels: true,
        }
    } else {
        Lod {
            back: 8,
            fwd: 56,
            ring_every: 3,
            threads: 6,
            links: 90,
            ghosts_max: 5,
            labels: true,
        }
    }
}

// --- sub-cell raster ---------------------------------------------------------------------

pub struct SubRaster {
    pub cw: u16,
    pub ch: u16,
    w: i32,
    h: i32,
    depth: Vec<f32>,
    color: Vec<Rgb>,
    pub plotted: u64,
    /// Depth (world units) at which a dot has faded to `FADE_MIN`.
    pub fade_far: f32,
}

impl SubRaster {
    pub fn new(cw: u16, ch: u16) -> SubRaster {
        let (w, h) = (cw as i32 * 2, ch as i32 * 4);
        SubRaster {
            cw,
            ch,
            w,
            h,
            depth: vec![f32::INFINITY; (w * h) as usize],
            color: vec![[0; 3]; (w * h) as usize],
            plotted: 0,
            fade_far: 70.0,
        }
    }
    pub fn dots(&self) -> (i32, i32) {
        (self.w, self.h)
    }
    pub fn clear(&mut self) {
        self.depth.iter_mut().for_each(|d| *d = f32::INFINITY);
        self.plotted = 0;
    }
    #[inline]
    fn plot(&mut self, x: i32, y: i32, z: f32, c: Rgb) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        let i = (y * self.w + x) as usize;
        if z <= self.depth[i] {
            self.depth[i] = z;
            // atmospheric perspective: dots dim with distance instead of whole segments
            let k = 1.0 - ((z - 3.0) / self.fade_far).clamp(0.0, 1.0) * 0.82;
            self.color[i] = [
                (c[0] as f32 * k) as u8,
                (c[1] as f32 * k) as u8,
                (c[2] as f32 * k) as u8,
            ];
            self.plotted += 1;
        }
    }
    /// Depth-tested line in dot space. `a`/`b` are `(x, y, depth)`.
    pub fn line(&mut self, a: (f64, f64, f64), b: (f64, f64, f64), c: Rgb) {
        // Liang–Barsky against the dot rectangle
        let (x0, y0, x1, y1) = (a.0, a.1, b.0, b.1);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        let p = [-dx, dx, -dy, dy];
        let q = [x0, self.w as f64 - 1.0 - x0, y0, self.h as f64 - 1.0 - y0];
        for k in 0..4 {
            if p[k] == 0.0 {
                if q[k] < 0.0 {
                    return;
                }
            } else {
                let r = q[k] / p[k];
                if p[k] < 0.0 {
                    if r > t1 {
                        return;
                    }
                    t0 = t0.max(r);
                } else {
                    if r < t0 {
                        return;
                    }
                    t1 = t1.min(r);
                }
            }
        }
        let (ax, ay) = (x0 + t0 * dx, y0 + t0 * dy);
        let (bx, by) = (x0 + t1 * dx, y0 + t1 * dy);
        // reciprocal-depth interpolation (perspective correct)
        let (ia, ib) = (1.0 / a.2.max(1e-6), 1.0 / b.2.max(1e-6));
        let (ra, rb) = (ia + (ib - ia) * t0, ia + (ib - ia) * t1);
        let steps = ((bx - ax).abs().max((by - ay).abs()).ceil() as i32).max(1);
        for s in 0..=steps {
            let t = s as f64 / steps as f64;
            let x = (ax + (bx - ax) * t).round() as i32;
            let y = (ay + (by - ay) * t).round() as i32;
            let r = ra + (rb - ra) * t;
            self.plot(x, y, (1.0 / r.max(1e-9)) as f32, c);
        }
    }
    pub fn dot_at(&self, x: i32, y: i32) -> Option<(f32, Rgb)> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return None;
        }
        let i = (y * self.w + x) as usize;
        (self.depth[i].is_finite()).then(|| (self.depth[i], self.color[i]))
    }

    /// Realize into cells. `bg` paints the deep-space field; `glow` adds a faint halo bg.
    pub fn to_surface(
        &self,
        mode: SubcellGlyphMode,
        depth: ColorDepth,
        bg: Color,
        glow: bool,
    ) -> Surface {
        let mut s = Surface::new(self.cw, self.ch);
        let bg_style = Style::new().bg(bg);
        for cy in 0..self.ch {
            for cx in 0..self.cw {
                let mut mask = 0u8;
                let mut best = f32::INFINITY;
                let mut col = [0u8; 3];
                for dy in 0..4i32 {
                    for dx in 0..2i32 {
                        let x = cx as i32 * 2 + dx;
                        let y = cy as i32 * 4 + dy;
                        let i = (y * self.w + x) as usize;
                        if self.depth[i].is_finite() {
                            mask |= match (dx, dy) {
                                (0, 0) => 0x01,
                                (0, 1) => 0x02,
                                (0, 2) => 0x04,
                                (0, 3) => 0x40,
                                (1, 0) => 0x08,
                                (1, 1) => 0x10,
                                (1, 2) => 0x20,
                                _ => 0x80,
                            };
                            if self.depth[i] < best {
                                best = self.depth[i];
                                col = self.color[i];
                            }
                        }
                    }
                }
                let cell = match mode.subcell_glyph(mask) {
                    Some(ch) => {
                        let mut st = Style::new();
                        if depth != ColorDepth::Mono {
                            st = st.fg(Color::Rgb(col[0], col[1], col[2]));
                        }
                        if glow
                            && mask.count_ones() >= 4
                            && matches!(depth, ColorDepth::TrueColor | ColorDepth::Ansi256)
                        {
                            st = st.bg(Color::Rgb(col[0] / 9 + 3, col[1] / 9 + 4, col[2] / 9 + 8));
                        } else if depth != ColorDepth::Mono {
                            st = st.bg(bg);
                        }
                        Cell::new(Glyph::from_char(ch), st)
                    }
                    None => Cell::new(
                        Glyph::space(),
                        if depth == ColorDepth::Mono {
                            Style::new()
                        } else {
                            bg_style
                        },
                    ),
                };
                s.set_cell(cx, cy, cell);
            }
        }
        s
    }
}

// --- camera / projection -----------------------------------------------------------------

pub struct Proj {
    pos: [f64; 3],
    right: [f64; 3],
    up: [f64; 3],
    fwd: [f64; 3],
    tan_x: f64,
    tan_y: f64,
    near: f64,
    w: f64,
    h: f64,
}

fn v3(v: Vec3) -> [f64; 3] {
    [v.x as f64, v.y as f64, v.z as f64]
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: [f64; 3]) -> [f64; 3] {
    let l = dot(a, a).sqrt().max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}

impl Proj {
    /// Built from the public `gibson::raster3d::Camera` description. (`Camera::project` itself
    /// rejects every point outside the frustum, which is useless for line clipping, so the
    /// view math is re-derived here from the camera's public fields.)
    pub fn new(cam: &Camera, wdots: i32, hdots: i32) -> Proj {
        let fwd = unit(sub(v3(cam.target), v3(cam.position)));
        let right = unit(cross(v3(cam.up), fwd));
        let up = cross(fwd, right);
        let tan_y = (cam.fov_y as f64 * 0.5).tan();
        Proj {
            pos: v3(cam.position),
            right,
            up,
            fwd,
            tan_x: tan_y * wdots as f64 / hdots as f64,
            tan_y,
            near: cam.near as f64,
            w: wdots as f64,
            h: hdots as f64,
        }
    }
    fn cam(&self, p: Vec3) -> [f64; 3] {
        let d = sub(v3(p), self.pos);
        [dot(d, self.right), dot(d, self.up), dot(d, self.fwd)]
    }
    fn scr(&self, c: [f64; 3]) -> (f64, f64, f64) {
        (
            (c[0] / (c[2] * self.tan_x) + 1.0) * self.w * 0.5,
            (1.0 - c[1] / (c[2] * self.tan_y)) * self.h * 0.5,
            c[2],
        )
    }
    pub fn point(&self, p: Vec3) -> Option<(f64, f64, f64)> {
        let c = self.cam(p);
        (c[2] >= self.near).then(|| self.scr(c))
    }
    #[allow(clippy::type_complexity)]
    pub fn segment(&self, a: Vec3, b: Vec3) -> Option<((f64, f64, f64), (f64, f64, f64))> {
        let (mut ca, mut cb) = (self.cam(a), self.cam(b));
        if ca[2] < self.near && cb[2] < self.near {
            return None;
        }
        if ca[2] < self.near {
            let t = (self.near - ca[2]) / (cb[2] - ca[2]);
            ca = [
                ca[0] + (cb[0] - ca[0]) * t,
                ca[1] + (cb[1] - ca[1]) * t,
                self.near,
            ];
        } else if cb[2] < self.near {
            let t = (self.near - cb[2]) / (ca[2] - cb[2]);
            cb = [
                cb[0] + (ca[0] - cb[0]) * t,
                cb[1] + (ca[1] - cb[1]) * t,
                self.near,
            ];
        }
        Some((self.scr(ca), self.scr(cb)))
    }
}

// --- colour helpers ----------------------------------------------------------------------

fn scale(c: Rgb, k: f32) -> Rgb {
    let k = k.clamp(0.0, 1.6);
    [
        (c[0] as f32 * k).min(255.0) as u8,
        (c[1] as f32 * k).min(255.0) as u8,
        (c[2] as f32 * k).min(255.0) as u8,
    ]
}
fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [
        (a[0] as f32 + (b[0] as f32 - a[0] as f32) * t) as u8,
        (a[1] as f32 + (b[1] as f32 - a[1] as f32) * t) as u8,
        (a[2] as f32 + (b[2] as f32 - a[2] as f32) * t) as u8,
    ]
}
pub fn desat(c: Rgb, t: f32) -> Rgb {
    let g = ((c[0] as u32 * 30 + c[1] as u32 * 59 + c[2] as u32 * 11) / 100) as u8;
    mix(c, [g, g, g], t)
}
fn epoch_rgb(e: Epoch) -> Rgb {
    let c = e.rgb();
    [c.0, c.1, c.2]
}

pub const VAR_COLORS: [Rgb; NVARS] = [
    [255, 90, 60],   // HEAT
    [255, 190, 60],  // LOAD
    [70, 220, 255],  // COOL
    [140, 255, 120], // CREDITS
    [60, 150, 70],   // LEDGER
    [120, 120, 140], // TICKS
    [255, 60, 90],   // FAULTS
    [230, 90, 255],  // ESC
    [250, 250, 250], // MODE
    [90, 90, 110],   // OUT
];

/// Which variables get threads, in priority order (LOD cuts the tail).
pub const THREAD_ORDER: [u8; 6] = [V_HEAT, V_LOAD, V_COOL, V_CREDITS, V_LEDGER, V_ESC];
const THREAD_ANGLE: [f32; NVARS] = [1.2, 0.2, -0.9, -2.0, -1.8, 0.0, 2.6, 2.2, 3.0, 0.0];

fn norm_var(v: u8, x: i32) -> f32 {
    let hi = match v {
        V_HEAT => 100.0,
        V_LOAD | V_COOL | V_ESC | V_FAULTS => 9.0,
        V_MODE => 2.0,
        _ => 12.0,
    };
    if v == V_CREDITS || v == V_LEDGER {
        // ledger values are monotone counters; show the last-window delta instead
        return ((x % 12) as f32 / 12.0).clamp(0.0, 1.0);
    }
    (x as f32 / hi).clamp(0.0, 1.0)
}

// --- scene geometry ----------------------------------------------------------------------

const R: f32 = 1.25;
pub const SPLIT: f32 = 14.0;

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn find(s: &Snapshot, id: BranchId) -> Option<&BranchView> {
    s.branches.iter().find(|b| b.id == id)
}

/// Centre of branch `b`'s tunnel at `step` (peels from its parent over [`SPLIT`] steps).
pub fn center(s: &Snapshot, b: &BranchView, step: f32) -> (f32, f32, f32) {
    let (lx, ly, lr) = (
        b.lane.0 * s.spread,
        b.lane.1 * s.spread,
        b.lane.2 * s.spread,
    );
    let t = smooth((step - b.fork_at as f32) / SPLIT);
    let (px, py, pr) = match b.parent.and_then(|p| find(s, p)) {
        Some(p) => center(s, p, step),
        None => (0.0, 0.0, 0.0),
    };
    (
        px + (lx - px) * t * if b.parent.is_some() { 1.0 } else { 0.0 },
        py + (ly - py) * t * if b.parent.is_some() { 1.0 } else { 0.0 },
        pr + (lr - pr) * t,
    )
}

fn rail_pos(c: (f32, f32, f32), rail: usize, r: f32, z: f32) -> Vec3 {
    let a = c.2 + std::f32::consts::FRAC_PI_2 - rail as f32 * std::f32::consts::FRAC_PI_3;
    Vec3::new(c.0 + r * a.cos(), c.1 + r * a.sin(), z)
}
fn polar(c: (f32, f32, f32), ang: f32, r: f32, z: f32) -> Vec3 {
    let a = c.2 + ang;
    Vec3::new(c.0 + r * a.cos(), c.1 + r * a.sin(), z)
}

#[derive(Clone, Copy, PartialEq)]
enum Style3 {
    Lived,
    Foretold,
    Ghost,
}

fn style_of(role: Role, step: u32, cursor: u32) -> Style3 {
    match role {
        Role::Active => {
            if step < cursor {
                Style3::Lived
            } else {
                Style3::Foretold
            }
        }
        Role::Ancestor { live_until } => {
            if step < live_until {
                if step < cursor {
                    Style3::Lived
                } else {
                    Style3::Foretold
                }
            } else {
                Style3::Ghost
            }
        }
        Role::Ghost => Style3::Ghost,
    }
}

/// Whether a dotted/dashed element should be drawn at this step (world-anchored stipple).
fn stipple(st: Style3, step: u32, salt: u32) -> bool {
    match st {
        Style3::Lived => true,
        Style3::Foretold => (step + salt).is_multiple_of(2),
        Style3::Ghost => (step + salt).is_multiple_of(3),
    }
}

fn dim_for(st: Style3) -> f32 {
    match st {
        Style3::Lived => 1.0,
        Style3::Foretold => 0.62,
        Style3::Ghost => 0.4,
    }
}

pub struct Frame {
    pub surface: Surface,
    pub stats: FrameStats,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct FrameStats {
    pub segments: u32,
    pub plotted: u64,
    pub branches_drawn: u32,
}

struct Pen<'a> {
    r: &'a mut SubRaster,
    p: &'a Proj,
    segs: u32,
}

impl<'a> Pen<'a> {
    fn seg(&mut self, a: Vec3, b: Vec3, c: Rgb) {
        if let Some((sa, sb)) = self.p.segment(a, b) {
            self.r.line(sa, sb, c);
            self.segs += 1;
        }
    }
    fn poly_ring(&mut self, c: (f32, f32, f32), radius: f32, z: f32, col: Rgb) {
        for i in 0..6 {
            let a = rail_pos(c, i, radius, z);
            let b = rail_pos(c, (i + 1) % 6, radius, z);
            self.seg(a, b, col);
        }
    }
    fn star(&mut self, c: (f32, f32, f32), z: f32, len: f32, n: usize, col: Rgb) {
        for i in 0..n {
            let ang = i as f32 * std::f32::consts::TAU / n as f32;
            let a = polar(c, ang, 0.1, z);
            let b = polar(c, ang, len, z + (i % 3) as f32 * 0.9 - 0.9);
            self.seg(a, b, col);
        }
    }
}

fn task_state_color(tc: u8, base: Rgb) -> (Rgb, bool) {
    // returns (colour, draw-this-step) — dashed states draw on alternate steps only via caller
    match tc {
        0 => (base, true),
        1 => (scale(base, 0.45), true),
        2 => (mix(scale(base, 0.5), [60, 90, 200], 0.5), true),
        3 => (mix(base, [255, 200, 60], 0.7), true),
        4 => (mix(base, [190, 120, 255], 0.8), true),
        5 => ([255, 40, 50], true),
        _ => ([90, 90, 100], true),
    }
}

/// Composite a filled RGB layer (half-block resolution) under the Braille wire layer.
/// Where a cell carries wire dots the glyph is Braille over a dimmed copy of the fill.
pub fn compose_layers(
    fill: Option<&RgbRaster>,
    sub: &SubRaster,
    mode: SubcellGlyphMode,
    depth: ColorDepth,
    bg: Color,
) -> Surface {
    let Some(fill) = fill else {
        return sub.to_surface(mode, depth, bg, true);
    };
    let mut s = Surface::new(sub.cw, sub.ch);
    for cy in 0..sub.ch {
        for cx in 0..sub.cw {
            let top = fill.get(cx as i32, cy as i32 * 2).unwrap_or((5, 7, 13));
            let bot = fill.get(cx as i32, cy as i32 * 2 + 1).unwrap_or((5, 7, 13));
            // wire dots in this cell
            let mut mask = 0u8;
            let mut best = f32::INFINITY;
            let mut col = [0u8; 3];
            for dy in 0..4i32 {
                for dx in 0..2i32 {
                    if let Some((d, c)) = sub.dot_at(cx as i32 * 2 + dx, cy as i32 * 4 + dy) {
                        mask |= match (dx, dy) {
                            (0, 0) => 0x01,
                            (0, 1) => 0x02,
                            (0, 2) => 0x04,
                            (0, 3) => 0x40,
                            (1, 0) => 0x08,
                            (1, 1) => 0x10,
                            (1, 2) => 0x20,
                            _ => 0x80,
                        };
                        if d < best {
                            best = d;
                            col = c;
                        }
                    }
                }
            }
            let cell = match mode.subcell_glyph(mask) {
                Some(ch) => {
                    let avg = (
                        ((top.0 as u16 + bot.0 as u16) / 2) as u8,
                        ((top.1 as u16 + bot.1 as u16) / 2) as u8,
                        ((top.2 as u16 + bot.2 as u16) / 2) as u8,
                    );
                    Cell::new(
                        Glyph::from_char(ch),
                        Style::new()
                            .fg(Color::Rgb(col[0], col[1], col[2]))
                            .bg(Color::Rgb(avg.0 / 2, avg.1 / 2, avg.2 / 2)),
                    )
                }
                None if top == bot => Cell::new(
                    Glyph::space(),
                    Style::new().bg(Color::Rgb(top.0, top.1, top.2)),
                ),
                None => Cell::new(
                    Glyph::from_char('▀'),
                    Style::new()
                        .fg(Color::Rgb(top.0, top.1, top.2))
                        .bg(Color::Rgb(bot.0, bot.1, bot.2)),
                ),
            };
            s.set_cell(cx, cy, cell);
        }
    }
    let _ = (depth, bg);
    s
}

fn px(c: Rgb) -> Px {
    (c[0], c[1], c[2])
}

fn tri(rz: &mut Rasterizer, cam: &Camera, a: Vec3, b: Vec3, c: Vec3, col: Rgb, emissive: f32) {
    rz.draw_triangle(
        [a, b, c],
        cam,
        Material {
            color: px(col),
            ambient: 0.35,
            diffuse: 0.3,
            emissive,
        },
    );
}

/// World-anchored dust: a deterministic cloud that streams past as the cursor moves, so
/// motion through time reads as motion through space even where no execution geometry is.
fn dust(pen: &mut Pen, cz: f32, fwd: f32, center: (f32, f32), depth_mono: bool) {
    let n = 260u32;
    let span = fwd * 1.6;
    for i in 0..n {
        let h1 = (i.wrapping_mul(2654435761)) ^ (i << 7);
        let h2 = h1.wrapping_mul(2246822519) ^ (h1 >> 13);
        let h3 = h2.wrapping_mul(3266489917) ^ (h2 >> 11);
        let fx = (h1 % 1000) as f32 / 1000.0;
        let fy = (h2 % 1000) as f32 / 1000.0;
        let fz = (h3 % 1000) as f32 / 1000.0;
        // z wraps relative to the camera so the cloud is endless
        let zr = (fz * span + 0.0) % span;
        let z0 = (cz / span).floor() * span;
        let mut z = z0 + zr;
        if z < cz - span * 0.25 {
            z += span;
        }
        let x = center.0 + (fx - 0.5) * 34.0;
        let y = center.1 + (fy - 0.45) * 15.0;
        // keep the dust out of the tunnel itself
        if (x - center.0).abs() < 2.2 && (y - center.1).abs() < 2.2 {
            continue;
        }
        let a = Vec3::new(x, y, z);
        let b = Vec3::new(x, y, z + 0.35);
        let c = if depth_mono {
            [200, 200, 200]
        } else {
            [70, 90, 130]
        };
        pen.seg(a, b, c);
    }
}

/// Render the navigation view.
pub fn render_navigate(s: &Snapshot, prm: Params) -> Frame {
    let lod = lod_for(prm.cw, prm.ch);
    let mut r = SubRaster::new(prm.cw, prm.ch);
    let (wd, hd) = r.dots();
    // The camera rides inside the cursor branch and faces where time is going: forward into the
    // foretold future, or (after a rewind) back along the lived past. `yaw` animates the turn.
    let (cz, cx, cy) = s.cam;
    let (sy, cyaw) = (s.yaw.sin(), s.yaw.cos());
    let dir = Vec3::new(sy, 0.0, cyaw);
    let focus = Vec3::new(cx, cy, cz);
    let cam = if s.inside {
        Camera {
            position: Vec3::new(focus.x - dir.x * 3.4, cy + 0.55, focus.z - dir.z * 3.4),
            target: Vec3::new(focus.x + dir.x * 22.0, cy + 0.1, focus.z + dir.z * 22.0),
            up: Vec3::new(0.0, 1.0, 0.0),
            fov_y: 1.25,
            near: 1.4,
            far: 400.0,
        }
    } else {
        // chase camera: outside and above the tunnel, looking along it, so forks read as forks
        Camera {
            position: Vec3::new(focus.x - dir.x * 7.5, cy + 3.9, focus.z - dir.z * 7.5),
            target: Vec3::new(focus.x + dir.x * 10.0, cy - 1.4, focus.z + dir.z * 10.0),
            up: Vec3::new(0.0, 1.0, 0.0),
            fov_y: 1.05,
            near: 0.6,
            far: 400.0,
        }
    };
    let proj = Proj::new(&cam, wd, hd);
    let use_fill = prm.depth != ColorDepth::Mono;
    let mut rz = Rasterizer::new(prm.cw, prm.ch * 2);
    rz.clear((5, 7, 13));
    rz.cull_backfaces = false;
    rz.fog = Some(Fog {
        color: (5, 7, 13),
        start: 6.0,
        end: lod.fwd as f32 * 1.25,
    });
    let mut pen = Pen {
        r: &mut r,
        p: &proj,
        segs: 0,
    };
    let (ahead, behind) = (lod.fwd, lod.back);
    let (wlo, whi) = if cyaw > 0.3 {
        (cz - behind as f32, cz + ahead as f32)
    } else if cyaw < -0.3 {
        (cz - ahead as f32, cz + behind as f32)
    } else {
        (cz - ahead as f32, cz + ahead as f32)
    };
    let lo = (wlo.floor() as i64).max(0) as u32;
    let hi = (wlo.max(whi).ceil() as u32).min(MAX_STEPS + 2);
    let fog = |_z: f32| -> f32 { 1.0 };
    r_fade(&mut pen, lod.fwd as f32 * 1.0);
    if lod.labels {
        let ctr = find(s, s.active)
            .map(|b| {
                let c = center(s, b, cz);
                (c.0, c.1)
            })
            .unwrap_or((0.0, 0.0));
        dust(
            &mut pen,
            cz,
            lod.fwd as f32,
            ctr,
            prm.depth == ColorDepth::Mono,
        );
    }

    // draw order: ghosts first (so lived geometry wins depth ties), then ancestors, then active
    let mut order: Vec<&BranchView> = s.branches.iter().collect();
    order.sort_by_key(|b| match b.role {
        Role::Ghost => 0,
        Role::Ancestor { .. } => 1,
        Role::Active => 2,
    });
    let mut drawn = 0u32;
    let mut ghosts = 0usize;
    for b in order {
        if b.role == Role::Ghost {
            ghosts += 1;
            if ghosts > lod.ghosts_max {
                continue;
            }
        }
        drawn += 1;
        let b0 = b.fork_at.max(lo);
        let b1 = b.end.min(hi);
        if b0 >= b1 && !(b.end >= lo && b.fork_at <= hi) {
            continue;
        }
        let bloom = s.bloom.filter(|(id, _)| *id == b.id).map(|(_, age)| age);
        for step in b0..b1 {
            let Some(rec) = b.rec(step) else { continue };
            let st = style_of(b.role, step, s.cursor);
            let z0 = step as f32;
            let z1 = step as f32 + 1.0;
            let c0 = center(s, b, z0);
            let c1 = center(s, b, z1);
            let f = fog(z0) * dim_for(st);
            let ep = epoch_rgb(rec.epoch);
            let e_s = {
                let g = |st: u32| b.rec(st).map(|r| r.energy).unwrap_or(rec.energy);
                (g(step.saturating_sub(1)) + 2.0 * rec.energy + g(step + 1)) * 0.25
            };
            let glow = 0.8 + 0.55 * (e_s * 7.0).clamp(0.0, 1.0);
            if use_fill && (st != Style3::Ghost || step % 3 != 1) {
                // the channel: two floor faces and two low walls, lit by the epoch colour
                let k = f * if st == Style3::Ghost { 0.55 } else { 1.0 } * glow;
                let fl = if st == Style3::Ghost {
                    desat(scale(ep, 0.5 * k), 0.6)
                } else {
                    scale(ep, 0.62 * k)
                };
                let wl = scale(fl, 0.5);
                let (v0, v1) = ((2usize, 3usize), (3usize, 4usize));
                for &(a, b) in &[v0, v1] {
                    let (p0, p1) = (rail_pos(c0, a, R, z0), rail_pos(c0, b, R, z0));
                    let (q0, q1) = (rail_pos(c1, a, R, z1), rail_pos(c1, b, R, z1));
                    tri(&mut rz, &cam, p0, p1, q0, fl, 0.55);
                    tri(&mut rz, &cam, p1, q1, q0, fl, 0.55);
                }
                for &(a, b) in &[(1usize, 2usize), (4usize, 5usize)] {
                    let (p0, p1) = (rail_pos(c0, a, R, z0), rail_pos(c0, b, R, z0));
                    let (q0, q1) = (rail_pos(c1, a, R, z1), rail_pos(c1, b, R, z1));
                    tri(&mut rz, &cam, p0, p1, q0, wl, 0.4);
                    tri(&mut rz, &cam, p1, q1, q0, wl, 0.4);
                }
            }
            // rails: one per task; state decides colour and dash. Healthy rails stay dim so the
            // anomalies (blocked / crashed) are what the eye finds.
            for rail in 0..6 {
                let tc = rec.tcode[rail];
                let (col, _) = task_state_color(tc, ep);
                let col = if tc <= 2 { scale(col, 0.42) } else { col };
                let dashed = matches!(tc, 3 | 4);
                if dashed && step % 2 == 1 {
                    continue;
                }
                if tc == 5 {
                    // crashed: broken rail, spark at the break
                    if step % 3 == 0 {
                        let a = rail_pos(c0, rail, R, z0);
                        pen.seg(
                            a,
                            rail_pos(c0, rail, R + 0.35, z0 + 0.3),
                            scale([255, 60, 60], f * 1.2),
                        );
                    }
                    continue;
                }
                if !stipple(st, step, rail as u32) {
                    continue;
                }
                pen.seg(
                    rail_pos(c0, rail, R, z0),
                    rail_pos(c1, rail, R, z1),
                    scale(col, f),
                );
            }
            // the epoch ribbon: a three-line floor band, glowing with the step's audio energy
            if stipple(st, step, 0) {
                let fl = scale(ep, f * glow);
                for k in [-1.0f32, 0.0, 1.0] {
                    let a = polar(c0, -std::f32::consts::FRAC_PI_2, R * 0.0, z0);
                    let b = polar(c1, -std::f32::consts::FRAC_PI_2, R * 0.0, z1);
                    let dx = k * R * 0.42;
                    pen.seg(
                        Vec3::new(a.x + dx, a.y - R * 0.86, a.z),
                        Vec3::new(b.x + dx, b.y - R * 0.86, b.z),
                        fl,
                    );
                }
            }
            // rings: dense near the cursor, sparse far away
            let near = (z0 - cz).abs() < 12.0;
            let ring_gap = if near {
                lod.ring_every
            } else {
                lod.ring_every * 2
            };
            if step % ring_gap == 0 && (st != Style3::Ghost || step % (ring_gap * 2) == 0) {
                pen.poly_ring(c0, R, z0, scale(ep, f * glow * 0.9));
            }
            // checkpoints: structural plates every 32 steps
            if step % 32 == 0 && step > 0 {
                pen.poly_ring(c0, R * 1.22, z0, scale([235, 245, 255], f * 0.95));
                pen.poly_ring(c0, R * 1.34, z0, scale(ep, f * 0.7));
                for rail in (0..6).step_by(2) {
                    pen.seg(
                        rail_pos(c0, rail, R, z0),
                        rail_pos(c0, rail, R * 1.34, z0),
                        scale([235, 245, 255], f * 0.8),
                    );
                }
            }
            // variable threads: only near the cursor, where you can read them
            let thread_k = (1.0 - ((z0 - cz).abs() - 6.0).max(0.0) / 14.0).clamp(0.0, 1.0);
            for (ti, &v) in THREAD_ORDER.iter().enumerate().take(lod.threads) {
                if thread_k <= 0.05 || !stipple(st, step, ti as u32 * 2) {
                    continue;
                }
                let Some(next) = b.rec(step + 1).or(Some(rec)) else {
                    continue;
                };
                let n0 = norm_var(v, rec.vars[v as usize]);
                let n1 = norm_var(v, next.vars[v as usize]);
                let ang = THREAD_ANGLE[v as usize] + s.time * 0.0;
                let r0 = R * (0.22 + 0.66 * n0);
                let r1 = R * (0.22 + 0.66 * n1);
                let mut col = VAR_COLORS[v as usize];
                if v == V_HEAT {
                    col = mix([255, 120, 50], [255, 245, 220], n0 * n0);
                }
                pen.seg(
                    polar(c0, ang, r0, z0),
                    polar(c1, ang, r1, z1),
                    scale(col, f * 1.1 * thread_k),
                );
            }
            // external input needle and decision markers
            if rec.flags & F_INPUT != 0 && st != Style3::Ghost {
                let a = polar(c0, 0.3, 3.4, z0);
                let t = rail_pos(c0, T_OPERATOR as usize, R, z0);
                pen.seg(a, t, scale([200, 255, 210], f * 1.3));
                pen.poly_ring(c0, R * 0.62, z0, scale([200, 255, 210], f * 1.2));
            }
            if rec.flags & F_OVERRIDDEN != 0 {
                // the intervention: a magenta diamond just outside the deciding rail
                let t = rec.task as usize % 6;
                let k = rail_pos(c0, t, R * 1.5, z0);
                let (d1, d2) = (Vec3::new(0.28, 0.0, 0.0), Vec3::new(0.0, 0.28, 0.0));
                let col = scale([255, 80, 230], f * 1.5);
                pen.seg(
                    Vec3::new(k.x + d1.x, k.y, k.z),
                    Vec3::new(k.x, k.y + d2.y, k.z),
                    col,
                );
                pen.seg(
                    Vec3::new(k.x, k.y + d2.y, k.z),
                    Vec3::new(k.x - d1.x, k.y, k.z),
                    col,
                );
                pen.seg(
                    Vec3::new(k.x - d1.x, k.y, k.z),
                    Vec3::new(k.x, k.y - d2.y, k.z),
                    col,
                );
                pen.seg(
                    Vec3::new(k.x, k.y - d2.y, k.z),
                    Vec3::new(k.x + d1.x, k.y, k.z),
                    col,
                );
            } else if rec.class == Class::Fate && rec.vars[0] >= 0 && st != Style3::Ghost {
                // fate points: small yellow tick on the rail (a place you could fork)
                let t = rec.task as usize % 6;
                let rare = rec.aux > 0 && rec.flags & F_OVERRIDDEN == 0;
                if rare {
                    pen.seg(
                        rail_pos(c0, t, R, z0),
                        rail_pos(c0, t, R * 1.22, z0),
                        scale([255, 235, 120], f),
                    );
                }
            }
            // deadlock braid: the wait cycle drawn across the cross-section
            if rec.dead && st != Style3::Ghost && step % 2 == 0 {
                {
                    let (a, bb) = (T_REACTOR as usize, T_COOLER as usize);
                    pen.seg(
                        rail_pos(c0, a, R, z0),
                        rail_pos(c0, bb, R, z0),
                        scale([200, 140, 255], f * 1.2),
                    );
                    pen.seg(
                        rail_pos(c0, a, R * 0.5, z0),
                        rail_pos(c0, bb, R * 0.5, z0 + 1.0),
                        scale([160, 100, 255], f),
                    );
                }
            }
        }
        // the end of the branch: catastrophe scar or terminal cap
        if b.end >= lo && b.end <= hi {
            let ce = center(s, b, b.end as f32);
            let z = b.end as f32;
            let st = style_of(b.role, b.end.saturating_sub(1), s.cursor);
            let f = fog(z) * if st == Style3::Ghost { 0.75 } else { 1.0 };
            match b.terminal {
                Some(Terminal::Meltdown) => {
                    pen.star(ce, z, 2.8, 14, scale([255, 60, 40], f * 1.3));
                    pen.star(ce, z + 1.5, 1.8, 10, scale([255, 170, 70], f * 1.1));
                    pen.poly_ring(ce, R * 1.7, z, scale([255, 60, 40], f));
                    pen.poly_ring(ce, R * 2.4, z + 1.0, scale([160, 30, 30], f));
                }
                Some(_) => {
                    pen.poly_ring(ce, R * 1.1, z, scale([200, 220, 255], f));
                    pen.poly_ring(ce, R * 0.6, z, scale([200, 220, 255], f * 0.8));
                }
                None => {}
            }
        }
        // fork bloom: the birth of this branch
        if let Some(age) = bloom {
            if b.fork_at >= lo.saturating_sub(10) && b.fork_at <= hi && age < 1.6 {
                let c = center(s, b, b.fork_at as f32);
                let k = (1.0 - age / 1.6).max(0.0);
                for i in 0..3 {
                    let rr = R * (1.2 + age * 2.4 + i as f32 * 0.4);
                    pen.poly_ring(
                        c,
                        rr,
                        b.fork_at as f32 + i as f32,
                        scale([255, 230, 255], k * (1.0 - i as f32 * 0.25)),
                    );
                }
            }
        }
    }

    // causal cross-links (messages, lock hand-offs, restarts, steals) in the visible window
    let mut links = 0usize;
    for b in s.branches.iter().filter(|b| !matches!(b.role, Role::Ghost)) {
        for step in b.fork_at.max(lo)..b.end.min(hi) {
            if links >= lod.links {
                break;
            }
            let Some(rec) = b.rec(step) else { continue };
            let (kind, parent) = match rec.class {
                Class::Recv if rec.parents[1] != NONE && rec.parents[1] & 0x8000_0000 == 0 => {
                    (0u8, rec.parents[1])
                }
                Class::Restart if rec.parents[1] != NONE => (2, rec.parents[1]),
                Class::LockSteal if rec.parents[1] != NONE => (3, rec.parents[1]),
                _ if rec.parents[2] != NONE && rec.parents[2] & 0x8000_0000 == 0 => {
                    (1, rec.parents[2])
                }
                _ => continue,
            };
            let Some(prec) = lookup_any(s, b, parent) else {
                continue;
            };
            if prec.task == rec.task || prec.task as usize >= 6 || rec.task as usize >= 6 {
                continue;
            }
            let st = style_of(b.role, step, s.cursor);
            let f = fog(step as f32) * dim_for(st);
            let col = match kind {
                0 => [90, 230, 255],
                1 => [190, 130, 255],
                2 => [110, 255, 150],
                _ => [255, 170, 60],
            };
            let c0 = center(s, b, step as f32);
            let cp = center(s, b, parent as f32);
            pen.seg(
                rail_pos(cp, prec.task as usize, R, parent as f32),
                rail_pos(c0, rec.task as usize, R, step as f32),
                scale(col, f * 1.1),
            );
            links += 1;
        }
    }
    // highlighted ancestry of the inspected event
    if let Some(b) = find(s, s.inspect_branch) {
        for w in s.ancestry.windows(1) {
            let (st, _d) = w[0];
            if let Some(rec) = b.rec(st).or_else(|| lookup_any(s, b, st)) {
                if rec.task < 6 {
                    let c = center(s, b, st as f32);
                    pen.poly_ring(c, R * 0.35, st as f32, [255, 255, 255]);
                }
            }
        }
        for pair in s.ancestry.iter().zip(s.ancestry.iter().skip(1)) {
            let (a, _) = *pair.0;
            let (p, _) = *pair.1;
            if let (Some(ra), Some(rp)) = (lookup_any(s, b, a), lookup_any(s, b, p)) {
                if ra.task < 6 && rp.task < 6 {
                    pen.seg(
                        rail_pos(center(s, b, p as f32), rp.task as usize, R * 0.8, p as f32),
                        rail_pos(center(s, b, a as f32), ra.task as usize, R * 0.8, a as f32),
                        [255, 255, 255],
                    );
                }
            }
        }
    }

    // the now-plane: spokes and bright ring at the cursor
    if let Some(b) = find(s, s.active) {
        let zc = s.cursor as f32;
        let c = center(s, b, zc);
        for k in 0..2 {
            pen.poly_ring(c, R * (1.04 + 0.05 * k as f32), zc, [255, 255, 255]);
        }
        for rail in 0..6 {
            pen.seg(
                rail_pos(c, rail, R, zc),
                polar(c, 0.0, 0.0, zc),
                scale([255, 255, 255], 0.55),
            );
        }
    }

    let segs = pen.segs;
    let plotted = r.plotted;
    let bg = if prm.depth == ColorDepth::Mono {
        Color::Reset
    } else {
        Color::Rgb(5, 7, 13)
    };
    let mut surface = compose_layers(
        if use_fill { Some(&rz.raster) } else { None },
        &r,
        prm.glyphs,
        prm.depth,
        bg,
    );
    if lod.labels {
        if let Some(b) = find(s, s.active) {
            let zc = s.cursor as f32;
            let c = center(s, b, zc);
            #[allow(clippy::needless_range_loop)]
            for rail in 0..6 {
                let p = rail_pos(c, rail, R * 1.45, zc);
                if let Some((sx, sy, _)) = proj.point(p) {
                    let cx = (sx / 2.0).round() as i32;
                    let cyy = (sy / 4.0).round() as i32;
                    let name = TASK_NAMES[rail];
                    let tc = b
                        .rec(s.cursor.saturating_sub(1))
                        .map(|r| r.tcode[rail])
                        .unwrap_or(0);
                    let col = match tc {
                        5 => Color::Rgb(255, 80, 80),
                        4 => Color::Rgb(200, 150, 255),
                        3 => Color::Rgb(255, 210, 90),
                        _ => Color::Rgb(190, 205, 225),
                    };
                    put_text(
                        &mut surface,
                        cx - 1,
                        cyy,
                        &name[..3.min(name.len())],
                        col,
                        prm.depth,
                    );
                }
            }
        }
    }
    Frame {
        surface,
        stats: FrameStats {
            segments: segs,
            plotted,
            branches_drawn: drawn,
        },
    }
}

fn put_text(surface: &mut Surface, x: i32, y: i32, text: &str, col: Color, depth: ColorDepth) {
    if y < 0 || y >= surface.height as i32 {
        return;
    }
    for (k, chr) in text.chars().enumerate() {
        let xx = x + k as i32;
        if xx >= 0 && xx < surface.width as i32 {
            let st = if depth == ColorDepth::Mono {
                Style::new().bold()
            } else {
                Style::new().fg(col).bold()
            };
            surface.set_cell(xx as u16, y as u16, Cell::new(Glyph::from_char(chr), st));
        }
    }
}

fn r_fade(pen: &mut Pen, far: f32) {
    pen.r.fade_far = far.max(10.0);
}

fn lookup_any<'a>(s: &'a Snapshot, from: &'a BranchView, step: u32) -> Option<&'a RecView> {
    // walk up the lineage until a branch owns `step`
    let mut cur = Some(from);
    while let Some(b) = cur {
        if step >= b.fork_at {
            return b.rec(step);
        }
        cur = b.parent.and_then(|p| find(s, p));
    }
    None
}

// --- compare braid -----------------------------------------------------------------------

/// The dimensions shown in the braid, in lane order (left to right).
pub fn braid_dims(max: usize) -> Vec<usize> {
    let all = [
        V_HEAT as usize,
        V_LOAD as usize,
        V_COOL as usize,
        V_CREDITS as usize,
        V_ESC as usize,
        V_MODE as usize,
        NVARS + T_REACTOR as usize,
        NVARS + T_COOLER as usize,
        NVARS + T_SUPERVISOR as usize,
        NVARS + MAX_TASKS + L_FUEL as usize,
        NVARS + MAX_TASKS + L_COOLANT as usize,
    ];
    all.iter().copied().take(max).collect()
}

fn dim_value(rec: &RecView, dim: usize) -> f32 {
    match dim_kind(dim) {
        DimKind::Var(v) => norm_var(v, rec.vars[v as usize]),
        DimKind::Task(t) => rec.tcode[t as usize] as f32 / 6.0,
        DimKind::Lock(l) => (rec.locks[l as usize] as f32) / 6.0,
        DimKind::Chan(_) => 0.0,
    }
}

/// The compare braid: same camera as navigation, but the lanes are *dimensions*.
pub fn render_compare(s: &Snapshot, prm: Params) -> Frame {
    let lod = lod_for(prm.cw, prm.ch);
    let mut r = SubRaster::new(prm.cw, prm.ch);
    let (wd, hd) = r.dots();
    let cmp = s.compare.as_ref();
    let (cz, _, _) = s.cam;
    let dims = braid_dims(match lod.threads {
        0..=2 => 5,
        3..=4 => 8,
        _ => 11,
    });
    let nd = dims.len() as f32;
    let spacing = 0.52f32;
    let (sy, cyaw) = (s.yaw.sin(), s.yaw.cos());
    let dir = Vec3::new(sy, 0.0, cyaw);
    let cam = Camera {
        position: Vec3::new(-dir.x * 6.0, 3.6, cz - dir.z * 6.0),
        target: Vec3::new(dir.x * 11.0, 0.3, cz + dir.z * 11.0),
        up: Vec3::new(0.0, 1.0, 0.0),
        fov_y: 1.05,
        near: 0.5,
        far: 400.0,
    };
    let proj = Proj::new(&cam, wd, hd);
    let use_fill = prm.depth != ColorDepth::Mono;
    let mut rz = Rasterizer::new(prm.cw, prm.ch * 2);
    rz.clear((5, 7, 13));
    rz.cull_backfaces = false;
    rz.fog = Some(Fog {
        color: (5, 7, 13),
        start: 5.0,
        end: lod.fwd as f32 * 1.2,
    });
    let mut pen = Pen {
        r: &mut r,
        p: &proj,
        segs: 0,
    };
    pen.r.fade_far = lod.fwd as f32;
    let (wlo, whi) = if cyaw >= -0.3 {
        (cz - lod.back as f32, cz + lod.fwd as f32)
    } else {
        (cz - lod.fwd as f32, cz + lod.back as f32)
    };
    let lo = (wlo.floor() as i64).max(0) as u32;
    let hi = (whi.ceil() as u32).min(MAX_STEPS + 2);
    let fog = |z: f32| -> f32 {
        (1.0 - ((z - cz).abs() / (lod.fwd as f32 * 1.15)).clamp(0.0, 1.0) * 0.8).max(0.15)
    };
    let (ba, bb) = match cmp {
        Some(c) => (find(s, c.a), find(s, c.b)),
        None => (None, None),
    };
    let (Some(ba), Some(bb), Some(cmp)) = (ba, bb, cmp) else {
        let surface = r.to_surface(prm.glyphs, prm.depth, Color::Reset, false);
        return Frame {
            surface,
            stats: FrameStats::default(),
        };
    };
    let col_a: Rgb = [70, 215, 255];
    let col_b: Rgb = [255, 150, 70];
    let base_y = |d: f32| -> f32 { d * 1.7 };
    let x_of = |i: usize| -> f32 { (i as f32 - (nd - 1.0) * 0.5) * spacing };

    // floor grid: lane rails (dimension axes) so convergence reads as "merged onto the rail"
    for (i, _) in dims.iter().enumerate() {
        let x = x_of(i);
        pen.seg(
            Vec3::new(x, 0.0, lo as f32),
            Vec3::new(x, 0.0, hi as f32),
            [28, 38, 56],
        );
    }
    let mut roots: Vec<(u32, RootKind, usize)> = vec![];
    for step in lo..hi {
        let (Some(ra), Some(rb)) = (lookup_any(s, ba, step), lookup_any(s, bb, step)) else {
            continue;
        };
        let na = lookup_any(s, ba, step + 1).unwrap_or(ra);
        let nb = lookup_any(s, bb, step + 1).unwrap_or(rb);
        let Some(row) = cmp.rows.get(step as usize) else {
            continue;
        };
        let z0 = step as f32;
        let z1 = z0 + 1.0;
        let lived = step < s.cursor;
        let f = fog(z0) * if lived { 1.0 } else { 0.6 };
        if use_fill {
            for (i, &d) in dims.iter().enumerate() {
                let rel = row.1[d];
                let col: Rgb = match rel {
                    Rel::Same => [26, 36, 52],
                    Rel::Equiv => [34, 150, 96],
                    Rel::Apart => [210, 46, 70],
                };
                let k = f * if lived { 1.0 } else { 0.7 };
                let (xa, xb) = (x_of(i) - spacing * 0.42, x_of(i) + spacing * 0.42);
                let (a, b, c2, d2) = (
                    Vec3::new(xa, 0.0, z0),
                    Vec3::new(xb, 0.0, z0),
                    Vec3::new(xa, 0.0, z1),
                    Vec3::new(xb, 0.0, z1),
                );
                tri(&mut rz, &cam, a, b, c2, scale(col, k), 0.5);
                tri(&mut rz, &cam, b, d2, c2, scale(col, k), 0.5);
            }
        }
        // whole-state verdict ribbon along the floor edge
        let vcol: Rgb = match row.0 {
            Verdict::Identical => [235, 250, 255],
            Verdict::ComputationallyEqual => [150, 230, 255],
            Verdict::Equivalent => [120, 255, 170],
            Verdict::Divergent => [255, 70, 90],
        };
        pen.seg(
            Vec3::new(-nd * spacing * 0.5 - 0.5, 0.0, z0),
            Vec3::new(-nd * spacing * 0.5 - 0.5, 0.0, z1),
            scale(vcol, f),
        );
        pen.seg(
            Vec3::new(nd * spacing * 0.5 + 0.5, 0.0, z0),
            Vec3::new(nd * spacing * 0.5 + 0.5, 0.0, z1),
            scale(vcol, f),
        );
        if step % 4 == 0 {
            pen.seg(
                Vec3::new(-nd * spacing * 0.5 - 0.5, 0.0, z0),
                Vec3::new(nd * spacing * 0.5 + 0.5, 0.0, z0),
                scale(vcol, f * 0.35),
            );
        }
        for (i, &d) in dims.iter().enumerate() {
            let rel = row.1[d];
            let x = x_of(i);
            let (va0, va1) = (dim_value(ra, d), dim_value(na, d));
            let (vb0, vb1) = (dim_value(rb, d), dim_value(nb, d));
            // separation: fused when identical, close when equivalent, far when divergent
            let sp = s.spread.max(0.0);
            let sep0 = sp
                * match rel {
                    Rel::Same => 0.0,
                    Rel::Equiv => 0.09,
                    Rel::Apart => 0.14 + 0.5 * (va0 - vb0).abs().min(1.0),
                };
            let next_rel = cmp
                .rows
                .get(step as usize + 1)
                .map(|r| r.1[d])
                .unwrap_or(rel);
            let sep1 = sp
                * match next_rel {
                    Rel::Same => 0.0,
                    Rel::Equiv => 0.09,
                    Rel::Apart => 0.14 + 0.5 * (va1 - vb1).abs().min(1.0),
                };
            let (a0, a1) = (
                Vec3::new(x - sep0, base_y(va0), z0),
                Vec3::new(x - sep1, base_y(va1), z1),
            );
            let (b0, b1) = (
                Vec3::new(x + sep0, base_y(vb0), z0),
                Vec3::new(x + sep1, base_y(vb1), z1),
            );
            if rel == Rel::Same && next_rel == Rel::Same {
                // fused: one white-ish thread
                pen.seg(a0, a1, scale([225, 235, 245], f * 0.9));
            } else {
                pen.seg(a0, a1, scale(col_a, f));
                pen.seg(b0, b1, scale(col_b, f));
                if rel == Rel::Equiv {
                    // "≈": weld ticks between the two near-parallel threads
                    if step % 2 == 0 {
                        pen.seg(a0, b0, scale([120, 255, 170], f * 0.8));
                    }
                }
            }
        }
        if let Some(rk) = row.2 {
            let dim_hit = dims
                .iter()
                .position(|&d| matches!(row.1[d], Rel::Apart | Rel::Equiv))
                .unwrap_or(0);
            roots.push((step, rk, dim_hit));
        }
    }
    // causal roots glow: interventions are cross-section blooms, reorders are small sparks
    for (step, rk, di) in roots {
        let z = step as f32;
        let f = fog(z);
        match rk {
            RootKind::Intervention => {
                if use_fill {
                    let (x0, x1) = (-nd * spacing * 0.5 - 0.6, nd * spacing * 0.5 + 0.6);
                    let col = scale([255, 255, 255], f);
                    tri(
                        &mut rz,
                        &cam,
                        Vec3::new(x0, 0.0, z),
                        Vec3::new(x1, 0.0, z),
                        Vec3::new(x0, 3.2, z),
                        col,
                        0.9,
                    );
                    tri(
                        &mut rz,
                        &cam,
                        Vec3::new(x1, 0.0, z),
                        Vec3::new(x1, 3.2, z),
                        Vec3::new(x0, 3.2, z),
                        col,
                        0.9,
                    );
                }
                for k in 0..4 {
                    let rr = 0.5 + k as f32 * 0.55;
                    let x0 = -nd * spacing * 0.5 - rr;
                    let x1 = nd * spacing * 0.5 + rr;
                    let y1 = 2.2 + rr;
                    let col = scale([255, 255, 255], f * (1.0 - k as f32 * 0.18));
                    pen.seg(Vec3::new(x0, 0.0, z), Vec3::new(x0, y1, z), col);
                    pen.seg(Vec3::new(x1, 0.0, z), Vec3::new(x1, y1, z), col);
                    pen.seg(Vec3::new(x0, y1, z), Vec3::new(x1, y1, z), col);
                }
            }
            RootKind::Reorder => {
                let x = x_of(di.min(dims.len() - 1));
                pen.seg(
                    Vec3::new(x, 0.1, z),
                    Vec3::new(x, 1.6, z),
                    scale([255, 120, 255], f),
                );
            }
        }
    }
    // cursor plane
    let zc = s.cursor as f32;
    let (x0, x1) = (-nd * spacing * 0.5 - 0.7, nd * spacing * 0.5 + 0.7);
    pen.seg(
        Vec3::new(x0, 0.0, zc),
        Vec3::new(x1, 0.0, zc),
        [255, 255, 255],
    );
    pen.seg(
        Vec3::new(x0, 2.4, zc),
        Vec3::new(x1, 2.4, zc),
        [255, 255, 255],
    );
    pen.seg(
        Vec3::new(x0, 0.0, zc),
        Vec3::new(x0, 2.4, zc),
        [255, 255, 255],
    );
    pen.seg(
        Vec3::new(x1, 0.0, zc),
        Vec3::new(x1, 2.4, zc),
        [255, 255, 255],
    );

    let segs = pen.segs;
    let plotted = r.plotted;
    let bg = if prm.depth == ColorDepth::Mono {
        Color::Reset
    } else {
        Color::Rgb(5, 7, 13)
    };
    let mut surface = compose_layers(
        if use_fill { Some(&rz.raster) } else { None },
        &r,
        prm.glyphs,
        prm.depth,
        bg,
    );
    // lane labels projected at the cursor plane
    if lod.labels {
        for (i, &d) in dims.iter().enumerate() {
            if let Some((sx, sy, _)) = proj.point(Vec3::new(x_of(i), -0.15, zc - 0.5)) {
                let cx = (sx / 2.0).round() as i32 - 1;
                let cy = (sy / 4.0).round() as i32;
                let name = dim_short(d);
                if cy >= 0 && cy < prm.ch as i32 {
                    for (k, chr) in name.chars().take(3).enumerate() {
                        let x = cx + k as i32;
                        if x >= 0 && x < prm.cw as i32 {
                            let mut st = Style::new();
                            if prm.depth != ColorDepth::Mono {
                                st = st.fg(Color::Rgb(200, 210, 225));
                            }
                            surface.set_cell(
                                x as u16,
                                cy as u16,
                                Cell::new(Glyph::from_char(chr), st),
                            );
                        }
                    }
                }
            }
        }
    }
    Frame {
        surface,
        stats: FrameStats {
            segments: segs,
            plotted,
            branches_drawn: 2,
        },
    }
}
