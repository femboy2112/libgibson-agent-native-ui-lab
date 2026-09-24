//! CUEBOX: an external, deterministic rehearsal desk over the frozen public API.
use crate::ui::{self, App};
use gibson::focus::{FocusId, FocusRing};
use gibson::input::{KeyCode, KeyEvent, KeyModifiers};
use gibson::scene::{Presentation, Scene, SceneEntity, SceneId};
use gibson::story::{Beat, Condition, Story, StoryAction};
use gibson::{BorderType, Cell, Color, ColorDepth, Node, Rect, Style, Surface};
use std::time::Duration;

pub const TICK_MS: u64 = 50;
pub const MAX_TICKS: u16 = 900;
pub const END_MS: u64 = 45_000;
pub const CUE_TICKS: u16 = 75;
pub const MODAL_FOCUS: FocusId = FocusId(900);
pub const PERFORMER_IDS: [FocusId; 8] = [
    FocusId(101),
    FocusId(102),
    FocusId(103),
    FocusId(104),
    FocusId(105),
    FocusId(106),
    FocusId(107),
    FocusId(108),
];
pub const NAMES: [&str; 8] = ["Ada", "Bram", "Cora", "Dax", "Eli", "Faye", "Gus", "Hana"];
const TITLES: [&str; 12] = [
    "Preset",
    "Ada crosses",
    "Bram answers",
    "Stand by entrance",
    "Hana enters",
    "Dax turns",
    "Company opens",
    "Faye crosses",
    "Gus joins",
    "Lanterns rise",
    "Final tableau",
    "Curtain",
];
const STARTS: [Point; 8] = [
    Point(12, 15),
    Point(85, 15),
    Point(12, 40),
    Point(85, 40),
    Point(12, 65),
    Point(85, 65),
    Point(40, 88),
    Point(94, 88),
];
const ENDS: [Point; 8] = [
    Point(30, 15),
    Point(68, 15),
    Point(30, 44),
    Point(70, 44),
    Point(30, 75),
    Point(70, 75),
    Point(42, 82),
    Point(58, 82),
];
const MOVE_CUES: [u16; 8] = [1, 2, 3, 5, 6, 7, 8, 4];
const LIGHT_COLORS: [Color; 4] = [
    Color::Rgb(233, 181, 96),
    Color::Rgb(113, 156, 239),
    Color::Rgb(192, 117, 214),
    Color::Rgb(99, 199, 173),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Branch {
    OnTime,
    Delayed,
}
impl Branch {
    pub fn label(self) -> &'static str {
        match self {
            Self::OnTime => "ON TIME",
            Self::Delayed => "DELAYED",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point(pub u16, pub u16);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Performer {
    pub id: FocusId,
    pub name: &'static str,
    pub position: Point,
    pub origin: Point,
    pub destination: Point,
    pub moving: bool,
    pub in_wing: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rehearsal {
    pub tick: u16,
    pub cue: usize,
    pub cue_id: String,
    pub title: String,
    pub branch: Branch,
    pub performers: [Performer; 8],
    pub lights: [bool; 4],
    pub entered: Vec<String>,
    pub update_count: usize,
}

/// Twelve cue slots; cue 5 has two graph nodes, followed by reconvergence.
/// Branch selection is explicit consumer state, installed as a Story entry fact.
pub fn rehearsal_story(branch: Branch) -> Story {
    let mut story = Story::new("cue01");
    for (cue, title) in TITLES.iter().enumerate() {
        if cue == 4 {
            continue;
        }
        let id = format!("cue{:02}", cue + 1);
        let mut beat = Beat::new(id)
            .label(*title)
            .on_enter(StoryAction::set_number("cue", cue as f32));
        if cue == 0 {
            beat = beat.on_enter(StoryAction::set_bool("delayed", branch == Branch::Delayed));
        }
        if cue == 3 {
            beat = beat
                .min_duration(Duration::from_millis(3750))
                .transition(Condition::fact_true("delayed"), "cue05-delay")
                .after(Duration::from_millis(3750), "cue05-on");
        } else if cue < 11 {
            beat = beat.after(Duration::from_millis(3750), format!("cue{:02}", cue + 2));
        }
        story = story.beat(beat);
    }
    for (id, label) in [
        ("cue05-on", "Hana enters"),
        ("cue05-delay", "Hold Hana in wing"),
    ] {
        story = story.beat(
            Beat::new(id)
                .label(label)
                .on_enter(StoryAction::set_number("cue", 4.0))
                .after(Duration::from_millis(3750), "cue06"),
        );
    }
    story
}

fn position(start: Point, end: Point, elapsed: u16) -> Point {
    let lerp = |a: u16, b: u16| -> u16 {
        (i32::from(a)
            + (i32::from(b) - i32::from(a)) * i32::from(elapsed.min(CUE_TICKS))
                / i32::from(CUE_TICKS)) as u16
    };
    Point(lerp(start.0, end.0), lerp(start.1, end.1))
}

/// Pure sampling on a declared 50ms grid; no frame cadence enters the model.
pub fn sample(tick: u16, branch: Branch) -> Rehearsal {
    let tick = tick.min(MAX_TICKS);
    let mut director = rehearsal_story(branch).start();
    for _ in 0..tick {
        director.update(Duration::from_millis(TICK_MS), &[]);
    }
    let cue = director.facts().number("cue") as usize;
    let performers = std::array::from_fn(|i| {
        let move_cue = if i == 7 && branch == Branch::Delayed {
            6
        } else {
            MOVE_CUES[i]
        };
        let begin = move_cue * CUE_TICKS;
        let elapsed = tick.saturating_sub(begin);
        Performer {
            id: PERFORMER_IDS[i],
            name: NAMES[i],
            position: position(STARTS[i], ENDS[i], elapsed),
            origin: STARTS[i],
            destination: ENDS[i],
            moving: tick >= begin && elapsed < CUE_TICKS,
            in_wing: i == 7 && tick <= begin,
        }
    });
    let lights = match cue {
        0 => [true, false, false, false],
        1..=2 => [true, true, false, false],
        3..=5 => [true, true, false, branch == Branch::OnTime],
        6..=8 => [true, true, true, true],
        9..=10 => [false, true, true, true],
        _ => [false, false, false, false],
    };
    Rehearsal {
        tick,
        cue,
        cue_id: director.current_beat().to_string(),
        title: director.beat_label().to_string(),
        branch,
        performers,
        lights,
        entered: director
            .trace()
            .beat_sequence()
            .iter()
            .map(|s| s.to_string())
            .collect(),
        update_count: director.trace().steps.len(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cuebox {
    focus: FocusRing,
    selected: usize,
    modal: bool,
    playing: bool,
    anchor_tick: u16,
    anchor_ms: u64,
    last_key_ms: u64,
    branch: Branch,
}
impl Default for Cuebox {
    fn default() -> Self {
        Self {
            focus: FocusRing::new(PERFORMER_IDS.into_iter().chain([MODAL_FOCUS])),
            selected: 0,
            modal: false,
            playing: true,
            anchor_tick: 0,
            anchor_ms: 0,
            last_key_ms: 0,
            branch: Branch::OnTime,
        }
    }
}
impl Cuebox {
    pub fn tick_at(&self, at_ms: u64) -> u16 {
        let elapsed = if self.playing {
            at_ms.min(END_MS).saturating_sub(self.anchor_ms) / TICK_MS
        } else {
            0
        };
        u64::from(self.anchor_tick)
            .saturating_add(elapsed)
            .min(u64::from(MAX_TICKS)) as u16
    }
    pub fn rehearsal(&self, at_ms: u64) -> Rehearsal {
        sample(self.tick_at(at_ms), self.branch)
    }
    pub fn focused(&self) -> Option<FocusId> {
        self.focus.current()
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn modal_open(&self) -> bool {
        self.modal
    }
    pub fn focus_captured(&self) -> bool {
        self.focus.is_captured()
    }
    pub fn playing(&self) -> bool {
        self.playing
    }
    pub fn branch(&self) -> Branch {
        self.branch
    }
    fn select(&mut self, index: usize) {
        self.selected = index % 8;
        self.focus.set(PERFORMER_IDS[self.selected]);
    }
    fn seek(&mut self, at_ms: u64, tick: u16) {
        self.anchor_tick = tick.min(MAX_TICKS);
        self.anchor_ms = at_ms;
    }
    pub fn stage_scene(&self, state: &Rehearsal, width: u16, height: u16) -> Scene {
        let mut scene = Scene::new();
        for (i, actor) in state.performers.iter().enumerate() {
            let (x, y) = map_point(actor.position, width, height);
            let token = if i == self.selected {
                format!("<{}>", (b'A' + i as u8) as char)
            } else {
                format!("[{}]", (b'A' + i as u8) as char)
            };
            let mut style = Style::new()
                .fg(Color::BrightWhite)
                .bg(Color::Rgb(18, 24, 35))
                .bold();
            if i == self.selected {
                style = style.reverse();
            }
            scene.add(
                SceneEntity::new(actor.name, Node::text(token, style).width(3.0).height(1.0))
                    .tag("performer")
                    .z(10)
                    .offset(i32::from(x), i32::from(y)),
            );
        }
        scene
    }
    pub fn scene_ids(&self, state: &Rehearsal, width: u16, height: u16) -> [SceneId; 8] {
        let scene = self.stage_scene(state, width, height);
        std::array::from_fn(|i| scene.id_of(NAMES[i]).expect("fixed performer roster"))
    }
}

impl App for Cuebox {
    fn end_ms(&self) -> u64 {
        END_MS
    }
    fn key(&mut self, at_ms: u64, key: KeyEvent) {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return;
        }
        let at_ms = at_ms.min(END_MS).max(self.last_key_ms);
        self.last_key_ms = at_ms;
        if self.modal {
            if matches!(key.code, KeyCode::Enter | KeyCode::Char('i')) {
                self.modal = false;
                self.focus.release();
            }
            return;
        }
        let tick = self.tick_at(at_ms);
        match key.code {
            KeyCode::Tab | KeyCode::Down => self.select(self.selected + 1),
            KeyCode::BackTab | KeyCode::Up => self.select((self.selected + 7) % 8),
            KeyCode::Char(c @ '1'..='8') => self.select((c as u8 - b'1') as usize),
            KeyCode::Enter | KeyCode::Char('i') => {
                self.focus.capture();
                self.focus.set(MODAL_FOCUS);
                self.modal = true;
            }
            KeyCode::Char('p') => {
                self.seek(at_ms, tick);
                self.playing = !self.playing;
            }
            KeyCode::Char('n') | KeyCode::Right => {
                self.seek(at_ms, (tick / CUE_TICKS + 1) * CUE_TICKS);
                self.playing = false;
            }
            KeyCode::Left => {
                self.seek(at_ms, (tick.saturating_sub(1) / CUE_TICKS) * CUE_TICKS);
                self.playing = false;
            }
            KeyCode::Char('d') if tick < 4 * CUE_TICKS => self.branch = Branch::Delayed,
            KeyCode::Char('o') if tick < 4 * CUE_TICKS => self.branch = Branch::OnTime,
            KeyCode::Char('r') => {
                self.seek(at_ms, 0);
                self.branch = Branch::OnTime;
                self.playing = false;
            }
            _ => {}
        }
    }
    fn frame(&self, at_ms: u64, w: u16, h: u16, depth: ColorDepth) -> Surface {
        let (w, h) = ui::dimensions(w, h);
        let mut out = Surface::new(w, h);
        let base = Style::new()
            .fg(Color::Rgb(199, 211, 221))
            .bg(Color::Rgb(12, 17, 25));
        let hot = Style::new().fg(Color::Rgb(241, 189, 99)).bold();
        out.fill_rect(out.area(), Cell::space(base));
        if w < 30 || h < 14 {
            ui::put(&mut out, 0, 0, "CUEBOX - enlarge to 56x24", hot);
            return ui::finish(out, depth);
        }
        let state = self.rehearsal(at_ms);
        ui::put(&mut out, 1, 0, "CUEBOX / THE LANTERN COMPANY", hot);
        ui::put(
            &mut out,
            1,
            1,
            &format!(
                "{}  {:02}.{:01}s /45  Q{:02}  {}",
                if self.playing && state.tick < MAX_TICKS {
                    "PLAY"
                } else {
                    "HOLD"
                },
                state.tick / 20,
                (state.tick % 20) / 2,
                state.cue + 1,
                state.branch.label()
            ),
            base,
        );
        let wide = w >= 100;
        let stage_w = if wide { w - 34 } else { w };
        let stage_h = h - if wide { 7 } else { 11 };
        let stage = Rect::new(0, 2, stage_w, stage_h);
        draw_stage(self, &mut out, stage, &state, depth);
        let strip_y = stage.y + stage.height;
        draw_cue_strip(&mut out, Rect::new(0, strip_y, w, 3), &state, hot, base);
        let inspector = if wide {
            Rect::new(stage_w, 2, 34, stage_h)
        } else {
            Rect::new(0, strip_y + 3, w, 4)
        };
        draw_inspector(&mut out, inspector, &state, self.selected, wide, hot, base);
        ui::put(
            &mut out,
            1,
            h - 2,
            "Tab/1-8 actor  Enter inspect  p play  n step",
            base,
        );
        ui::put(
            &mut out,
            1,
            h - 1,
            "d delay / o on-time before Q05  r reset  <- back",
            base,
        );
        if self.modal {
            draw_modal(&mut out, &state, self.selected, hot, base);
        }
        ui::finish(out, depth)
    }
}

fn map_point(point: Point, width: u16, height: u16) -> (u16, u16) {
    (
        2 + point.0 * width.saturating_sub(7) / 100,
        2 + point.1 * height.saturating_sub(5) / 100,
    )
}
fn line(surface: &mut Surface, from: (u16, u16), to: (u16, u16), style: Style) {
    let dx = i32::from(to.0) - i32::from(from.0);
    let dy = i32::from(to.1) - i32::from(from.1);
    let count = dx.unsigned_abs().max(dy.unsigned_abs()).max(1) as i32;
    for step in 0..=count {
        let x = i32::from(from.0) + dx * step / count;
        let y = i32::from(from.1) + dy * step / count;
        ui::put(surface, x as u16, y as u16, "+", style);
    }
}
fn draw_stage(app: &Cuebox, out: &mut Surface, rect: Rect, state: &Rehearsal, depth: ColorDepth) {
    let mut floor = Surface::new(rect.width, rect.height);
    let patterns = ["/", "\\", ".", ":"];
    for y in 1..rect.height.saturating_sub(1) {
        for x in 1..rect.width.saturating_sub(1) {
            let region = usize::from(x >= rect.width / 2) + 2 * usize::from(y >= rect.height / 2);
            let active = state.lights[region];
            let glyph = if active && (u32::from(x) + u32::from(y)) % 3 == 0 {
                patterns[region]
            } else {
                " "
            };
            let mut style = Style::new().fg(LIGHT_COLORS[region]).bg(if active {
                Color::Rgb(28, 34, 48)
            } else {
                Color::Rgb(16, 20, 29)
            });
            if depth == ColorDepth::Mono {
                style = style.dim();
            }
            ui::put(&mut floor, x, y, glyph, style);
        }
    }
    let rim = Style::new().fg(Color::Rgb(110, 128, 145));
    floor.draw_border(floor.area(), BorderType::Rounded, rim);
    ui::put(
        &mut floor,
        2,
        0,
        &format!(" STAGE / Q{:02} ", state.cue + 1),
        rim.bold(),
    );
    let actor = &state.performers[app.selected];
    line(
        &mut floor,
        map_point(actor.origin, rect.width, rect.height),
        map_point(actor.destination, rect.width, rect.height),
        Style::new().fg(Color::Yellow),
    );
    let (tx, ty) = map_point(Point(46, 27), rect.width, rect.height);
    ui::put(&mut floor, tx, ty, "###", rim.bold());
    let (rx, ry) = map_point(Point(46, 59), rect.width, rect.height);
    ui::put(&mut floor, rx, ry, "===", rim.bold());
    let light_status: String = state
        .lights
        .iter()
        .enumerate()
        .map(|(i, on)| format!("L{}{}{} ", i + 1, patterns[i], if *on { "+" } else { "-" }))
        .collect();
    ui::put(&mut floor, 2, 1, &light_status, rim);
    ui::put(
        &mut floor,
        2,
        rect.height - 1,
        " AUDIENCE / + path  # throne  = rostrum ",
        rim,
    );
    let mut scene = app.stage_scene(state, rect.width, rect.height);
    // Realize the floor and performers together: ui::mount starts opaque.
    scene.add(SceneEntity::new("stage floor", Node::raster(floor)).z(-1));
    ui::mount(
        out,
        scene.to_node(
            &Presentation::new(),
            f32::from(rect.width),
            f32::from(rect.height),
        ),
        rect,
    );
}
fn draw_cue_strip(out: &mut Surface, rect: Rect, state: &Rehearsal, hot: Style, base: Style) {
    ui::put(
        out,
        rect.x + 1,
        rect.y,
        &format!(
            "CUES / Q{:02} {} / {}",
            state.cue + 1,
            state.title,
            state.branch.label()
        ),
        hot,
    );
    for cue in 0..12 {
        let x = 1 + cue * usize::from(rect.width.saturating_sub(2)) / 12;
        let marker = if cue == state.cue {
            ">"
        } else if cue < state.cue {
            "+"
        } else {
            "."
        };
        ui::put(
            out,
            x as u16,
            rect.y + 1,
            &format!("{marker}{:02}", cue + 1),
            if cue == state.cue {
                hot.reverse()
            } else {
                base
            },
        );
    }
    ui::put(
        out,
        1,
        rect.y + 2,
        "[A-H] actors  L1/ L2\\ L3. L4: lights (+on/-off)",
        base,
    );
}
fn draw_inspector(
    out: &mut Surface,
    rect: Rect,
    state: &Rehearsal,
    selected: usize,
    wide: bool,
    hot: Style,
    base: Style,
) {
    let actor = &state.performers[selected];
    let status = if actor.in_wing {
        "WING"
    } else if actor.moving {
        "MOVING"
    } else {
        "MARK"
    };
    if wide {
        out.draw_border(rect, BorderType::Rounded, base);
        let lines = [
            " PERFORMER INSPECTOR".to_string(),
            format!(
                " [{}] {} / {status}",
                (b'A' + selected as u8) as char,
                actor.name
            ),
            format!(" Q{:02} {}", state.cue + 1, state.title),
            format!(" Branch: {}", state.branch.label()),
            format!(" Mark: {:02},{:02}", actor.position.0, actor.position.1),
            format!(
                " Target: {:02},{:02}",
                actor.destination.0, actor.destination.1
            ),
            " Enter opens cue card".to_string(),
            "".into(),
            " COMPANY".into(),
        ];
        for (i, text) in lines.iter().enumerate() {
            clipped_text(
                out,
                rect,
                1,
                i as u16 + 1,
                text,
                if i == 0 { hot } else { base },
            );
        }
        for (i, name) in NAMES.iter().enumerate() {
            clipped_text(
                out,
                rect,
                2,
                i as u16 + 10,
                &format!(
                    "{} {} {name}",
                    if i == selected { ">" } else { " " },
                    (b'A' + i as u8) as char
                ),
                base,
            );
        }
    } else {
        ui::put(
            out,
            1,
            rect.y,
            &format!(
                "INSPECT [{}] {} {status} / Q{:02} / {}",
                (b'A' + selected as u8) as char,
                actor.name,
                state.cue + 1,
                state.branch.label()
            ),
            hot,
        );
        ui::put(
            out,
            1,
            rect.y + 1,
            &format!(
                "Mark {:02},{:02} -> {:02},{:02}   Enter: cue card",
                actor.position.0, actor.position.1, actor.destination.0, actor.destination.1
            ),
            base,
        );
    }
}
fn clipped_text(out: &mut Surface, rect: Rect, dx: u16, dy: u16, text: &str, style: Style) {
    if dx < rect.width && dy < rect.height.saturating_sub(1) {
        out.print_str(
            rect.x + dx,
            rect.y + dy,
            text,
            style,
            Some(rect.width.saturating_sub(dx + 1)),
        );
    }
}
fn draw_modal(out: &mut Surface, state: &Rehearsal, selected: usize, hot: Style, base: Style) {
    out.apply_dim_rect(out.area());
    let width = out.width.saturating_sub(6).min(66);
    let rect = Rect::new((out.width - width) / 2, (out.height - 12) / 2, width, 12);
    out.fill_rect(rect, Cell::space(base));
    out.draw_border(rect, BorderType::Double, hot);
    let actor = &state.performers[selected];
    let lines = [
        format!("CUE CARD / Q{:02}", state.cue + 1),
        state.title.clone(),
        format!("Branch: {}", state.branch.label()),
        format!(
            "Performer [{}] {}",
            (b'A' + selected as u8) as char,
            actor.name
        ),
        format!(
            "Position {:02},{:02} / target {:02},{:02}",
            actor.position.0, actor.position.1, actor.destination.0, actor.destination.1
        ),
        if state.branch == Branch::Delayed {
            "Hana waits Q05; enters during Q07.".into()
        } else {
            "Hana enters during Q05.".into()
        },
        "".into(),
        "Focus captured; actor/transport keys held.".into(),
        "Enter / i closes and restores actor focus.".into(),
    ];
    for (i, line) in lines.iter().enumerate() {
        clipped_text(
            out,
            rect,
            2,
            i as u16 + 1,
            line,
            if i == 0 { hot } else { base },
        );
    }
}
